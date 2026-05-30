//! Void UI components.
//!
//! A general-purpose Xilem/Masonry component library: buttons, data grids,
//! sidebars, overlays, and shared layout primitives. The crate absorbs Xilem
//! view-layer churn so application code stays insulated from upstream
//! renames, and stays product-agnostic so it can be reused across
//! independent UIs.
//!
//! ## Design tokens
//!
//! Components read their colors, sizes, and type stack from a [`Theme`]
//! value owned by the host application; see [`theme`] for the primitives.

#![forbid(unsafe_code)]

pub mod components;
pub mod floating;
pub mod gallery;
pub mod layout;
pub mod pointer_inert;
pub mod theme;

pub use components::{
    Button, ButtonVariant, ButtonView, CellAlign, ColumnDef, DataGrid, FilterState,
    ScrollContainer, ScrollContainerView, SelectionState, SidebarItem, SidebarItemView,
    SortDirection, SortState, Tooltip, TooltipView, button, colored_text_column, data_grid,
    filtered_indices, optional_text_column, scroll_container, sidebar_item, text_column, tooltip,
};
pub use floating::{FloatingOverlay, FloatingOverlayView, floating, interactive_floating};
pub use gallery::code_block;
pub use pointer_inert::{PointerInert, PointerInertView, pointer_inert};
pub use theme::{Density, FontStack, Palette, Radii, Theme, ThemeVariant, Typography};
