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

impl PopoverAnchor {
    /// Compute the content's local-coordinate origin given the trigger's and
    /// content's measured sizes and this anchor.
    #[must_use]
    pub(crate) fn child_offset(self, trigger: masonry::kurbo::Size, content: masonry::kurbo::Size) -> masonry::kurbo::Point {
        use masonry::kurbo::Point;
        match self {
            Self::BottomStart => Point::new(0.0, trigger.height),
            Self::BottomCenter => Point::new((trigger.width - content.width) / 2.0, trigger.height),
            Self::BottomEnd => Point::new(trigger.width - content.width, trigger.height),
            Self::TopStart => Point::new(0.0, -content.height),
            Self::TopCenter => Point::new((trigger.width - content.width) / 2.0, -content.height),
            Self::TopEnd => Point::new(trigger.width - content.width, -content.height),
        }
    }
}
