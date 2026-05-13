//! Masonry widget that owns the Tessera `.tb-btn` state machine.
//!
//! The widget tracks pointer hover, press, and focus through masonry's widget
//! context, then paints background + border itself from a `Theme` it holds.
//! Going through the property-stack system would require reaching into
//! `PropertyArena` from a xilem `View`, which is not exposed today; painting
//! directly keeps `Theme` as a value flowing through `app_logic` rather than
//! global state.
//!
//! Emits [`ButtonPress`] (the same action type as `masonry::widgets::Button`)
//! on primary-pointer release inside the widget and on Space/Enter while
//! focused.

use std::sync::Arc;

use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, BezPath, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, SizeDef};
use masonry::peniko::Color;
use masonry::widgets::ButtonPress;

use super::ButtonVariant;
use crate::Theme;

/// Corner radius (`border-radius: 5px`).
const CORNER_RADIUS: f64 = 5.0;
/// Border thickness for the active and focus states.
const BORDER_WIDTH: f64 = 1.0;
/// Focus-ring stroke width (inset 2px from button edge).
const FOCUS_RING_WIDTH: f64 = 1.5;
/// Inset of the focus ring from the button edge.
const FOCUS_RING_INSET: f64 = 2.0;
/// Gap between a leading icon and the label.
const ICON_GAP: f64 = 5.0;

/// Themed, interactive button widget.
///
/// Owns its child (typically a `Label`) and a [`Theme`] value used to
/// resolve background / border / text colors at paint time. The host drives
/// the `active` flag for "currently-selected toggle" semantics. Pointer state
/// (hovered, pressed) is read from the widget context.
pub struct ThemedButton {
    child: WidgetPod<dyn Widget>,
    theme: Theme,
    /// Host-controlled toggle — Tessera's `.tb-btn.active`.
    active: bool,
    /// When true, all interaction is suppressed and colors are muted.
    disabled: bool,
    /// Visual style variant.
    variant: ButtonVariant,
    /// Optional leading icon: a unit-square `BezPath` (0..1 coordinate
    /// space) scaled to the theme's UI font size at paint time.
    icon: Option<Arc<BezPath>>,
}

// --- MARK: BUILDERS
impl ThemedButton {
    /// Creates a new themed button with the supplied child and theme.
    ///
    /// The child should be a non-interactive widget — typically a `Label`. An
    /// interactive child would steal pointer capture from the button.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            child: child.erased().to_pod(),
            theme: *theme,
            active: false,
            disabled: false,
            variant: ButtonVariant::Default,
            icon: None,
        }
    }

    /// Marks the button as the currently-selected toggle.
    #[must_use]
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Suppresses all interaction and mutes visual appearance.
    #[must_use]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Sets the visual style variant.
    #[must_use]
    pub fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Attaches a leading icon.
    #[must_use]
    pub fn with_icon(mut self, icon: Option<Arc<BezPath>>) -> Self {
        self.icon = icon;
        self
    }
}

// --- MARK: WIDGETMUT
impl ThemedButton {
    /// Replaces the theme. Requests layout + repaint if the value changed.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Toggles the host-driven `active` flag. Requests a repaint on change.
    pub fn set_active(this: &mut WidgetMut<'_, Self>, active: bool) {
        if this.widget.active != active {
            this.widget.active = active;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the disabled state. Propagates to masonry's system-level
    /// disabled flag (for event routing + accessibility) and requests a repaint.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the visual variant. Requests a repaint on change.
    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: ButtonVariant) {
        if this.widget.variant != variant {
            this.widget.variant = variant;
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the leading icon. Compares by `Arc` pointer; requests
    /// layout + repaint only when the icon actually changes.
    pub fn set_icon(this: &mut WidgetMut<'_, Self>, icon: Option<Arc<BezPath>>) {
        let changed = match (&this.widget.icon, &icon) {
            (None, None) => false,
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        };
        if changed {
            this.widget.icon = icon;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: PAINT STATE
impl ThemedButton {
    fn icon_size(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
    }

    /// Resolves `(background, border)` colors for the current state.
    ///
    /// | state              | Default bg   | Danger bg    | border         |
    /// |--------------------|--------------|--------------|----------------|
    /// | disabled           | transparent  | transparent  | none           |
    /// | default            | transparent  | transparent  | none           |
    /// | hover              | `surface_2`  | `coral_soft` | none           |
    /// | pressed            | `surface_hi` | `coral`      | none           |
    /// | active (toggle)    | `surface_2`  | `coral_soft` | border / coral |
    /// | active + pressed   | `surface_hi` | `coral`      | border / coral |
    fn resolve_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (Color::TRANSPARENT, Color::TRANSPARENT);
        }
        match self.variant {
            ButtonVariant::Default => {
                let bg = if pressed {
                    p.surface_hi
                } else if self.active || hovered {
                    p.surface_2
                } else {
                    Color::TRANSPARENT
                };
                let border = if self.active {
                    p.border
                } else {
                    Color::TRANSPARENT
                };
                (bg, border)
            }
            ButtonVariant::Danger => {
                let bg = if pressed {
                    p.coral
                } else if self.active || hovered {
                    p.coral_soft
                } else {
                    Color::TRANSPARENT
                };
                let border = if self.active {
                    p.coral
                } else {
                    Color::TRANSPARENT
                };
                (bg, border)
            }
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedButton {
    type Action = ButtonPress;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled {
            return;
        }
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: button @ Some(PointerButton::Primary),
                ..
            }) => {
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<Self::Action>(ButtonPress { button: *button });
                }
                ctx.request_paint_only();
            }
            _ => (),
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.disabled {
            return;
        }
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if self.disabled {
            return;
        }
        if event.action == accesskit::Action::Click {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(_) | Update::DisabledChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<f64>,
    ) -> f64 {
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let (main_pad, cross_pad) = match axis {
            Axis::Horizontal => (2.0 * pad_h, 2.0 * pad_v),
            Axis::Vertical => (2.0 * pad_v, 2.0 * pad_h),
        };
        let icon_extra = if self.icon.is_some() && axis == Axis::Horizontal {
            self.icon_size() + ICON_GAP
        } else {
            0.0
        };
        let inner_cross = cross_length.map(|c| (c - cross_pad).max(0.0));
        let auto_length = len_req.into();
        let context_size = LayoutSize::maybe(axis.cross(), inner_cross);
        let child_length = ctx.compute_length(
            &mut self.child,
            auto_length,
            context_size,
            axis,
            inner_cross,
        );
        child_length + main_pad + icon_extra
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let icon_extra = if self.icon.is_some() {
            self.icon_size() + ICON_GAP
        } else {
            0.0
        };
        let inner = Size::new(
            (size.width - 2.0 * pad_h - icon_extra).max(0.0),
            (size.height - 2.0 * pad_v).max(0.0),
        );
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(inner), inner.into());
        ctx.run_layout(&mut self.child, child_size);

        // Label starts immediately after the icon area; no horizontal
        // centering within the remaining space keeps icon+text as a visual unit.
        let child_x = pad_h + icon_extra;
        let child_y = pad_v + ((inner.height - child_size.height) * 0.5).max(0.0);
        ctx.place_child(&mut self.child, Point::new(child_x, child_y));
        ctx.derive_baselines(&self.child);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let hovered = ctx.is_hovered();
        let pressed = ctx.is_active() && hovered;
        let focused = ctx.is_focus_target();
        let p = &self.theme.palette;
        let (bg, border) = self.resolve_colors(hovered, pressed);

        let rect = RoundedRect::from_origin_size(Point::ORIGIN, size, CORNER_RADIUS);
        if bg.components[3] > 0.0 {
            painter.fill(rect, bg).draw();
        }
        if border.components[3] > 0.0 {
            painter
                .stroke(rect, &Stroke::new(BORDER_WIDTH), border)
                .draw();
        }

        if focused && !self.disabled {
            let inset = FOCUS_RING_INSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                (CORNER_RADIUS - inset).max(0.0),
            );
            painter
                .stroke(focus_rect, &Stroke::new(FOCUS_RING_WIDTH), p.teal)
                .draw();
        }

        if let Some(icon) = &self.icon {
            let icon_size = self.icon_size();
            let icon_y = (size.height - icon_size) * 0.5;
            let icon_color = if self.disabled { p.text_faint } else { p.text };
            let pad_h = f64::from(self.theme.density.button_pad_h);
            let transform = Affine::translate((pad_h, icon_y)) * Affine::scale(icon_size);
            painter.fill(transform * icon.as_ref(), icon_color).draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.add_action(accesskit::Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        !self.disabled
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
