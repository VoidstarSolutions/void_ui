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
//!
//! At `build`, the view looks for the nearest [`crate::overlay_scope`]'s
//! [`OverlayPortal`] in the xilem `Environment`: if present, the menu is
//! registered as a [`MenuContentView`] with [`PortalPlacement::BareTrigger`]
//! (the scope's own view mounts it in the always-on-top `PortalSlot`) and the
//! dropdown hosts only the trigger; otherwise the menu is built in-tree under
//! the dropdown's `AnchoredOverlay`, exactly as before.

use std::marker::PhantomData;
use std::sync::Arc;

use lucide_icons::Icon as LucideIcon;
use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::menu_layer::{MenuContent, MenuItemSelected};
use super::widget::{
    DropdownButtonAction, DropdownButtonConfig, DropdownButtonHandle, ThemedDropdownButton,
};
use crate::Theme;
use crate::components::button::ButtonVariant;
use crate::overlay::SurfaceStyle;
use crate::overlay_portal::{OverlayPortal, PortalContentView, PortalPlacement, portal_from_env};

type ItemCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// Builder for a dropdown button.
///
/// Create with [`dropdown_button`]; add menu items via [`Self::item`].
/// Materialize as a xilem view via [`Self::render`].
#[must_use = "DropdownButton does nothing until rendered with .render(&theme)"]
pub struct DropdownButton<State, Action> {
    label: ArcStr,
    icon: Option<LucideIcon>,
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

    /// Attach a leading icon from the Lucide icon set.
    pub fn icon(mut self, name: LucideIcon) -> Self {
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
    icon: Option<LucideIcon>,
    items: Arc<Vec<(ArcStr, ItemCallback<State, Action>)>>,
    item_labels: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> ViewMarker for DropdownButtonView<State, Action> {}

/// Where this dropdown's menu is bound: the nearest scope's portal
/// (registered by key; the scope's view mounts/rebuilds it), or in-tree under
/// our own `ThemedDropdownButton` (fallback, handled entirely by the widget).
enum MenuBinding<State: 'static, Action: 'static> {
    Portal {
        portal: OverlayPortal<State, Action>,
        key: u64,
        handle: DropdownButtonHandle,
    },
    InTree,
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

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx);
        if let Some(portal) = portal {
            let handle = DropdownButtonHandle::new();
            let menu_view = MenuContentView {
                items: self.items.clone(),
                dropdown_handle: handle.clone(),
                theme: self.theme,
            };
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
            );
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
            let widget = ThemedDropdownButton::new(
                self.label.clone(),
                self.icon,
                self.item_labels.clone(),
                self.variant,
                self.disabled,
                &self.theme,
            );
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                DropdownButtonViewState {
                    binding: MenuBinding::InTree,
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
        _app_state: &mut State,
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

        if let MenuBinding::Portal {
            portal,
            key,
            handle,
        } = &mut view_state.binding
        {
            // Content rebuild happens when the scope's view diffs the
            // registry (after our subtree's rebuild returns) — we only
            // refresh the registered view value here, mirroring
            // `PopoverView::rebuild`'s `ContentBinding::Portal` arm.
            if !Arc::ptr_eq(&self.items, &prev.items) || self.theme != prev.theme {
                let menu_view = MenuContentView {
                    items: self.items.clone(),
                    dropdown_handle: handle.clone(),
                    theme: self.theme,
                };
                let content: Arc<PortalContentView<State, Action>> = Arc::new(menu_view);
                portal.update(
                    *key,
                    content,
                    &self.theme,
                    PortalPlacement::BareTrigger,
                    SurfaceStyle::Popover,
                );
            }
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        if let MenuBinding::Portal { portal, key, .. } = &mut view_state.binding {
            portal.deregister(*key);
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let _ = &view_state.binding;
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

/// The content view registered with the scope's [`OverlayPortal`] for a
/// portal-mode dropdown menu — wraps [`MenuContent`] and, on item selection,
/// both calls the item's callback (producing `Action`) and notifies the
/// owning [`ThemedDropdownButton`] (via [`DropdownButtonHandle`]) to close
/// the menu and clear the keyboard highlight. The menu is not a descendant of
/// the dropdown in this mode, so normal action bubbling never reaches
/// `ThemedDropdownButton::on_action`.
struct MenuContentView<State, Action> {
    items: Arc<Vec<(ArcStr, ItemCallback<State, Action>)>>,
    dropdown_handle: DropdownButtonHandle,
    theme: Theme,
}

impl<State, Action> ViewMarker for MenuContentView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for MenuContentView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<MenuContent>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let item_labels: Vec<ArcStr> = self.items.iter().map(|(lbl, _)| lbl.clone()).collect();
        let widget = MenuContent::new(item_labels, &self.theme);
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
            MenuContent::set_theme(&mut element, &self.theme);
        }
        if !Arc::ptr_eq(&self.items, &prev.items) {
            let item_labels: Vec<ArcStr> = self.items.iter().map(|(lbl, _)| lbl.clone()).collect();
            MenuContent::set_items(&mut element, item_labels);
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
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<MenuItemSelected>() {
            Some(boxed) => {
                let MenuItemSelected(index) = *boxed;
                if let Some(dropdown_id) = self.dropdown_handle.widget_id() {
                    element.ctx.mutate_later(dropdown_id, |mut w| {
                        let mut dropdown = w.downcast::<ThemedDropdownButton>();
                        ThemedDropdownButton::mark_closed(&mut dropdown);
                    });
                }
                match self.items.get(index) {
                    Some((_, cb)) => MessageResult::Action(cb(app_state)),
                    None => MessageResult::Stale,
                }
            }
            None => MessageResult::Stale,
        }
    }
}
