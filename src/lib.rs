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
    Button, ButtonGroup, ButtonVariant, ButtonView, CellAlign, ColumnDef, Label, LabelAlignment,
    ScrollContainer, ScrollContainerView, SelectionState, SidebarItem, SidebarItemView, Tooltip,
    TooltipView, button, button_group, data_grid, label, optional_text_column, scroll_container,
    sidebar_item, text_column, toggle_button_group, tooltip,
};
pub use floating::{FloatingOverlay, FloatingOverlayView, floating, interactive_floating};
pub use gallery::code_block;
pub use pointer_inert::{PointerInert, PointerInertView, pointer_inert};
pub use theme::{Density, FontStack, Palette, Radii, Theme, ThemeVariant, Typography};
