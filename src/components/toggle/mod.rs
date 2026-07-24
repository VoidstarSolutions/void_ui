//! A pill-shaped on/off switch control.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { on: bool }
//! # let state = State { on: false };
//! use void_ui::components::toggle;
//! toggle(state.on, |s: &mut State, _| s.on = !s.on)
//!     .label("Enable feature")
//!     .render(&theme)
//! # ;
//! ```

mod view;
mod widget;

#[cfg(feature = "gallery")]
pub mod demo;

pub use view::{Toggle, ToggleView, toggle};

/// Action emitted by `ToggleWidget` on activation.
#[derive(Debug, Clone)]
pub struct TogglePress;
