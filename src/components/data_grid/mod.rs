//! `data_grid` — a virtualized, theme-driven table view for very large,
//! append-only data streams.
//!
//! v1 is read-only with row-only selection and TSV clipboard copy. The
//! widget is generic over the row type so the same grid can browse
//! [`citadel_core::Tick`] streams, `ColumnDelta` event logs, or any
//! other in-memory `&[R]` exposed by the host's app state.
//!
//! Backed by masonry's [`VirtualScroll`][masonry::widgets::VirtualScroll]
//! for row virtualization. The view is mostly xilem stock; the only
//! custom widget is a tiny focusable wrapper used to catch Ctrl/Cmd+C
//! for clipboard copy (lands in a later commit).
//!
//! See [`column::ColumnDef`] for the per-column contract,
//! [`selection::SelectionState`] for the selection model. The xilem
//! `data_grid` view itself arrives in a follow-up commit.

pub mod column;
pub mod copy_shortcut;
pub mod demo;
pub mod selection;
mod view;

pub use column::{CellAlign, ColumnDef, optional_text_column, text_column};
pub use copy_shortcut::CopyOnShortcut;
pub use selection::SelectionState;
pub use view::data_grid;
