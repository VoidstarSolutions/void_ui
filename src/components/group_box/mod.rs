//! Group box component — a titled container that visually groups related content.
//!
//! There is no custom masonry widget and no view state: [`GroupBox::render`]
//! wraps the child in `sized_box` styling derived from the selected variant
//! and an optional title label.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{GroupBox, NoTitle, TitleState, WithTitle, group_box};
