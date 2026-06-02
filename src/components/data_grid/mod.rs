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
//! header to emit a sort-cycle request).
//!
//! Entry points: [`view::data_grid`] for the xilem view,
//! [`column::ColumnDef`] for the per-column contract,
//! [`selection::SelectionState`] for the selection model,
//! [`sort::SortState`] for the sort model.
//!
//! ## The host owns row order (sorting *and* filtering)
//!
//! This grid is presentation-only: **the host owns the row order**, and
//! the grid renders rows in exactly the order it is handed. Both sorting
//! and filtering follow one contract — the grid emits an *intent* (a
//! header-click cycle, a filter-input edit) and the host recomputes its
//! ordered/filtered view and serves it back. The grid never reorders or
//! hides data itself. This is the prevailing data-grid architecture
//! (AG Grid's server-side row model, Kendo, `TanStack` Table's
//! `manualSorting`/`manualFiltering`) and the universal Rust-GUI idiom
//! (egui, gpui-component, xilem all keep ordering app-side); it also
//! keeps the component product-agnostic, since comparators and predicates
//! are domain logic that lives with the host's data.
//!
//! The host composes the two stages in the canonical order — **filter,
//! then sort** — using the two mirror helpers
//! [`filtered_indices`](filter::filtered_indices) and
//! [`sort_indices`](sort::sort_indices) over an index list, then
//! materializes the surviving rows. The grid virtualizes over that
//! ordered slice.
//!
//! ## Sorting
//!
//! Single-column sorting is driven by [`sort::SortState`], held by the
//! host. Clicking a sortable column header invokes the
//! [`DataGrid::sort`](view::DataGrid::sort) callback with the column; the
//! host cycles its `SortState` (ascending → descending → unsorted, via
//! [`SortState::cycle`](sort::SortState::cycle)) and re-derives its view
//! with [`sort_indices`](sort::sort_indices). The active column shows a
//! ▲/▼ arrow, and sortable headers highlight on hover. A column is
//! sortable only if its [`ColumnDef`] carries a comparator (see
//! [`column::ColumnDef::sortable_by_key`]) — the comparator orders the
//! row's *underlying* value, independent of the cell's display
//! formatting, so a `"$100.00"` price column sorts numerically rather
//! than lexicographically.
//!
//! ## Selection is keyed by stable row id
//!
//! Selection ([`selection::SelectionState`]) is keyed by a **stable row
//! id** supplied via [`DataGrid::row_id`](view::DataGrid::row_id) — the
//! `getRowId` contract from `TanStack`/AG Grid/Kendo — *not* by slice
//! position. Because the host reorders the slice when sorting/filtering,
//! a positional key would point at a different row after any reorder
//! (the documented index-keying bug); an id key makes the selection
//! follow its rows for free. If no `row_id` is supplied the grid falls
//! back to slice position, which is correct only for a static, unsorted,
//! unfiltered grid.
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
//! - **Shift-extend across not-yet-loaded rows.** Shift-extend now
//!   follows the *visual* order (the grid resolves the inclusive id span
//!   from the ordered slice the host serves), so it is correct whenever
//!   both the anchor and target rows are present in that slice. If the
//!   anchor was filtered out of the current view, the range has no
//!   on-screen start and shift degrades to a single select. (This
//!   supersedes the earlier "shift-extend ignores sort order" limitation,
//!   which the host-owns-order + stable-id model fixed.)
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
mod single_child;
pub mod sort;
pub mod view;
pub mod width;

pub use column::{
    CellAlign, ColumnDef, ColumnId, RowComparator, RowFilter, colored_text_column,
    optional_text_column, text_column,
};
pub use column_strip::{ColumnResize, ColumnStrip, SeparatorStyle};
pub use copy_shortcut::CopyOnShortcut;
pub use filter::{FilterState, filtered_indices};
pub use header_click::{HeaderClickable, HeaderClicked};
pub use row_click::{RowClickAction, RowClickable};
pub use selection::SelectionState;
pub use sort::{SortDirection, SortState, sort_indices};
pub use view::{DataGrid, data_grid};
pub use width::{ColumnWidths, MIN_COLUMN_WIDTH};
