//! Masonry widget for the standalone spinner component.
//!
//! `SpinnerWidget` is a leaf widget — no children — that drives its own
//! animation loop. It requests an anim frame on [`Update::WidgetAdded`] and
//! advances the rotation phase each frame, requesting the next frame until the
//! widget is removed or stashed.
//!
//! The stashed check is load-bearing, not a micro-optimization. Masonry's anim
//! pass does not skip stashed widgets, and neither masonry nor vello tracks
//! damage — so a spinner hidden inside a closed overlay or a collapsed panel
//! would otherwise keep the entire window re-encoding and re-resolving at
//! refresh rate forever.

use std::sync::LazyLock;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Arc as KurboArc, Axis, BezPath, Point, Shape, Size, Stroke, Vec2};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

use crate::anim;

/// Sweep angle (radians) of the spinner arc — leaves a ~60° gap. An angle,
/// not spacing, so it doesn't scale with density.
const SPINNER_SWEEP: f64 = std::f64::consts::TAU * (300.0 / 360.0);

/// Partial-circle `BezPath` in unit-square (0..1) space. Computed once and
/// cached; callers scale and rotate via `Affine` at paint time.
pub(crate) static SPINNER_PATH: LazyLock<BezPath> = LazyLock::new(|| {
    let arc = KurboArc {
        center: Point::new(0.5, 0.5),
        radii: Vec2::new(0.45, 0.45),
        start_angle: 0.0,
        sweep_angle: SPINNER_SWEEP,
        x_rotation: 0.0,
    };
    let mut path = BezPath::new();
    arc.into_path(0.01).iter().for_each(|el| path.push(el));
    path
});

/// Animated loading spinner widget.
///
/// A leaf widget (no children) that continuously rotates a partial-circle arc.
/// Color and size are configurable; both default to theme values when the
/// spinner is created via [`super::view::Spinner::render`].
pub struct SpinnerWidget {
    pub(super) color: Color,
    pub(super) size: f64,
    /// Rotation phase [0, 1). Advanced each anim frame.
    t: f64,
}

// --- MARK: BUILDERS
impl SpinnerWidget {
    /// Create a spinner with explicit color and size in pixels.
    #[must_use]
    pub fn new(color: Color, size: f64) -> Self {
        Self {
            color,
            size,
            t: 0.0,
        }
    }
}

// --- MARK: WIDGETMUT
impl SpinnerWidget {
    /// Sets the spinner color. Requests a repaint on change.
    pub fn set_color(this: &mut WidgetMut<'_, Self>, color: Color) {
        if this.widget.color != color {
            this.widget.color = color;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the spinner size in pixels. Requests layout + repaint on change.
    pub fn set_size(this: &mut WidgetMut<'_, Self>, size: f64) {
        if (this.widget.size - size).abs() > f64::EPSILON {
            this.widget.size = size;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for SpinnerWidget {
    type Action = NoAction;

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        // A stashed spinner is invisible, but masonry's anim pass does not skip
        // stashed widgets — so without this guard a hidden spinner keeps
        // re-arming forever, and since nothing in the paint/encode pipeline
        // tracks damage, every one of those frames re-encodes and re-resolves
        // the entire window. Dropping out here (rather than advancing without
        // re-arming) also means the phase is preserved while hidden.
        if ctx.is_stashed() {
            return;
        }
        self.t = (self.t + anim::elapsed_secs(interval)).rem_euclid(1.0);
        ctx.request_anim_frame();
        ctx.request_paint_only();
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // `StashedChanged(false)` is what restarts the loop the guard in
        // `on_anim_frame` stopped; without it an unstashed spinner sits frozen.
        if matches!(event, Update::WidgetAdded | Update::StashedChanged(false)) {
            ctx.request_anim_frame();
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(self.size)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let spinner = &*SPINNER_PATH;
        let angle = self.t * std::f64::consts::TAU;
        let spin =
            Affine::translate((0.5, 0.5)) * Affine::rotate(angle) * Affine::translate((-0.5, -0.5));
        let transform = Affine::scale(self.size) * spin;
        painter
            .stroke(transform * spinner, &Stroke::new(1.5), self.color)
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::ProgressIndicator
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
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

    use super::SpinnerWidget;
    use crate::test_support::StashBox;

    fn harness() -> TestHarness<SpinnerWidget> {
        TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(SpinnerWidget::new(Color::BLACK, 16.0)),
        )
    }

    fn stashable_harness() -> TestHarness<StashBox<SpinnerWidget>> {
        StashBox::harness(SpinnerWidget::new(Color::BLACK, 16.0))
    }

    fn child_phase(h: &mut TestHarness<StashBox<SpinnerWidget>>) -> f64 {
        h.edit_root_widget(|mut wm| StashBox::child_mut(&mut wm).widget.t)
    }

    #[test]
    fn a_stashed_spinner_stops_animating() {
        // A spinner nobody can see still drove masonry's anim pass, and because
        // neither masonry nor vello tracks damage, each of those frames
        // re-encoded and re-resolved the *entire* window. A hidden spinner must
        // let the app go idle.
        let mut h = stashable_harness();

        h.animate_ms(17);
        let while_visible = child_phase(&mut h);
        assert!(
            while_visible > 0.0,
            "a visible spinner should advance; got {while_visible}"
        );

        h.edit_root_widget(|mut wm| StashBox::set_child_stashed(&mut wm, true));
        h.animate_ms(17);
        h.animate_ms(17);

        let while_stashed = child_phase(&mut h);
        assert!(
            (while_stashed - while_visible).abs() < f64::EPSILON,
            "a stashed spinner must not advance: {while_visible} -> {while_stashed}"
        );
    }

    #[test]
    fn unstashing_a_spinner_resumes_animation() {
        // Stopping while hidden is only correct if showing it again restarts
        // the loop — otherwise a re-opened overlay shows a frozen spinner.
        let mut h = stashable_harness();
        h.edit_root_widget(|mut wm| StashBox::set_child_stashed(&mut wm, true));
        h.animate_ms(17);

        let frozen = child_phase(&mut h);

        h.edit_root_widget(|mut wm| StashBox::set_child_stashed(&mut wm, false));
        h.animate_ms(17);

        let resumed = child_phase(&mut h);
        assert!(
            resumed > frozen,
            "an unstashed spinner must animate again: {frozen} -> {resumed}"
        );
    }

    #[test]
    fn set_color_updates_the_field_when_it_changes() {
        let mut h = harness();
        h.edit_root_widget(|mut wm| {
            SpinnerWidget::set_color(&mut wm, Color::WHITE);
            assert_eq!(wm.widget.color, Color::WHITE);
        });
    }

    #[test]
    fn set_size_updates_the_field_when_it_changes() {
        let mut h = harness();
        h.edit_root_widget(|mut wm| {
            SpinnerWidget::set_size(&mut wm, 32.0);
            assert!((wm.widget.size - 32.0).abs() < f64::EPSILON);
        });
    }

    #[test]
    fn set_size_is_a_no_op_within_epsilon() {
        let mut h = harness();
        h.edit_root_widget(|mut wm| {
            SpinnerWidget::set_size(&mut wm, 16.0);
            assert!((wm.widget.size - 16.0).abs() < f64::EPSILON);
        });
    }

    #[test]
    fn rotation_phase_advances_and_wraps_within_zero_one() {
        let mut h = harness();
        h.edit_root_widget(|wm| {
            assert!((wm.widget.t - 0.0).abs() < f64::EPSILON);
        });

        // 16.7ms (~1/60s) is a plausible single-frame interval; the phase
        // should advance but stay well under a full turn.
        h.animate_ms(17);
        h.edit_root_widget(|wm| {
            assert!(wm.widget.t > 0.0 && wm.widget.t < 1.0);
        });

        // Many frames' worth of elapsed time must wrap back into [0, 1)
        // rather than growing unbounded.
        h.animate_ms(10_000);
        h.edit_root_widget(|wm| {
            assert!((0.0..1.0).contains(&wm.widget.t));
        });
    }
}
