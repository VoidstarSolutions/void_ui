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

use crate::anim;

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
    collapse_progress: f32,
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
            collapse_progress: if open { 0.0 } else { 1.0 },
            natural_extent: 0.0,
        }
    }

    fn animated_extent(&self) -> f64 {
        (self.natural_extent * f64::from(1.0 - self.collapse_progress)).max(0.0)
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
            if (target - this.widget.collapse_progress).abs() > SNAP_EPSILON {
                this.ctx.request_anim_frame();
            }
        }
    }
}

/// How close `collapse_progress` must be to its target to count as settled.
const SNAP_EPSILON: f32 = 1e-4;

/// Advances `progress` (0.0 open … 1.0 closed) toward its target for a frame
/// of `interval` nanoseconds, returning the new progress.
///
/// The per-frame step comes from [`crate::anim::elapsed_fraction`], which
/// guarantees a non-zero step for a non-zero interval — the property whose
/// absence (integer ns→ms truncation) stalled this animation and spun the CPU
/// forever (#139). Returns `progress` unchanged once within [`SNAP_EPSILON`] of
/// the target.
fn advance_progress(progress: f32, open: bool, interval: u64) -> f32 {
    let target: f32 = if open { 0.0 } else { 1.0 };
    let diff = target - progress;
    if diff.abs() <= SNAP_EPSILON {
        return progress;
    }
    let delta = anim::elapsed_fraction(interval, SLIDE_MILLIS);
    if diff > 0.0 {
        (progress + delta).min(target)
    } else {
        (progress - delta).max(target)
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
        let next = advance_progress(self.collapse_progress, self.open, interval);
        if (next - self.collapse_progress).abs() > f32::EPSILON {
            self.collapse_progress = next;
            ctx.request_layout();
        }
        // Keep animating until settled. `advance_progress` guarantees a
        // non-zero step for any non-zero interval, so this terminates.
        if (target - self.collapse_progress).abs() > SNAP_EPSILON {
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
    use super::{SLIDE_MILLIS, advance_progress};

    const MS: u64 = 1_000_000; // one millisecond in nanoseconds
    const FRAME_16MS: u64 = 16 * MS; // a typical ~60 Hz frame

    #[test]
    fn sub_millisecond_frames_still_advance() {
        // Regression for #139: a frame shorter than 1 ms used to truncate the
        // per-frame delta to 0 (integer nanoseconds → milliseconds), so the
        // slide never progressed and re-armed an anim frame forever.
        let next = advance_progress(0.0, false, MS / 2); // 0.5 ms, closing
        assert!(next > 0.0, "a 0.5 ms frame must make progress, got {next}");
    }

    #[test]
    fn closing_climbs_toward_one_and_clamps() {
        let mut p = 0.0_f32;
        for _ in 0..1_000 {
            p = advance_progress(p, false, FRAME_16MS);
        }
        assert!((p - 1.0).abs() < 1e-3, "should reach fully closed, got {p}");
        // Overshoot is clamped: once settled, it stays put.
        assert!((advance_progress(1.0, false, FRAME_16MS) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn opening_falls_toward_zero_and_clamps() {
        let mut p = 1.0_f32;
        for _ in 0..1_000 {
            p = advance_progress(p, true, FRAME_16MS);
        }
        assert!((p - 0.0).abs() < 1e-3, "should reach fully open, got {p}");
        assert!(advance_progress(0.0, true, FRAME_16MS).abs() < f32::EPSILON);
    }

    #[test]
    fn settled_progress_is_unchanged() {
        // No animation when already at target — the caller stops requesting
        // anim frames, which is what lets the loop terminate.
        assert!((advance_progress(1.0, false, FRAME_16MS) - 1.0).abs() < f32::EPSILON);
        assert!(advance_progress(0.0, true, FRAME_16MS).abs() < f32::EPSILON);
    }

    #[test]
    fn one_full_duration_frame_completes_the_slide() {
        // A single frame lasting the whole slide advances a full unit (clamped).
        let full = 250 * MS; // SLIDE_MILLIS is 250.0
        assert!(
            (SLIDE_MILLIS - 250.0).abs() < f32::EPSILON,
            "duration assumption"
        );
        let next = advance_progress(0.0, false, full);
        assert!(
            (next - 1.0).abs() < 1e-4,
            "full-duration frame completes, got {next}"
        );
    }
}
