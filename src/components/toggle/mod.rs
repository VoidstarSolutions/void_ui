//! A pill-shaped on/off switch control.
//!
//! ```ignore
//! use void_ui::components::toggle;
//! toggle(state.on, |s: &mut State| s.on = !s.on)
//!     .label("Enable feature")
//!     .render(&theme)
//! ```

pub mod view;
pub mod widget;

#[cfg(feature = "gallery")]
pub mod demo;

pub use view::{Toggle, ToggleView, toggle};

/// Action emitted by [`ToggleWidget`](widget::ToggleWidget) on activation.
#[derive(Debug, Clone)]
pub struct TogglePress;
