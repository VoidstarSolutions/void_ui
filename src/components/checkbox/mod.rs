//! Checkbox component — a two-state toggle with an optional text label.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{Checkbox, CheckboxView, checkbox};

/// Action emitted by [`widget::CheckboxWidget`] on primary-pointer release,
/// Space, Enter, or an accessibility Click while the widget is focused.
#[derive(Debug, Clone, Copy)]
pub struct CheckboxPress;
