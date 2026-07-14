//! Masonry widget for the standalone spinner component.
//!
//! `SpinnerWidget` is a leaf widget — no children — that drives its own
//! animation loop. It requests an anim frame on [`Update::WidgetAdded`] and
//! advances the rotation phase each frame, requesting the next frame until
//! the widget is removed.

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
        self.t = (self.t + anim::elapsed_secs(interval)).rem_euclid(1.0);
        ctx.request_anim_frame();
        ctx.request_paint_only();
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
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
