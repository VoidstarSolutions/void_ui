//! Dropdown button — a click-anywhere trigger that opens a menu of items.
//!
//! When a [`crate::overlay_scope`] ancestor is present, the menu is registered
//! into the scope's portal and painted above everything else in the region;
//! otherwise it falls back to being hosted in-tree via
//! [`crate::AnchoredOverlay`], a real descendant of the button, anchored below
//! it and free to overflow the button's own box, but otherwise subject to
//! normal paint order and ancestor clipping — confined by whatever scroll
//! viewport or card actually clips it, rather than floating above all other
//! content.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! # impl State { fn save_as(&mut self) {} fn export(&mut self) {} }
//! use void_ui::components::dropdown_button;
//! use void_ui::components::ButtonVariant;
//! dropdown_button("Save")
//!     .item("Save as…", |s: &mut State| s.save_as())
//!     .item("Export PDF", |s: &mut State| s.export())
//!     .variant(ButtonVariant::Primary)
//!     .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
pub(crate) mod menu_layer;
mod view;
mod widget;

pub use view::{DropdownButton, DropdownButtonView, dropdown_button};
