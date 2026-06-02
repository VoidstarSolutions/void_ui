//! Sort model for [`super::data_grid`].
//!
//! Sorting is single-column in this first increment. It follows the
//! **same host-side contract as filtering** (see [`super::filter`]): the
//! grid is presentation-only and never reorders data itself. The host
//! owns the row order — it composes [`sort_indices`] after
//! [`filtered_indices`](super::filter::filtered_indices) where its data
//! lives, materializes the ordered rows, and serves them to the grid.
//! The grid keeps [`SortState`] only as the *descriptor* it draws the
//! header arrow from and emits header-click cycles through (mirroring how
//! a controlled `state` + `onSortingChange` works in headless table
//! libraries such as `TanStack` Table).
//!
//! This replaced an earlier design where the grid sorted internally each
//! rebuild via a per-frame order cache — which re-sorted the whole
//! dataset on every rebuild and left selection keyed to a slice whose
//! identity flipped under filtering. Moving order to the host unifies
//! sort with the already-host-side filtering, fixes both, and matches the
//! prevailing data-grid architecture (AG Grid server-side row model,
//! Kendo, `TanStack` `manualSorting`; and every surveyed Rust GUI —
//! egui/gpui-component/xilem — keeps ordering app-side).
//!
//! The header-click cycle matches the convention every spreadsheet and
//! the Kendo grid use: clicking a column's header advances
//! **unsorted → ascending → descending → unsorted**, and clicking a
//! *different* column jumps straight to ascending on that column.
//!
//! Columns are identified by their stable [`ColumnId`], so an active sort
//! stays attached to its column across reorder/hide — the same identity
//! contract selection, filter, and widths use.

use super::column::{ColumnDef, ColumnId};

/// Ascending or descending order for the sorted column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortDirection {
    /// Smallest-first (A→Z, 0→9, oldest→newest).
    #[default]
    Ascending,
    /// Largest-first (Z→A, 9→0, newest→oldest).
    Descending,
}

impl SortDirection {
    /// The opposite direction.
    #[must_use]
    pub const fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// Which column is currently sorted, and in which direction.
///
/// The sorted column is identified by its stable [`ColumnId`], not its
/// position — so an active sort stays attached to its column when the
/// host reorders or hides columns (the same identity contract as
/// [`SelectionState`](super::selection::SelectionState) /
/// [`FilterState`](super::filter::FilterState) /
/// [`ColumnWidths`](super::width::ColumnWidths)). A `column` of `None`
/// means the grid is unsorted and rows display in their natural source
/// order. Held in the host's app state and read by the grid through a
/// lens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SortState {
    column: Option<ColumnId>,
    direction: SortDirection,
}

impl SortState {
    /// An unsorted state (natural source order).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The id of the sorted column, or `None` when unsorted.
    #[must_use]
    pub fn column(&self) -> Option<&ColumnId> {
        self.column.as_ref()
    }

    /// The active sort direction. Only meaningful when
    /// [`Self::column`] is `Some`.
    #[must_use]
    pub fn direction(&self) -> SortDirection {
        self.direction
    }

    /// The direction `column` is sorted in, or `None` if `column` is
    /// not the currently-sorted one. Header rendering uses this to
    /// decide whether (and which way) to draw a sort arrow.
    #[must_use]
    pub fn direction_for(&self, column: &ColumnId) -> Option<SortDirection> {
        (self.column.as_ref() == Some(column)).then_some(self.direction)
    }

    /// Advance the sort state as if the user clicked `column`'s header.
    ///
    /// - Clicking the already-sorted column advances
    ///   ascending → descending → unsorted.
    /// - Clicking any other column starts a fresh ascending sort on it.
    pub fn cycle(&mut self, column: ColumnId) {
        if self.column.as_ref() == Some(&column) {
            match self.direction {
                SortDirection::Ascending => self.direction = SortDirection::Descending,
                SortDirection::Descending => *self = Self::new(),
            }
        } else {
            self.column = Some(column);
            self.direction = SortDirection::Ascending;
        }
    }

    /// Reset to the unsorted state.
    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

/// Reorders `indices` (positions into `rows`) in place to reflect
/// `sort`, using the sorted column's comparator from `columns`.
///
/// This is the host's sort entry point and the **mirror of
/// [`filtered_indices`](super::filter::filtered_indices)**: it operates
/// on an index list, so the host composes the two —
/// `let mut idx = filtered_indices(..); sort_indices(&mut idx, ..)` —
/// and materializes `idx.map(|i| rows[i])` to hand the grid an already
/// filtered-then-sorted slice. (Filter-before-sort is the canonical
/// pipeline order; see `TanStack`'s row-model pipeline.)
///
/// A no-op when the grid is unsorted (`sort.column()` is `None`), when no
/// column matches the sorted id (e.g. it was hidden), or when that column
/// carries no comparator (it isn't sortable) — in every such case
/// `indices` is left in its incoming (natural or filtered) order.
///
/// The sort is **stable**, so rows the comparator deems equal keep their
/// incoming relative order rather than reshuffling on each recompute.
pub fn sort_indices<R, State>(
    indices: &mut [usize],
    rows: &[R],
    sort: &SortState,
    columns: &[ColumnDef<R, State>],
) {
    let Some(id) = sort.column() else { return };
    let Some(comparator) = columns
        .iter()
        .find(|c| &c.effective_id() == id)
        .and_then(|c| c.comparator.as_ref())
    else {
        return;
    };
    indices.sort_by(|&a, &b| {
        let ord = comparator(&rows[a], &rows[b]);
        match sort.direction() {
            SortDirection::Ascending => ord,
            SortDirection::Descending => ord.reverse(),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{sort_indices, ColumnId, SortDirection, SortState};
    use crate::components::data_grid::column::{text_column, CellAlign, ColumnDef};

    fn id(s: &str) -> ColumnId {
        ColumnId::from(s)
    }

    /// One sortable column "N" over `i32` rows, keyed on the value itself.
    fn int_columns() -> Vec<ColumnDef<i32, ()>> {
        vec![
            text_column::<i32, (), _>("N", 10.0, CellAlign::End, |r: &i32| r.to_string())
                .sortable_by_key(|r: &i32| *r),
        ]
    }

    /// A second column "Plain" with no comparator — for the
    /// unsortable-column no-op path.
    fn columns_with_unsortable() -> Vec<ColumnDef<i32, ()>> {
        let mut cols = int_columns();
        cols.push(text_column::<i32, (), _>(
            "Plain",
            10.0,
            CellAlign::End,
            |r: &i32| r.to_string(),
        ));
        cols
    }

    #[test]
    fn cycle_advances_asc_desc_then_clears() {
        let mut s = SortState::new();
        assert_eq!(s.column(), None);

        s.cycle(id("price"));
        assert_eq!(s.column(), Some(&id("price")));
        assert_eq!(s.direction(), SortDirection::Ascending);

        s.cycle(id("price"));
        assert_eq!(s.direction(), SortDirection::Descending);

        s.cycle(id("price"));
        assert_eq!(s.column(), None, "third click on same column clears the sort");
    }

    #[test]
    fn cycle_to_different_column_starts_ascending() {
        let mut s = SortState::new();
        s.cycle(id("size"));
        s.cycle(id("size")); // now descending on size
        assert_eq!(s.direction(), SortDirection::Descending);

        s.cycle(id("price"));
        assert_eq!(s.column(), Some(&id("price")));
        assert_eq!(
            s.direction(),
            SortDirection::Ascending,
            "switching columns resets to ascending"
        );
    }

    #[test]
    fn direction_for_only_matches_active_column() {
        let mut s = SortState::new();
        s.cycle(id("price"));
        assert_eq!(s.direction_for(&id("price")), Some(SortDirection::Ascending));
        assert_eq!(s.direction_for(&id("size")), None);
    }

    #[test]
    fn reversed_flips() {
        assert_eq!(SortDirection::Ascending.reversed(), SortDirection::Descending);
        assert_eq!(SortDirection::Descending.reversed(), SortDirection::Ascending);
    }

    #[test]
    fn sort_indices_unsorted_state_is_a_noop() {
        let rows = vec![3, 1, 2];
        let mut idx = vec![0, 1, 2];
        // `SortState::new()` has no active column.
        sort_indices(&mut idx, &rows, &SortState::new(), &int_columns());
        assert_eq!(idx, vec![0, 1, 2], "unsorted leaves incoming order untouched");
    }

    #[test]
    fn sort_indices_orders_ascending_and_descending() {
        let rows = vec![30, 10, 20];
        let cols = int_columns();

        let mut asc = vec![0, 1, 2];
        let mut s = SortState::new();
        s.cycle(id("N")); // ascending on column "N"
        sort_indices(&mut asc, &rows, &s, &cols);
        assert_eq!(asc, vec![1, 2, 0], "indices ordered by ascending value");

        let mut desc = vec![0, 1, 2];
        s.cycle(id("N")); // descending on column "N"
        sort_indices(&mut desc, &rows, &s, &cols);
        assert_eq!(desc, vec![0, 2, 1], "indices ordered by descending value");
    }

    #[test]
    fn sort_indices_is_stable_on_ties() {
        // Rows compare equal on the key (all 0); stable sort must keep
        // incoming order rather than reshuffle.
        let rows = vec![0, 0, 0, 0];
        let mut idx = vec![0, 1, 2, 3];
        let mut s = SortState::new();
        s.cycle(id("N"));
        s.cycle(id("N")); // descending — ties must still hold incoming order
        sort_indices(&mut idx, &rows, &s, &int_columns());
        assert_eq!(idx, vec![0, 1, 2, 3]);
    }

    #[test]
    fn sort_indices_composes_after_a_filtered_subset() {
        // Host pipeline: filtered index list (a subset, not 0..n) then
        // sorted. Only even-valued rows survived the (hypothetical) filter.
        let rows = vec![50, 11, 40, 13, 20]; // indices 0,2,4 are "even"
        let mut idx = vec![0, 2, 4]; // pre-filtered subset, natural order
        let mut s = SortState::new();
        s.cycle(id("N")); // ascending
        sort_indices(&mut idx, &rows, &s, &int_columns());
        // Sorted by value: 20(idx4) < 40(idx2) < 50(idx0).
        assert_eq!(idx, vec![4, 2, 0]);
    }

    #[test]
    fn sort_indices_unsortable_column_is_a_noop() {
        let rows = vec![3, 1, 2];
        let mut idx = vec![0, 1, 2];
        let mut s = SortState::new();
        s.cycle(id("Plain")); // "Plain" has no comparator
        sort_indices(&mut idx, &rows, &s, &columns_with_unsortable());
        assert_eq!(idx, vec![0, 1, 2], "unsortable column leaves order untouched");
    }

    #[test]
    fn sort_indices_sorted_column_absent_is_a_noop() {
        // The sorted id ("N") isn't among the passed columns (e.g. it was
        // hidden from the arranged set) → no column matches → incoming
        // order is left untouched (natural fallback), no panic.
        let rows = vec![30, 10, 20];
        let mut idx = vec![0, 1, 2];
        let mut s = SortState::new();
        s.cycle(id("N"));
        let only_plain = vec![text_column::<i32, (), _>(
            "Plain",
            10.0,
            CellAlign::End,
            |r: &i32| r.to_string(),
        )];
        sort_indices(&mut idx, &rows, &s, &only_plain);
        assert_eq!(idx, vec![0, 1, 2], "absent sorted column leaves order untouched");
    }

    #[test]
    fn sort_follows_its_column_across_a_reorder() {
        // A sort set on "N" still sorts by N after columns are reordered,
        // where a positional key would point at a different column.
        let rows = vec![30, 10, 20];
        let mut s = SortState::new();
        s.cycle(id("N")); // ascending by N

        // Reordered columns: "Plain" first, "N" second.
        let reordered = vec![
            text_column::<i32, (), _>("Plain", 10.0, CellAlign::End, |r: &i32| r.to_string()),
            text_column::<i32, (), _>("N", 10.0, CellAlign::End, |r: &i32| r.to_string())
                .sortable_by_key(|r: &i32| *r),
        ];
        let mut idx = vec![0, 1, 2];
        sort_indices(&mut idx, &rows, &s, &reordered);
        assert_eq!(idx, vec![1, 2, 0], "still sorted by N after the reorder");
    }
}
