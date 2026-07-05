//! Generic popover component — trigger widget + floating content panel.
//!
//! ```ignore
//! use void_ui::components::popover;
//! popover(
//!     button(|_| {}).label("Show info").render(&theme),
//!     label("Here is some info.").render(&theme),
//! )
//! .anchor(PopoverAnchor::BottomStart)
//! .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{Popover, PopoverView, popover};
pub use widget::{PopoverHost, PopoverOpenChanged};

/// Compatibility alias for the pre-consolidation name of
/// [`crate::overlay::OverlayAnchor`]. Prefer `OverlayAnchor` in new code.
pub use crate::overlay::OverlayAnchor as PopoverAnchor;
