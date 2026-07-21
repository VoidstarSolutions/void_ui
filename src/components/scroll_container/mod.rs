//! Scroll container component — a clipping viewport with scrollbars on both axes.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub(crate) mod widget;

pub use view::{ScrollBarVisibility, ScrollContainer, ScrollContainerView, scroll_container};
