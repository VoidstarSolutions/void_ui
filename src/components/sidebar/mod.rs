//! Sidebar navigation component — items, collapse button, and animated panel.
//!
//! Use [`sidebar_item`] for individual nav rows and [`sidebar_panel`] to wrap
//! them in a collapsible container:
//!
//! ```ignore
//! use void_ui::components::{sidebar_item, sidebar_panel, sidebar_collapse_button};
//! sidebar_panel(
//!     flex_col((
//!         sidebar_collapse_button(|s: &mut State| s.sidebar_collapsed = true).render(&theme),
//!         sidebar_item("Charts", |s: &mut State| s.focused = Section::Charts)
//!             .active(state.focused == Section::Charts)
//!             .render(&theme),
//!     ))
//!     .cross_axis_alignment(CrossAxisAlignment::Stretch)
//!     .gap(Length::px(2.0)),
//!     state.sidebar_collapsed,
//! )
//! .render(&theme)
//! ```
//!
//! The widget types are exposed publicly so view `Element` associated types
//! can name them without leaking private types.

mod collapse_button;
pub mod demo;
pub(crate) mod panel_widget;
mod panel_view;
mod view;
pub mod widget;

pub use collapse_button::{
    SidebarCollapseButton, SidebarCollapseButtonView, sidebar_collapse_button,
};
pub use panel_view::{SidebarPanel, SidebarPanelView, sidebar_panel};
pub use view::{SidebarItem, SidebarItemView, sidebar_item};
