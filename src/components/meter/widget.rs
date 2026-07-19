//! Masonry widget owning the meter's paint — a themed track with a
//! proportional, optionally heat-tinted fill.
//!
//! Presentation only: no pointer/keyboard interaction, no emitted actions.
//!
//! Painting is built entirely from masonry's own native primitives rather
//! than hand-rolled geometry: [`paint_background`] (the same helper
//! `sized_box` and masonry's built-in `ProgressBar` use internally) paints
//! the track, and the fill reuses `ProgressBar`'s own technique — a single
//! [`peniko::Gradient`](Gradient) spanning the *entire* track width, with a
//! hard transition to fully transparent at `fraction`, so the track
//! (already painted underneath) shows through beyond the fill point. No
//! separate, independently-rounded "fill rect" is sized or painted by hand.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef, RegisterCtx,
    UpdateCtx, Widget, WidgetMut, paint_background,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::color::HueDirection;
use masonry::peniko::{Color, Gradient};
use masonry::properties::{Background, BorderWidth, CornerRadius};

use super::view::MeterFill;

/// Builds the fill gradient. Spans the *entire* track width
/// (`0..track_width`) unconditionally — see [`MeterFill::Gradient`]'s doc
/// comment for why — transitioning to fully transparent right at
/// `fraction`, mirroring masonry's own `ProgressBar` widget, which paints
/// its single-color fill the identical way (a hard-cutoff gradient over the
/// full bar, rather than a separately sized smaller rect). Extracted from
/// `paint` so its geometry is independently testable.
fn fill_gradient(track_width: f64, fraction: f64, fill: MeterFill) -> Gradient {
    let (from, to) = match fill {
        MeterFill::Solid(color) => (color, color),
        MeterFill::Gradient(from, to) => (from, to),
    };
    // Fraction is always clamped to 0.0..=1.0 by every setter, so this is a
    // narrow, non-lossy range for f32's purposes — the gradient API only
    // needs f32 stop offsets.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "fraction is clamped 0.0..=1.0"
    )]
    let cutoff = fraction as f32;
    // The color an unclipped `from..to` gradient would show right at the
    // cutoff — using it as the last visible stop keeps the visible portion
    // identical to what a plain two-stop gradient would render up to that
    // point, before the hard drop to transparent.
    let boundary_color = from.lerp(to, cutoff, HueDirection::default());
    Gradient::new_linear(Point::new(0.0, 0.0), Point::new(track_width, 0.0)).with_stops([
        (0.0, from),
        (cutoff, boundary_color),
        (cutoff, Color::TRANSPARENT),
        (1.0, Color::TRANSPARENT),
    ])
}

/// Themed track + fill bar widget. `fraction` is always kept clamped to
/// `0.0..=1.0`, by every setter.
pub struct MeterWidget {
    pub(super) fraction: f64,
    pub(super) fill: MeterFill,
    pub(super) track_color: Color,
    pub(super) height: f64,
    pub(super) width: Option<f64>,
}

impl MeterWidget {
    /// Sets the fill fraction, clamped to `0.0..=1.0`. Requests a repaint on change.
    pub(super) fn set_fraction(this: &mut WidgetMut<'_, Self>, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        if (this.widget.fraction - fraction).abs() > f64::EPSILON {
            this.widget.fraction = fraction;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the fill style. Requests a repaint on change.
    pub(super) fn set_fill(this: &mut WidgetMut<'_, Self>, fill: MeterFill) {
        if this.widget.fill != fill {
            this.widget.fill = fill;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the track (unfilled background) color. Requests a repaint on change.
    pub(super) fn set_track_color(this: &mut WidgetMut<'_, Self>, color: Color) {
        if this.widget.track_color != color {
            this.widget.track_color = color;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the fixed height in px. Requests layout on change.
    pub(super) fn set_height(this: &mut WidgetMut<'_, Self>, height: f64) {
        if (this.widget.height - height).abs() > f64::EPSILON {
            this.widget.height = height;
            this.ctx.request_layout();
        }
    }

    /// Sets a fixed width, or clears it to fill available width. Requests
    /// layout on change.
    pub(super) fn set_width(this: &mut WidgetMut<'_, Self>, width: Option<f64>) {
        if this.widget.width != width {
            this.widget.width = width;
            this.ctx.request_layout();
        }
    }
}

impl Widget for MeterWidget {
    type Action = NoAction;

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
        match axis {
            Axis::Horizontal => match self.width {
                Some(w) => Length::px(w),
                // No fixed width: fill the available width.
                None => match len_req {
                    LenReq::FitContent(available) => available,
                    _ => Length::px(0.0),
                },
            },
            Axis::Vertical => Length::px(self.height),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    /// Paints the track's background — theme-driven, so it's built from an
    /// explicit `Background`/`CornerRadius` value here rather than the
    /// ambient property stack (this widget has no `Theme` reachable through
    /// the property system; see `button/widget.rs`'s doc comment for why
    /// `void_ui`'s custom widgets read `Theme` as a value instead). Overriding
    /// `pre_paint` to call the same [`paint_background`] helper masonry's
    /// own widgets use is what lets us skip hand-rolling the rounded-rect
    /// math ourselves.
    fn pre_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let bbox = ctx.border_box();
        let corner_radius = CornerRadius::all(Length::px(bbox.height() / 2.0));
        paint_background(
            painter,
            bbox,
            &Background::Color(self.track_color),
            &BorderWidth::default(),
            &corner_radius,
        );
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if self.fraction <= 0.0 {
            return;
        }
        let bbox = ctx.border_box();
        let corner_radius = CornerRadius::all(Length::px(bbox.height() / 2.0));
        let bg_rect = BorderWidth::default().bg_rect(bbox, &corner_radius);
        let gradient = fill_gradient(bbox.width(), self.fraction, self.fill);
        painter.fill(bg_rect, &gradient).draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::ProgressIndicator
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_numeric_value(self.fraction);
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(1.0);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::kurbo::Point;
    use masonry::peniko::color::DynamicColor;
    use masonry::peniko::{Color, GradientKind};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;

    use super::{MeterFill, MeterWidget, fill_gradient};

    fn widget(fraction: f64, fill: MeterFill) -> MeterWidget {
        MeterWidget {
            fraction,
            fill,
            track_color: Color::from_rgb8(60, 60, 60),
            height: 8.0,
            width: Some(160.0),
        }
    }

    /// Mounting, laying out, and painting must not panic for either fill
    /// style across the fraction range, including the edges.
    #[test]
    fn mounts_and_paints_without_panicking() {
        let solid = MeterFill::Solid(Color::from_rgb8(0, 200, 120));
        let gradient =
            MeterFill::Gradient(Color::from_rgb8(0, 200, 120), Color::from_rgb8(220, 60, 60));
        for fill in [solid, gradient] {
            for fraction in [0.0, 0.5, 1.0] {
                let mut h = TestHarness::create_with_size(
                    default_property_set(),
                    NewWidget::new(widget(fraction, fill)),
                    (160, 20),
                );
                h.redraw();
            }
        }
    }

    /// `set_fraction` clamps out-of-range input to `0.0..=1.0` — a caller
    /// handing in a raw, occasionally out-of-range score must not panic or
    /// paint an overflowing/negative-width fill.
    #[test]
    fn set_fraction_clamps_to_unit_range() {
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget(0.5, MeterFill::Solid(Color::from_rgb8(0, 0, 0)))),
            (160, 20),
        );

        h.edit_root_widget(|mut wm| MeterWidget::set_fraction(&mut wm, -0.5));
        assert!((h.edit_root_widget(|wm| wm.widget.fraction) - 0.0).abs() < f64::EPSILON);

        h.edit_root_widget(|mut wm| MeterWidget::set_fraction(&mut wm, 1.5));
        assert!((h.edit_root_widget(|wm| wm.widget.fraction) - 1.0).abs() < f64::EPSILON);
    }

    /// The gradient always spans `0..track_width` — its start/end points
    /// must not depend on `fraction` (only where the transparency cutoff
    /// falls does).
    #[test]
    fn gradient_spans_full_track_regardless_of_fraction() {
        let fill =
            MeterFill::Gradient(Color::from_rgb8(0, 200, 120), Color::from_rgb8(220, 60, 60));
        for fraction in [0.2, 0.9] {
            let g = fill_gradient(200.0, fraction, fill);
            match g.kind {
                GradientKind::Linear(pos) => {
                    assert_eq!(pos.start, Point::new(0.0, 0.0));
                    assert_eq!(pos.end, Point::new(200.0, 0.0));
                }
                other => panic!("expected a linear gradient, got {other:?}"),
            }
        }
    }

    /// Stop offsets are `[0.0, fraction, fraction, 1.0]` — the hard
    /// transparency cutoff sits exactly at `fraction`, not before or after.
    #[test]
    fn stop_offsets_place_the_cutoff_exactly_at_fraction() {
        let fill =
            MeterFill::Gradient(Color::from_rgb8(0, 200, 120), Color::from_rgb8(220, 60, 60));
        let g = fill_gradient(200.0, 0.3, fill);
        let offsets: Vec<f32> = g.stops.0.iter().map(|s| s.offset).collect();
        assert_eq!(offsets, vec![0.0, 0.3, 0.3, 1.0]);
    }

    /// The stop at `x = 0` is always `from`, and the stops past the cutoff
    /// are fully transparent — regardless of `fraction`.
    #[test]
    fn leading_stop_is_from_color_and_tail_is_transparent() {
        let from = Color::from_rgb8(0, 200, 120);
        let to = Color::from_rgb8(220, 60, 60);
        let g = fill_gradient(200.0, 0.6, MeterFill::Gradient(from, to));
        assert_eq!(g.stops.0[0].color, DynamicColor::from_alpha_color(from));
        assert_eq!(
            g.stops.0[2].color,
            DynamicColor::from_alpha_color(Color::TRANSPARENT)
        );
        assert_eq!(
            g.stops.0[3].color,
            DynamicColor::from_alpha_color(Color::TRANSPARENT)
        );
    }

    /// A solid fill degenerates to a flat color from `0` to `fraction`,
    /// then transparent — matching masonry's own `ProgressBar` stop shape
    /// for its single-color case.
    #[test]
    fn solid_fill_uses_the_same_color_at_both_leading_stops() {
        let color = Color::from_rgb8(10, 20, 30);
        let g = fill_gradient(200.0, 0.4, MeterFill::Solid(color));
        assert_eq!(g.stops.0[0].color, DynamicColor::from_alpha_color(color));
        assert_eq!(g.stops.0[1].color, DynamicColor::from_alpha_color(color));
    }
}
