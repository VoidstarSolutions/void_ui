//! Scroll container component — a clipping viewport with scrollbars on both axes.

pub mod demo;
mod view;
pub mod widget;

pub use view::{ScrollBarVisibility, ScrollContainer, ScrollContainerView, scroll_container};
pub use widget::ScrollView;
