//! `data_grid` — a virtualized, theme-driven table view for very large,
//! append-only data streams.
//!
//! The grid is read-only (no cell editing) with row-only selection,
//! TSV clipboard copy, single- and multi-column (tiebreak) sorting via
//! clickable headers, per-column filtering, horizontal scrolling for wide
//! tables, drag-to-resize, and column show/hide + reorder. The widget is
//! generic over the row type so
//! the same grid can browse synthetic tick streams (see
//! `demo::DemoTick`, behind the `gallery` feature), event logs, or any other in-memory `&[R]`
//! exposed by the host's app state.
//!
//! Backed by masonry's [`VirtualScroll`][masonry::widgets::VirtualScroll]
//! for row virtualization, and wrapped in a horizontal-only
//! [`scroll_container`](crate::components::scroll_container()) so columns
//! wider than the viewport are reachable. That `VirtualScroll` wiring now
//! lives one layer down in the crate-internal `collection` substrate: row
//! virtualization, selection, scroll-to, and keyboard nav are provided by
//! that substrate (the grid's body is a `collection_body`), while
//! `data_grid` owns columns, the header, sort/filter, horizontal scroll,
//! and copy. Each row (header, body, filter)
//! is a [`column_strip::ColumnStrip`] — a multi-child widget that places
//! cells at authoritative x-positions from a shared width list, so the
//! three rows share column geometry *by construction* (independent
//! flex-rows only line up by coincidence). A resizable `ColumnStrip`
//! also owns its column-boundary drag zones, like masonry's `Split` owns
//! its bar. The rest is xilem stock plus three small custom masonry
//! wrappers: [`copy_shortcut::CopyOnShortcut`] (catches Ctrl/Cmd+C and
//! dumps a TSV payload to the clipboard), `RowClickable`
//! (emits modifier-aware row clicks for selection), and
//! [`header_click::HeaderClickable`] (emits a plain click on a column
//! header to emit a sort-cycle request).
//!
//! Entry points: [`data_grid`] for the xilem view,
//! [`column::ColumnDef`] for the per-column contract,
//! [`SelectionState`] for the selection model,
//! [`sort::SortState`] for the sort model,
//! [`ScrollState`] for programmatic scroll-to-row.
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
//! [`filtered_indices`] and
//! [`sort_indices`] over an index list, then
//! materializes the surviving rows. The grid virtualizes over that
//! ordered slice.
//!
//! ## Sorting (single + multi-column)
//!
//! Sorting is driven by [`sort::SortState`] — an ordered list of
//! `(column, direction)` levels (position = priority), held by the host.
//! Clicking a sortable header invokes the
//! [`DataGrid::sort`](view::DataGrid::sort) callback with the column and a
//! `multi` flag (whether Shift was held); the host cycles its `SortState`
//! and re-derives its view with [`sort_indices`],
//! which applies the levels as tiebreakers in priority order.
//!
//! - **Plain click** replaces the sort with that one column, cycling
//!   ascending → descending → unsorted
//!   ([`SortState::cycle`](sort::SortState::cycle)).
//! - **Shift+click** adds/updates the column as a tiebreaker without
//!   disturbing the others — append ascending, then cycle its own
//!   direction asc → desc → removed in place
//!   ([`cycle_additive`](sort::SortState::cycle_additive)).
//!
//! Each sorted column shows a ▲/▼ arrow; when more than one is sorted,
//! a 1-based priority badge (1, 2, 3 …) follows the arrow (the AG Grid
//! `sortIndex` / Kendo `showIndexes` convention). Sortable headers
//! highlight on hover. A column is sortable only if its [`ColumnDef`]
//! carries a comparator (see [`column::ColumnDef::sortable_by_key`]) — the
//! comparator orders the row's *underlying* value, independent of the
//! cell's display formatting, so a `"$100.00"` price column sorts
//! numerically rather than lexicographically.
//!
//! ## Selection is keyed by stable row id
//!
//! Selection ([`SelectionState`]) is keyed by a **stable row
//! id** supplied via [`DataGrid::row_id`](view::DataGrid::row_id) — the
//! `getRowId` contract from `TanStack`/AG Grid/Kendo — *not* by slice
//! position. Because the host reorders the slice when sorting/filtering,
//! a positional key would point at a different row after any reorder
//! (the documented index-keying bug); an id key makes the selection
//! follow its rows for free. If no `row_id` is supplied the grid falls
//! back to slice position, which is correct only for a static, unsorted,
//! unfiltered grid.
//!
//! ## Columns are keyed by stable id (show/hide + reorder)
//!
//! Every column has a stable [`ColumnId`] — explicit
//! (via [`ColumnDef::id`](column::ColumnDef::id)) or derived from its
//! title. **All column state — sort, filter, and width — is keyed by
//! that id, never by the column's position** in the `Vec<ColumnDef>`.
//! This is the column-level analogue of the row `row_id` contract, and
//! the `id`/`colId` model every reputable grid uses (`TanStack`, AG Grid,
//! Kendo); index keying is the documented anti-pattern.
//!
//! The payoff is **show/hide and reorder for free**: the host expresses
//! them simply by *which* `ColumnDef`s it passes to the grid and *in what
//! order* — hide a column by omitting it, reorder by moving it in the
//! `Vec`. Because state is id-keyed, an active sort/filter/width stays
//! attached to its column across any rearrangement (a positional key
//! would mis-attribute it). The grid needs no dedicated column-layout
//! feature; the layout lives with the host like every other piece of
//! state. (The demo's `arrange_columns` + `column_layout` show the
//! pattern.) In-grid drag-to-reorder and a show/hide menu are deliberately
//! *not* built in — they're optional UI layered over the host's order, as
//! in headless `TanStack` (which ships neither).
//!
//! Column ids must be **unique**; a duplicate makes two columns share
//! state, so a debug build asserts uniqueness when the grid materializes.
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
//! ### Viewport fill (flex columns)
//!
//! By default columns are fixed-width and the grid scrolls horizontally
//! when their total exceeds the viewport. Give a column a **flex weight**
//! ([`ColumnDef::flex`](column::ColumnDef::flex)) — the CSS `flex-grow` /
//! AG-Grid `flex` numeric convention — and it grows to absorb surplus width
//! when the grid is *wider* than the columns' natural total, sharing that
//! surplus with the other flex columns in proportion to their weights
//! (`ColumnDef` width acts as the flex-basis). When the viewport is
//! narrower than the natural total the grid falls back to scrolling, so
//! `.flex(..)` means "fill the extra space, don't waste it" without giving
//! up horizontal scroll on small screens. Fill is **opt-in**: with no flex
//! column the *layout* is exactly as above (fixed widths + scroll).
//!
//! One thing is **not** gated on flex: the horizontal scrollbar auto-hides
//! (`OnActivity`) on every grid — flex or not. That's a deliberate chrome
//! choice (the app-wide scroll convention), independent of fill and applied
//! uniformly; it changes when the bar is *drawn*, never the column geometry.
//!
//! Flex and resize compose: **a width override pins a column to fixed**
//! (its flex weight is dropped), and since a drag writes an override,
//! resizing a flex column pins it (AG-Grid's behavior). Clearing the
//! override — e.g. a "reset columns" action — restores its flex.
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
//! - **Column pin / freeze** (a frozen identifier column while metric
//!   columns scroll) is not implemented — deliberately deferred.
//! - Columns start at their `ColumnDef` width and can be drag-resized
//!   (see [`width::ColumnWidths`]) or flex to fill the viewport (see
//!   *Viewport fill* above); the grid scrolls horizontally when the natural
//!   total exceeds the viewport. Width auto-fit (double-click to size a
//!   column to its content) is not implemented yet.

pub mod column;
pub mod column_strip;
pub mod copy_shortcut;
#[cfg(feature = "gallery")]
pub mod demo;
pub mod expand;
pub mod filter;
pub mod header_click;
mod measure;
pub mod sort;
mod view;
pub mod width;

pub use crate::collection::ScrollState;
pub use crate::collection::SelectionState;
pub use crate::collection::row_click::{ClickableRow, RowClickAction, clickable_row};
pub use column::{
    CellAlignment, ColumnDef, ColumnId, RowComparator, RowFilter, colored_text_column,
    optional_text_column, text_column,
};
pub use column_strip::{ColumnResize, ColumnSeparatorStyle, ColumnStrip};
pub use copy_shortcut::CopyOnShortcut;
pub use expand::ExpansionState;
pub use filter::{FilterState, filtered_indices};
pub use header_click::{HeaderClickable, HeaderClicked};
pub use sort::{SortDirection, SortState, sort_indices};
pub use view::{DataGrid, data_grid};
pub use width::{ColumnWidths, MIN_COLUMN_WIDTH};
