//! Badge/pill component — a small inline chip with a themed background and
//! optional semantic accent (shared with [`crate::AlertVariant`]).
//!
//! There is no custom masonry widget and no view state: [`Badge::render`]
//! composes the existing themed [`crate::label()`] inside `sized_box` styling.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{Badge, badge, pill};
