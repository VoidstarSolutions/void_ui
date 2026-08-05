//! Separator / divider component for void-ui.
//!
//! A thin themed line that visually divides content.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::separator;
//!
//! // Horizontal solid (default)
//! separator().render::<(), ()>(&theme);
//!
//! // Horizontal dashed with a label
//! separator().label("Section").render::<(), ()>(&theme);
//!
//! // Vertical dashed with a custom color
//! separator().vertical().dashed().color(theme.palette.accent).render::<(), ()>(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{Separator, SeparatorVariant, separator};
