//! Masonry widget owning the slider's paint and pointer/keyboard interaction.
//!
//! Paints a track, an active-range fill, and a draggable circular thumb
//! directly from a [`Theme`] value. `value`, `min`, `max`, and `step` are
//! host-controlled — the widget never mutates them itself, it only emits
//! [`SliderChanged`] for the host to apply via [`set_value`](Self::set_value).
//!
//! Emits [`SliderChanged`] continuously while the thumb is dragged, on
//! click-to-jump within the track, on Left/Right/Up/Down/Home/End while
//! focused, and on an accessibility `SetValue`/`Increment`/`Decrement`.

use masonry::accesskit::{self, ActionData, Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Circle, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

use super::SliderChanged;
use crate::Theme;

/// Diameter of the draggable thumb circle, in logical pixels.
const THUMB_DIAMETER: f64 = 14.0;
/// Thickness of the track and fill bar.
const TRACK_HEIGHT: f64 = 4.0;
/// Focus-ring stroke width.
const FOCUS_RING_WIDTH: f64 = 1.5;
/// Gap between the thumb edge and the focus ring.
const FOCUS_RING_OUTSET: f64 = 2.0;
/// Clearance from the widget edge to the thumb's travel limits — keeps the
/// thumb and its focus ring from being clipped at the ends of the track.
const EDGE_PAD: f64 = FOCUS_RING_OUTSET + FOCUS_RING_WIDTH;

/// Interactive horizontal slider widget.
///
/// `value`, `min`, `max`, and `step` mirror host state; the host drives them
/// via the `set_*` associated functions in response to [`SliderChanged`].
pub struct SliderWidget {
    value: f64,
    min: f64,
    max: f64,
    /// Snap increment. `0.0` (or negative) means continuous — no snapping.
    step: f64,
    disabled: bool,
    theme: Theme,
    /// Tracks an in-progress primary-pointer drag gesture.
    dragging: bool,
    /// Last value emitted during the current gesture — deduplicates
    /// [`SliderChanged`] while the host's `value` round-trips back.
    last_emitted: Option<f64>,
}

// --- MARK: BUILDERS
impl SliderWidget {
    /// Creates a new slider in the given state.
    #[must_use]
    pub fn new(theme: &Theme, value: f64, min: f64, max: f64, step: f64, disabled: bool) -> Self {
        Self {
            value,
            min,
            max,
            step,
            disabled,
            theme: *theme,
            dragging: false,
            last_emitted: None,
        }
    }
}

// --- MARK: WIDGETMUT
impl SliderWidget {
    /// Sets the current value. Requests a repaint and accessibility update on change.
    pub fn set_value(this: &mut WidgetMut<'_, Self>, value: f64) {
        if (this.widget.value - value).abs() > f64::EPSILON {
            this.widget.value = value;
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets the value range. Requests a repaint and accessibility update on change.
    pub fn set_range(this: &mut WidgetMut<'_, Self>, min: f64, max: f64) {
        if (this.widget.min - min).abs() > f64::EPSILON || (this.widget.max - max).abs() > f64::EPSILON {
            this.widget.min = min;
            this.widget.max = max;
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets the snap increment (`0.0` means continuous).
    pub fn set_step(this: &mut WidgetMut<'_, Self>, step: f64) {
        if (this.widget.step - step).abs() > f64::EPSILON {
            this.widget.step = step;
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets the disabled state. Syncs with masonry's event-routing flag.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the theme. Requests a repaint on change.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: VALUE MATH
impl SliderWidget {
    /// Normalized thumb position in `[0.0, 1.0]`.
    fn progress(&self) -> f64 {
        let span = self.max - self.min;
        if span <= 0.0 {
            0.0
        } else {
            ((self.value - self.min) / span).clamp(0.0, 1.0)
        }
    }

    /// Snaps `raw` to the configured step (if any) and clamps to `[min, max]`.
    fn snap(&self, raw: f64) -> f64 {
        let snapped = if self.step > 0.0 {
            self.min + ((raw - self.min) / self.step).round() * self.step
        } else {
            raw
        };
        snapped.clamp(self.min, self.max)
    }

    /// The keyboard/accessibility step increment — the configured `step`,
    /// or one hundredth of the range when continuous.
    fn nudge(&self) -> f64 {
        if self.step > 0.0 {
            self.step
        } else {
            (self.max - self.min) / 100.0
        }
    }

    /// Range of the thumb center's horizontal travel within `size`, as
    /// `(start_x, usable_width)`.
    fn travel(size: Size) -> (f64, f64) {
        let thumb_radius = THUMB_DIAMETER / 2.0;
        let start = thumb_radius + EDGE_PAD;
        let end = (size.width - thumb_radius - EDGE_PAD).max(start);
        (start, end - start)
    }

    /// Converts a local-space pointer position to a snapped value.
    fn value_from_position(&self, size: Size, pos: Point) -> f64 {
        let (start, usable) = Self::travel(size);
        let t = ((pos.x - start) / usable.max(1e-6)).clamp(0.0, 1.0);
        self.snap(self.min + t * (self.max - self.min))
    }

    /// Emits [`SliderChanged`] if `new_value` differs from the last value
    /// emitted during this gesture (or from the host value, outside a gesture).
    fn emit_if_changed(&mut self, ctx: &mut EventCtx<'_>, new_value: f64) {
        let baseline = self.last_emitted.unwrap_or(self.value);
        if (new_value - baseline).abs() > f64::EPSILON {
            self.last_emitted = Some(new_value);
            ctx.submit_action::<<Self as Widget>::Action>(SliderChanged(new_value));
        }
    }
}

// --- MARK: PAINT STATE
impl SliderWidget {
    /// Resolves `(track, fill, thumb)` colors for the current interaction state.
    fn resolve_colors(&self, hovered: bool, active: bool) -> (Color, Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (p.surface_2, p.text_faint, p.text_faint);
        }
        let thumb = if active || hovered {
            p.teal_deep
        } else {
            p.teal
        };
        (p.surface_2, p.teal, thumb)
    }
}

// --- MARK: IMPL WIDGET
impl Widget for SliderWidget {
    type Action = SliderChanged;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled {
            return;
        }
        let size = ctx.border_box_size();
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                self.dragging = true;
                let local = ctx.local_position(state.position);
                let new_value = self.value_from_position(size, local);
                self.emit_if_changed(ctx, new_value);
                ctx.request_paint_only();
            }
            PointerEvent::Move(update) if self.dragging => {
                let local = ctx.local_position(update.current.position);
                let new_value = self.value_from_position(size, local);
                self.emit_if_changed(ctx, new_value);
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            })
            | PointerEvent::Cancel(_)
                if self.dragging =>
            {
                self.dragging = false;
                self.last_emitted = None;
                ctx.request_paint_only();
            }
            _ => {}
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
        let TextEvent::Keyboard(event) = event else {
            return;
        };
        if !event.state.is_down() {
            return;
        }
        let nudge = self.nudge();
        let new_value = match &event.key {
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
                Some(self.snap(self.value - nudge))
            }
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
                Some(self.snap(self.value + nudge))
            }
            Key::Named(NamedKey::Home) => Some(self.min),
            Key::Named(NamedKey::End) => Some(self.max),
            _ => None,
        };
        if let Some(new_value) = new_value
            && (new_value - self.value).abs() > f64::EPSILON
        {
            ctx.submit_action::<Self::Action>(SliderChanged(new_value));
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
        let new_value = match (event.action, &event.data) {
            (accesskit::Action::SetValue, Some(ActionData::NumericValue(v))) => Some(self.snap(*v)),
            (accesskit::Action::Increment, _) => Some(self.snap(self.value + self.nudge())),
            (accesskit::Action::Decrement, _) => Some(self.snap(self.value - self.nudge())),
            _ => None,
        };
        if let Some(new_value) = new_value
            && (new_value - self.value).abs() > f64::EPSILON
        {
            ctx.submit_action::<Self::Action>(SliderChanged(new_value));
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

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let cross = Length::px(THUMB_DIAMETER + 2.0 * EDGE_PAD);
        match axis {
            Axis::Horizontal => match len_req {
                LenReq::FitContent(available) => available,
                _ => cross,
            },
            Axis::Vertical => cross,
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let hovered = ctx.is_hovered();
        let active = ctx.is_active() && hovered;
        let focused = ctx.is_focus_target();
        let p = &self.theme.palette;

        let (start, usable) = Self::travel(size);
        let thumb_x = start + self.progress() * usable;
        let center_y = size.height * 0.5;
        let thumb_radius = THUMB_DIAMETER / 2.0;

        let (track_color, fill_color, thumb_color) = self.resolve_colors(hovered, active);

        let track_rect = RoundedRect::from_origin_size(
            Point::new(start, center_y - TRACK_HEIGHT * 0.5),
            Size::new(usable, TRACK_HEIGHT),
            TRACK_HEIGHT * 0.5,
        );
        painter.fill(track_rect, track_color).draw();

        let fill_width = thumb_x - start;
        if fill_width > 0.0 {
            let fill_rect = RoundedRect::from_origin_size(
                Point::new(start, center_y - TRACK_HEIGHT * 0.5),
                Size::new(fill_width, TRACK_HEIGHT),
                TRACK_HEIGHT * 0.5,
            );
            painter.fill(fill_rect, fill_color).draw();
        }

        let center = Point::new(thumb_x, center_y);
        painter.fill(Circle::new(center, thumb_radius), thumb_color).draw();

        if focused && !self.disabled {
            painter
                .stroke(
                    Circle::new(center, thumb_radius + FOCUS_RING_OUTSET),
                    &Stroke::new(FOCUS_RING_WIDTH),
                    p.teal,
                )
                .draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Slider
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, node: &mut Node) {
        if !self.disabled {
            node.add_action(accesskit::Action::SetValue);
            node.add_action(accesskit::Action::Increment);
            node.add_action(accesskit::Action::Decrement);
        }
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
        if self.step > 0.0 {
            node.set_numeric_value_step(self.step);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
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
