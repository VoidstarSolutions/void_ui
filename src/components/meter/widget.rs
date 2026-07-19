//! Masonry widget owning the meter's paint — a themed track with a
//! proportional, optionally heat-tinted fill.
//!
//! Presentation only: no pointer/keyboard interaction, no emitted actions.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef, RegisterCtx,
    UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Color, Gradient};

use super::view::MeterFill;

/// Builds the fill gradient across the *full track width*, not the filled
/// portion — see [`MeterFill::Gradient`]'s doc comment. Extracted from
/// `paint` so its geometry is independently testable.
fn full_track_gradient(track_width: f64, from: Color, to: Color) -> Gradient {
    Gradient::new_linear(Point::new(0.0, 0.0), Point::new(track_width, 0.0)).with_stops([from, to])
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

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box().size();
        let radius = size.height / 2.0;

        let track = RoundedRect::from_origin_size(Point::ORIGIN, size, radius);
        painter.fill(track, self.track_color).draw();

        let fill_width = size.width * self.fraction;
        if fill_width <= 0.0 {
            return;
        }
        let fill_rect = RoundedRect::from_origin_size(
            Point::ORIGIN,
            Size::new(fill_width, size.height),
            radius,
        );
        match self.fill {
            MeterFill::Solid(color) => {
                painter.fill(fill_rect, color).draw();
            }
            MeterFill::Gradient(from, to) => {
                let gradient = full_track_gradient(size.width, from, to);
                painter.fill(fill_rect, &gradient).draw();
            }
        }
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
    use masonry::peniko::{Color, GradientKind};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;

    use super::{MeterFill, MeterWidget, full_track_gradient};

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
    /// must not depend on `fraction` (that's the fill *rect*'s job, via
    /// clipping to `size.width * fraction` in `paint`).
    #[test]
    fn gradient_spans_full_track_regardless_of_fraction() {
        let from = Color::from_rgb8(0, 200, 120);
        let to = Color::from_rgb8(220, 60, 60);
        let g = full_track_gradient(200.0, from, to);
        match g.kind {
            GradientKind::Linear(pos) => {
                assert_eq!(pos.start, Point::new(0.0, 0.0));
                assert_eq!(pos.end, Point::new(200.0, 0.0));
            }
            other => panic!("expected a linear gradient, got {other:?}"),
        }
    }
}
