//! Animated collapsible section component.
//!
//! Use [`collapsible`] to wrap any body content in a disclosure section with a
//! clickable header. The body animates vertically between its natural height and
//! zero when the open state changes.
//!
//! ```ignore
//! use void_ui::components::collapsible;
//! collapsible(
//!     "Advanced options",
//!     flex_col((
//!         checkbox("Enable debug mode", |s: &mut State| s.debug = !s.debug)
//!             .checked(s.debug)
//!             .render(&theme),
//!     )),
//!     |s: &mut State| s.advanced_open = !s.advanced_open,
//! )
//! .open(state.advanced_open)
//! .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
pub mod widget;
mod view;

pub use view::{Collapsible, CollapsibleView, collapsible};
