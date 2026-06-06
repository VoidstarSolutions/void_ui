//! Slider component — a draggable single-value range control.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{Slider, SliderView, slider};

/// Action emitted by [`widget::SliderWidget`] with the slider's new value.
///
/// Fired continuously while the thumb is dragged or a key is held, on
/// click-to-jump within the track, and on accessibility `SetValue` /
/// `Increment` / `Decrement`. The host applies it to its own state; the
/// widget never updates `value` itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderChanged(pub f64);
