//! `list` — a virtualized, theme-driven vertical list of items.
//!
//! A single-column, row-virtualized list — a stripped-down
//! [`data_grid`](super::data_grid) without table chrome (no
//! header/columns/sort/per-column-filter/horizontal scroll). Backed by the
//! same masonry [`VirtualScroll`][masonry::widgets::VirtualScroll] as
//! `data_grid`, it supports an optional loading spinner, a search input,
//! item selection, lazy loading (an `on_load_more` callback fired as the
//! visible range nears the end of the data), and programmatic scrolling
//! (scroll-to-top / scroll-to-selected via [`ScrollState`]).
//!
//! ## The host owns the data
//!
//! Like `data_grid`, this list is presentation-only: it never filters,
//! sorts, or pages its own data. It emits *intents* — a search-query edit
//! ([`List::search`]), "the visible range is near the end of `item_count`"
//! ([`List::on_load_more`]), a scroll-to-index request
//! ([`List::scroll_to`]) — and the host supplies the resulting
//! `items`/`item_count`.
//!
//! Selection ([`SelectionState`]) is keyed by a stable item id
//! ([`List::item_id`]), the same `getRowId` contract as
//! [`DataGrid::row_id`](super::data_grid::DataGrid::row_id), so it follows
//! items across host-side reordering/filtering.
//!
//! Entry point: [`list`] / [`List`].
//!
//! [`ScrollState`]: super::data_grid::ScrollState
//! [`SelectionState`]: super::data_grid::SelectionState

mod body;
#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{List, list};
