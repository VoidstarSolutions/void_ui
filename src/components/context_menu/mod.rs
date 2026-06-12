//! Context menu — a rich, theme-driven menu surface.
//!
//! A [`MenuPanel`](widget::MenuPanel) widget renders a list of rows — command
//! actions (with optional leading icon, keyboard-shortcut text, checkable state,
//! sub-title, and disabled state), separators, muted section headers, and
//! hover-open submenus. Build it with [`menu`] / [`item`] / [`submenu`] and
//! render it inline, or wrap content with [`context_menu_area`] to pop it at the
//! cursor on right-click. Rows are selectable by pointer, keyboard, or an
//! accessibility invoke; selecting an enabled row fires its callback.
//!
//! ```ignore
//! use void_ui::components::context_menu::{menu, item, submenu};
//! menu()
//!     .item(item("Copy").icon(IconName::Copy).shortcut("Ctrl+C").on_select(|s: &mut State| s.copy()))
//!     .item(item("Paste").disabled(true).on_select(|s: &mut State| s.paste()))
//!     .separator()
//!     .submenu(submenu("Open Recent").item(item("…").on_select(|s: &mut State| s.open())))
//!     .render(&theme)
//! ```

pub mod area;
#[cfg(feature = "gallery")]
pub mod demo;
pub(crate) mod item_node;
mod view;
pub mod widget;

pub use area::{ContextMenuArea, ContextMenuAction};
pub use view::{
    ContextMenuAreaBuilder, ContextMenuAreaView, Menu, MenuItem, MenuView, Submenu,
    context_menu_area, item, menu, submenu,
};
pub use widget::{MenuAction, MenuPanel, MenuRowSpec};
