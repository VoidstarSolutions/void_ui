//! `ThemedDropdownButton` — a [`crate::components::button::widget::ThemedButton`]
//! trigger (with a trailing chevron) that opens a floating menu on click.
//!
//! Composes a real `ThemedButton` as its trigger and delegates all
//! chrome/color/layout/focus/click handling to it — this widget owns only what's
//! genuinely dropdown-shaped: menu hosting, `items`, `open` state, and
//! item-selection routing. Clicking anywhere on the wrapped button (label,
//! leading icon, or chevron) toggles the menu — the inner `ThemedButton`
//! submits a single `ButtonPress` for the whole surface, same as it would for
//! a plain action button.
//!
//! Two hosting modes, mirroring [`crate::components::popover::widget::PopoverHost`]:
//!
//! - **Portal** (scope ancestor present): the menu is registered as a
//!   [`super::view::MenuContentView`] in the scope's [`crate::overlay_portal::OverlayPortal`]
//!   with [`crate::overlay_portal::PortalPlacement::BareTrigger`] — anchored
//!   like a popover but unwrapped, since `MenuContent` paints its own chrome.
//!   Open/close/highlight are pushed as plain data via `mutate_later`.
//! - **In-tree** (fallback, no scope): `MenuContent` is permanently mounted in
//!   an [`AnchoredOverlay`] below the trigger, toggled via
//!   `AnchoredOverlay::set_overlay_visible`.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ComposeCtx, ErasedAction, EventCtx, LayoutCtx,
    MeasureCtx, NewWidget, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty,
    TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label, Passthrough};

use super::menu_layer::{MenuContent, MenuItemSelected};
use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::button::ButtonVariant;
use crate::components::button::widget::ThemedButton;
use crate::components::icon::IconName;
use crate::overlay::OverlayAnchor;
use crate::overlay::binding::{PortalBinding, PortalCtx, PortalOpenCtx};
use crate::overlay_portal::PortalSlot;
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// Action type emitted by [`ThemedDropdownButton`].
#[derive(Debug)]
pub enum DropdownButtonAction {
    /// Menu item at `index` was selected.
    ItemSelected(usize),
}

widget_id_handle!(
    /// Self-filling handle to a [`ThemedDropdownButton`]'s widget id, filled at
    /// `Update::WidgetAdded` — mirrors [`OverlayScopeHandle`]'s bootstrapping.
    ///
    /// Given to a portal-mounted [`super::view::MenuContentView`] so an item
    /// selection can `mutate_later` back into the dropdown to close the menu and
    /// clear the keyboard highlight: in portal mode the menu is not a descendant
    /// of the dropdown, so normal action bubbling never reaches
    /// [`ThemedDropdownButton::on_action`].
    DropdownButtonHandle
);

/// Trigger-construction inputs shared by [`ThemedDropdownButton::new`] and
/// [`ThemedDropdownButton::new_portal`]; bundled to keep `new_portal`'s
/// argument count under clippy's `too_many_arguments` threshold.
pub(crate) struct DropdownButtonConfig {
    pub(crate) label_text: ArcStr,
    pub(crate) icon: Option<IconName>,
    pub(crate) items: Vec<ArcStr>,
    pub(crate) variant: ButtonVariant,
    pub(crate) disabled: bool,
    pub(crate) theme: Theme,
}

/// How this dropdown mounts its menu: permanently in-tree (fallback, no scope
/// ancestor), or portal-mounted in the nearest scope's `PortalSlot` (the menu
/// is a view child of the *scope*; we only hold the key).
enum Hosting {
    InTree {
        overlay_host: WidgetPod<AnchoredOverlay>,
    },
    Portal {
        trigger: WidgetPod<dyn Widget>,
        binding: PortalBinding,
    },
}

/// Get the wrapped `ThemedButton` trigger as a `WidgetMut`, regardless of
/// hosting mode — in-tree it's `AnchoredOverlay::primary`, in portal mode
/// it's the lone hosted child.
macro_rules! with_trigger {
    ($this:ident, |$trigger:ident| $body:block) => {
        match &mut $this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut overlay_host = $this.ctx.get_mut(overlay_host);
                let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
                let mut $trigger = primary.downcast::<ThemedButton>();
                $body
            }
            Hosting::Portal { trigger, .. } => {
                let mut trigger = $this.ctx.get_mut(trigger);
                let mut $trigger = trigger.downcast::<ThemedButton>();
                $body
            }
        }
    };
}

/// Button widget that opens a floating dropdown menu on click.
///
/// Wraps a [`ThemedButton`] (label + optional leading icon + trailing chevron)
/// as its trigger; toggles the menu in response to the trigger's `ButtonPress`.
pub struct ThemedDropdownButton {
    hosting: Hosting,
    handle: DropdownButtonHandle,
    label: ArcStr,
    icon: Option<IconName>,
    items: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    pub(super) open: bool,
    /// Keyboard-highlighted item index for roving-tab-stop navigation.
    /// `None` means no keyboard highlight; updated on arrow keys and cleared
    /// when the menu closes or an item is selected.
    highlighted: Option<usize>,
}

// --- MARK: BUILDERS
impl ThemedDropdownButton {
    fn build_trigger(
        label_text: &ArcStr,
        icon: Option<IconName>,
        variant: ButtonVariant,
        disabled: bool,
        theme: &Theme,
    ) -> NewWidget<dyn Widget> {
        let text_color = Self::text_color_for(theme, variant, disabled);
        let icon_color = Self::icon_color_for(theme, disabled);

        let label = Label::new(label_text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        let mut label = label.erased();
        label.properties.insert(ContentColor::new(text_color));

        let mut trigger = ThemedButton::new(label, theme)
            .with_variant(variant)
            .with_disabled(disabled)
            .with_trailing_icon(
                crate::components::icon::icon(IconName::ChevronDown)
                    .color(icon_color)
                    .build_widget(theme),
            );
        if let Some(name) = icon {
            trigger = trigger.with_icon(
                crate::components::icon::icon(name)
                    .color(icon_color)
                    .build_widget(theme),
            );
        }
        NewWidget::new(trigger).erased()
    }

    /// In-tree constructor (fallback, no scope ancestor): `MenuContent` is
    /// permanently mounted in an `AnchoredOverlay` below the trigger.
    #[must_use]
    pub fn new(
        label_text: ArcStr,
        icon: Option<IconName>,
        items: Vec<ArcStr>,
        variant: ButtonVariant,
        disabled: bool,
        theme: &Theme,
    ) -> Self {
        let trigger = Self::build_trigger(&label_text, icon, variant, disabled, theme);
        let overlay_host = AnchoredOverlay::new(
            trigger,
            NewWidget::new(MenuContent::new(items.clone(), theme)),
            false,
            OverlayAnchor::BottomStart,
        );
        Self {
            hosting: Hosting::InTree {
                overlay_host: NewWidget::new(overlay_host).to_pod(),
            },
            handle: DropdownButtonHandle::new(),
            label: label_text,
            icon,
            items,
            variant,
            disabled,
            theme: *theme,
            open: false,
            highlighted: None,
        }
    }

    /// Portal-mode constructor: the menu lives in the scope's slot under
    /// `key`, registered by the view layer as a `MenuContentView`; we host
    /// only the trigger. `handle` is filled at `Update::WidgetAdded` and
    /// given to the registered `MenuContentView` so it can notify us back.
    #[must_use]
    pub(crate) fn new_portal(
        config: DropdownButtonConfig,
        handle: DropdownButtonHandle,
        scope: OverlayScopeHandle,
        key: u64,
    ) -> Self {
        let DropdownButtonConfig {
            label_text,
            icon,
            items,
            variant,
            disabled,
            theme,
        } = config;
        let trigger = Self::build_trigger(&label_text, icon, variant, disabled, &theme);
        Self {
            hosting: Hosting::Portal {
                trigger: trigger.to_pod(),
                binding: PortalBinding::new(scope, key, dropdown_dismiss_hook),
            },
            handle,
            label: label_text,
            icon,
            items,
            variant,
            disabled,
            theme,
            open: false,
            highlighted: None,
        }
    }

    fn text_color_for(theme: &Theme, variant: ButtonVariant, disabled: bool) -> Color {
        if disabled {
            theme.palette.text_faint
        } else if variant == ButtonVariant::Link {
            theme.palette.teal
        } else {
            theme.palette.text
        }
    }

    fn icon_color_for(theme: &Theme, disabled: bool) -> Color {
        if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text
        }
    }

    /// Refreshes the leading/trailing icon labels' color and font size —
    /// mirrors `ButtonView::rebuild`'s icon-prop refresh
    /// (`components/button/view.rs`), since this widget builds those icon
    /// children directly rather than through the view layer.
    fn refresh_icon_props(trigger: &mut WidgetMut<'_, ThemedButton>, color: Color, theme: &Theme) {
        if let Some(mut m) = ThemedButton::icon_mut(trigger) {
            m.insert_prop(ContentColor::new(color));
            Label::insert_style(&mut m, StyleProperty::FontSize(theme.density.ui_font_size));
        }
        if let Some(mut m) = ThemedButton::trailing_icon_mut(trigger) {
            m.insert_prop(ContentColor::new(color));
            Label::insert_style(&mut m, StyleProperty::FontSize(theme.density.ui_font_size));
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl ThemedDropdownButton {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();

            let text_color = Self::text_color_for(theme, this.widget.variant, this.widget.disabled);
            let icon_color = Self::icon_color_for(theme, this.widget.disabled);
            with_trigger!(this, |trigger| {
                ThemedButton::set_theme(&mut trigger, theme);
                {
                    let mut child = ThemedButton::child_mut(&mut trigger);
                    child.insert_prop(ContentColor::new(text_color));
                    let mut child = child.downcast::<Label>();
                    Label::insert_style(
                        &mut child,
                        StyleProperty::FontSize(theme.density.ui_font_size),
                    );
                }
                Self::refresh_icon_props(&mut trigger, icon_color, theme);
            });

            if let Hosting::InTree { overlay_host } = &mut this.widget.hosting {
                let mut overlay_host = this.ctx.get_mut(overlay_host);
                let mut menu = AnchoredOverlay::overlay_mut(&mut overlay_host);
                let mut menu = menu.downcast::<MenuContent>();
                MenuContent::set_theme(&mut menu, theme);
            }
        }
    }

    pub fn set_label(this: &mut WidgetMut<'_, Self>, label: ArcStr) {
        if this.widget.label == label {
            return;
        }
        this.widget.label = label.clone();
        let theme = this.widget.theme;
        let text_color = Self::text_color_for(&theme, this.widget.variant, this.widget.disabled);
        with_trigger!(this, |trigger| {
            let mut child = ThemedButton::child_mut(&mut trigger);
            child.insert_prop(ContentColor::new(text_color));
            let mut child = child.downcast::<Label>();
            Label::insert_style(
                &mut child,
                StyleProperty::FontSize(theme.density.ui_font_size),
            );
            Label::set_text(&mut child, label);
        });
    }

    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();

            // Disabling mid-open must close the menu — a disabled trigger
            // can no longer be clicked to dismiss it, and a stale open menu
            // would stay interactable (selections could still fire). Mirrors
            // `PopoverHost`'s `Update::StashedChanged(true)` path.
            if disabled && this.widget.open {
                this.widget.open = false;
                this.widget.highlighted = None;
                this.widget.close_menu(&mut this.ctx);
            }

            let theme = this.widget.theme;
            let text_color = Self::text_color_for(&theme, this.widget.variant, disabled);
            let icon_color = Self::icon_color_for(&theme, disabled);
            with_trigger!(this, |trigger| {
                ThemedButton::set_disabled(&mut trigger, disabled);
                {
                    let mut child = ThemedButton::child_mut(&mut trigger);
                    child.insert_prop(ContentColor::new(text_color));
                }
                Self::refresh_icon_props(&mut trigger, icon_color, &theme);
            });
        }
    }

    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: ButtonVariant) {
        let prev_variant = this.widget.variant;
        if prev_variant != variant {
            this.widget.variant = variant;
            this.ctx.request_paint_only();

            let theme = this.widget.theme;
            let disabled = this.widget.disabled;
            with_trigger!(this, |trigger| {
                ThemedButton::set_variant(&mut trigger, variant);
                // Link gains/loses teal text; other variants share the default color.
                if !disabled
                    && (variant == ButtonVariant::Link || prev_variant == ButtonVariant::Link)
                {
                    let text_color = Self::text_color_for(&theme, variant, disabled);
                    let mut child = ThemedButton::child_mut(&mut trigger);
                    child.insert_prop(ContentColor::new(text_color));
                }
            });
        }
    }

    /// Replace the item list. In-tree this updates the permanently-mounted
    /// `MenuContent` directly; in portal mode the registered
    /// `MenuContentView` rebuilds its own `MenuContent` via the scope's
    /// registry diff, so only our copy of `items` (used for Home/End bounds
    /// checks) is updated here.
    pub fn set_items(this: &mut WidgetMut<'_, Self>, items: Vec<ArcStr>) {
        this.widget.items.clone_from(&items);
        if let Hosting::InTree { overlay_host } = &mut this.widget.hosting {
            let mut overlay_host = this.ctx.get_mut(overlay_host);
            let mut menu = AnchoredOverlay::overlay_mut(&mut overlay_host);
            let mut menu = menu.downcast::<MenuContent>();
            MenuContent::set_items(&mut menu, items);
        }
    }

    pub fn set_icon(this: &mut WidgetMut<'_, Self>, icon: Option<IconName>) {
        this.widget.icon = icon;
        let theme = this.widget.theme;
        let icon_color = Self::icon_color_for(&theme, this.widget.disabled);
        with_trigger!(this, |trigger| {
            match icon {
                Some(name) => ThemedButton::attach_icon(
                    &mut trigger,
                    crate::components::icon::icon(name)
                        .color(icon_color)
                        .build_widget(&theme),
                ),
                None => ThemedButton::detach_icon(&mut trigger),
            }
        });
    }

    /// Sync `open`/`highlighted` after the menu closed without going through
    /// our own event handlers — either the portal slot dismissed it (outside
    /// press; `PortalSlot::dismiss_outside`) or a portal-mounted
    /// `MenuContentView` handled an item selection directly (see
    /// `super::view::MenuContentView::message`). Both call this via
    /// `mutate_later(handle)`. Also pushes the closed state to the slot —
    /// idempotent if it's already hidden.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if this.widget.open {
            this.widget.open = false;
            this.widget.highlighted = None;
            this.widget.close_menu(&mut this.ctx);
            this.ctx.request_paint_only();
        }
    }
}

/// Dismiss hook registered with the portal slot (see
/// [`crate::overlay_portal::DismissHook`]): syncs `open`/`highlighted`
/// after an outside-press dismissal via [`ThemedDropdownButton::mark_closed`].
pub(crate) fn dropdown_dismiss_hook(mut w: WidgetMut<'_, dyn Widget>) {
    let mut dropdown = w.downcast::<ThemedDropdownButton>();
    ThemedDropdownButton::mark_closed(&mut dropdown);
}

// --- MARK: INTERNAL HELPERS
impl ThemedDropdownButton {
    /// Close the menu in whichever host mounts it. Shared by every close path
    /// (item selection, Escape, outside focus loss, disabled mid-open, stash);
    /// generic over the context — see [`PortalCtx`].
    fn close_menu(&mut self, ctx: &mut impl PortalCtx) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.queue_mutate(overlay_host.id(), |mut w| {
                    let mut overlay = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut overlay, false);
                });
            }
            Hosting::Portal { binding, .. } => binding.close(ctx),
        }
    }

    /// Open the menu in whichever host mounts it (trigger toggle only).
    fn open_menu(&mut self, ctx: &mut impl PortalOpenCtx) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.queue_mutate(overlay_host.id(), |mut w| {
                    let mut overlay = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut overlay, true);
                });
            }
            Hosting::Portal { binding, .. } => {
                binding.open(ctx, OverlayAnchor::BottomStart, 0.0);
            }
        }
    }

    /// Push `index` into the `MenuContent` widget for painting, then store it.
    fn set_highlight(&mut self, ctx: &mut EventCtx<'_>, index: Option<usize>) {
        self.highlighted = index;
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    let mut menu = AnchoredOverlay::overlay_mut(&mut w);
                    let mut menu = menu.downcast::<MenuContent>();
                    MenuContent::set_highlighted(&mut menu, index);
                });
            }
            Hosting::Portal { binding, .. } => {
                let Some(scope_id) = binding.scope_widget_id() else {
                    return;
                };
                let key = binding.key();
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                    if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                        let mut pass = child.downcast::<Passthrough>();
                        let mut menu = Passthrough::child_mut(&mut pass);
                        let mut menu = menu.downcast::<MenuContent>();
                        MenuContent::set_highlighted(&mut menu, index);
                    }
                });
            }
        }
    }

    /// Move the keyboard highlight by `delta` positions (wrapping).
    ///
    /// Called from `on_text_event` in response to arrow keys.
    fn move_highlight(&mut self, ctx: &mut EventCtx<'_>, delta: isize) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        let next = match self.highlighted {
            None => {
                if delta >= 0 {
                    0usize
                } else {
                    n - 1
                }
            }
            Some(i) => (i.cast_signed() + delta)
                .rem_euclid(n.cast_signed())
                .cast_unsigned(),
        };
        self.set_highlight(ctx, Some(next));
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedDropdownButton {
    type Action = DropdownButtonAction;

    /// Routes actions bubbling up from our children: a `ButtonPress` from the
    /// wrapped `ThemedButton` trigger (any click on the button surface, or
    /// Space/Enter while it's focused) toggles the menu, or — if the menu is
    /// open with a keyboard highlight — selects that item directly (Enter
    /// goes to the focused trigger, not the menu, regardless of hosting
    /// mode). A `MenuItemSelected` from `MenuContent` only bubbles here in
    /// in-tree mode (the menu is our descendant); in portal mode it's handled
    /// by `MenuContentView::message` instead.
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        if let Some(press) = action.downcast_ref::<ButtonPress>() {
            if !self.disabled {
                if self.open
                    && press.button.is_none()
                    && let Some(index) = self.highlighted
                {
                    self.open = false;
                    self.highlighted = None;
                    self.close_menu(ctx);
                    ctx.submit_action::<Self::Action>(DropdownButtonAction::ItemSelected(index));
                    ctx.set_handled();
                    ctx.request_paint_only();
                    return;
                }
                if self.open {
                    self.open = false;
                    self.highlighted = None;
                    self.close_menu(ctx);
                } else {
                    self.open = true;
                    self.open_menu(ctx);
                }
            }
            ctx.set_handled();
            ctx.request_paint_only();
            return;
        }
        if let Some(&MenuItemSelected(index)) = action.downcast_ref::<MenuItemSelected>() {
            self.open = false;
            self.highlighted = None;
            self.close_menu(ctx);
            ctx.submit_action::<Self::Action>(DropdownButtonAction::ItemSelected(index));
            ctx.set_handled();
            ctx.request_paint_only();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if !self.open {
            return;
        }
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        match &key.key {
            Key::Named(NamedKey::ArrowDown) => {
                self.move_highlight(ctx, 1);
                ctx.set_handled();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.move_highlight(ctx, -1);
                ctx.set_handled();
            }
            Key::Named(NamedKey::Home) if !self.items.is_empty() => {
                self.set_highlight(ctx, Some(0));
                ctx.set_handled();
            }
            Key::Named(NamedKey::End) if !self.items.is_empty() => {
                self.set_highlight(ctx, Some(self.items.len() - 1));
                ctx.set_handled();
            }
            Key::Named(NamedKey::Escape) => {
                // Close without selecting; returns focus to the trigger button
                // naturally (it was always focused).
                self.open = false;
                self.highlighted = None;
                self.close_menu(ctx);
                ctx.set_handled();
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
                self.handle.set(ctx.widget_id());
            }
            // The wrapped `ThemedButton` is the actual focus target now, so we
            // react to `ChildFocusChanged` — masonry's "focus entered/left my
            // subtree" signal for ancestors — rather than `FocusChanged`. Only
            // meaningful in-tree: there the menu is a *descendant* (clicks on
            // the trigger or inside the menu keep our subtree focused), so
            // this only fires for genuine outside clicks. In portal mode the
            // menu lives in the scope's slot, not under us — the slot's own
            // outside-press dismissal handles it instead (see
            // `PortalSlot::dismiss_outside`).
            Update::ChildFocusChanged(false)
                if self.open && matches!(self.hosting, Hosting::InTree { .. }) =>
            {
                self.open = false;
                self.highlighted = None;
                self.close_menu(ctx);
                ctx.request_paint_only();
            }
            // A trigger stashed mid-open (e.g. a tab/panel container hiding us
            // without tearing us down) can no longer be clicked to dismiss its
            // menu, and the menu would stay visible/painted once the host is
            // unstashed even though `self.open` is now false. Close eagerly —
            // mirrors `PopoverHost`'s `Update::StashedChanged(true)` path —
            // for both hosting modes.
            Update::StashedChanged(true) if self.open => {
                self.open = false;
                self.highlighted = None;
                self.close_menu(ctx);
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    /// Re-anchors a still-open portal-mode menu as we move in window space —
    /// e.g. while the user scrolls a `ScrollContainer` containing us. The menu
    /// lives in a structurally separate subtree (the scope's portal slot), so
    /// — unlike `AnchoredOverlay`, which tracks for free as a rigidly-attached
    /// descendant — nothing re-places it automatically. Mirrors
    /// `PopoverHost::compose`; no-op in-tree.
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        if !self.open {
            return;
        }
        if let Hosting::Portal { binding, .. } = &mut self.hosting {
            binding.reanchor(ctx);
        }
    }

    /// Keeps a still-open portal-mode menu's [`Self::compose`] running every
    /// frame so it re-anchors regardless of pointer position or which
    /// ancestor scrolled. Mirrors `PopoverHost::on_anim_frame`; no-op in-tree.
    fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _: u64) {
        if !self.open || !matches!(self.hosting, Hosting::Portal { .. }) {
            return;
        }
        ctx.request_compose();
        ctx.request_anim_frame();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => ctx.register_child(overlay_host),
            Hosting::Portal { trigger, .. } => ctx.register_child(trigger),
        }
    }

    /// Pure transparent forward to whichever child hosts the trigger —
    /// `AnchoredOverlay` (in-tree) already sizes itself to its `primary`, and
    /// in portal mode the trigger is our only child. Mirrors `pointer_inert`'s
    /// single-child passthrough.
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.redirect_measurement(overlay_host, axis, cross_length)
            }
            Hosting::Portal { trigger, .. } => {
                ctx.redirect_measurement(trigger, axis, cross_length)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.run_layout(overlay_host, size);
                ctx.place_child(overlay_host, Point::ORIGIN);
                ctx.derive_baselines(overlay_host);
            }
            Hosting::Portal { trigger, .. } => {
                ctx.run_layout(trigger, size);
                ctx.place_child(trigger, Point::ORIGIN);
                ctx.derive_baselines(trigger);
            }
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — whichever child hosts the trigger paints itself.
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &masonry::core::PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if !self.disabled {
            node.add_action(masonry::accesskit::Action::Click);
        }
        node.add_action(masonry::accesskit::Action::Expand);
    }

    fn children_ids(&self) -> ChildrenIds {
        match &self.hosting {
            Hosting::InTree { overlay_host } => ChildrenIds::from_slice(&[overlay_host.id()]),
            Hosting::Portal { trigger, .. } => ChildrenIds::from_slice(&[trigger.id()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_always_wins_regardless_of_variant() {
        let theme = Theme::default();
        for variant in [
            ButtonVariant::Default,
            ButtonVariant::Link,
            ButtonVariant::Primary,
        ] {
            assert_eq!(
                ThemedDropdownButton::text_color_for(&theme, variant, true),
                theme.palette.text_faint,
                "{variant:?} should still read as faint while disabled"
            );
        }
        assert_eq!(
            ThemedDropdownButton::icon_color_for(&theme, true),
            theme.palette.text_faint
        );
    }

    #[test]
    fn link_variant_reads_in_the_accent_color_when_enabled() {
        let theme = Theme::default();
        assert_eq!(
            ThemedDropdownButton::text_color_for(&theme, ButtonVariant::Link, false),
            theme.palette.teal
        );
    }

    #[test]
    fn non_link_variants_read_as_plain_text_when_enabled() {
        let theme = Theme::default();
        for variant in [
            ButtonVariant::Default,
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
        ] {
            assert_eq!(
                ThemedDropdownButton::text_color_for(&theme, variant, false),
                theme.palette.text,
                "{variant:?} should use the plain text color"
            );
        }
    }

    #[test]
    fn icon_color_ignores_variant_and_only_responds_to_disabled() {
        let theme = Theme::default();
        assert_eq!(
            ThemedDropdownButton::icon_color_for(&theme, false),
            theme.palette.text
        );
        assert_eq!(
            ThemedDropdownButton::icon_color_for(&theme, true),
            theme.palette.text_faint
        );
    }
}
