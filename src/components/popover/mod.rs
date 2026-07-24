//! Generic popover component — trigger widget + floating content panel.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! use void_ui::components::popover;
//! use void_ui::components::PopoverAnchor;
//! use void_ui::{button, label};
//!
//! popover(
//!     button(|_: &mut State| {}).label("Show info").render(&theme),
//!     label("Here is some info.").render(&theme),
//! )
//! .anchor(PopoverAnchor::BottomStart)
//! .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{Popover, PopoverView, popover};
pub use widget::PopoverOpenChanged;

/// Compatibility alias for the pre-consolidation name of
/// [`crate::overlay::OverlayAnchor`]. Prefer `OverlayAnchor` in new code.
pub use crate::overlay::OverlayAnchor as PopoverAnchor;
