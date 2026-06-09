//! Split dropdown button — primary action (left) + chevron menu toggle (right).
//!
//! The menu is hosted in-tree via [`crate::AnchoredOverlay`]: it's a real
//! descendant of the button, anchored below it and free to overflow the
//! button's own box, but otherwise subject to normal paint order and
//! ancestor clipping — confined by whatever scroll viewport or card actually
//! clips it, rather than floating above all other content.
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
