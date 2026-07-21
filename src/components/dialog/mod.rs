//! Generic dialog component — content positioned above everything else,
//! centered horizontally and a quarter of the way down the enclosing
//! [`crate::overlay_scope`].
//!
//! ```ignore
//! use void_ui::components::dialog;
//! dialog(state.open, my_content_view)
//!     .show_close_button()
//!     .on_close(|state: &mut State| state.open = false)
//!     .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{Dialog, DialogView, dialog};
