//! Sort model for [`super::data_grid`].
//!
//! Sorting is single-column in this first increment, following the
//! same "host owns the state, the grid reads it through a lens" shape
//! that [`super::selection::SelectionState`] already uses. The grid
//! itself stays presentation-only: it never mutates the host's row
//! data. Instead it derives a *display order* (a permutation of source
//! indices) each rebuild from the active [`SortState`] and the sorted
//! column's [`RowComparator`], and maps virtual rows through it.
//!
//! The header-click cycle matches the convention every spreadsheet and
//! the Kendo grid use: clicking a column's header advances
//! **unsorted → ascending → descending → unsorted**, and clicking a
//! *different* column jumps straight to ascending on that column.
//!
//! Columns are identified by their index in the `Vec<ColumnDef>` handed
//! to [`data_grid`](super::data_grid) — the same positional identity the
//! header and row builders already use.

use super::column::RowComparator;

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
/// A `column` of `None` means the grid is unsorted and rows display in
/// their natural source order. Held in the host's app state and read by
/// the grid through a lens (mirroring
/// [`SelectionState`](super::selection::SelectionState)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SortState {
    column: Option<usize>,
    direction: SortDirection,
}

impl SortState {
    /// An unsorted state (natural source order).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The index of the sorted column, or `None` when unsorted.
    #[must_use]
    pub fn column(&self) -> Option<usize> {
        self.column
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
    pub fn direction_for(&self, column: usize) -> Option<SortDirection> {
        (self.column == Some(column)).then_some(self.direction)
    }

    /// Advance the sort state as if the user clicked `column`'s header.
    ///
    /// - Clicking the already-sorted column advances
    ///   ascending → descending → unsorted.
    /// - Clicking any other column starts a fresh ascending sort on it.
    pub fn cycle(&mut self, column: usize) {
        if self.column == Some(column) {
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

/// Compute the display order — a permutation of `0..rows.len()` — for
/// `rows` under `comparator` and `direction`.
///
/// Uses a *stable* sort so rows the comparator deems equal keep their
/// source order, which keeps the view from reshuffling ties on every
/// rebuild. When `comparator` is `None` (the active column isn't
/// sortable) the identity order is returned so the caller can use a
/// single code path; the grid's unsorted path skips this entirely and
/// indexes source rows directly.
pub(crate) fn display_order<R>(
    rows: &[R],
    comparator: Option<&RowComparator<R>>,
    direction: SortDirection,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    if let Some(cmp) = comparator {
        order.sort_by(|&a, &b| {
            let ord = cmp(&rows[a], &rows[b]);
            match direction {
                SortDirection::Ascending => ord,
                SortDirection::Descending => ord.reverse(),
            }
        });
    }
    order
}

#[cfg(test)]
mod tests {
    use super::{display_order, SortDirection, SortState};
    use crate::components::data_grid::column::RowComparator;

    #[test]
    fn cycle_advances_asc_desc_then_clears() {
        let mut s = SortState::new();
        assert_eq!(s.column(), None);

        s.cycle(2);
        assert_eq!(s.column(), Some(2));
        assert_eq!(s.direction(), SortDirection::Ascending);

        s.cycle(2);
        assert_eq!(s.direction(), SortDirection::Descending);

        s.cycle(2);
        assert_eq!(s.column(), None, "third click on same column clears the sort");
    }

    #[test]
    fn cycle_to_different_column_starts_ascending() {
        let mut s = SortState::new();
        s.cycle(1);
        s.cycle(1); // now descending on column 1
        assert_eq!(s.direction(), SortDirection::Descending);

        s.cycle(3);
        assert_eq!(s.column(), Some(3));
        assert_eq!(
            s.direction(),
            SortDirection::Ascending,
            "switching columns resets to ascending"
        );
    }

    #[test]
    fn direction_for_only_matches_active_column() {
        let mut s = SortState::new();
        s.cycle(0);
        assert_eq!(s.direction_for(0), Some(SortDirection::Ascending));
        assert_eq!(s.direction_for(1), None);
    }

    #[test]
    fn reversed_flips() {
        assert_eq!(SortDirection::Ascending.reversed(), SortDirection::Descending);
        assert_eq!(SortDirection::Descending.reversed(), SortDirection::Ascending);
    }

    #[test]
    fn display_order_none_comparator_is_identity() {
        let rows = vec![3, 1, 2];
        let order = display_order::<i32>(&rows, None, SortDirection::Ascending);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn display_order_sorts_ascending_and_descending() {
        let rows = vec![30, 10, 20];
        let cmp: RowComparator<i32> = Box::new(i32::cmp);

        let asc = display_order(&rows, Some(&cmp), SortDirection::Ascending);
        assert_eq!(asc, vec![1, 2, 0], "indices ordered by ascending value");

        let desc = display_order(&rows, Some(&cmp), SortDirection::Descending);
        assert_eq!(desc, vec![0, 2, 1], "indices ordered by descending value");
    }

    #[test]
    fn display_order_is_stable_on_ties() {
        // Rows compare equal on the key (all 0); stable sort must keep
        // source order rather than reshuffle.
        let rows = vec![0, 0, 0, 0];
        let cmp: RowComparator<i32> = Box::new(i32::cmp);
        let order = display_order(&rows, Some(&cmp), SortDirection::Descending);
        assert_eq!(order, vec![0, 1, 2, 3]);
    }
}
