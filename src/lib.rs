//! Void UI components.
//!
//! Product-agnostic Xilem widget library — scanner grid, watch list,
//! instrument context, settings forms, and shared primitives. This crate
//! absorbs Xilem view-layer churn so application code stays insulated
//! from upstream renames. It deliberately has no dependency on
//! `citadel-chart` (or any other Citadel-specific crate) so it can be
//! released independently.
//!
//! ## Design tokens
//!
//! The visual language is sourced from the Tessera P&F prototype and lives
//! in [`theme`]. Components read their colors, sizes, and type stack from a
//! [`Theme`] value owned by the host application.

#![forbid(unsafe_code)]

pub mod components;
pub mod floating;
pub mod gallery;
pub mod layout;
pub mod pointer_inert;
pub mod theme;

pub use components::{
    Button, ButtonVariant, ButtonView, CellAlign, ColumnDef, SelectionState, SidebarItem,
    SidebarItemView, Tooltip, TooltipView, button, data_grid, optional_text_column, sidebar_item,
    text_column, tooltip,
};
pub use floating::{FloatingOverlay, FloatingOverlayView, floating};
pub use gallery::code_block;
pub use pointer_inert::{PointerInert, PointerInertView, pointer_inert};
pub use theme::{Density, FontStack, Palette, Radii, Theme, ThemeVariant, Typography};
