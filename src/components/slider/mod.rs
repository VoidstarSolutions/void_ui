//! Slider component — a draggable single-value or dual-thumb range control.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{RangeSlider, RangeSliderView, Slider, SliderView, range_slider, slider};

/// The value carried by a slider.
///
/// `Single` backs [`slider`] (one draggable thumb); `Range` backs
/// [`range_slider`] (independent low/high thumbs that cannot cross).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliderValue {
    /// A single position in `[min, max]`.
    Single(f64),
    /// A `(low, high)` pair with `low <= high`, both within `[min, max]`.
    Range(f64, f64),
}

/// Action emitted by `SliderWidget` with the slider's new value.
///
/// Fired continuously while a thumb is dragged or a key is held, on
/// click-to-jump within the track, and on accessibility `SetValue` /
/// `Increment` / `Decrement`. The host applies it to its own state; the
/// widget never updates its value fields itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderChanged(pub SliderValue);
