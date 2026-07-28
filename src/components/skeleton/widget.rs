//! Masonry widget for the skeleton loading placeholder.
//!
//! [`SkeletonWidget`] is a leaf widget — no children — that paints a single
//! rounded shape standing in for content that has not loaded yet. Unless
//! [`SkeletonAnimation::None`] is set, it drives its own animation loop: it
//! requests an anim frame on [`Update::WidgetAdded`] and advances phase `t`
//! each frame, requesting the next frame until the widget is removed or
//! stashed.
//!
//! The stashed check is load-bearing, not a micro-optimization. Masonry's anim
//! pass does not skip stashed widgets, and neither masonry nor vello tracks
//! damage — so a skeleton hidden inside a closed overlay or a collapsed panel
//! would otherwise keep the entire window re-encoding and re-resolving at
//! refresh rate forever.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Color, Gradient};

use super::view::SkeletonAnimation;
use crate::anim;

/// One full animation cycle in seconds — pulse breathes and the wave sweeps
/// at ~0.66 Hz.
const ANIM_PERIOD_SECS: f64 = 1.5;
/// Minimum fill opacity at the trough of the pulse.
const PULSE_MIN_ALPHA: f32 = 0.4;
/// Maximum fill opacity at the crest of the pulse.
const PULSE_MAX_ALPHA: f32 = 1.0;
/// Half-width of the wave's bright band, as a fraction of the shape width.
const WAVE_BAND_FRACTION: f64 = 0.35;

/// Animated loading placeholder widget.
///
/// A leaf widget (no children) that paints a rounded rectangle — or a circle,
/// when [`Self::circle`] is set — in place of pending content. Color, radius,
/// size, and animation are all configured when the skeleton is created via
/// [`super::view::Skeleton::render`].
pub struct SkeletonWidget {
    pub(super) color: Color,
    /// Peak color of the wave shimmer. Unused by other animations.
    pub(super) highlight: Color,
    /// Corner radius in px. Ignored when `circle` is set.
    pub(super) radius: f64,
    /// When set, paints a full circle (radius = half the shorter side),
    /// overriding `radius`.
    pub(super) circle: bool,
    /// Fixed width in px, or `None` to fill the available width.
    pub(super) width: Option<f64>,
    /// Fixed height in px.
    pub(super) height: f64,
    /// Which animation, if any, drives the fill.
    pub(super) animation: SkeletonAnimation,
    /// Animation phase [0, 1). Advanced each anim frame; unused when `None`.
    pub(super) t: f64,
}

// --- MARK: WIDGETMUT
impl SkeletonWidget {
    /// Sets the base fill color. Requests a repaint on change.
    pub(super) fn set_color(this: &mut WidgetMut<'_, Self>, color: Color) {
        if this.widget.color != color {
            this.widget.color = color;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the wave highlight color. Requests a repaint on change.
    pub(super) fn set_highlight(this: &mut WidgetMut<'_, Self>, highlight: Color) {
        if this.widget.highlight != highlight {
            this.widget.highlight = highlight;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the corner radius in px. Requests a repaint on change.
    pub(super) fn set_radius(this: &mut WidgetMut<'_, Self>, radius: f64) {
        if (this.widget.radius - radius).abs() > f64::EPSILON {
            this.widget.radius = radius;
            this.ctx.request_paint_only();
        }
    }

    /// Sets whether the placeholder is a circle. Requests a repaint on change.
    pub(super) fn set_circle(this: &mut WidgetMut<'_, Self>, circle: bool) {
        if this.widget.circle != circle {
            this.widget.circle = circle;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the fixed width, or clears it to fill available width. Requests
    /// layout on change.
    pub(super) fn set_width(this: &mut WidgetMut<'_, Self>, width: Option<f64>) {
        if this.widget.width != width {
            this.widget.width = width;
            this.ctx.request_layout();
        }
    }

    /// Sets the fixed height in px. Requests layout on change.
    pub(super) fn set_height(this: &mut WidgetMut<'_, Self>, height: f64) {
        if (this.widget.height - height).abs() > f64::EPSILON {
            this.widget.height = height;
            this.ctx.request_layout();
        }
    }

    /// Sets the active animation. Starts a fresh anim frame when moving away
    /// from `None`, and repaints at full opacity when moving to it.
    pub(super) fn set_animation(this: &mut WidgetMut<'_, Self>, animation: SkeletonAnimation) {
        if this.widget.animation != animation {
            this.widget.animation = animation;
            if animation == SkeletonAnimation::None {
                this.ctx.request_paint_only();
            } else {
                this.ctx.request_anim_frame();
            }
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for SkeletonWidget {
    type Action = NoAction;

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        // A stashed skeleton is invisible, but masonry's anim pass does not
        // skip stashed widgets — so without this guard a hidden skeleton keeps
        // re-arming forever, and since nothing in the paint/encode pipeline
        // tracks damage, every one of those frames re-encodes and re-resolves
        // the entire window. Dropping out here (rather than advancing without
        // re-arming) also means the phase is preserved while hidden.
        if self.animation == SkeletonAnimation::None || ctx.is_stashed() {
            return;
        }
        let delta = anim::elapsed_secs(interval) / ANIM_PERIOD_SECS;
        self.t = (self.t + delta).rem_euclid(1.0);
        ctx.request_anim_frame();
        ctx.request_paint_only();
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // `StashedChanged(false)` is what restarts the loop the guard in
        // `on_anim_frame` stopped; without it an unstashed skeleton sits frozen.
        if matches!(event, Update::WidgetAdded | Update::StashedChanged(false))
            && self.animation != SkeletonAnimation::None
        {
            ctx.request_anim_frame();
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
        match axis {
            Axis::Horizontal => match self.width {
                Some(w) => Length::px(w),
                // No fixed width: fill the available space.
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
        let radius = if self.circle {
            size.min_side() / 2.0
        } else {
            self.radius.min(size.min_side() / 2.0)
        };
        let rect = RoundedRect::from_origin_size(Point::ORIGIN, size, radius);

        match self.animation {
            SkeletonAnimation::Pulse => {
                let phase = (self.t * std::f64::consts::TAU).sin();
                // Map sine [-1, 1] onto [0, 1], then onto the alpha band.
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "pulse alpha needs only f32"
                )]
                let unit = (0.5 + 0.5 * phase) as f32;
                let alpha = PULSE_MIN_ALPHA + (PULSE_MAX_ALPHA - PULSE_MIN_ALPHA) * unit;
                painter.fill(rect, self.color.with_alpha(alpha)).draw();
            }
            SkeletonAnimation::Wave => {
                // The bright band travels from off the left edge to off the
                // right edge as `t` advances from 0 to 1.
                let band = size.width * WAVE_BAND_FRACTION;
                let sweep_x = -band + self.t * (size.width + 2.0 * band);
                let gradient = Gradient::new_linear(
                    Point::new(sweep_x - band, 0.0),
                    Point::new(sweep_x + band, 0.0),
                )
                .with_stops([self.color, self.highlight, self.color]);
                painter.fill(rect, &gradient).draw();
            }
            SkeletonAnimation::None => {
                painter.fill(rect, self.color).draw();
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        // A placeholder shape carries no content worth announcing; hide it
        // from assistive tech rather than exposing an empty generic container.
        node.set_hidden();
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::peniko::Color;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;

    use super::{SkeletonAnimation, SkeletonWidget};
    use crate::test_support::StashBox;

    fn widget(animation: SkeletonAnimation) -> SkeletonWidget {
        SkeletonWidget {
            color: Color::from_rgb8(80, 80, 80),
            highlight: Color::from_rgb8(160, 160, 160),
            radius: 4.0,
            circle: false,
            width: Some(120.0),
            height: 16.0,
            animation,
            t: 0.0,
        }
    }

    /// Mounting, laying out, and painting the widget must not panic for any
    /// animation — this exercises the pulse alpha math, the wave gradient, and
    /// the static fill through the harness's redraw.
    #[test]
    fn mounts_and_paints_without_panicking() {
        for animation in [
            SkeletonAnimation::Pulse,
            SkeletonAnimation::Wave,
            SkeletonAnimation::None,
        ] {
            let mut h = TestHarness::create_with_size(
                default_property_set(),
                NewWidget::new(widget(animation)),
                (160, 40),
            );
            h.redraw();
        }
    }

    /// A placeholder shape carries no announceable content, so its node must
    /// stay hidden from assistive tech.
    #[test]
    fn node_is_hidden_from_assistive_tech() {
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget(SkeletonAnimation::None)),
            (160, 40),
        );
        h.redraw();
        let node = h.access_node(h.root_id()).expect("node exists");
        assert!(node.is_hidden());
    }

    fn stashable_harness(animation: SkeletonAnimation) -> TestHarness<StashBox<SkeletonWidget>> {
        StashBox::harness(widget(animation))
    }

    fn child_phase(h: &mut TestHarness<StashBox<SkeletonWidget>>) -> f64 {
        h.edit_root_widget(|mut wm| StashBox::child_mut(&mut wm).widget.t)
    }

    #[test]
    fn a_stashed_skeleton_stops_animating() {
        // A skeleton nobody can see still drove masonry's anim pass, and
        // because nothing in the paint/encode pipeline tracks damage, each of
        // those frames re-encoded and re-resolved the entire window.
        let mut h = stashable_harness(SkeletonAnimation::Pulse);

        h.animate_ms(17);
        let while_visible = child_phase(&mut h);
        assert!(
            while_visible > 0.0,
            "a visible skeleton should advance; got {while_visible}"
        );

        h.edit_root_widget(|mut wm| StashBox::set_child_stashed(&mut wm, true));
        h.animate_ms(17);
        h.animate_ms(17);

        let while_stashed = child_phase(&mut h);
        assert!(
            (while_stashed - while_visible).abs() < f64::EPSILON,
            "a stashed skeleton must not advance: {while_visible} -> {while_stashed}"
        );
    }

    #[test]
    fn unstashing_a_skeleton_resumes_animation() {
        // Stopping while hidden is only correct if showing it again restarts
        // the loop — otherwise a re-opened panel shows a frozen skeleton.
        let mut h = stashable_harness(SkeletonAnimation::Pulse);
        h.edit_root_widget(|mut wm| StashBox::set_child_stashed(&mut wm, true));
        h.animate_ms(17);

        let frozen = child_phase(&mut h);

        h.edit_root_widget(|mut wm| StashBox::set_child_stashed(&mut wm, false));
        h.animate_ms(17);

        let resumed = child_phase(&mut h);
        assert!(
            resumed > frozen,
            "an unstashed skeleton must animate again: {frozen} -> {resumed}"
        );
    }
}
