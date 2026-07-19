//! Masonry widget owning the meter's paint — a themed track with a
//! proportional, optionally heat-tinted fill.
//!
//! Presentation only: no pointer/keyboard interaction, no emitted actions.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesRef, RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::{Color, Gradient};
use masonry::widgets::Label;

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
    /// Centered overlay text, if any.
    pub(super) label: Option<WidgetPod<Label>>,
    /// The raw text behind `label`, kept alongside the built child so
    /// `accessibility` can hand it to `node.set_value` without reading text
    /// back out of a `Label` widget.
    pub(super) label_text: Option<ArcStr>,
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

    /// Replaces the centered label. Removes any existing label first.
    pub(super) fn attach_label(
        this: &mut WidgetMut<'_, Self>,
        label: NewWidget<Label>,
        text: ArcStr,
    ) {
        if let Some(old) = this.widget.label.take() {
            this.ctx.remove_child(old);
        }
        this.widget.label = Some(label.to_pod());
        this.widget.label_text = Some(text);
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Removes the label, if present.
    pub(super) fn detach_label(this: &mut WidgetMut<'_, Self>) {
        if let Some(old) = this.widget.label.take() {
            this.ctx.remove_child(old);
            this.widget.label_text = None;
            this.ctx.children_changed();
            this.ctx.request_layout();
        }
    }
}

impl Widget for MeterWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        if let Some(label) = &mut self.label {
            ctx.register_child(label);
        }
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
        if let Some(label) = &mut self.label {
            let context_size = LayoutSize::maybe(axis.cross(), cross_length);
            let _ = ctx.compute_length(label, len_req.into(), context_size, axis, cross_length);
        }
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

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if let Some(label) = &mut self.label {
            let label_size = ctx.compute_size(label, SizeDef::fit(size), size.into());
            ctx.run_layout(label, label_size);
            let x = ((size.width - label_size.width) * 0.5).max(0.0);
            let y = ((size.height - label_size.height) * 0.5).max(0.0);
            ctx.place_child(label, Point::new(x, y));
        }
    }

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
        if let Some(text) = &self.label_text {
            node.set_value(text.to_string());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        match &self.label {
            Some(label) => ChildrenIds::from_slice(&[label.id()]),
            None => ChildrenIds::from_slice(&[]),
        }
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::{NewWidget, Widget};
    use masonry::kurbo::Point;
    use masonry::peniko::{Color, GradientKind};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{MeterFill, MeterWidget, full_track_gradient};

    fn widget(fraction: f64, fill: MeterFill) -> MeterWidget {
        MeterWidget {
            fraction,
            fill,
            track_color: Color::from_rgb8(60, 60, 60),
            height: 8.0,
            width: Some(160.0),
            label: None,
            label_text: None,
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

    use masonry::core::StyleProperty;
    use masonry::properties::ContentColor;

    fn label_widget(text: &str) -> MeterWidget {
        let mut lbl = Label::new(text)
            .with_style(StyleProperty::FontSize(12.0))
            .prepare();
        lbl.properties
            .insert(ContentColor::new(Color::from_rgb8(240, 240, 240)));
        let mut w = widget(0.5, MeterFill::Solid(Color::from_rgb8(0, 200, 120)));
        w.label = Some(lbl.to_pod());
        w.label_text = Some("72%".into());
        w
    }

    /// A meter with a label mounts and paints without panicking (exercises
    /// the child measure/layout/paint path added in this task).
    #[test]
    fn label_mounts_and_paints_without_panicking() {
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(label_widget("72%")),
            (160, 20),
        );
        h.redraw();
    }

    /// The label's text is exposed to assistive tech as the node's value —
    /// domain-agnostic (works for "72%", "B+", whatever the caller means).
    #[test]
    fn label_text_is_exposed_as_accessibility_value() {
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(label_widget("72%")),
            (160, 20),
        );
        h.redraw();
        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.value(), Some("72%".to_string()));
    }

    /// No label attached means no accessibility value is set at all.
    #[test]
    fn no_label_means_no_accessibility_value() {
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget(0.5, MeterFill::Solid(Color::from_rgb8(0, 200, 120)))),
            (160, 20),
        );
        h.redraw();
        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.value(), None);
    }
}
