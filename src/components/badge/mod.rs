//! Badge/pill component — a small inline chip with a themed background,
//! optional semantic accent (shared with [`crate::AlertVariant`]), an optional
//! leading icon, and an optional trailing dismiss button.
//!
//! There is no custom masonry widget and no view state: [`Badge::render`]
//! composes the existing themed [`crate::label()`] (with an optional
//! [`crate::icon()`] and dismiss [`crate::button()`]) in a `flex_row` inside
//! `sized_box` styling.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{Badge, DismissCallback, badge, pill};
