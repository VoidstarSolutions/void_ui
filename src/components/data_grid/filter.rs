//! Filter model for [`super::data_grid`].
//!
//! Filtering follows the grid's "host owns the data, the grid owns the
//! UI + state" split. The grid never mutates or hides rows itself:
//! instead it exposes a [`FilterState`] (held by the host, read/written
//! through a lens like [`super::SelectionState`]) plus the
//! pure [`filtered_indices`] helper. The host calls that helper where it
//! has its data in hand, materializes the surviving rows, and passes
//! their count + accessor back to the grid — so virtualization and
//! sorting operate on the already-filtered set with no special casing.
//!
//! v1 is per-column text filtering: each filterable column carries a
//! [`RowFilter`](super::column::RowFilter) predicate (see
//! [`ColumnDef::filterable_by_text`](super::column::ColumnDef::filterable_by_text)),
//! and a row survives only if it passes *every* active column filter
//! (logical AND — the spreadsheet/Kendo default). Columns are identified
//! by their stable [`ColumnId`], so a filter stays attached to its column
//! across reorder/hide — the same identity contract sorting and widths
//! use.

use std::collections::BTreeMap;

use super::column::{ColumnDef, ColumnId};

/// The active per-column filter queries: [`ColumnId`] → query string.
///
/// Held in the host's app state and read by the grid through a lens. An
/// empty map means "no filtering" (every row is visible). A query is
/// stored only while non-blank, so [`Self::is_empty`] reflects whether
/// any column is actually constraining the view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterState {
    queries: BTreeMap<ColumnId, String>,
}

impl FilterState {
    /// An empty filter (everything visible).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `column`'s query. A blank/whitespace-only query clears the
    /// column's filter rather than storing an always-pass entry.
    pub fn set(&mut self, column: ColumnId, query: impl Into<String>) {
        let query = query.into();
        if query.trim().is_empty() {
            self.queries.remove(&column);
        } else {
            self.queries.insert(column, query);
        }
    }

    /// Clears `column`'s filter.
    pub fn clear(&mut self, column: &ColumnId) {
        self.queries.remove(column);
    }

    /// Clears every column's filter.
    pub fn clear_all(&mut self) {
        self.queries.clear();
    }

    /// The active query for `column`, if any.
    #[must_use]
    pub fn get(&self, column: &ColumnId) -> Option<&str> {
        self.queries.get(column).map(String::as_str)
    }

    /// `true` when no column is filtered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queries.is_empty()
    }

    /// Number of columns with an active filter.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queries.len()
    }

    /// Iterates `(column_id, query)` pairs in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = (&ColumnId, &str)> + '_ {
        self.queries
            .iter()
            .map(|(col, query)| (col, query.as_str()))
    }
}

/// Computes the source-row indices that survive `filter`, in natural
/// (ascending) order.
///
/// A row passes only if it satisfies *every* active column filter. A
/// filtered column with no [`RowFilter`](super::column::RowFilter)
/// predicate is treated as always-pass (it can't constrain anything).
/// When `filter` is empty, the identity `0..rows.len()` is returned so
/// callers can use a single code path.
///
/// This is the host's entry point: call it where the data lives, then
/// pass the resulting count + a row accessor to the grid. The grid then
/// sorts/virtualizes the surviving rows like any other slice.
#[must_use]
pub fn filtered_indices<R, State>(
    rows: &[R],
    filter: &FilterState,
    columns: &[ColumnDef<R, State>],
) -> Vec<usize> {
    if filter.is_empty() {
        return (0..rows.len()).collect();
    }
    // Resolve each active filter's ColumnId to its column's predicate
    // once, up front, rather than per row. A query whose id matches no
    // column, or matches a column with no predicate, is always-pass (it
    // can't constrain anything) and is simply dropped here.
    let active: Vec<(&super::column::RowFilter<R>, &str)> = filter
        .iter()
        .filter_map(|(id, query)| {
            columns
                .iter()
                .find(|c| &c.effective_id() == id)
                .and_then(|c| c.filter.as_ref())
                .map(|predicate| (predicate, query))
        })
        .collect();
    if active.is_empty() {
        return (0..rows.len()).collect();
    }
    (0..rows.len())
        .filter(|&i| {
            let row = &rows[i];
            active
                .iter()
                .all(|(predicate, query)| predicate(row, query))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ColumnId, FilterState, filtered_indices};
    use crate::components::data_grid::column::{CellAlign, ColumnDef, text_column};

    #[derive(Clone)]
    struct Row {
        symbol: &'static str,
        sector: &'static str,
    }

    fn id(s: &str) -> ColumnId {
        ColumnId::from(s)
    }

    fn columns() -> Vec<ColumnDef<Row, ()>> {
        vec![
            // Col 0: Symbol — filterable.
            text_column::<Row, (), _>("Symbol", 10.0, CellAlign::Start, |r: &Row| {
                r.symbol.to_string()
            })
            .filterable_by_text(|r: &Row| r.symbol.to_string()),
            // Col 1: Sector — filterable.
            text_column::<Row, (), _>("Sector", 10.0, CellAlign::Start, |r: &Row| {
                r.sector.to_string()
            })
            .filterable_by_text(|r: &Row| r.sector.to_string()),
            // Col 2: Plain — intentionally NOT filterable.
            text_column::<Row, (), _>("Plain", 10.0, CellAlign::Start, |r: &Row| {
                r.symbol.to_string()
            }),
        ]
    }

    fn rows() -> Vec<Row> {
        vec![
            Row {
                symbol: "AAPL",
                sector: "Tech",
            },
            Row {
                symbol: "MSFT",
                sector: "Tech",
            },
            Row {
                symbol: "AMZN",
                sector: "Retail",
            },
        ]
    }

    #[test]
    fn set_blank_query_clears_the_column() {
        let mut f = FilterState::new();
        f.set(id("Symbol"), "aap");
        assert_eq!(f.get(&id("Symbol")), Some("aap"));
        f.set(id("Symbol"), "   ");
        assert!(f.get(&id("Symbol")).is_none());
        assert!(f.is_empty());
    }

    #[test]
    fn empty_filter_returns_every_index() {
        let f = FilterState::new();
        let idx = filtered_indices(&rows(), &f, &columns());
        assert_eq!(idx, vec![0, 1, 2]);
    }

    #[test]
    fn text_filter_is_case_insensitive_substring() {
        let mut f = FilterState::new();
        f.set(id("Symbol"), "a"); // matches AAPL and AMZN, not MSFT
        let idx = filtered_indices(&rows(), &f, &columns());
        assert_eq!(idx, vec![0, 2]);
    }

    #[test]
    fn filter_on_column_without_predicate_is_ignored() {
        let mut f = FilterState::new();
        // "Plain" has no filter predicate → always-pass.
        f.set(id("Plain"), "AAPL");
        let idx = filtered_indices(&rows(), &f, &columns());
        assert_eq!(idx, vec![0, 1, 2]);
    }

    #[test]
    fn filter_on_unknown_column_id_is_ignored() {
        let mut f = FilterState::new();
        // An id that matches no column can't constrain anything.
        f.set(id("DoesNotExist"), "AAPL");
        let idx = filtered_indices(&rows(), &f, &columns());
        assert_eq!(idx, vec![0, 1, 2]);
    }

    #[test]
    fn multiple_columns_are_anded() {
        // Genuine cross-column AND: symbol contains "a" AND sector
        // contains "tech". AAPL/Tech passes both; AMZN/Retail fails the
        // sector filter; MSFT/Tech fails the symbol filter.
        let mut f = FilterState::new();
        f.set(id("Symbol"), "a");
        f.set(id("Sector"), "tech");
        let idx = filtered_indices(&rows(), &f, &columns());
        assert_eq!(idx, vec![0]);
    }

    #[test]
    fn setting_a_column_query_twice_overwrites() {
        let mut f = FilterState::new();
        f.set(id("Symbol"), "m"); // matches MSFT, AMZN
        f.set(id("Symbol"), "ms"); // narrows the same column to MSFT
        let idx = filtered_indices(&rows(), &f, &columns());
        assert_eq!(idx, vec![1]);
        assert_eq!(f.len(), 1, "still one active column filter");
    }

    #[test]
    fn filter_follows_its_column_across_a_reorder() {
        // THE point of id-keying: a filter set on "Sector" still filters
        // by sector after the columns are reordered, where a positional
        // key (col 1) would now point at a different column.
        let mut f = FilterState::new();
        f.set(id("Sector"), "tech"); // keeps AAPL + MSFT (both Tech)

        // Reorder: Sector is now first (index 0), Symbol second.
        let cols = columns();
        let reordered = vec![
            // Sector
            text_column::<Row, (), _>("Sector", 10.0, CellAlign::Start, |r: &Row| {
                r.sector.to_string()
            })
            .filterable_by_text(|r: &Row| r.sector.to_string()),
            // Symbol
            text_column::<Row, (), _>("Symbol", 10.0, CellAlign::Start, |r: &Row| {
                r.symbol.to_string()
            })
            .filterable_by_text(|r: &Row| r.symbol.to_string()),
        ];
        let _ = cols; // original order unused beyond documentation

        let idx = filtered_indices(&rows(), &f, &reordered);
        // Still filtering by *sector* → Tech rows AAPL(0), MSFT(1).
        assert_eq!(idx, vec![0, 1]);
    }
}
