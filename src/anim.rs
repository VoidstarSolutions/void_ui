//! Shared frame-timing helpers for animated widgets.
//!
//! Masonry hands [`Widget::on_anim_frame`] a **nanosecond** interval. Turning
//! that into a useful per-frame step was, until now, reinvented in every
//! animator — in three different ways — and one of those hand-rolls was wrong:
//! `animated_clip` did `interval / 1_000_000` (integer nanoseconds →
//! milliseconds), which truncated the step to `0` for any frame shorter than a
//! millisecond. The animation then never reached its target, re-armed
//! `request_anim_frame()` every tick, and pegged a CPU core even while idle
//! (#139).
//!
//! Centralizing the conversion here is the same move [`crate::components::click`]
//! and [`crate::components::interaction`] already made for the press state
//! machine: one tested definition, so a change (or a bug) lands once instead of
//! drifting across copies.
//!
//! [`elapsed_secs`] guarantees a **non-zero step for any non-zero interval**,
//! which is what lets [`advance_toward`]'s "animate until settled" loop
//! actually terminate.
//!
//! [`Widget::on_anim_frame`]: masonry::core::Widget::on_anim_frame

/// Seconds elapsed during an [`on_anim_frame`] frame of `interval` nanoseconds.
///
/// The interval is clamped to `u32::MAX` ns (~4.3 s) before conversion, so a
/// pathologically long frame (a stalled or backgrounded window) yields a large
/// but finite step rather than overflowing. Never truncates a non-zero interval
/// to zero.
///
/// Use this directly for animations that advance in real time (a spinner's
/// rotation, a pulse's phase). For "advance a value toward a target at a
/// constant rate" use [`advance_toward`].
///
/// [`on_anim_frame`]: masonry::core::Widget::on_anim_frame
pub(crate) fn elapsed_secs(interval: u64) -> f64 {
    let interval_ns = u32::try_from(interval).unwrap_or(u32::MAX);
    f64::from(interval_ns) * 1e-9
}

/// How close an [`AnimatedF32`] driven by [`advance_toward`] must be to its
/// target to count as settled, in the same units the animation operates in
/// (typically a `0.0..=1.0` progress or opacity fraction).
///
/// [`AnimatedF32`]: masonry::widgets::AnimatedF32
pub(crate) const SETTLE_EPSILON: f32 = 1e-4;

/// Advances `anim` toward `target` at a constant velocity that would cross
/// the entire `0.0..=1.0` range in `full_duration_millis`, for a frame of
/// `interval` nanoseconds.
///
/// [`AnimatedF32::move_to`] fixes a rate of `(target - value) / over_millis`
/// for the whole transition at call time, so calling it once with a fixed
/// duration and later reversing direction mid-flight would re-run the full
/// duration over a now-shorter remaining distance — a visible animation-feel
/// change. Rescaling `over_millis` by the current remaining distance
/// (`diff.abs()`) on every call cancels that distance back out of the rate,
/// leaving a constant `1.0 / full_duration_millis` regardless of how far
/// `anim` is from `target` — reproducing the constant-velocity feel a
/// fixed-duration `move_to` call would not.
///
/// Returns [`AnimationStatus::Completed`] once within [`SETTLE_EPSILON`] of
/// `target`, checked before touching `anim` at all so a settled value isn't
/// fed a near-zero `over_millis`.
///
/// `full_duration_millis` must be finite and positive: a non-positive value
/// snaps `anim` straight to `target` (matching the old "complete immediately"
/// behavior for a misconfigured duration), but a NaN value panics via
/// [`AnimatedF32::move_to`]'s own `is_finite` assertion rather than
/// completing gracefully — unlike this crate's fixed-literal durations
/// (`SLIDE_MILLIS`, `FADE_MILLIS`), a caller computing `full_duration_millis`
/// dynamically must keep it finite.
///
/// [`AnimatedF32::move_to`]: masonry::widgets::AnimatedF32::move_to
/// [`AnimationStatus::Completed`]: masonry::widgets::AnimationStatus::Completed
pub(crate) fn advance_toward(
    anim: &mut masonry::widgets::AnimatedF32,
    target: f32,
    full_duration_millis: f32,
    interval: u64,
) -> masonry::widgets::AnimationStatus {
    use masonry::widgets::AnimationStatus;

    let diff = target - anim.value();
    if diff.abs() <= SETTLE_EPSILON {
        return AnimationStatus::Completed;
    }
    anim.move_to(target, full_duration_millis * diff.abs());
    #[expect(
        clippy::cast_possible_truncation,
        reason = "animation progress only needs f32"
    )]
    let by_millis = (elapsed_secs(interval) * 1e3) as f32;
    anim.advance(by_millis)
}

#[cfg(test)]
mod tests {
    use masonry::widgets::{AnimatedF32, AnimationStatus};

    use super::{advance_toward, elapsed_secs};

    const MS: u64 = 1_000_000; // one millisecond in nanoseconds
    const FRAME_16MS: u64 = 16 * MS; // a typical ~60 Hz frame
    const DURATION_MILLIS: f32 = 250.0;

    #[test]
    fn sub_millisecond_frames_still_step() {
        // The #139 regression: an integer ns->ms conversion truncated any frame
        // under 1 ms to a zero step. elapsed_secs works in f64 seconds and is
        // the property advance_toward is built on, so this stays pinned here.
        assert!(elapsed_secs(1_000) > 0.0, "1 µs frame");
    }

    #[test]
    fn seconds_conversion_is_exact_enough() {
        assert!((elapsed_secs(1_000_000_000) - 1.0).abs() < 1e-9, "1 s");
        assert!((elapsed_secs(16 * MS) - 0.016).abs() < 1e-9, "16 ms frame");
        assert!(elapsed_secs(0).abs() < f64::EPSILON, "zero interval");
    }

    #[test]
    fn long_frames_clamp_instead_of_overflowing() {
        // A frame longer than u32::MAX ns (~4.3 s) clamps rather than wrapping.
        let huge = u64::MAX;
        let secs = elapsed_secs(huge);
        assert!(secs.is_finite() && secs > 4.0, "clamped, got {secs}");
    }

    #[test]
    fn sub_millisecond_frame_still_advances() {
        // Same #139 property, at the advance_toward level: a sub-millisecond
        // frame must not truncate to a zero step.
        let mut anim = AnimatedF32::stable(0.0);
        let status = advance_toward(&mut anim, 1.0, DURATION_MILLIS, MS / 2);
        assert!(
            anim.value() > 0.0,
            "a 0.5 ms frame must make progress, got {}",
            anim.value()
        );
        assert_eq!(status, AnimationStatus::Ongoing);
    }

    #[test]
    fn climbs_toward_a_higher_target_and_clamps() {
        let mut anim = AnimatedF32::stable(0.0);
        for _ in 0..1_000 {
            advance_toward(&mut anim, 1.0, DURATION_MILLIS, FRAME_16MS);
        }
        assert!(
            (anim.value() - 1.0).abs() < 1e-3,
            "should reach the target, got {}",
            anim.value()
        );
        // Overshoot is clamped: once settled, it stays put and reports Completed.
        let status = advance_toward(&mut anim, 1.0, DURATION_MILLIS, FRAME_16MS);
        assert_eq!(status, AnimationStatus::Completed);
        assert!((anim.value() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn falls_toward_a_lower_target_and_clamps() {
        let mut anim = AnimatedF32::stable(1.0);
        for _ in 0..1_000 {
            advance_toward(&mut anim, 0.0, DURATION_MILLIS, FRAME_16MS);
        }
        assert!(
            anim.value().abs() < 1e-3,
            "should reach the target, got {}",
            anim.value()
        );
    }

    #[test]
    fn stall_longer_than_duration_snaps_to_target() {
        // A frame interval longer than the configured duration itself
        // (backgrounded window, stalled compositor) means that much real time
        // already elapsed while unrendered, so the transition should complete
        // in one step rather than crawl toward the target over several more
        // frames.
        let mut anim = AnimatedF32::stable(1.0);
        let stalled_frame = 800 * MS; // longer than DURATION_MILLIS (250ms)
        let status = advance_toward(&mut anim, 0.0, DURATION_MILLIS, stalled_frame);
        assert!(
            anim.value().abs() < f32::EPSILON,
            "should snap to 0.0, got {}",
            anim.value()
        );
        assert_eq!(status, AnimationStatus::Completed);
    }

    #[test]
    fn one_full_duration_frame_completes() {
        // A single frame lasting the whole transition advances a full unit.
        let mut anim = AnimatedF32::stable(0.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "DURATION_MILLIS is a small positive test constant"
        )]
        let full = (DURATION_MILLIS as u64) * MS;
        let status = advance_toward(&mut anim, 1.0, DURATION_MILLIS, full);
        assert!(
            (anim.value() - 1.0).abs() < 1e-4,
            "full-duration frame completes, got {}",
            anim.value()
        );
        assert_eq!(status, AnimationStatus::Completed);
    }

    #[test]
    fn settled_value_is_left_untouched() {
        let mut anim = AnimatedF32::stable(1.0);
        let status = advance_toward(&mut anim, 1.0, DURATION_MILLIS, FRAME_16MS);
        assert_eq!(status, AnimationStatus::Completed);
        assert!((anim.value() - 1.0).abs() < f32::EPSILON);
    }
}
