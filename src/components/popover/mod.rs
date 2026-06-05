//! Generic popover component — trigger widget + floating content panel.
//!
//! ```ignore
//! use void_ui::components::popover;
//! popover(
//!     button("Show info").render(&theme),
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
pub use widget::{PopoverClosed, PopoverHost};

/// Where the popover content appears relative to the trigger widget.
///
/// ```text
/// TopStart    TopCenter    TopEnd
/// [  trigger widget  ]
/// BottomStart BottomCenter BottomEnd
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverAnchor {
    /// Below the trigger, left-aligned.
    #[default]
    BottomStart,
    /// Below the trigger, centered.
    BottomCenter,
    /// Below the trigger, right-aligned.
    BottomEnd,
    /// Above the trigger, left-aligned.
    TopStart,
    /// Above the trigger, centered.
    TopCenter,
    /// Above the trigger, right-aligned.
    TopEnd,
}
