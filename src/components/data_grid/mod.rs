//! `data_grid` — a virtualized, theme-driven table view for very large,
//! append-only data streams.
//!
//! The grid is read-only (no cell editing) with row-only selection,
//! TSV clipboard copy, and single-column sorting via clickable headers.
//! The widget is generic over the row type so the same grid can browse
//! synthetic tick streams (see [`demo::DemoTick`]), event logs, or
//! any other in-memory `&[R]` exposed by the host's app state.
//!
//! Backed by masonry's [`VirtualScroll`][masonry::widgets::VirtualScroll]
//! for row virtualization. Mostly composed of xilem stock, with four
//! small custom masonry wrappers: [`copy_shortcut::CopyOnShortcut`]
//! (catches Ctrl/Cmd+C and dumps a TSV payload to the clipboard),
//! [`row_click::RowClickable`] (emits modifier-aware row clicks for
//! selection), [`header_click::HeaderClickable`] (emits a plain click
//! on a column header to cycle its sort), and
//! [`overflow_warn::OverflowWarn`] (one-shot `tracing::warn!` when the
//! viewport is narrower than the sum of column widths).
//!
//! Entry points: [`view::data_grid`] for the xilem view,
//! [`column::ColumnDef`] for the per-column contract,
//! [`selection::SelectionState`] for the selection model,
//! [`sort::SortState`] for the sort model.
//!
//! ## Sorting
//!
//! Single-column sorting is driven by [`sort::SortState`], held by the
//! host and read by the grid each frame. Clicking a sortable column
//! header cycles ascending → descending → unsorted; the active column
//! shows a ▲/▼ arrow, and sortable headers highlight on hover. A column
//! is sortable only if its [`ColumnDef`] carries a comparator
//! (see [`column::ColumnDef::sortable_by_key`]) — the comparator orders
//! the row's *underlying* value, independent of the cell's display
//! formatting, so a `"$100.00"` price column sorts numerically rather
//! than lexicographically. Selection tracks *source* row indices, so it
//! stays attached to the same data rows across sort changes.
//!
//! ## Known limitations (v1)
//!
//! - **Shift-extend selection while sorted** fills an inclusive range in
//!   *source-index* space, so it does not follow the visually contiguous
//!   range between anchor and target under a non-identity sort. Plain
//!   click and ctrl/cmd-toggle are order-independent and unaffected.
//! - **Clipboard copy** emits the selection in ascending source-index
//!   order, not the on-screen (sorted) order.
//! - Sorting is single-column — there is no multi-column / tiebreak sort.
//! - Columns are fixed-width; there is no horizontal fill, resize, or
//!   scroll syncing yet.

pub mod column;
pub mod copy_shortcut;
pub mod demo;
pub mod header_click;
pub mod overflow_warn;
pub mod row_click;
pub mod selection;
pub mod sort;
mod view;

pub use column::{CellAlign, ColumnDef, RowComparator, optional_text_column, text_column};
pub use copy_shortcut::CopyOnShortcut;
pub use header_click::{HeaderClickable, HeaderClicked};
pub use overflow_warn::OverflowWarn;
pub use row_click::{RowClickAction, RowClickable};
pub use selection::SelectionState;
pub use sort::{SortDirection, SortState};
pub use view::DataGrid;
