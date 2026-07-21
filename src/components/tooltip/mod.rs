//! Hover-driven tooltip component.
//!
//! Wraps any child view with a hover-idle delay that pops a themed
//! tooltip surface as a window-level layer:
//!
//! ```ignore
//! use void_ui::components::{button, tooltip};
//! tooltip(
//!     "Reset the chart to defaults",
//!     button(|_: &mut State| {}).label("Reset").render(&theme),
//! )
//! .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{DEFAULT_DELAY_MS, Tooltip, TooltipRow, TooltipView, tooltip, tooltip_rows};
