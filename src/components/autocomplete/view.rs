//! Xilem view layer for the autocomplete component.
//!
//! [`autocomplete`] returns an [`Autocomplete`] builder. Call `.render(&theme)`
//! to get the concrete xilem view.
//!
//! ```ignore
//! use void_ui::components::autocomplete::autocomplete;
//!
//! autocomplete(state.city.clone(), |s: &mut State, text| s.city = text)
//!     .suggestions(["New York", "Los Angeles", "Chicago", "Houston"])
//!     .placeholder("Enter city…")
//!     .render(&theme)
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

use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{
    AutocompleteAction, AutocompleteConfig, AutocompleteHandle, AutocompleteWidget,
    LabelListHandle, SuggestionList, SuggestionSelected, TextAreaHandle, compute_filtered,
};
use crate::Theme;
use crate::components::popover::widget::SurfaceStyle;
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
    ) -> impl WidgetView<State, Action> + use<F, State, Action>
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

// ─────────────────────────────────────────────────────────────────────────────
// SuggestionListView — portal content view
// ─────────────────────────────────────────────────────────────────────────────

/// Xilem view registered with the overlay scope's portal when a scope ancestor
/// exists. Wraps [`SuggestionList`] and routes [`SuggestionSelected`] actions
/// back to the owning [`AutocompleteWidget`] via [`AutocompleteHandle`].
pub(crate) struct SuggestionListView {
    pub(crate) filtered: Arc<Vec<ArcStr>>,
    pub(crate) handle: AutocompleteHandle,
    pub(crate) listbox_handle: LabelListHandle,
    pub(crate) text_area_handle: TextAreaHandle,
    pub(crate) theme: Theme,
}

impl ViewMarker for SuggestionListView {}

impl<State, Action> View<State, Action, ViewCtx> for SuggestionListView
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<SuggestionList>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = SuggestionList::new(
            (*self.filtered).iter().cloned(),
            &self.theme,
            self.listbox_handle.clone(),
            self.handle.clone(),
            self.text_area_handle.clone(),
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _vs: &mut (),
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) {
        if self.theme != prev.theme {
            SuggestionList::set_theme(&mut element, &self.theme);
        }
        if !Arc::ptr_eq(&self.filtered, &prev.filtered) {
            SuggestionList::set_items(&mut element, (*self.filtered).iter().cloned());
        }
    }

    fn teardown(&self, _vs: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _vs: &mut (),
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) -> MessageResult<Action> {
        if let Some(boxed) = message.take_message::<SuggestionSelected>() {
            let SuggestionSelected(text) = *boxed;
            if let Some(ac_id) = self.handle.widget_id() {
                element.ctx.mutate_later(ac_id, move |mut w| {
                    let mut ac = w.downcast::<AutocompleteWidget>();
                    AutocompleteWidget::portal_select(&mut ac, text.to_string());
                });
            }
        }
        MessageResult::Nop
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AutocompleteView
// ─────────────────────────────────────────────────────────────────────────────

/// The materialized xilem `View` backing an [`Autocomplete`].
///
/// Not constructed directly; use [`Autocomplete::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct AutocompleteView<State, Action> {
    contents: String,
    placeholder: ArcStr,
    suggestions: Arc<Vec<ArcStr>>,
    disabled: bool,
    theme: Theme,
    on_changed: OnChanged<State, Action>,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Where this autocomplete's suggestion list is bound.
enum ViewBinding<State: 'static, Action: 'static> {
    Portal {
        portal: OverlayPortal<State, Action>,
        key: u64,
        /// Retained so rebuilds can re-register with the *same* handle
        /// instances instead of detached defaults — `SuggestionListView`'s
        /// `message`/`build` rely on these resolving to real widget ids.
        handle: AutocompleteHandle,
        listbox_handle: LabelListHandle,
        text_area_handle: TextAreaHandle,
        /// Pre-lowercased mirror of the current suggestions, kept in sync with
        /// `suggestions` changes so `compute_filtered` on the keystroke path
        /// never allocates per item.
        suggestions_lower: Vec<String>,
    },
    InTree,
}

/// View state for `AutocompleteView`.
#[doc(hidden)]
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

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx);
        if let Some(portal) = portal {
            let handle = AutocompleteHandle::new();
            let listbox_handle = LabelListHandle::new();
            let text_area_handle = TextAreaHandle::new();
            let suggestions_lower: Vec<String> =
                self.suggestions.iter().map(|s| s.to_lowercase()).collect();
            let filtered = Arc::new(compute_filtered(
                &self.suggestions,
                &suggestions_lower,
                &self.contents,
            ));
            let list_view = SuggestionListView {
                filtered: filtered.clone(),
                handle: handle.clone(),
                listbox_handle: listbox_handle.clone(),
                text_area_handle: text_area_handle.clone(),
                theme: self.theme,
            };
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
                    all_suggestions: (*self.suggestions).clone(),
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
                        suggestions_lower,
                    },
                },
            )
        } else {
            let widget = AutocompleteWidget::new(
                &self.contents,
                self.placeholder.clone(),
                (*self.suggestions).clone(),
                self.disabled,
                &self.theme,
            );
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                AutocompleteViewState {
                    binding: ViewBinding::InTree,
                },
            )
        }
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) {
        let contents_changed = self.contents != prev.contents;
        let suggestions_changed = self.suggestions != prev.suggestions;

        if contents_changed {
            AutocompleteWidget::set_contents(&mut element, &self.contents);
        }
        if suggestions_changed {
            AutocompleteWidget::set_all_suggestions(&mut element, (*self.suggestions).clone());
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

        // Keep the pre-lowercased mirror in sync whenever suggestions change
        // — cheap, and needed regardless of whether a re-registration below
        // actually happens this rebuild.
        if let ViewBinding::Portal {
            suggestions_lower, ..
        } = &mut view_state.binding
            && suggestions_changed
        {
            *suggestions_lower = self.suggestions.iter().map(|s| s.to_lowercase()).collect();
        }
        // Only re-register the portal content — and only then compute
        // `filtered` — when suggestions or theme actually change.
        // Content-only changes (keystrokes) are already handled by
        // set_contents → mutate_later in the widget layer, so recomputing
        // `filtered` here on every keystroke would just be thrown away.
        if let ViewBinding::Portal {
            portal,
            key,
            handle,
            listbox_handle,
            text_area_handle,
            suggestions_lower,
        } = &view_state.binding
            && (suggestions_changed || self.theme != prev.theme)
        {
            let filtered = Arc::new(compute_filtered(
                &self.suggestions,
                suggestions_lower,
                &self.contents,
            ));
            // Re-use the *same* handle instances from the original portal
            // registration (not detached defaults) — `SuggestionListView`'s
            // `message` and `LabelList`'s back-channels both need
            // `widget_id()` to resolve to the real, already-mounted widgets,
            // which only the original `Arc<OnceLock<_>>`s have.
            let list_view = SuggestionListView {
                filtered,
                handle: handle.clone(),
                listbox_handle: listbox_handle.clone(),
                text_area_handle: text_area_handle.clone(),
                theme: self.theme,
            };
            let content: Arc<PortalContentView<State, Action>> = Arc::new(list_view);
            portal.update(
                *key,
                content,
                &self.theme,
                PortalPlacement::BareTrigger,
                SurfaceStyle::Popover,
            );
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        if let ViewBinding::Portal { portal, key, .. } = &view_state.binding {
            portal.deregister(*key);
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if let Some(boxed) = message.take_message::<AutocompleteAction>() {
            let AutocompleteAction::TextChanged(text) = *boxed;
            return MessageResult::Action((self.on_changed)(app_state, text));
        }
        tracing::error!(?message, "unexpected message in AutocompleteView");
        MessageResult::Stale
    }
}
