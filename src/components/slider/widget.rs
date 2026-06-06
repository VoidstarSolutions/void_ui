//! Masonry widget owning the slider's paint and pointer/keyboard interaction.
//!
//! Paints a track, an active-range fill, and one or two draggable circular
//! thumbs directly from a [`Theme`] value. `value`, `min`, `max`, and `step`
//! are host-controlled — the widget never mutates them itself, it only emits
//! [`SliderChanged`] for the host to apply via [`set_value`](Self::set_value).
//!
//! In [`SliderValue::Range`] mode the widget tracks two independent thumbs
//! (low/high) that cannot cross; whichever thumb is closer to a press or the
//! most recently dragged/nudged thumb receives keyboard and accessibility
//! adjustments.
//!
//! Emits [`SliderChanged`] continuously while a thumb is dragged, on
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

use super::{SliderChanged, SliderValue};
use crate::{Orientation, Theme};

/// Diameter of a draggable thumb circle, in logical pixels.
const THUMB_DIAMETER: f64 = 14.0;
/// Thickness of the track and fill bar.
const TRACK_HEIGHT: f64 = 4.0;
/// Focus-ring stroke width.
const FOCUS_RING_WIDTH: f64 = 1.5;
/// Gap between the thumb edge and the focus ring.
const FOCUS_RING_OUTSET: f64 = 2.0;
/// Clearance from the widget edge to the thumbs' travel limits — keeps the
/// thumbs and their focus rings from being clipped at the ends of the track.
const EDGE_PAD: f64 = FOCUS_RING_OUTSET + FOCUS_RING_WIDTH;

/// Identifies which thumb a gesture or keyboard adjustment targets.
///
/// `Single` is the only variant in [`SliderValue::Single`] mode. `Low`/`High`
/// only arise in [`SliderValue::Range`] mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Thumb {
    Single,
    Low,
    High,
}

/// Interactive horizontal slider widget — single-thumb or dual-thumb range.
///
/// `value`, `min`, `max`, and `step` mirror host state; the host drives them
/// via the `set_*` associated functions in response to [`SliderChanged`].
pub struct SliderWidget {
    value: SliderValue,
    min: f64,
    max: f64,
    /// Snap increment. `0.0` (or negative) means continuous — no snapping.
    step: f64,
    disabled: bool,
    /// Layout axis. Horizontal travels left-to-right; vertical travels
    /// bottom-to-top (`min` at the bottom, `max` at the top).
    orientation: Orientation,
    theme: Theme,
    /// Thumb captured by an in-progress primary-pointer drag gesture.
    dragging: Option<Thumb>,
    /// Last value emitted during the current gesture — deduplicates
    /// [`SliderChanged`] while the host's `value` round-trips back.
    last_emitted: Option<SliderValue>,
    /// The thumb most recently targeted by a gesture or key/access action —
    /// keyboard nudges and accessibility actions apply to this thumb. Always
    /// `Single` outside range mode.
    focused_thumb: Thumb,
}

// --- MARK: BUILDERS
impl SliderWidget {
    /// Creates a new single-thumb slider in the given state.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new_single(
        theme: &Theme,
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        disabled: bool,
        orientation: Orientation,
    ) -> Self {
        Self::new(theme, SliderValue::Single(value), min, max, step, disabled, orientation)
    }

    /// Creates a new dual-thumb range slider in the given state.
    ///
    /// `low` and `high` are clamped to `low <= high` by the host; the widget
    /// trusts the values it's given.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new_range(
        theme: &Theme,
        low: f64,
        high: f64,
        min: f64,
        max: f64,
        step: f64,
        disabled: bool,
        orientation: Orientation,
    ) -> Self {
        Self::new(
            theme,
            SliderValue::Range(low, high),
            min,
            max,
            step,
            disabled,
            orientation,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        theme: &Theme,
        value: SliderValue,
        min: f64,
        max: f64,
        step: f64,
        disabled: bool,
        orientation: Orientation,
    ) -> Self {
        let focused_thumb = match value {
            SliderValue::Single(_) => Thumb::Single,
            SliderValue::Range(..) => Thumb::High,
        };
        Self {
            value,
            min,
            max,
            step,
            disabled,
            orientation,
            theme: *theme,
            dragging: None,
            last_emitted: None,
            focused_thumb,
        }
    }
}

// --- MARK: WIDGETMUT
impl SliderWidget {
    /// Sets the current value. Requests a repaint and accessibility update on change.
    ///
    /// Switching between [`SliderValue::Single`] and [`SliderValue::Range`]
    /// is supported and resets the focused-thumb tracking accordingly.
    pub fn set_value(this: &mut WidgetMut<'_, Self>, value: SliderValue) {
        if this.widget.value != value {
            let mode_changed = std::mem::discriminant(&this.widget.value) != std::mem::discriminant(&value);
            this.widget.value = value;
            if mode_changed {
                this.widget.focused_thumb = match value {
                    SliderValue::Single(_) => Thumb::Single,
                    SliderValue::Range(..) => Thumb::High,
                };
            }
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

    /// Sets the layout axis. Requests a re-layout and repaint on change.
    pub fn set_orientation(this: &mut WidgetMut<'_, Self>, orientation: Orientation) {
        if this.widget.orientation != orientation {
            this.widget.orientation = orientation;
            this.ctx.request_layout();
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

    /// The widget's main (travel) axis length and cross-axis length, given
    /// its border-box `size` and [`Self::orientation`] — horizontal sliders
    /// travel along the width, vertical sliders along the height.
    fn axis_lengths(&self, size: Size) -> (f64, f64) {
        match self.orientation {
            Orientation::Horizontal => (size.width, size.height),
            Orientation::Vertical => (size.height, size.width),
        }
    }

    /// Range of a thumb center's travel along the main axis within `size`, as
    /// `(start, usable_length)`.
    fn travel(&self, size: Size) -> (f64, f64) {
        let (main, _) = self.axis_lengths(size);
        let thumb_radius = THUMB_DIAMETER / 2.0;
        let start = thumb_radius + EDGE_PAD;
        let end = (main - thumb_radius - EDGE_PAD).max(start);
        (start, end - start)
    }

    /// Position along the main axis of the thumb center representing value `v`.
    ///
    /// Horizontal sliders place `min` at the start (left); vertical sliders
    /// place `min` at the end (bottom), matching conventional vertical-slider
    /// layout where values increase upward.
    fn thumb_main_axis_pos(&self, size: Size, v: f64) -> f64 {
        let (start, usable) = self.travel(size);
        let span = self.max - self.min;
        let t = if span > 0.0 {
            ((v - self.min) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        match self.orientation {
            Orientation::Horizontal => start + t * usable,
            Orientation::Vertical => start + (1.0 - t) * usable,
        }
    }

    /// Center point of the thumb representing value `v`.
    fn thumb_center(&self, size: Size, v: f64) -> Point {
        let main = self.thumb_main_axis_pos(size, v);
        let (_, cross_len) = self.axis_lengths(size);
        let cross = cross_len * 0.5;
        match self.orientation {
            Orientation::Horizontal => Point::new(main, cross),
            Orientation::Vertical => Point::new(cross, main),
        }
    }

    /// Converts a local-space pointer position to a snapped value in `[min, max]`.
    fn value_from_position(&self, size: Size, pos: Point) -> f64 {
        let (start, usable) = self.travel(size);
        let coord = match self.orientation {
            Orientation::Horizontal => pos.x,
            Orientation::Vertical => pos.y,
        };
        let raw_t = ((coord - start) / usable.max(1e-6)).clamp(0.0, 1.0);
        let t = match self.orientation {
            Orientation::Horizontal => raw_t,
            Orientation::Vertical => 1.0 - raw_t,
        };
        self.snap(self.min + t * (self.max - self.min))
    }

    /// The current `(low, high)` in range mode, or `None` outside it.
    fn range_bounds(&self) -> Option<(f64, f64)> {
        match self.value {
            SliderValue::Range(low, high) => Some((low, high)),
            SliderValue::Single(_) => None,
        }
    }

    /// Picks the thumb a press at local-space `pos` should drive: the nearer
    /// of the two thumbs in range mode, or the only thumb otherwise.
    fn thumb_at(&self, size: Size, pos: Point) -> Thumb {
        match self.range_bounds() {
            Some((low, high)) => {
                let low_center = self.thumb_center(size, low);
                let high_center = self.thumb_center(size, high);
                if pos.distance(low_center) <= pos.distance(high_center) {
                    Thumb::Low
                } else {
                    Thumb::High
                }
            }
            None => Thumb::Single,
        }
    }

    /// Computes the slider's new value when `thumb` is driven to raw value
    /// `target` (already snapped to the step grid). Range-mode thumbs are
    /// clamped against each other so they cannot cross.
    fn value_for_thumb(&self, thumb: Thumb, target: f64) -> SliderValue {
        match thumb {
            Thumb::Single => SliderValue::Single(target),
            Thumb::Low => {
                let high = self.range_bounds().map_or(self.max, |(_, high)| high);
                SliderValue::Range(target.min(high), high)
            }
            Thumb::High => {
                let low = self.range_bounds().map_or(self.min, |(low, _)| low);
                SliderValue::Range(low, target.max(low))
            }
        }
    }

    /// Applies `nudge` to `self.focused_thumb`'s current value and returns the result.
    fn nudged_value(&self, delta: f64) -> SliderValue {
        // `set_value` resets `focused_thumb` on mode changes, so `Low` only
        // arises paired with `Range`; `Single`/`High` both read the value
        // that the corresponding `SliderValue` variant carries as "the" value.
        let current = match self.value {
            SliderValue::Range(low, _) if self.focused_thumb == Thumb::Low => low,
            SliderValue::Range(_, high) => high,
            SliderValue::Single(v) => v,
        };
        self.value_for_thumb(self.focused_thumb, self.snap(current + delta))
    }

    /// Emits [`SliderChanged`] if `new_value` differs from the last value
    /// emitted during this gesture (or from the host value, outside a gesture).
    fn emit_if_changed(&mut self, ctx: &mut EventCtx<'_>, new_value: SliderValue) {
        let baseline = self.last_emitted.unwrap_or(self.value);
        if baseline != new_value {
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
        let thumb = if active || hovered { p.teal_deep } else { p.teal };
        (p.surface_2, p.teal, thumb)
    }

    /// A bar spanning `[main_start, main_end]` along the main axis, centered
    /// on the cross axis with the given `thickness` and `corner_radius`.
    fn bar_rect(&self, size: Size, main_start: f64, main_end: f64, thickness: f64, corner_radius: f64) -> RoundedRect {
        let half_thickness = thickness * 0.5;
        let (_, cross_len) = self.axis_lengths(size);
        let cross_center = cross_len * 0.5;
        let (origin, bar_size) = match self.orientation {
            Orientation::Horizontal => (
                Point::new(main_start, cross_center - half_thickness),
                Size::new(main_end - main_start, thickness),
            ),
            Orientation::Vertical => (
                Point::new(cross_center - half_thickness, main_start),
                Size::new(thickness, main_end - main_start),
            ),
        };
        RoundedRect::from_origin_size(origin, bar_size, corner_radius)
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
                let local = ctx.local_position(state.position);
                let thumb = self.thumb_at(size, local);
                self.dragging = Some(thumb);
                self.focused_thumb = thumb;
                let target = self.value_from_position(size, local);
                let new_value = self.value_for_thumb(thumb, target);
                self.emit_if_changed(ctx, new_value);
                ctx.request_paint_only();
            }
            PointerEvent::Move(update) => {
                if let Some(thumb) = self.dragging {
                    let local = ctx.local_position(update.current.position);
                    let target = self.value_from_position(size, local);
                    let new_value = self.value_for_thumb(thumb, target);
                    self.emit_if_changed(ctx, new_value);
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            })
            | PointerEvent::Cancel(_)
                if self.dragging.is_some() =>
            {
                self.dragging = None;
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
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => Some(self.nudged_value(-nudge)),
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => Some(self.nudged_value(nudge)),
            Key::Named(NamedKey::Home) => Some(self.value_for_thumb(self.focused_thumb, self.min)),
            Key::Named(NamedKey::End) => Some(self.value_for_thumb(self.focused_thumb, self.max)),
            _ => None,
        };
        if let Some(new_value) = new_value
            && new_value != self.value
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
            (accesskit::Action::SetValue, Some(ActionData::NumericValue(v))) => {
                Some(self.value_for_thumb(self.focused_thumb, self.snap(*v)))
            }
            (accesskit::Action::Increment, _) => Some(self.nudged_value(self.nudge())),
            (accesskit::Action::Decrement, _) => Some(self.nudged_value(-self.nudge())),
            _ => None,
        };
        if let Some(new_value) = new_value
            && new_value != self.value
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
        let main_axis = match self.orientation {
            Orientation::Horizontal => Axis::Horizontal,
            Orientation::Vertical => Axis::Vertical,
        };
        if axis == main_axis {
            match len_req {
                LenReq::FitContent(available) => available,
                _ => cross,
            }
        } else {
            cross
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

        let (start, usable) = self.travel(size);
        let thumb_radius = THUMB_DIAMETER / 2.0;
        let half_track = TRACK_HEIGHT * 0.5;

        let (track_color, fill_color, thumb_color) = self.resolve_colors(hovered, active);

        let track_rect = self.bar_rect(size, start, start + usable, TRACK_HEIGHT, half_track);
        painter.fill(track_rect, track_color).draw();

        // Fill spans [min, value] in single mode, or [low, high] in range mode.
        let (fill_lo, fill_hi, thumb_values): (f64, f64, [Option<f64>; 2]) = match self.value {
            SliderValue::Single(v) => (self.min, v, [Some(v), None]),
            SliderValue::Range(low, high) => (low, high, [Some(low), Some(high)]),
        };

        let fill_a = self.thumb_main_axis_pos(size, fill_lo);
        let fill_b = self.thumb_main_axis_pos(size, fill_hi);
        let (fill_start, fill_end) = (fill_a.min(fill_b), fill_a.max(fill_b));

        if fill_end > fill_start {
            let fill_rect = self.bar_rect(size, fill_start, fill_end, TRACK_HEIGHT, half_track);
            painter.fill(fill_rect, fill_color).draw();
        }

        for value in thumb_values.into_iter().flatten() {
            let center = self.thumb_center(size, value);
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
        // Accessibility trees model a single numeric value; in range mode we
        // report the focused thumb's bound, matching what keyboard/access
        // adjustments operate on.
        let reported = match (self.focused_thumb, self.value) {
            (Thumb::Low, SliderValue::Range(low, _)) => low,
            (_, SliderValue::Range(_, high)) => high,
            (_, SliderValue::Single(v)) => v,
        };
        node.set_numeric_value(reported);
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
