//! Notification (toast) card — themed, typed, with close button and
//! optional auto-dismiss timeout.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! # impl State { fn dismiss(&mut self) {} }
//! use std::time::Duration;
//! use void_ui::components::notification::notification;
//! use void_ui::AlertVariant;
//!
//! notification("Saved successfully.")
//!     .variant(AlertVariant::Success)
//!     .on_close(|s: &mut State| s.dismiss())
//!     .with_timeout(Duration::from_secs(3))
//!     .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{
    DEFAULT_NOTIFICATION_WIDTH, DEFAULT_TIMEOUT, Notification, NotificationLayerView,
    NotificationPosition, NotificationView, OnClose, notification, notification_layer,
    notification_overlay, notification_stack,
};
