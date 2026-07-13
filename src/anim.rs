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
//! Both helpers guarantee a **non-zero step for any non-zero interval**, which
//! is what lets an "animate until settled" loop actually terminate.
//!
//! [`Widget::on_anim_frame`]: masonry::core::Widget::on_anim_frame

/// Seconds elapsed during an [`on_anim_frame`] frame of `interval` nanoseconds.
///
/// The interval is clamped to `u32::MAX` ns (~4.3 s) before conversion, so a
/// pathologically long frame (a stalled or backgrounded window) yields a large
/// but finite step rather than overflowing. Never truncates a non-zero interval
/// to zero.
///
/// Use this for animations that advance in real time (a spinner's rotation, a
/// pulse's phase). For "fraction of a fixed-duration transition" use
/// [`elapsed_fraction`].
///
/// [`on_anim_frame`]: masonry::core::Widget::on_anim_frame
pub(crate) fn elapsed_secs(interval: u64) -> f64 {
    let interval_ns = u32::try_from(interval).unwrap_or(u32::MAX);
    f64::from(interval_ns) * 1e-9
}

/// The fraction of a `duration_millis`-long transition that elapsed during an
/// [`on_anim_frame`] frame of `interval` nanoseconds.
///
/// Computed in `f64` and only narrowed at the end, so sub-millisecond frames
/// still produce a non-zero step — the property whose absence caused #139.
/// Callers add this to a `0.0..=1.0` progress value and clamp to the target.
///
/// `duration_millis` must be positive; a non-positive duration yields a step of
/// `1.0` (i.e. "complete immediately"), which keeps callers terminating rather
/// than looping forever on a misconfigured duration.
///
/// [`on_anim_frame`]: masonry::core::Widget::on_anim_frame
pub(crate) fn elapsed_fraction(interval: u64, duration_millis: f32) -> f32 {
    if duration_millis <= 0.0 {
        return 1.0;
    }
    let elapsed_millis = elapsed_secs(interval) * 1e3;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "animation progress only needs f32"
    )]
    let fraction = (elapsed_millis / f64::from(duration_millis)) as f32;
    fraction
}

#[cfg(test)]
mod tests {
    use super::{elapsed_fraction, elapsed_secs};

    const MS: u64 = 1_000_000; // one millisecond in nanoseconds

    #[test]
    fn sub_millisecond_frames_still_step() {
        // The #139 regression: an integer ns→ms conversion truncated any frame
        // under 1 ms to a zero step, so the animation never finished and the
        // widget re-armed an anim frame forever.
        assert!(elapsed_fraction(MS / 2, 250.0) > 0.0, "0.5 ms frame");
        assert!(elapsed_fraction(1_000, 250.0) > 0.0, "1 µs frame");
        assert!(elapsed_secs(1_000) > 0.0, "1 µs frame in seconds");
    }

    #[test]
    fn seconds_conversion_is_exact_enough() {
        assert!((elapsed_secs(1_000_000_000) - 1.0).abs() < 1e-9, "1 s");
        assert!((elapsed_secs(16 * MS) - 0.016).abs() < 1e-9, "16 ms frame");
        assert!(elapsed_secs(0).abs() < f64::EPSILON, "zero interval");
    }

    #[test]
    fn a_full_duration_frame_is_one_whole_step() {
        // A frame lasting the entire transition advances a full unit.
        assert!((elapsed_fraction(250 * MS, 250.0) - 1.0).abs() < 1e-6);
        // Half the duration advances half.
        assert!((elapsed_fraction(125 * MS, 250.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn long_frames_clamp_instead_of_overflowing() {
        // A frame longer than u32::MAX ns (~4.3 s) clamps rather than wrapping.
        let huge = u64::MAX;
        let secs = elapsed_secs(huge);
        assert!(secs.is_finite() && secs > 4.0, "clamped, got {secs}");
    }

    #[test]
    fn non_positive_duration_completes_immediately() {
        // Rather than dividing by zero and looping forever on a misconfigured
        // duration, report the transition as complete.
        assert!((elapsed_fraction(16 * MS, 0.0) - 1.0).abs() < f32::EPSILON);
        assert!((elapsed_fraction(16 * MS, -5.0) - 1.0).abs() < f32::EPSILON);
    }
}
