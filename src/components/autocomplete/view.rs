//! Xilem view layer for the autocomplete component.
//!
//! [`autocomplete`] returns an [`Autocomplete`] builder. Call `.render(&theme)`
//! to get the concrete xilem view.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { city: String }
//! # let state = State { city: String::new() };
//! use void_ui::components::autocomplete::autocomplete;
//!
//! autocomplete(state.city.clone(), |s: &mut State, text| s.city = text)
//!     .suggestions(["New York", "Los Angeles", "Chicago", "Houston"])
//!     .placeholder("Enter city…")
//!     .render(&theme)
//! # ;
//! ```
//!
//! The text is **host-controlled**: on every keystroke the `on_changed`
//! callback fires with the full updated string, and the host stores it and
//! passes it back in on the next render (same contract as [`Input`]).
//! Selecting a suggestion also fires `on_changed` with the selected text.
//!
//! When inside an [`crate::overlay_scope`] the suggestion dropdown is mounted
//! in the scope's always-on-top portal slot so it paints above siblings
//! (same pattern as [`crate::components::dropdown_button`]); otherwise it
//! falls back to an in-tree [`crate::AnchoredOverlay`].

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::accesskit::Role;
use masonry::core::{ArcStr, EventCtx};
use masonry::widgets::Passthrough;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::widget::{
    AutocompleteAction, AutocompleteConfig, AutocompleteHandle, AutocompleteWidget, ListboxHandle,
    SuggestionList, TextAreaHandle,
};
use crate::Theme;
use crate::collection::{OnActivated, OnSelect, overlay_list};
use crate::overlay::SurfaceStyle;
use crate::overlay_portal::{OverlayPortal, PortalContentView, PortalPlacement, portal_from_env};

type OnChanged<State, Action> = Arc<dyn Fn(&mut State, String) -> Action + Send + Sync + 'static>;

/// Builder for an autocomplete text input.
///
/// Created with [`autocomplete`]. Returns a xilem view via [`Self::render`].
#[must_use = "Autocomplete does nothing until rendered with .render(&theme)"]
pub struct Autocomplete<F> {
    contents: String,
    placeholder: ArcStr,
    suggestions: Vec<ArcStr>,
    disabled: bool,
    on_changed: F,
}

/// Create a single-line autocomplete field with the given contents and change
/// callback.
///
/// `contents` is host-controlled — the widget never mutates it directly.
/// `on_changed` fires with the full updated string on every keystroke and on
/// suggestion selection; the host stores it and passes it back in on the next
/// render.
pub fn autocomplete<F>(contents: impl Into<String>, on_changed: F) -> Autocomplete<F> {
    Autocomplete {
        contents: contents.into(),
        placeholder: ArcStr::default(),
        suggestions: Vec::new(),
        disabled: false,
        on_changed,
    }
}

impl<F> Autocomplete<F> {
    /// Set the placeholder shown while the field is empty.
    pub fn placeholder(mut self, text: impl Into<ArcStr>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Provide the full list of candidate suggestions. The component filters
    /// them with a case-insensitive prefix match as the user types.
    pub fn suggestions<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<ArcStr>,
    {
        self.suggestions = items.into_iter().map(Into::into).collect();
        self
    }

    /// Disable the field: stops accepting input and paints muted.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action, Widget: Sized> + use<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    {
        AutocompleteView {
            contents: self.contents,
            placeholder: self.placeholder,
            suggestions: Arc::new(self.suggestions),
            disabled: self.disabled,
            theme: *theme,
            on_changed: Arc::new(self.on_changed),
            phantom: PhantomData,
        }
    }
}

/// Returns every candidate matching `query` (case-insensitive prefix match),
/// or the full list when `query` is empty. Moved here from
/// `AutocompleteWidget` (widget-event-time) because `SuggestionListView` now
/// needs the result as a real View prop, computed at view-build/rebuild time
/// from `AutocompleteView`'s own `contents`/`suggestions` fields — see the
/// design spec's "Key insight" for why.
///
/// Unlike the pre-virtualization version, this is *not* capped (virtualizing
/// the list is the whole point of this task — see `crate::collection::
/// overlay_list`) and does not take a precomputed-lowercase mirror of `all`.
/// That mirror existed to avoid a `to_lowercase` allocation per item on the
/// hot *keystroke* path, when this ran once per keystroke against a widget
/// hand-rolled loop over every candidate; it now runs at most a couple of
/// times per render (here, and once more for `AutocompleteWidget::
/// set_match_summary`'s callers below), triggered only by genuine prop
/// changes (typing, focus, host-driven suggestion updates) rather than every
/// masonry action dispatch — a categorically cheaper call pattern that
/// doesn't carry its weight. If a future host passes a suggestion list large
/// enough for this to show up in a profile, reintroducing a cached
/// lowercase mirror (keyed on `suggestions` only recomputing when that
/// `Arc` actually changes) is the natural next step.
fn compute_filtered(all: &[ArcStr], query: &str) -> Vec<ArcStr> {
    if query.is_empty() {
        return all.to_vec();
    }
    let q = query.to_lowercase();
    all.iter()
        .filter(|s| s.to_lowercase().starts_with(&q))
        .cloned()
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionListView — the suggestion dropdown's content, in both hosting modes
// ─────────────────────────────────────────────────────────────────────────────

/// Xilem view wrapping [`SuggestionList`], built directly by [`AutocompleteView`]
/// in both hosting modes now (see its module doc): registered with the
/// overlay scope's portal when a scope ancestor exists, or nested directly
/// inside `AutocompleteView`'s own element (behind `AnchoredOverlay`'s
/// overlay slot) otherwise. Generic over the child view `V` —
/// `overlay_list(...)`'s own (opaque) return type — mirroring
/// `ClickableRow<V, State, Action, F>` (`collection/row_click.rs`): the
/// wrapped widget ([`SuggestionList<W>`]) stays generic too, so `rebuild`/
/// `teardown`/`message` can forward straight into the child view's own
/// `Mut<'_, Pod<W>>` with no downcast at all — `this.ctx.get_mut` on a
/// concretely-typed `WidgetPod<W>` field already produces exactly that type.
///
/// `W` (and so `V`) never needs to be erased *inside* this pairing — it only
/// gets erased one level up, at `AutocompleteView`'s own boundary, via
/// [`WidgetView::boxed`]/[`Arc::new`] into `Pod<Passthrough>` (the same
/// `AnyElement<Pod<W>, ViewCtx> for Pod<Passthrough>` blanket impl
/// `xilem_masonry::any_view` already provides for any `W: Widget +
/// FromDynWidget`), which both `AnchoredOverlay`'s in-tree overlay slot and
/// the portal's own content registration already expect erased content for.
struct SuggestionListView<V, State, Action> {
    child: V,
    theme: Theme,
    handle: AutocompleteHandle,
    text_area_handle: TextAreaHandle,
    listbox_handle: ListboxHandle,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for SuggestionListView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for SuggestionListView<V, State, Action>
where
    V: WidgetView<State, Action>,
    V::Widget: masonry::core::FromDynWidget + Sized,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<SuggestionList<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = SuggestionList::new(
            child_pod.new_widget,
            &self.theme,
            self.handle.clone(),
            self.text_area_handle.clone(),
            self.listbox_handle.clone(),
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            SuggestionList::set_theme(&mut element, &self.theme);
        }
        let child = SuggestionList::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child, app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let child = SuggestionList::child_mut(&mut element);
            self.child.teardown(view_state, ctx, child);
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let child = SuggestionList::child_mut(&mut element);
        self.child.message(view_state, message, child, app_state)
    }
}

/// Builds the (opaque-typed) `SuggestionListView` shared by both hosting
/// modes and both `build`/`rebuild` — a plain value constructed fresh every
/// call (the normal xilem pattern; nothing here needs to persist beyond the
/// call that builds it, since the state that *does* need to persist —
/// `V::ViewState`/`AnyViewState` — lives in `AutocompleteViewState` instead).
///
/// `on_select` calls `on_changed` directly (the host's real action), and
/// `on_activated` is the synchronous, `EventCtx`-level side effect that runs
/// when a *click* completes a row selection — refocusing the text input and
/// closing the dropdown. Keyboard selection (Enter once Tab'd into the
/// listbox) doesn't need this: `CollectionListWidget::on_text_event`'s own
/// Enter handler doesn't consume the keypress, so it bubbles to
/// `SuggestionList::on_text_event`, which refocuses/closes for
/// Enter/Escape/Tab uniformly, synchronously, from real `EventCtx` — see
/// both those modules' docs for why refocus/close can't happen from
/// `on_select` itself (fired later, from `View::message`, which only ever
/// has `MutateCtx` — masonry's `set_focus`/`request_focus` are `EventCtx`/
/// `ActionCtx`-only).
fn build_list_view<State, Action>(
    items: Arc<Vec<ArcStr>>,
    theme: &Theme,
    handle: AutocompleteHandle,
    text_area_handle: TextAreaHandle,
    listbox_handle: ListboxHandle,
    on_changed: OnChanged<State, Action>,
) -> SuggestionListView<impl WidgetView<State, Action, Widget: Sized>, State, Action>
where
    State: 'static,
    Action: 'static,
{
    let on_select: OnSelect<State, Action> =
        Arc::new(move |state: &mut State, text: ArcStr| (on_changed)(state, text.to_string()));
    let on_activated: OnActivated = {
        let text_area_handle = text_area_handle.clone();
        let handle = handle.clone();
        Arc::new(move |ctx: &mut EventCtx<'_>| {
            if let Some(id) = text_area_handle.widget_id() {
                ctx.set_focus(id);
            }
            if let Some(ac_id) = handle.widget_id() {
                ctx.mutate_later(ac_id, |mut w| {
                    let mut ac = w.downcast::<AutocompleteWidget>();
                    AutocompleteWidget::mark_closed_after_click(&mut ac);
                });
            }
        })
    };
    SuggestionListView {
        child: overlay_list(
            items,
            None,
            theme,
            Role::ListBox,
            Role::ListBoxOption,
            on_select,
            Some(on_activated),
        ),
        theme: *theme,
        handle,
        text_area_handle,
        listbox_handle,
        phantom: PhantomData,
    }
}

/// `AutocompleteViewState`'s persisted state for the in-tree nested
/// `SuggestionListView` — the `View::ViewState` of a `Box<AnyWidgetView<
/// State, Action>>`, named via projection (mirrors
/// `overlay_portal::PortalContentViewState`) so this doesn't have to depend
/// on `xilem_core`'s internal `AnyViewState` type, which isn't part of its
/// public API surface.
type BoxedListViewState<State, Action> =
    <Box<AnyWidgetView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

// ─────────────────────────────────────────────────────────────────────────────
// AutocompleteView
// ─────────────────────────────────────────────────────────────────────────────

/// The materialized xilem `View` backing an [`Autocomplete`].
///
/// Not constructed directly; use [`Autocomplete::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct AutocompleteView<State, Action> {
    contents: String,
    placeholder: ArcStr,
    suggestions: Arc<Vec<ArcStr>>,
    disabled: bool,
    theme: Theme,
    on_changed: OnChanged<State, Action>,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Where this autocomplete's suggestion list is bound. Both variants retain
/// the same handle instances the `SuggestionListView` they built was given —
/// `SuggestionList`'s own Enter/Escape/Tab handling and a click's
/// `on_activated` hook both resolve `AutocompleteHandle`/`TextAreaHandle`
/// lazily, but reusing the *same* `Arc`-backed handles (not fresh, detached
/// ones) is what lets a later rebuild still reach the original mounted
/// widgets.
enum ViewBinding<State: 'static, Action: 'static> {
    Portal {
        portal: OverlayPortal<State, Action>,
        key: u64,
        handle: AutocompleteHandle,
        listbox_handle: ListboxHandle,
        text_area_handle: TextAreaHandle,
    },
    InTree {
        handle: AutocompleteHandle,
        listbox_handle: ListboxHandle,
        text_area_handle: TextAreaHandle,
        /// Persisted `View::ViewState` for the nested `SuggestionListView`,
        /// built/rebuilt directly against `AutocompleteWidget`'s
        /// `AnchoredOverlay` overlay slot — see
        /// `AutocompleteWidget::with_overlay_content`.
        list_state: BoxedListViewState<State, Action>,
    },
}

/// View state for `AutocompleteView`.
pub struct AutocompleteViewState<State: 'static, Action: 'static> {
    binding: ViewBinding<State, Action>,
}

impl<State, Action> ViewMarker for AutocompleteView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for AutocompleteView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<AutocompleteWidget>;
    type ViewState = AutocompleteViewState<State, Action>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx);
        let handle = AutocompleteHandle::new();
        let listbox_handle = ListboxHandle::new();
        let text_area_handle = TextAreaHandle::new();
        let filtered = Arc::new(compute_filtered(&self.suggestions, &self.contents));
        let first_suggestion = filtered.first().cloned();

        if let Some(portal) = portal {
            let list_view = build_list_view(
                filtered,
                &self.theme,
                handle.clone(),
                text_area_handle.clone(),
                listbox_handle.clone(),
                Arc::clone(&self.on_changed),
            );
            let content: Arc<PortalContentView<State, Action>> = Arc::new(list_view);
            let key = portal.register(
                content,
                &self.theme,
                PortalPlacement::BareTrigger,
                SurfaceStyle::Popover,
            );
            let widget = AutocompleteWidget::new_portal(
                AutocompleteConfig {
                    contents: self.contents.clone(),
                    placeholder: self.placeholder.clone(),
                    first_suggestion,
                    disabled: self.disabled,
                    theme: self.theme,
                },
                portal.scope().clone(),
                key,
                handle.clone(),
                listbox_handle.clone(),
                &text_area_handle,
            );
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                AutocompleteViewState {
                    binding: ViewBinding::Portal {
                        portal,
                        key,
                        handle,
                        listbox_handle,
                        text_area_handle,
                    },
                },
            )
        } else {
            let list_view = build_list_view(
                filtered,
                &self.theme,
                handle.clone(),
                text_area_handle.clone(),
                listbox_handle.clone(),
                Arc::clone(&self.on_changed),
            )
            .boxed();
            let (list_element, list_state) = list_view.build(ctx, state);
            let widget = AutocompleteWidget::new(
                AutocompleteConfig {
                    contents: self.contents.clone(),
                    placeholder: self.placeholder.clone(),
                    first_suggestion,
                    disabled: self.disabled,
                    theme: self.theme,
                },
                list_element.new_widget.erased(),
                handle.clone(),
                listbox_handle.clone(),
                &text_area_handle,
            );
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                AutocompleteViewState {
                    binding: ViewBinding::InTree {
                        handle,
                        listbox_handle,
                        text_area_handle,
                        list_state,
                    },
                },
            )
        }
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        let contents_changed = self.contents != prev.contents;
        let suggestions_changed = self.suggestions != prev.suggestions;

        if contents_changed {
            AutocompleteWidget::set_contents(&mut element, &self.contents);
        }
        // Decoupled from `contents_changed`'s guard above on purpose: a typed
        // keystroke round-trips `contents` back unchanged (host-controlled),
        // so `set_contents` alone would never re-evaluate the matching set —
        // see `AutocompleteWidget::set_match_summary`'s doc comment.
        if contents_changed || suggestions_changed {
            let filtered = compute_filtered(&self.suggestions, &self.contents);
            AutocompleteWidget::set_match_summary(&mut element, filtered.first().cloned());
        }
        if self.placeholder != prev.placeholder {
            AutocompleteWidget::set_placeholder(&mut element, self.placeholder.clone());
        }
        if self.disabled != prev.disabled {
            AutocompleteWidget::set_disabled(&mut element, self.disabled);
        }
        if self.theme != prev.theme {
            AutocompleteWidget::set_theme(&mut element, &self.theme);
        }

        // Forward into the suggestion list (both hosting modes) whenever
        // anything it depends on changed. This reverses the old theme-only
        // re-registration optimization: previously the portal content was an
        // empty shell the widget populated imperatively via `set_items`, so
        // re-registering for anything but a theme change was pure churn.
        // Items now flow through this View path on every keystroke
        // regardless — the whole point of virtualizing via `overlay_list` —
        // so re-registering/rebuilding on content or suggestion-set changes
        // too is not optional, it's how the new items actually reach the
        // list at all.
        let list_changed = contents_changed || suggestions_changed || self.theme != prev.theme;
        if !list_changed {
            return;
        }

        match &mut view_state.binding {
            ViewBinding::Portal {
                portal,
                key,
                handle,
                listbox_handle,
                text_area_handle,
            } => {
                let filtered = Arc::new(compute_filtered(&self.suggestions, &self.contents));
                let list_view = build_list_view(
                    filtered,
                    &self.theme,
                    handle.clone(),
                    text_area_handle.clone(),
                    listbox_handle.clone(),
                    Arc::clone(&self.on_changed),
                );
                let content: Arc<PortalContentView<State, Action>> = Arc::new(list_view);
                portal.update(
                    *key,
                    content,
                    &self.theme,
                    PortalPlacement::BareTrigger,
                    SurfaceStyle::Popover,
                );
            }
            ViewBinding::InTree {
                handle,
                listbox_handle,
                text_area_handle,
                list_state,
            } => {
                let prev_filtered = Arc::new(compute_filtered(&prev.suggestions, &prev.contents));
                let prev_list_view = build_list_view(
                    prev_filtered,
                    &prev.theme,
                    handle.clone(),
                    text_area_handle.clone(),
                    listbox_handle.clone(),
                    Arc::clone(&prev.on_changed),
                )
                .boxed();
                let filtered = Arc::new(compute_filtered(&self.suggestions, &self.contents));
                let list_view = build_list_view(
                    filtered,
                    &self.theme,
                    handle.clone(),
                    text_area_handle.clone(),
                    listbox_handle.clone(),
                    Arc::clone(&self.on_changed),
                )
                .boxed();
                AutocompleteWidget::with_overlay_content(&mut element, |mut content| {
                    let passthrough = content.downcast::<Passthrough>();
                    list_view.rebuild(&prev_list_view, list_state, ctx, passthrough, state);
                });
            }
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        match &mut view_state.binding {
            ViewBinding::Portal { portal, key, .. } => {
                portal.deregister(*key);
            }
            ViewBinding::InTree {
                handle,
                listbox_handle,
                text_area_handle,
                list_state,
            } => {
                let filtered = Arc::new(compute_filtered(&self.suggestions, &self.contents));
                let list_view = build_list_view(
                    filtered,
                    &self.theme,
                    handle.clone(),
                    text_area_handle.clone(),
                    listbox_handle.clone(),
                    Arc::clone(&self.on_changed),
                )
                .boxed();
                AutocompleteWidget::with_overlay_content(&mut element, |mut content| {
                    let passthrough = content.downcast::<Passthrough>();
                    list_view.teardown(list_state, ctx, passthrough);
                });
            }
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // A message addressed to *this* view's own `AutocompleteWidget`
        // arrives fully routed (empty remaining path) — that's
        // `AutocompleteAction`, submitted directly by the widget. A message
        // with a non-empty path is bound for the nested in-tree
        // `SuggestionListView` (only possible in-tree: portal mode's
        // `SuggestionListView` lives in a *separate* view subtree — the
        // scope's own portal registry — dispatched by `OverlayScope`
        // directly, never routed through here at all). Same guard shape as
        // `ClickableRow`/`CollapsibleView` (`collection/row_click.rs`,
        // `components/collapsible/view.rs`).
        if message.remaining_path().is_empty() {
            if let Some(boxed) = message.take_message::<AutocompleteAction>() {
                let AutocompleteAction::TextChanged(text) = *boxed;
                return MessageResult::Action((self.on_changed)(app_state, text));
            }
            tracing::error!(?message, "unexpected message in AutocompleteView");
            return MessageResult::Stale;
        }

        let ViewBinding::InTree {
            handle,
            listbox_handle,
            text_area_handle,
            list_state,
        } = &mut view_state.binding
        else {
            tracing::error!(
                ?message,
                "AutocompleteView received a routed message in portal mode, which should be \
                 impossible — portal-mode SuggestionListView messages are dispatched by \
                 OverlayScope directly, never through here"
            );
            return MessageResult::Stale;
        };
        let filtered = Arc::new(compute_filtered(&self.suggestions, &self.contents));
        let list_view = build_list_view(
            filtered,
            &self.theme,
            handle.clone(),
            text_area_handle.clone(),
            listbox_handle.clone(),
            Arc::clone(&self.on_changed),
        )
        .boxed();
        AutocompleteWidget::with_overlay_content(&mut element, |mut content| {
            let passthrough = content.downcast::<Passthrough>();
            list_view.message(list_state, message, passthrough, app_state)
        })
    }
}
