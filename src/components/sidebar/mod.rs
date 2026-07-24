//! Sidebar navigation component — items and animated collapsible panel.
//!
//! Use [`sidebar_item`] for individual nav rows and [`sidebar_panel`] to wrap
//! them in a collapsible container with a built-in toggle strip:
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # #[derive(PartialEq)]
//! # enum Section { Dashboard, Charts }
//! # struct State { focused: Section, sidebar_collapsed: bool }
//! # let state = State { focused: Section::Dashboard, sidebar_collapsed: false };
//! use void_ui::components::{sidebar_item, sidebar_panel};
//! use xilem::view::{CrossAxisAlignment, flex_col};
//! # use xilem::style::Style as _;
//! use masonry::layout::Length;
//!
//! sidebar_panel(
//!     flex_col((
//!         sidebar_item("Charts", |s: &mut State| s.focused = Section::Charts)
//!             .selected(state.focused == Section::Charts)
//!             .render(&theme),
//!     ))
//!     .cross_axis_alignment(CrossAxisAlignment::Stretch)
//!     .gap(Length::px(2.0)),
//!     |s: &mut State| s.sidebar_collapsed = !s.sidebar_collapsed,
//! )
//! .collapsed(state.sidebar_collapsed)
//! .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod nav_view;
mod nav_widget;
mod panel_view;
mod panel_widget;
mod view;
mod widget;

pub use nav_view::{SidebarNav, SidebarNavItem, SidebarNavView, sidebar_nav};
pub use panel_view::{SidebarPanel, SidebarPanelView, sidebar_panel};
pub use view::{SidebarItem, SidebarItemView, sidebar_item};
