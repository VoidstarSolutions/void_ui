//! Group box component — a titled container that visually groups related content.
//!
//! There is no custom masonry widget and no view state: [`GroupBox::render`]
//! wraps the child in `sized_box` styling derived from the
//! [`GroupBoxVariant`] and an optional title label.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{GroupBox, GroupBoxVariant, group_box};
