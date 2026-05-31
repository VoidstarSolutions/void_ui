//! `data_grid` — a virtualized, theme-driven table view for very large,
//! append-only data streams.
//!
//! The grid is read-only (no cell editing) with row-only selection,
//! TSV clipboard copy, single-column sorting via clickable headers,
//! per-column filtering, horizontal scrolling for wide tables, and
//! drag-to-resize columns. The widget is generic over the row type so
//! the same grid can browse synthetic tick streams (see
//! [`demo::DemoTick`]), event logs, or any other in-memory `&[R]`
//! exposed by the host's app state.
//!
//! Backed by masonry's [`VirtualScroll`][masonry::widgets::VirtualScroll]
//! for row virtualization, and wrapped in a horizontal-only
//! [`scroll_container`](crate::components::scroll_container) so columns
//! wider than the viewport are reachable. Each row (header, body, filter)
//! is a [`column_strip::ColumnStrip`] — a multi-child widget that places
//! cells at authoritative x-positions from a shared width list, so the
//! three rows share column geometry *by construction* (independent
//! flex-rows only line up by coincidence). A resizable `ColumnStrip`
//! also owns its column-boundary drag zones, like masonry's `Split` owns
//! its bar. The rest is xilem stock plus three small custom masonry
//! wrappers: [`copy_shortcut::CopyOnShortcut`] (catches Ctrl/Cmd+C and
//! dumps a TSV payload to the clipboard), [`row_click::RowClickable`]
//! (emits modifier-aware row clicks for selection), and
//! [`header_click::HeaderClickable`] (emits a plain click on a column
//! header to cycle its sort).
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
//! ## Resizing & widths
//!
//! Each column starts at its [`ColumnDef`] width. A
//! [`width::ColumnWidths`] override map (held by the host, supplied via
//! [`DataGrid::column_widths`](view::DataGrid::column_widths)) gives the
//! *effective* width per column — resolved once and shared by the
//! header, filter inputs, body cells, and the total content width. When
//! [`DataGrid::on_column_resize`](view::DataGrid::on_column_resize) is
//! set, the header [`column_strip::ColumnStrip`] becomes resizable: it
//! hit-tests a grab zone at each column's trailing edge (drawing a
//! separator there) and reports the column's proposed absolute width as
//! it's dragged; the host stores it in `ColumnWidths` (clamped to
//! [`width::MIN_COLUMN_WIDTH`]) and passes the snapshot back.
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
//! - Columns start at their `ColumnDef` width and can be drag-resized
//!   (see [`width::ColumnWidths`]); the grid scrolls horizontally when
//!   the total exceeds the viewport. Width auto-fit (double-click to fit
//!   content) is not implemented yet.

pub mod column;
pub mod column_strip;
pub mod copy_shortcut;
pub mod demo;
pub mod filter;
pub mod header_click;
pub mod row_click;
pub mod selection;
pub mod sort;
pub mod view;
pub mod width;

pub use column::{
    CellAlign, ColumnDef, RowComparator, RowFilter, colored_text_column, optional_text_column,
    text_column,
};
pub use column_strip::{ColumnResize, ColumnStrip, SeparatorStyle};
pub use copy_shortcut::CopyOnShortcut;
pub use filter::{FilterState, filtered_indices};
pub use header_click::{HeaderClickable, HeaderClicked};
pub use row_click::{RowClickAction, RowClickable};
pub use selection::SelectionState;
pub use sort::{SortDirection, SortState};
pub use view::{DataGrid, data_grid};
pub use width::{ColumnWidths, MIN_COLUMN_WIDTH};
