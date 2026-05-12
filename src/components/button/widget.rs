//! Masonry widget that owns the Tessera `.tb-btn` state machine.
//!
//! The widget tracks pointer hover, press, and focus through masonry's
//! widget context, then paints background + border itself from a `Theme`
//! it holds. Going through the property-stack system would require
//! reaching into `PropertyArena` from a xilem `View`, which is not
//! exposed today; painting directly keeps `Theme` as a value flowing
//! through `app_logic` rather than global state.
//!
//! Emits [`ButtonPress`] (the same action type as `masonry::widgets::Button`)
//! on primary-pointer release inside the widget and on Space/Enter while
//! focused.

use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, SizeDef};
use masonry::peniko::Color;
use masonry::widgets::ButtonPress;

use crate::Theme;

/// Tessera `.tb-btn` corner radius (`border-radius: 5px`).
const CORNER_RADIUS: f64 = 5.0;
/// Border thickness when the active selector applies.
const BORDER_WIDTH: f64 = 1.0;

/// Themed, interactive button widget.
///
/// Owns its child (typically a `Label`) and a [`Theme`] value used to
/// resolve background / border / text colors at paint time. The host
/// drives the `active` flag for "currently-selected toggle" semantics
/// (which density step is picked, which panel is open). Pointer state
/// (hovered, pressed) is read from the widget context — masonry tracks
/// it for us.
pub struct ThemedButton {
    child: WidgetPod<dyn Widget>,
    theme: Theme,
    /// Host-controlled toggle. Tessera's `.tb-btn.active`.
    active: bool,
}

// --- MARK: BUILDERS
impl ThemedButton {
    /// Creates a new themed button with the supplied child and theme.
    ///
    /// The child should be a non-interactive widget — typically a
    /// `Label`. An interactive child would steal pointer capture from
    /// the button.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            child: child.erased().to_pod(),
            theme: *theme,
            active: false,
        }
    }

    /// Marks the button as the currently-selected toggle.
    #[must_use]
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

// --- MARK: WIDGETMUT
impl ThemedButton {
    /// Replaces the theme. Requests layout + repaint if the value
    /// changed — density-driven padding shifts the button's size.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Toggles the host-driven `active` flag. Requests a repaint on
    /// change.
    pub fn set_active(this: &mut WidgetMut<'_, Self>, active: bool) {
        if this.widget.active != active {
            this.widget.active = active;
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
    /// Resolves `(background, border)` colors for the current state.
    ///
    /// Tessera's source defines three rules; we add a "pressed" tier
    /// between hover and active so a real click registers visually:
    ///
    /// | state                       | background     | border |
    /// |-----------------------------|----------------|--------|
    /// | default                     | transparent    | none   |
    /// | hover                       | `surface_2`    | none   |
    /// | pressed (pointer-down)      | `surface_hi`   | none   |
    /// | active (host toggle)        | `surface_2`    | `border` |
    /// | active + pressed            | `surface_hi`   | `border` |
    fn resolve_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
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
        // Only react to the primary button — right/middle clicks
        // should not capture pointer or fire `ButtonPress`, matching
        // the docstring promise of "primary-pointer release."
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                // Pointer capture flips `is_active`; repaint for the
                // pressed visual.
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: button @ Some(PointerButton::Primary),
                ..
            }) => {
                // Only fire on release *over* the widget — matches the
                // standard masonry button.
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
        // Forward to the child with the cross-axis padding peeled off,
        // then add the main-axis padding back.
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let (main_pad, cross_pad) = match axis {
            Axis::Horizontal => (2.0 * pad_h, 2.0 * pad_v),
            Axis::Vertical => (2.0 * pad_v, 2.0 * pad_h),
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
        child_length + main_pad
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let inner = Size::new(
            (size.width - 2.0 * pad_h).max(0.0),
            (size.height - 2.0 * pad_v).max(0.0),
        );
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(inner), inner.into());
        ctx.run_layout(&mut self.child, child_size);

        // Center child within the padded box.
        let child_origin = Point::new(
            pad_h + ((inner.width - child_size.width) * 0.5).max(0.0),
            pad_v + ((inner.height - child_size.height) * 0.5).max(0.0),
        );
        ctx.place_child(&mut self.child, child_origin);
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
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
