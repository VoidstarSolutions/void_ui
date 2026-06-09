//! Animated loading spinner component.
//!
//! A standalone, continuously-animating arc that conveys background activity.
//! The widget drives its own animation loop — no host state required.
//!
//! ```ignore
//! use void_ui::spinner;
//!
//! spinner().render(&theme)
//! spinner().color(theme.palette.teal).size(24.0).render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{Spinner, SpinnerView, spinner};
pub use widget::SpinnerWidget;
