//! Status dot component — a small filled circle for per-row/per-item status
//! (connection state, online/offline, etc.).
//!
//! There is no custom masonry widget and no view state: [`StatusDot::render`]
//! wraps an empty [`crate::label`] in `sized_box` styling.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{StatusDot, status_dot};
