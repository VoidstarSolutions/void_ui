//! Separator / divider component for void-ui.
//!
//! A thin themed line that visually divides content.
//!
//! ```ignore
//! use void_ui::separator;
//!
//! // Horizontal solid (default)
//! separator().render(&theme)
//!
//! // Horizontal dashed with a label
//! separator().label("Section").render(&theme)
//!
//! // Vertical dashed with a custom color
//! separator().vertical().dashed().color(theme.palette.teal).render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub(super) mod widget;

pub use view::{Orientation, Separator, SeparatorStyle, separator};
