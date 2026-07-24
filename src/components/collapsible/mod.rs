//! Animated collapsible section component.
//!
//! Use [`collapsible`] to wrap any body content in a disclosure section with a
//! clickable header. The body animates vertically between its natural height and
//! zero when the open state changes.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { debug: bool, advanced_open: bool }
//! # let state = State { debug: false, advanced_open: false };
//! use void_ui::components::collapsible;
//! use void_ui::components::checkbox;
//! use xilem::view::flex_col;
//!
//! collapsible(
//!     "Advanced options",
//!     flex_col((
//!         checkbox(state.debug, |s: &mut State, checked: bool| s.debug = checked)
//!             .label("Enable debug mode")
//!             .render(&theme),
//!     )),
//!     |s: &mut State| s.advanced_open = !s.advanced_open,
//! )
//! .open(state.advanced_open)
//! .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{Collapsible, CollapsibleView, collapsible};
