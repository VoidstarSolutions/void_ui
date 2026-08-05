//! Alert message card — themed, typed, with optional title and close button.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! # impl State { fn dismiss(&mut self) {} }
//! use void_ui::alert;
//! use void_ui::components::AlertVariant;
//!
//! alert("This is a notification.").render::<(), ()>(&theme);
//!
//! alert("There was a problem with your request.")
//!     .variant(AlertVariant::Error)
//!     .title("Uh oh! Something went wrong.")
//!     .on_close(|s: &mut State| s.dismiss())
//!     .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{Alert, AlertVariant, alert};
