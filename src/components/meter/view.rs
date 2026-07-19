//! Xilem view for the meter component.

use masonry::peniko::Color;

/// How the fill portion of a [`crate::meter`] is painted.
///
/// [`MeterFill::Gradient`] spans the *full track width* regardless of the
/// current fraction — the fill rect is a window onto a fixed gradient, so a
/// given x-coordinate is always the same color no matter how much of the
/// track is filled. See `full_track_gradient` in `widget.rs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeterFill {
    /// A flat fill color.
    Solid(Color),
    /// A two-stop gradient: `from` at the track's left edge (fraction 0.0),
    /// `to` at its right edge (fraction 1.0).
    Gradient(Color, Color),
}
