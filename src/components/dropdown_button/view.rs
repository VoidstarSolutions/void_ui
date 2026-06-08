//! Xilem view layer for the dropdown button component.
//!
//! `DropdownButton<State, Action>` is the builder; `.render(&theme)` produces a
//! `DropdownButtonView`. Clicking the button (anywhere on it) opens or closes the
//! floating menu; selecting an item from the menu fires the corresponding callback.
//!
//! ```ignore
//! use void_ui::components::dropdown_button;
//! dropdown_button("Save")
//!     .item("Save as…", |s: &mut State| s.save_as())
//!     .item("Export", |s: &mut State| s.export())
//!     .render(&theme)
//! ```

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::core::ArcStr;
use masonry::kurbo::BezPath;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx};

use super::widget::{DropdownButtonAction, ThemedDropdownButton};
use crate::Theme;
use crate::components::button::ButtonVariant;
use crate::overlay_scope::OverlayScopeHandle;

type ItemCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// Builder for a dropdown button.
///
/// Create with [`dropdown_button`]; add menu items via [`Self::item`].
/// Materialize as a xilem view via [`Self::render`].
#[must_use = "DropdownButton does nothing until rendered with .render(&theme)"]
pub struct DropdownButton<State, Action> {
    label: ArcStr,
    icon: Option<Arc<BezPath>>,
    items: Vec<(ArcStr, ItemCallback<State, Action>)>,
    variant: ButtonVariant,
    disabled: bool,
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

    /// Attach a leading icon (unit-square `BezPath`, scaled to UI font size).
    pub fn icon(mut self, path: BezPath) -> Self {
        self.icon = Some(Arc::new(path));
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

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> DropdownButtonView<State, Action> {
        let item_labels: Vec<ArcStr> = self.items.iter().map(|(lbl, _)| lbl.clone()).collect();
        DropdownButtonView {
            label: self.label,
            icon: self.icon,
            items: self.items,
            item_labels,
            variant: self.variant,
            disabled: self.disabled,
            theme: *theme,
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
    icon: Option<Arc<BezPath>>,
    items: Vec<(ArcStr, ItemCallback<State, Action>)>,
    item_labels: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> ViewMarker for DropdownButtonView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for DropdownButtonView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ThemedDropdownButton>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        // Discover an `OverlayScope` ancestor without `with_context` (which
        // panics when absent) — this lookup must tolerate "no scope in this
        // tree" so that `dropdown_button` keeps working at every existing
        // call site, falling back to its in-tree `AnchoredOverlay`. See
        // `crate::overlay_scope` for the handle/`Environment` design.
        let scope = ctx
            .environment()
            .get_slot_for_type::<OverlayScopeHandle>()
            .and_then(|i| ctx.environment().slots[i as usize].item.as_ref())
            .and_then(|item| item.value.downcast_ref::<OverlayScopeHandle>())
            .cloned();
        let widget = ThemedDropdownButton::new(
            self.label.clone(),
            self.icon.clone(),
            self.item_labels.clone(),
            self.variant,
            self.disabled,
            &self.theme,
            scope,
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            ThemedDropdownButton::set_theme(&mut element, &self.theme);
        }
        if self.disabled != prev.disabled {
            ThemedDropdownButton::set_disabled(&mut element, self.disabled);
        }
        if self.variant != prev.variant {
            ThemedDropdownButton::set_variant(&mut element, self.variant);
        }
        let icon_changed = match (&self.icon, &prev.icon) {
            (None, None) => false,
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        };
        if icon_changed {
            ThemedDropdownButton::set_icon(&mut element, self.icon.clone());
        }
        if self.item_labels != prev.item_labels {
            ThemedDropdownButton::set_items(&mut element, self.item_labels.clone());
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<DropdownButtonAction>() {
            Some(action) => match *action {
                DropdownButtonAction::ItemSelected(i) => {
                    if let Some((_, cb)) = self.items.get(i) {
                        MessageResult::Action(cb(app_state))
                    } else {
                        MessageResult::Stale
                    }
                }
            },
            None => MessageResult::Stale,
        }
    }
}
