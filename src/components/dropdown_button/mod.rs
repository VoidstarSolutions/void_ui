//! Split dropdown button — primary action (left) + chevron menu toggle (right).
//!
//! The menu floats above all other content using masonry's window-level layer
//! infrastructure so it is never clipped by the button's parent container.
//!
//! ```ignore
//! use void_ui::components::dropdown_button;
//! dropdown_button("Save", |s: &mut State| s.save())
//!     .item("Save as…", |s: &mut State| s.save_as())
//!     .item("Export PDF", |s: &mut State| s.export())
//!     .variant(ButtonVariant::Primary)
//!     .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
pub(crate) mod menu_layer;
mod view;
pub mod widget;

pub use view::{DropdownButton, DropdownButtonView, dropdown_button};
pub use widget::ThemedDropdownButton;
