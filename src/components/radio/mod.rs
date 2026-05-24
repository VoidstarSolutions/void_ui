//! Themed radio button component.
//!
//! The xilem [`Radio`] builder lives in [`view`]; the masonry widget that
//! owns the pointer state machine lives in [`widget`]. Group mutual-exclusion
//! is host-managed: each radio in a group receives `active(selected == value)`
//! and fires a callback that writes the new selection into app state.

pub mod demo;
pub mod view;
pub mod widget;

pub use view::{Radio, RadioView, radio};
pub use widget::ThemedRadio;
