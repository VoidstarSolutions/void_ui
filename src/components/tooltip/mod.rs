//! Hover-driven tooltip component.
//!
//! Wraps any child view with a hover-idle delay that pops a themed
//! tooltip surface as a window-level layer:
//!
//! ```ignore
//! use void_ui::components::{button, tooltip};
//! tooltip(
//!     "Reset the chart to defaults",
//!     button("Reset", |_: &mut State| {}).render(&theme),
//! )
//! .render(&theme)
//! ```
//!
//! The widget is exposed publicly so the view's `Element` associated type
//! can name it without leaking a private type.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{DEFAULT_DELAY_MS, Tooltip, TooltipView, tooltip};
