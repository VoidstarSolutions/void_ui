//! Card component — a themed rounded, padded surface for floating panels.
//!
//! A lighter-weight sibling of [`crate::group_box()`] with no title bar. There
//! is no custom masonry widget and no view state: [`Card::render`] wraps the
//! child in `sized_box` styling.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{Card, card};
