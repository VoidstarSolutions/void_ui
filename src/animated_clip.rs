//! Axis-animated clip widget — shared primitive for collapsible components.
//!
//! [`AnimatedClip`] wraps any child widget and animates its visible extent on
//! one axis between the child's natural size (open) and zero (closed) over
//! 250 ms. The child is always laid out at its full natural size so content
//! does not reflow during the animation; a `set_clip_path` call masks the
//! in-progress region.
//!
//! Pass `Axis::Horizontal` to get a horizontal slide (used by
//! `ThemedSidebarPanel`) or `Axis::Vertical` for a vertical slide (used by
//! `CollapsibleWidget`).

use std::any::TypeId;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, FromDynWidget, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::widgets::{AnimatedF32, AnimationStatus};

/// Duration of the open/close animation — an animation timing value, not
/// spacing, so it doesn't scale with density.
const SLIDE_MILLIS: f32 = 250.0;

/// Clips its child to an animated extent on one axis.
///
/// Created by `ThemedSidebarPanel` (horizontal) and `CollapsibleWidget`
/// (vertical). Public so the access-path methods `panel_mut` →
/// [`AnimatedClip::child_mut`] and `body_mut` → [`AnimatedClip::child_mut`]
/// are nameable outside the crate.
pub struct AnimatedClip<W: Widget + ?Sized> {
    child: WidgetPod<W>,
    /// Axis to animate: `Horizontal` slides width, `Vertical` slides height.
    axis: Axis,
    /// `true` = fully open (progress → 0.0), `false` = fully closed (progress → 1.0).
    open: bool,
    /// 0.0 = fully visible, 1.0 = fully hidden.
    collapse_progress: AnimatedF32,
    /// Child's natural size on [`Self::axis`] from the most recent measure pass.
    natural_extent: f64,
}

// --- MARK: CONSTRUCTORS

impl<W: Widget + ?Sized> AnimatedClip<W> {
    /// Wrap `child` with an animated clip on `axis`.
    ///
    /// `open = true` starts fully visible; `open = false` starts fully hidden.
    #[must_use]
    pub fn new(child: NewWidget<W>, axis: Axis, open: bool) -> Self {
        Self {
            child: child.to_pod(),
            axis,
            open,
            collapse_progress: AnimatedF32::stable(if open { 0.0 } else { 1.0 }),
            natural_extent: 0.0,
        }
    }

    fn animated_extent(&self) -> f64 {
        (self.natural_extent * f64::from(1.0 - self.collapse_progress.value())).max(0.0)
    }
}

// --- MARK: WIDGETMUT

impl<W: Widget + FromDynWidget> AnimatedClip<W> {
    /// Returns a `WidgetMut` for the wrapped child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + ?Sized> AnimatedClip<W> {
    /// Drive the open/closed state. Starts the animation if the value changes.
    pub fn set_open(this: &mut WidgetMut<'_, Self>, open: bool) {
        if this.widget.open != open {
            this.widget.open = open;
            let target: f32 = if open { 0.0 } else { 1.0 };
            if (target - this.widget.collapse_progress.value()).abs() > crate::anim::SETTLE_EPSILON
            {
                this.ctx.request_anim_frame();
            }
        }
    }
}

// --- MARK: IMPL WIDGET

impl<W: Widget + ?Sized> Widget for AnimatedClip<W> {
    type Action = NoAction;

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let target: f32 = if self.open { 0.0 } else { 1.0 };
        let before = self.collapse_progress.value();
        let status = crate::anim::advance_toward(
            &mut self.collapse_progress,
            target,
            SLIDE_MILLIS,
            interval,
        );
        if (self.collapse_progress.value() - before).abs() > f32::EPSILON {
            ctx.request_layout();
        }
        if status == AnimationStatus::Ongoing {
            ctx.request_anim_frame();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        let child_length = ctx.compute_length(
            &mut self.child,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        );
        if axis == self.axis {
            self.natural_extent = child_length.get();
            Length::px(self.animated_extent())
        } else {
            child_length
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // Always lay out the child at its full natural extent so content does
        // not reflow during the animation.
        let child_size = match self.axis {
            Axis::Horizontal => Size::new(self.natural_extent.max(size.width), size.height),
            Axis::Vertical => Size::new(size.width, self.natural_extent.max(size.height)),
        };
        ctx.run_layout(&mut self.child, child_size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        // Clip to the animated extent; content slides out of view.
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use masonry::widgets::AnimatedF32;

    use super::SLIDE_MILLIS;
    use crate::anim::advance_toward;

    const MS: u64 = 1_000_000; // one millisecond in nanoseconds

    #[test]
    fn one_full_duration_frame_completes_the_slide() {
        // A single frame lasting the whole 250ms slide advances a full unit.
        // The generic constant-velocity/clamp/settle behavior of
        // advance_toward itself is covered in crate::anim's own tests.
        let mut progress = AnimatedF32::stable(0.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "SLIDE_MILLIS is a small positive constant"
        )]
        let full = (SLIDE_MILLIS as u64) * MS;
        advance_toward(&mut progress, 1.0, SLIDE_MILLIS, full);
        assert!(
            (progress.value() - 1.0).abs() < 1e-4,
            "full-duration frame completes the slide, got {}",
            progress.value()
        );
    }
}
