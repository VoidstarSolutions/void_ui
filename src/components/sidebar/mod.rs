//! Sidebar navigation component — items and animated collapsible panel.
//!
//! Use [`sidebar_item`] for individual nav rows and [`sidebar_panel`] to wrap
//! them in a collapsible container with a built-in toggle strip:
//!
//! ```ignore
//! use void_ui::components::{sidebar_item, sidebar_panel};
//! sidebar_panel(
//!     flex_col((
//!         sidebar_item("Charts", |s: &mut State| s.focused = Section::Charts)
//!             .active(state.focused == Section::Charts)
//!             .render(&theme),
//!     ))
//!     .cross_axis_alignment(CrossAxisAlignment::Stretch)
//!     .gap(Length::px(2.0)),
//!     |s: &mut State| s.sidebar_collapsed = !s.sidebar_collapsed,
//! )
//! .collapsed(state.sidebar_collapsed)
//! .render(&theme)
//! ```
//!
//! The widget type is exposed publicly so view `Element` associated types can
//! name it without leaking a private type.

pub mod demo;
pub(crate) mod panel_widget;
mod panel_view;
mod view;
pub mod widget;

pub use panel_view::{SidebarPanel, SidebarPanelView, sidebar_panel};
pub use view::{SidebarItem, SidebarItemView, sidebar_item};
