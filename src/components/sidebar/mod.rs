//! Tessera sidebar navigation item component.
//!
//! A full-width, left-aligned nav row. Use [`sidebar_item`] to build one:
//!
//! ```ignore
//! use void_ui::components::sidebar_item;
//! sidebar_item("Charts", |s: &mut State| s.focused = Section::Charts)
//!     .active(state.focused == Section::Charts)
//!     .render(&theme)
//! ```
//!
//! The widget is exposed publicly so the view's public `Element` associated
//! type can name it without leaking a private type.

pub mod demo;
mod view;
pub mod widget;

pub use view::{SidebarItem, SidebarItemView, sidebar_item};
