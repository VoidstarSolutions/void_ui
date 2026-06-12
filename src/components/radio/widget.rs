//! Masonry widget for the themed radio button.
//!
//! Paints a circle ring with an inner dot when selected. Layout is
//! `[circle] [gap] [label]` — the host controls the `active` flag to indicate
//! which option in a group is currently selected.
//!
//! Emits [`ButtonPress`] on primary-pointer release and on Space while focused.

use masonry::accesskit;
use masonry::accesskit::{Node, Role, Toggled};
use masonry::core::keyboard::Key;
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Circle, Point, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::widgets::ButtonPress;

use crate::Theme;
use crate::focus_ring::paint_focus_ring;

/// Diameter of the radio circle in logical pixels.
const RADIO_DIAMETER: f64 = 14.0;
/// Gap between the circle and the label.
const RADIO_GAP: f64 = 6.0;
/// Circle border stroke width.
const BORDER_WIDTH: f64 = 1.5;
/// Radius of the inner selection dot (drawn when `active`).
const DOT_RADIUS: f64 = 3.5;
/// Focus-ring outset from the circle edge.
const FOCUS_RING_OUTSET: f64 = 2.0;

/// Themed radio button widget.
///
/// Owns its label child and a [`Theme`]. The host drives `active` to indicate
/// the selected option; pointer and focus state are read from the widget context.
/// Group mutual-exclusion is host-managed: each radio in a group gets
/// `active(selected_value == my_value)` and fires a callback that updates the
/// selection in app state.
pub struct ThemedRadio {
    child: WidgetPod<dyn Widget>,
    theme: Theme,
    /// Host-controlled selected state.
    active: bool,
    /// When true, all interaction is suppressed and colors are muted.
    disabled: bool,
}

// --- MARK: BUILDERS
impl ThemedRadio {
    /// Creates a new themed radio button with the supplied child and theme.
    ///
    /// The child should be a non-interactive widget — typically a `Label`.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            child: child.erased().to_pod(),
            theme: *theme,
            active: false,
            disabled: false,
        }
    }

    /// Marks this radio as the currently-selected option.
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
}

// --- MARK: WIDGETMUT
impl ThemedRadio {
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

    /// Sets the disabled state. Propagates to masonry's system-level disabled
    /// flag and requests a repaint.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the label child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: PAINT STATE
impl ThemedRadio {
    /// Resolves `(ring_color, dot_color)` for the current interaction state.
    ///
    /// `dot_color` is only meaningful when `active == true`; callers should
    /// skip painting the dot otherwise.
    fn resolve_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (p.text_faint, p.text_faint);
        }
        let ring = if self.active || pressed {
            p.teal
        } else if hovered {
            p.border_strong
        } else {
            p.border
        };
        let dot = p.teal;
        (ring, dot)
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedRadio {
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
            && matches!(&event.key, Key::Character(c) if c == " ")
        {
            ctx.set_handled();
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
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
            }
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
        cross_length: Option<Length>,
    ) -> Length {
        // The circle occupies [RADIO_DIAMETER + RADIO_GAP] of horizontal space.
        // On the vertical axis it doesn't stack, so contributes 0 length but
        // reduces the horizontal cross-space available to the label.
        let circle_h = Length::px(RADIO_DIAMETER + RADIO_GAP);
        let circle_v = Length::px(0.0);

        let extra_main = match axis {
            Axis::Horizontal => circle_h,
            Axis::Vertical => circle_v,
        };
        let extra_cross = match axis {
            Axis::Horizontal => circle_v,
            Axis::Vertical => circle_h,
        };

        let cross_space = cross_length.map(|c| Length::px((c.get() - extra_cross.get()).max(0.0)));
        let auto_length = len_req.reduce(extra_main).into();
        let context_size = LayoutSize::maybe(axis.cross(), cross_space);

        let label_len = ctx.compute_length(
            &mut self.child,
            auto_length,
            context_size,
            axis,
            cross_space,
        );

        match axis {
            Axis::Horizontal => Length::px(label_len.get() + circle_h.get()),
            Axis::Vertical => Length::px(label_len.get().max(RADIO_DIAMETER) + circle_v.get()),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let label_x = RADIO_DIAMETER + RADIO_GAP;
        let label_space = Size::new((size.width - label_x).max(0.0), size.height);
        let label_size = ctx.compute_size(
            &mut self.child,
            SizeDef::fit(label_space),
            label_space.into(),
        );
        ctx.run_layout(&mut self.child, label_size);

        let label_y = ((size.height - label_size.height) * 0.5).max(0.0);
        ctx.place_child(&mut self.child, Point::new(label_x, label_y));
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

        let (ring_color, dot_color) = self.resolve_colors(hovered, pressed);

        let cx = RADIO_DIAMETER * 0.5;
        let cy = size.height * 0.5;
        let center = Point::new(cx, cy);

        let ring_radius = (RADIO_DIAMETER - BORDER_WIDTH) * 0.5;
        painter
            .stroke(
                Circle::new(center, ring_radius),
                &Stroke::new(BORDER_WIDTH),
                ring_color,
            )
            .draw();

        if self.active {
            painter
                .fill(Circle::new(center, DOT_RADIUS), dot_color)
                .draw();
        }

        if focused && !self.disabled {
            let focus_radius = (RADIO_DIAMETER * 0.5) + FOCUS_RING_OUTSET;
            paint_focus_ring(painter, Circle::new(center, focus_radius), &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::RadioButton
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if !self.disabled {
            node.add_action(accesskit::Action::Click);
        }
        node.set_toggled(if self.active {
            Toggled::True
        } else {
            Toggled::False
        });
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
