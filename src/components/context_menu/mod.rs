//! Context menu — a rich, theme-driven menu surface.
//!
//! The foundation is a [`MenuPanel`](widget::MenuPanel) widget that renders a
//! list of rows — command actions (with optional disabled state), separators,
//! and (in later chunks) icon/shortcut/check columns, section labels, and
//! submenus. [`menu`] / [`item`] build it; selecting an enabled row fires that
//! row's callback.
//!
//! ```ignore
//! use void_ui::components::context_menu::{menu, item};
//! menu()
//!     .item(item("Copy").on_select(|s: &mut State| s.copy()))
//!     .item(item("Paste").disabled(true).on_select(|s: &mut State| s.paste()))
//!     .separator()
//!     .item(item("Select All").on_select(|s: &mut State| s.select_all()))
//!     .render(&theme)
//! ```
//!
//! A right-click `context_menu_area` trigger that pops this panel at the cursor
//! lands in a following chunk; for now the panel renders inline.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use view::{Menu, MenuItem, MenuView, item, menu};
pub use widget::{MenuItemSelected, MenuPanel, MenuRowSpec};
