//! Generic dialog component — content positioned above everything else,
//! centered horizontally and a quarter of the way down the enclosing
//! [`crate::overlay_scope`].
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { open: bool }
//! # let state = State { open: false };
//! # let my_content_view = void_ui::label("content").render(&theme);
//! use void_ui::components::dialog;
//! dialog(state.open, my_content_view)
//!     .show_close_button()
//!     .on_close(|s: &mut State| s.open = false)
//!     .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{Dialog, DialogView, dialog};
