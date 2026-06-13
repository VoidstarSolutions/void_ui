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
    DEFAULT_NOTIFICATION_WIDTH, DEFAULT_TIMEOUT, DismissCallback, Notification,
    NotificationLayerView, NotificationPosition, NotificationView, notification,
    notification_layer, notification_overlay, notification_stack,
};
