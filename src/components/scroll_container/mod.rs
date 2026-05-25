//! Scroll container component — a clipping viewport with scrollbars on both axes.

pub mod demo;
pub mod widget;
mod view;

pub use view::{ScrollBarVisibility, ScrollContainer, ScrollContainerView, scroll_container};
pub use widget::ScrollView;
