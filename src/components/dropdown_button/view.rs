//! Xilem view layer for the dropdown button component.
//!
//! `DropdownButton<State, Action>` is the builder; `.render(&theme)` produces a
//! `DropdownButtonView`. Clicking the button (anywhere on it) opens or closes the
//! floating menu; selecting an item from the menu fires the corresponding callback.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! # impl State { fn save_as(&mut self) {} fn export(&mut self) {} }
//! use void_ui::components::dropdown_button;
//! dropdown_button("Save")
//!     .item("Save as…", |s: &mut State| s.save_as())
//!     .item("Export", |s: &mut State| s.export())
//!     .render(&theme)
//! # ;
//! ```
//!
//! At `build`, the view looks for the nearest [`crate::overlay_scope`]'s
//! [`OverlayPortal`] in the xilem `Environment`: if present, the menu is
//! registered as a [`MenuContentView`] with [`PortalPlacement::BareTrigger`]
//! (the scope's own view mounts it in the always-on-top `PortalSlot`) and the
//! dropdown hosts only the trigger; otherwise the menu is built in-tree under
//! the dropdown's `AnchoredOverlay`, exactly as before.

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::accesskit::Role;
use masonry::core::{ArcStr, EventCtx};
use masonry::widgets::Passthrough;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::menu_layer::MenuContent;
use super::widget::{
    DropdownButtonAction, DropdownButtonConfig, DropdownButtonHandle, ThemedDropdownButton,
};
use crate::collection::{OnActivated, OnSelect, overlay_list};
use crate::components::button::ButtonVariant;
use crate::overlay::SurfaceStyle;
use crate::overlay_portal::{OverlayPortal, PortalContentView, PortalPlacement, portal_from_env};
use crate::{IconName, Theme};

type ItemCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// Boxed open-change observer: `Fn(&mut State, new_open) -> Action`.
type OpenChangeFn<State, Action> = Arc<dyn Fn(&mut State, bool) -> Action + Send + Sync>;

/// Builder for a dropdown button.
///
/// Create with [`dropdown_button`]; add menu items via [`Self::item`].
/// Materialize as a xilem view via [`Self::render`].
#[must_use = "DropdownButton does nothing until rendered with .render(&theme)"]
pub struct DropdownButton<State, Action> {
    label: ArcStr,
    icon: Option<IconName>,
    items: Vec<(ArcStr, ItemCallback<State, Action>)>,
    variant: ButtonVariant,
    disabled: bool,
    open: Option<bool>,
    on_open_change: Option<OpenChangeFn<State, Action>>,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Construct a dropdown button with the given label.
///
/// Clicking anywhere on the button opens the floating menu. Attach items via
/// [`DropdownButton::item`]; selecting an item fires the item's callback and
/// closes the menu.
pub fn dropdown_button<State, Action>(label: impl Into<ArcStr>) -> DropdownButton<State, Action>
where
    State: 'static,
    Action: 'static,
{
    DropdownButton {
        label: label.into(),
        icon: None,
        items: Vec::new(),
        variant: ButtonVariant::Default,
        disabled: false,
        open: None,
        on_open_change: None,
        phantom: PhantomData,
    }
}

impl<State, Action> DropdownButton<State, Action>
where
    State: 'static,
    Action: 'static,
{
    /// Add a menu item that fires `callback` when selected.
    pub fn item<G>(mut self, label: impl Into<ArcStr>, callback: G) -> Self
    where
        G: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        self.items.push((label.into(), Box::new(callback)));
        self
    }

    /// Attach a leading icon from the Lucide icon set.
    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = Some(name);
        self
    }

    /// Set the visual style variant.
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Host-control the menu's open state (controlled mode). See
    /// [`Self::on_open_change`]. Omit for the default uncontrolled behavior.
    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Observe open/close transitions (fires in both controlled and
    /// uncontrolled mode) with the new open state.
    pub fn on_open_change<G>(mut self, f: G) -> Self
    where
        G: Fn(&mut State, bool) -> Action + Send + Sync + 'static,
    {
        self.on_open_change = Some(Arc::new(f));
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> DropdownButtonView<State, Action> {
        let item_labels: Vec<ArcStr> = self.items.iter().map(|(lbl, _)| lbl.clone()).collect();
        DropdownButtonView {
            label: self.label,
            icon: self.icon,
            items: Arc::new(self.items),
            item_labels,
            variant: self.variant,
            disabled: self.disabled,
            theme: *theme,
            open: self.open,
            on_open_change: self.on_open_change,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`DropdownButton`].
///
/// Not constructed directly; use [`DropdownButton::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct DropdownButtonView<State, Action> {
    label: ArcStr,
    icon: Option<IconName>,
    items: Arc<Vec<(ArcStr, ItemCallback<State, Action>)>>,
    item_labels: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    open: Option<bool>,
    on_open_change: Option<OpenChangeFn<State, Action>>,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> ViewMarker for DropdownButtonView<State, Action> {}

/// Where this dropdown's menu is bound: the nearest scope's portal
/// (registered by key; the scope's view mounts/rebuilds it), or in-tree
/// under our own `ThemedDropdownButton`'s `AnchoredOverlay` overlay slot.
///
/// Unlike the pre-virtualization split, *both* modes now build/rebuild the
/// menu content as a real, nested `MenuContentView` (`overlay_list` needs a
/// real, rebuild-diffed View to virtualize at all, not a one-off `.build()`
/// call) — mirroring the in-tree/portal unification Task 6 required for
/// autocomplete's `SuggestionListView`.
enum MenuBinding<State: 'static, Action: 'static> {
    Portal {
        portal: OverlayPortal<State, Action>,
        key: u64,
        handle: DropdownButtonHandle,
    },
    InTree {
        handle: DropdownButtonHandle,
        /// Persisted `View::ViewState` for the nested `MenuContentView`,
        /// built/rebuilt directly against `ThemedDropdownButton`'s
        /// `AnchoredOverlay` overlay slot — see
        /// `ThemedDropdownButton::with_overlay_content`.
        list_state: BoxedListViewState<State, Action>,
    },
}

/// View state for `DropdownButtonView`: just the menu binding (see
/// [`MenuBinding`]) — the trigger has no nested view-layer children of its
/// own (it's built directly by `ThemedDropdownButton`).
#[doc(hidden)]
pub struct DropdownButtonViewState<State: 'static, Action: 'static> {
    binding: MenuBinding<State, Action>,
}

impl<State, Action> View<State, Action, ViewCtx> for DropdownButtonView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ThemedDropdownButton>;
    type ViewState = DropdownButtonViewState<State, Action>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx);
        let handle = DropdownButtonHandle::new();
        if let Some(portal) = portal {
            let menu_view = build_menu_view(&self.items, &self.theme, &handle);
            let content: Arc<PortalContentView<State, Action>> = Arc::new(menu_view);
            let key = portal.register(
                content,
                &self.theme,
                PortalPlacement::BareTrigger,
                SurfaceStyle::Popover,
            );
            let widget = ThemedDropdownButton::new_portal(
                DropdownButtonConfig {
                    label_text: self.label.clone(),
                    icon: self.icon,
                    items: self.item_labels.clone(),
                    variant: self.variant,
                    disabled: self.disabled,
                    theme: self.theme,
                },
                handle.clone(),
                portal.scope().clone(),
                key,
            )
            .with_open_state(self.open.unwrap_or(false), self.open.is_some());
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                DropdownButtonViewState {
                    binding: MenuBinding::Portal {
                        portal,
                        key,
                        handle,
                    },
                },
            )
        } else {
            let list_view = build_menu_view(&self.items, &self.theme, &handle).boxed();
            let (list_element, list_state) = list_view.build(ctx, state);
            let widget = ThemedDropdownButton::new(
                DropdownButtonConfig {
                    label_text: self.label.clone(),
                    icon: self.icon,
                    items: self.item_labels.clone(),
                    variant: self.variant,
                    disabled: self.disabled,
                    theme: self.theme,
                },
                list_element.new_widget.erased(),
                handle.clone(),
            )
            .with_open_state(self.open.unwrap_or(false), self.open.is_some());
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                DropdownButtonViewState {
                    binding: MenuBinding::InTree { handle, list_state },
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
        if self.theme != prev.theme {
            ThemedDropdownButton::set_theme(&mut element, &self.theme);
        }
        if self.label != prev.label {
            ThemedDropdownButton::set_label(&mut element, self.label.clone());
        }
        if self.disabled != prev.disabled {
            ThemedDropdownButton::set_disabled(&mut element, self.disabled);
        }
        if self.variant != prev.variant {
            ThemedDropdownButton::set_variant(&mut element, self.variant);
        }
        if self.icon.map(char::from) != prev.icon.map(char::from) {
            ThemedDropdownButton::set_icon(&mut element, self.icon);
        }
        if self.item_labels != prev.item_labels {
            ThemedDropdownButton::set_items(&mut element, self.item_labels.clone());
        }
        if self.open.is_some() != prev.open.is_some() {
            ThemedDropdownButton::set_controlled(&mut element, self.open.is_some());
        }
        if let Some(open) = self.open {
            ThemedDropdownButton::set_open(&mut element, open);
        }

        // Forward into the menu content (both hosting modes) whenever
        // anything it depends on changed — mirrors `AutocompleteView::
        // rebuild`'s reversal of the old theme-only re-registration
        // optimization: items now flow through this View path, not an
        // imperative `set_items` on the chrome widget, so re-registering on
        // an item-set or theme change is how the new content actually
        // reaches the list at all. `items` is re-wrapped in a fresh `Arc` on
        // every `render()` call, so pointer equality can never hold across
        // rebuilds; `item_labels` is the real diff signal (as it already is
        // for the `set_items` guard above).
        let items_changed = self.item_labels != prev.item_labels;
        let list_changed = items_changed || self.theme != prev.theme;
        if !list_changed {
            return;
        }

        match &mut view_state.binding {
            MenuBinding::Portal {
                portal,
                key,
                handle,
            } => {
                let menu_view = build_menu_view(&self.items, &self.theme, handle);
                let content: Arc<PortalContentView<State, Action>> = Arc::new(menu_view);
                portal.update(
                    *key,
                    content,
                    &self.theme,
                    PortalPlacement::BareTrigger,
                    SurfaceStyle::Popover,
                );
            }
            MenuBinding::InTree { handle, list_state } => {
                let prev_list_view = build_menu_view(&prev.items, &prev.theme, handle).boxed();
                let list_view = build_menu_view(&self.items, &self.theme, handle).boxed();
                ThemedDropdownButton::with_overlay_content(&mut element, |mut content| {
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
            MenuBinding::Portal { portal, key, .. } => {
                portal.deregister(*key);
            }
            MenuBinding::InTree { handle, list_state } => {
                let list_view = build_menu_view(&self.items, &self.theme, handle).boxed();
                ThemedDropdownButton::with_overlay_content(&mut element, |mut content| {
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
        // A message addressed to *this* view's own `ThemedDropdownButton`
        // arrives fully routed (empty remaining path) — that's
        // `DropdownButtonAction`, submitted directly by the widget. A
        // message with a non-empty path is bound for the nested in-tree
        // `MenuContentView` (only possible in-tree: portal mode's
        // `MenuContentView` lives in a *separate* view subtree — the
        // scope's own portal registry — dispatched by `OverlayScope`
        // directly, never routed through here at all). Same guard shape as
        // `AutocompleteView::message`.
        if message.remaining_path().is_empty() {
            let Some(action) = message.take_message::<DropdownButtonAction>() else {
                tracing::error!(?message, "unexpected message in DropdownButtonView");
                return MessageResult::Stale;
            };
            return match *action {
                DropdownButtonAction::ItemSelected(i) => match self.items.get(i) {
                    Some((_, cb)) => MessageResult::Action(cb(app_state)),
                    None => MessageResult::Stale,
                },
                DropdownButtonAction::OpenChanged(open) => match &self.on_open_change {
                    Some(f) => MessageResult::Action(f(app_state, open)),
                    None => MessageResult::Nop,
                },
            };
        }

        let MenuBinding::InTree { handle, list_state } = &mut view_state.binding else {
            tracing::error!(
                ?message,
                "DropdownButtonView received a routed message in portal mode, which should be \
                 impossible — portal-mode MenuContentView messages are dispatched by \
                 OverlayScope directly, never through here"
            );
            return MessageResult::Stale;
        };
        let list_view = build_menu_view(&self.items, &self.theme, handle).boxed();
        ThemedDropdownButton::with_overlay_content(&mut element, |mut content| {
            let passthrough = content.downcast::<Passthrough>();
            list_view.message(list_state, message, passthrough, app_state)
        })
    }
}

/// `DropdownButtonViewState`'s persisted state for the in-tree nested
/// `MenuContentView` — the `View::ViewState` of a `Box<AnyWidgetView<State,
/// Action>>`, named via projection (mirrors `AutocompleteViewState`'s
/// `BoxedListViewState`) so this doesn't have to depend on `xilem_core`'s
/// internal `AnyViewState` type, which isn't part of its public API surface.
type BoxedListViewState<State, Action> =
    <Box<AnyWidgetView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// Resolves a click-selected item back to its callback by index and invokes
/// it — the "resolve at the moment of the click" pattern `collection::
/// apply_row_activate` uses elsewhere. Indexing (rather than text-matching)
/// is required here because two menu items can share a label: `overlay_list`
/// now carries the row's own index through `on_select`
/// (`crate::collection::item_row_view::OnSelect`), so this resolves the
/// *exact* row that was clicked rather than the first row with matching
/// text. Falls back to the first item (with a `tracing::error!`) if `pos` is
/// out of range for a non-empty `items` — `pos` can only be a row's own
/// index at the moment a live pointer click on that row completed, and
/// `items` is (re-resolved fresh at message time against) the same kind of
/// list that row was built from, so this should be unreachable in practice:
/// a stale closure outliving a shrunk `items` list mid-flight is the only
/// theoretical way to reach it. `items` shrinking all the way to empty in
/// that same window is the same race, just further along — `overlay_list_body`
/// hits an analogous "pos past the end" case for row *rendering* (see its
/// `on_select` closure's doc comment) — so unlike that fallback-to-index-0
/// case, an empty `items` returns `None` instead of indexing a slice that
/// isn't there to index.
fn invoke_selected<State, Action>(
    items: &[(ArcStr, ItemCallback<State, Action>)],
    pos: usize,
    state: &mut State,
) -> Option<Action> {
    let Some((_, cb)) = items.get(pos) else {
        tracing::error!(
            pos,
            len = items.len(),
            "MenuContentView on_select: index out of range for the current item list — this \
             should be unreachable"
        );
        let (_, cb) = items.first()?;
        return Some(cb(state));
    };
    Some(cb(state))
}

/// Builds the (opaque-typed) `MenuContentView` shared by both hosting modes
/// and both `build`/`rebuild` — a plain value constructed fresh every call,
/// mirroring `autocomplete::view::build_list_view`.
///
/// `on_select` resolves the selected row's index back to its callback via
/// [`invoke_selected`] and calls it directly — the final host `Action`,
/// skipping the old `MenuItemSelected` masonry-action hop entirely, now that
/// resolution happens in the View layer. `on_activated` is the synchronous,
/// `EventCtx`-level side effect that closes the menu right after a *click*
/// completes a row selection (`ThemedDropdownButton::close_for_selection`,
/// via `mutate_later`) — unlike autocomplete's `on_activated`, this doesn't
/// also need to move focus: `ThemedDropdownButton` keeps real keyboard focus
/// on its trigger button throughout (no Tab-into-listbox model, no focus
/// gap), so there is nothing to refocus and no `suppress_focus_open`-style
/// hazard to guard against — see `ThemedDropdownButton::set_highlight`'s
/// doc comment and the module-level docs on `crate::collection`'s
/// `CollectionListWidget` re-export for the fuller picture of how
/// `dropdown_button`'s keyboard model differs from autocomplete's. Keyboard
/// selection (Enter on a highlighted item while the trigger has focus)
/// doesn't go through `on_select`/`on_activated` at all — it's resolved
/// entirely inside `ThemedDropdownButton::on_action`'s own `ButtonPress`
/// handling, using its own `highlighted` field.
fn build_menu_view<State, Action>(
    items: &Arc<Vec<(ArcStr, ItemCallback<State, Action>)>>,
    theme: &Theme,
    handle: &DropdownButtonHandle,
) -> MenuContentView<impl WidgetView<State, Action, Widget: Sized>, State, Action>
where
    State: 'static,
    Action: 'static,
{
    let item_labels: Arc<Vec<ArcStr>> =
        Arc::new(items.iter().map(|(label, _)| label.clone()).collect());
    let on_select: OnSelect<State, Action> = {
        let items = Arc::clone(items);
        Arc::new(move |state: &mut State, pos: usize, _text: ArcStr| {
            invoke_selected(&items, pos, state)
        })
    };
    let on_activated: OnActivated = {
        let handle = handle.clone();
        Arc::new(move |ctx: &mut EventCtx<'_>| {
            if let Some(id) = handle.widget_id() {
                ctx.mutate_later(id, |mut w| {
                    let mut dropdown = w.downcast::<ThemedDropdownButton>();
                    ThemedDropdownButton::close_for_selection(&mut dropdown);
                });
            }
        })
    };
    MenuContentView {
        child: overlay_list(
            item_labels,
            None,
            theme,
            Role::Menu,
            Role::MenuItem,
            on_select,
            Some(on_activated),
            // `ThemedDropdownButton` keeps real keyboard focus on its
            // trigger the whole time and drives the menu imperatively via
            // `set_highlight`/`move_highlight` (roving-highlight model,
            // matching how a native `<select>` doesn't expose its options as
            // separate Tab stops) — so this must NOT be a Tab stop. See
            // `CollectionListWidget::accepts_focus`'s doc for the contrast
            // with autocomplete, which passes `true`.
            false,
        ),
        theme: *theme,
        phantom: PhantomData,
    }
}

/// Xilem view wrapping [`MenuContent`], built directly by
/// [`DropdownButtonView`] in both hosting modes (see its module doc):
/// registered with the overlay scope's portal when a scope ancestor exists,
/// or nested directly inside `DropdownButtonView`'s own element (behind
/// `AnchoredOverlay`'s overlay slot) otherwise. Generic over the child view
/// `V` — `overlay_list(...)`'s own (opaque) return type — mirroring
/// `autocomplete::view::SuggestionListView`: the wrapped widget
/// ([`MenuContent<W>`]) stays generic too, so `rebuild`/`teardown`/`message`
/// can forward straight into the child view's own `Mut<'_, Pod<W>>` with no
/// downcast at all.
struct MenuContentView<V, State, Action> {
    child: V,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for MenuContentView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for MenuContentView<V, State, Action>
where
    V: WidgetView<State, Action>,
    V::Widget: masonry::core::FromDynWidget + Sized,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<MenuContent<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = MenuContent::new(child_pod.new_widget, &self.theme);
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
            MenuContent::set_theme(&mut element, &self.theme);
        }
        let child = MenuContent::child_mut(&mut element);
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
            let child = MenuContent::child_mut(&mut element);
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
        let child = MenuContent::child_mut(&mut element);
        self.child.message(view_state, message, child, app_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real behavior of the index->callback resolution `build_menu_view`'s
    /// `on_select` closure relies on — extracted into a standalone,
    /// directly-testable function so this doesn't need a full `ViewCtx`/
    /// `View::message` harness (that end-to-end path, generically, is
    /// covered by `crate::collection::overlay_list_body`'s own
    /// `overlay_list_body_virtualizes_and_routes_selection_through_real_view_messages`
    /// test).
    #[test]
    fn invoke_selected_resolves_by_index_and_calls_the_matching_callback() {
        let items: Vec<(ArcStr, ItemCallback<i32, i32>)> = vec![
            (
                "A".into(),
                Box::new(|s: &mut i32| {
                    *s += 1;
                    10
                }),
            ),
            (
                "B".into(),
                Box::new(|s: &mut i32| {
                    *s += 2;
                    20
                }),
            ),
        ];
        let mut state = 0;
        let result = invoke_selected(&items, 1, &mut state);
        assert_eq!(
            result,
            Some(20),
            "should invoke item B's callback, not item A's"
        );
        assert_eq!(state, 2);
    }

    #[test]
    fn invoke_selected_falls_back_to_index_0_when_pos_is_out_of_range() {
        let items: Vec<(ArcStr, ItemCallback<i32, i32>)> =
            vec![("A".into(), Box::new(|_s: &mut i32| 10))];
        let mut state = 0;
        let result = invoke_selected(&items, 5, &mut state);
        assert_eq!(
            result,
            Some(10),
            "unreachable-in-practice fallback should not panic"
        );
    }

    #[test]
    fn invoke_selected_returns_none_for_an_empty_item_list_instead_of_panicking() {
        let items: Vec<(ArcStr, ItemCallback<i32, i32>)> = vec![];
        let mut state = 0;
        let result = invoke_selected(&items, 0, &mut state);
        assert_eq!(
            result, None,
            "an empty items list has no callback to fall back to — must not index a slice \
             that isn't there"
        );
        assert_eq!(state, 0, "no callback should have run");
    }

    /// Proves the bug this fix closes: two items with the same label
    /// resolved to different callbacks depending on which index was
    /// clicked. Before this fix, `invoke_selected` scanned `items` for a
    /// text match and always found index 0 first, so clicking the second
    /// "Duplicate" row silently invoked the first row's callback.
    #[test]
    fn invoke_selected_resolves_the_second_of_two_duplicate_labeled_items_correctly() {
        let items: Vec<(ArcStr, ItemCallback<i32, i32>)> = vec![
            (
                "Duplicate".into(),
                Box::new(|s: &mut i32| {
                    *s += 1;
                    10
                }),
            ),
            (
                "Duplicate".into(),
                Box::new(|s: &mut i32| {
                    *s += 2;
                    20
                }),
            ),
        ];
        let mut state = 0;
        let result = invoke_selected(&items, 1, &mut state);
        assert_eq!(
            result,
            Some(20),
            "should invoke index 1's callback, not index 0's, even though both \
             items share the same label"
        );
        assert_eq!(state, 2);
    }
}
