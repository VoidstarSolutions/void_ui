//! Notification (toast) card — themed, typed, with close button and
//! optional auto-dismiss timeout.
//!
//! ```ignore
//! use std::time::Duration;
//! use void_ui::components::notification::notification;
//! use void_ui::AlertVariant;
//!
//! notification("Saved successfully.")
//!     .variant(AlertVariant::Success)
//!     .timeout(Duration::from_secs(3))
//!     .on_close(|s: &mut State| s.dismiss())
//!     .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{
    DEFAULT_TIMEOUT, DismissCallback, Notification, NotificationOverlayView, NotificationPosition,
    NotificationView, notification, notification_overlay, notification_stack,
};
