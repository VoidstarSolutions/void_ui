//! `data_grid` — a virtualized, theme-driven table view for very large,
//! append-only data streams.
//!
//! v1 is read-only with row-only selection and TSV clipboard copy. The
//! widget is generic over the row type so the same grid can browse
//! synthetic tick streams (see [`demo::DemoTick`]), event logs, or
//! any other in-memory `&[R]` exposed by the host's app state.
//!
//! Backed by masonry's [`VirtualScroll`][masonry::widgets::VirtualScroll]
//! for row virtualization. Mostly composed of xilem stock, with three
//! small custom masonry wrappers: [`copy_shortcut::CopyOnShortcut`]
//! (catches Ctrl/Cmd+C and dumps a TSV payload to the clipboard),
//! [`row_click::RowClickable`] (emits modifier-aware row clicks for
//! selection), and [`overflow_warn::OverflowWarn`] (one-shot
//! `tracing::warn!` when the viewport is narrower than the sum of
//! column widths).
//!
//! Entry points: [`view::data_grid`] for the xilem view,
//! [`column::ColumnDef`] for the per-column contract,
//! [`selection::SelectionState`] for the selection model.

pub mod column;
pub mod copy_shortcut;
pub mod demo;
pub mod overflow_warn;
pub mod row_click;
pub mod selection;
mod view;

pub use column::{CellAlign, ColumnDef, optional_text_column, text_column};
pub use copy_shortcut::CopyOnShortcut;
pub use overflow_warn::OverflowWarn;
pub use row_click::{RowClickAction, RowClickable};
pub use selection::SelectionState;
pub use view::data_grid;
