//! Sort model for [`super::data_grid`].
//!
//! Supports **multi-column (tiebreak) sort**: [`SortState`] is an ordered
//! list of `(column, direction)` levels where position = priority (the
//! first level is primary, the rest break ties). An empty list is the
//! unsorted state; a single level is plain single-column sort.
//!
//! It follows the **same host-side contract as filtering** (see
//! [`super::filter`]): the grid is presentation-only and never reorders
//! data itself. The host owns the row order — it composes [`sort_indices`]
//! after [`filtered_indices`](super::filter::filtered_indices) where its
//! data lives, materializes the ordered rows, and serves them to the
//! grid. The grid keeps [`SortState`] only as the *descriptor* it draws
//! the header arrows from and emits header-click cycles through (mirroring
//! a controlled `state` + `onSortingChange` in headless table libraries
//! such as `TanStack` Table, whose `SortingState` is likewise an ordered
//! `[{id, desc}]` list).
//!
//! Header-click semantics match the cross-grid standard (AG Grid,
//! `TanStack`):
//! - **Plain click** replaces the whole sort with just that column,
//!   cycling **unsorted → ascending → descending → unsorted** ([`cycle`]).
//! - **Shift+click** adds/updates that column as a tiebreaker without
//!   disturbing the others: append ascending, then cycle its own
//!   direction asc → desc → removed in place ([`cycle_additive`]).
//!   Removing a level leaves the remaining levels' relative priority
//!   intact.
//!
//! Single-column sort stays the default; multi-sort is opt-in per click
//! via Shift, so the unmodified UX is unchanged.
//!
//! Columns are identified by their stable [`ColumnId`], so an active sort
//! stays attached to its column across reorder/hide — the same identity
//! contract selection, filter, and widths use.
//!
//! [`cycle`]: SortState::cycle
//! [`cycle_additive`]: SortState::cycle_additive

use std::cmp::Ordering;

use super::column::{ColumnDef, ColumnId, RowComparator};

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

/// One level of the sort: a column and the direction it's sorted in.
/// Levels are ordered by priority within [`SortState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortLevel {
    /// The sorted column's stable id.
    pub column: ColumnId,
    /// Ascending or descending for this level.
    pub direction: SortDirection,
}

/// The active multi-column sort: an **ordered list of levels**, where
/// position is priority (level 0 is primary, later levels break ties).
/// An empty list is the unsorted state.
///
/// Columns are identified by stable [`ColumnId`], not position — so an
/// active sort stays attached to its column when the host reorders or
/// hides columns (the same identity contract as
/// [`SelectionState`](super::selection::SelectionState) /
/// [`FilterState`](super::filter::FilterState) /
/// [`ColumnWidths`](super::width::ColumnWidths)). Held in the host's app
/// state and read by the grid through a lens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SortState {
    levels: Vec<SortLevel>,
}

impl SortState {
    /// An unsorted state (natural source order).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` when nothing is sorted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// Number of active sort levels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    /// The sort levels in priority order (primary first).
    #[must_use]
    pub fn levels(&self) -> &[SortLevel] {
        &self.levels
    }

    /// The id of the *primary* (highest-priority) sorted column, or
    /// `None` when unsorted.
    #[must_use]
    pub fn primary(&self) -> Option<&ColumnId> {
        self.levels.first().map(|l| &l.column)
    }

    /// The direction `column` is sorted in, or `None` if it isn't part
    /// of the sort. Header rendering uses this to decide whether (and
    /// which way) to draw a sort arrow.
    #[must_use]
    pub fn direction_for(&self, column: &ColumnId) -> Option<SortDirection> {
        self.levels
            .iter()
            .find(|l| &l.column == column)
            .map(|l| l.direction)
    }

    /// The 0-based priority of `column` within the sort, or `None` if it
    /// isn't sorted. `0` is the primary column. Header rendering uses
    /// this to draw a priority badge (1, 2, 3 …) when multi-sorting.
    #[must_use]
    pub fn priority_of(&self, column: &ColumnId) -> Option<usize> {
        self.levels.iter().position(|l| &l.column == column)
    }

    /// Plain-click cycle: **replace** the whole sort with just `column`,
    /// advancing unsorted → ascending → descending → unsorted.
    ///
    /// - Clicking the current sole-primary column advances its direction,
    ///   then clears on the third click.
    /// - Clicking any other column (or when multi-sorted) collapses to a
    ///   fresh ascending single-column sort on it.
    pub fn cycle(&mut self, column: ColumnId) {
        // Collapse-to-single is the plain-click contract. Only when this
        // column is *already the lone sort* do we advance/clear it.
        if let [level] = self.levels.as_slice()
            && level.column == column
        {
            match level.direction {
                SortDirection::Ascending => self.levels[0].direction = SortDirection::Descending,
                SortDirection::Descending => self.levels.clear(),
            }
        } else {
            self.levels = vec![SortLevel {
                column,
                direction: SortDirection::Ascending,
            }];
        }
    }

    /// Shift-click cycle: add or update `column` as a tiebreaker
    /// **without disturbing the other levels**.
    ///
    /// - If `column` isn't sorted yet, append it ascending at the lowest
    ///   priority.
    /// - If it is, advance its own direction ascending → descending →
    ///   removed (in place; the remaining levels keep their relative
    ///   order).
    pub fn cycle_additive(&mut self, column: ColumnId) {
        if let Some(pos) = self.levels.iter().position(|l| l.column == column) {
            match self.levels[pos].direction {
                SortDirection::Ascending => {
                    self.levels[pos].direction = SortDirection::Descending;
                }
                SortDirection::Descending => {
                    self.levels.remove(pos);
                }
            }
        } else {
            self.levels.push(SortLevel {
                column,
                direction: SortDirection::Ascending,
            });
        }
    }

    /// Reset to the unsorted state.
    pub fn clear(&mut self) {
        self.levels.clear();
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
/// Each sort level is applied in priority order: rows are compared by the
/// primary column, ties broken by the next level, and so on. A level
/// whose column isn't found (e.g. it was hidden) or carries no comparator
/// (it isn't sortable) is skipped. When *no* level resolves to a usable
/// comparator, `indices` is left in its incoming (natural or filtered)
/// order.
///
/// The sort is **stable**, so rows that compare equal on *every* active
/// level keep their incoming relative order rather than reshuffling on
/// each recompute.
pub fn sort_indices<R, State>(
    indices: &mut [usize],
    rows: &[R],
    sort: &SortState,
    columns: &[ColumnDef<R, State>],
) {
    // Resolve each level's comparator + direction once, up front (not per
    // comparison). Levels with no matching/sortable column are dropped.
    let resolved: Vec<(&RowComparator<R>, SortDirection)> = sort
        .levels()
        .iter()
        .filter_map(|level| {
            columns
                .iter()
                .find(|c| c.effective_id() == level.column)
                .and_then(|c| c.comparator.as_ref())
                .map(|cmp| (cmp, level.direction))
        })
        .collect();
    if resolved.is_empty() {
        return;
    }
    indices.sort_by(|&a, &b| {
        // Compare by each level in priority order; first non-Equal wins.
        for (comparator, direction) in &resolved {
            let ord = comparator(&rows[a], &rows[b]);
            let ord = match direction {
                SortDirection::Ascending => ord,
                SortDirection::Descending => ord.reverse(),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    });
}

#[cfg(test)]
mod tests {
    use super::{ColumnId, SortDirection, SortState, sort_indices};
    use crate::components::data_grid::column::{CellAlign, ColumnDef, text_column};

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
        assert_eq!(s.primary(), None);

        s.cycle(id("price"));
        assert_eq!(s.primary(), Some(&id("price")));
        assert_eq!(
            s.direction_for(&id("price")),
            Some(SortDirection::Ascending)
        );

        s.cycle(id("price"));
        assert_eq!(
            s.direction_for(&id("price")),
            Some(SortDirection::Descending)
        );

        s.cycle(id("price"));
        assert_eq!(
            s.primary(),
            None,
            "third click on same column clears the sort"
        );
        assert!(s.is_empty());
    }

    #[test]
    fn cycle_to_different_column_starts_ascending() {
        let mut s = SortState::new();
        s.cycle(id("size"));
        s.cycle(id("size")); // now descending on size
        assert_eq!(
            s.direction_for(&id("size")),
            Some(SortDirection::Descending)
        );

        s.cycle(id("price"));
        assert_eq!(s.primary(), Some(&id("price")));
        assert_eq!(
            s.direction_for(&id("price")),
            Some(SortDirection::Ascending),
            "switching columns resets to ascending"
        );
        assert_eq!(s.len(), 1, "plain click collapses to a single column");
    }

    #[test]
    fn direction_for_only_matches_active_column() {
        let mut s = SortState::new();
        s.cycle(id("price"));
        assert_eq!(
            s.direction_for(&id("price")),
            Some(SortDirection::Ascending)
        );
        assert_eq!(s.direction_for(&id("size")), None);
    }

    #[test]
    fn cycle_additive_appends_then_cycles_then_removes_in_place() {
        let mut s = SortState::new();
        s.cycle(id("price")); // primary: price asc
        s.cycle_additive(id("size")); // tiebreaker: size asc (appended)
        assert_eq!(s.len(), 2);
        assert_eq!(s.priority_of(&id("price")), Some(0));
        assert_eq!(s.priority_of(&id("size")), Some(1));
        assert_eq!(s.direction_for(&id("size")), Some(SortDirection::Ascending));

        // Shift-click size again: its own direction flips, priority kept.
        s.cycle_additive(id("size"));
        assert_eq!(
            s.direction_for(&id("size")),
            Some(SortDirection::Descending)
        );
        assert_eq!(s.priority_of(&id("size")), Some(1), "priority unchanged");
        // Price (primary) is untouched throughout.
        assert_eq!(
            s.direction_for(&id("price")),
            Some(SortDirection::Ascending)
        );

        // Third shift-click removes size; price stays primary.
        s.cycle_additive(id("size"));
        assert_eq!(s.priority_of(&id("size")), None, "removed");
        assert_eq!(s.primary(), Some(&id("price")));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn removing_a_middle_level_keeps_others_relative_priority() {
        let mut s = SortState::new();
        s.cycle(id("a")); // a (0)
        s.cycle_additive(id("b")); // b (1)
        s.cycle_additive(id("c")); // c (2)
        // Remove b: a stays primary, c closes up to priority 1.
        s.cycle_additive(id("b")); // b → desc
        s.cycle_additive(id("b")); // b → removed
        assert_eq!(s.priority_of(&id("a")), Some(0));
        assert_eq!(s.priority_of(&id("c")), Some(1));
        assert_eq!(s.priority_of(&id("b")), None);
    }

    #[test]
    fn plain_cycle_collapses_a_multi_sort() {
        let mut s = SortState::new();
        s.cycle(id("a"));
        s.cycle_additive(id("b"));
        s.cycle_additive(id("c"));
        assert_eq!(s.len(), 3);
        // A plain click on any column collapses to single-sort on it.
        s.cycle(id("b"));
        assert_eq!(s.len(), 1);
        assert_eq!(s.primary(), Some(&id("b")));
        assert_eq!(s.direction_for(&id("b")), Some(SortDirection::Ascending));
    }

    #[test]
    fn reversed_flips() {
        assert_eq!(
            SortDirection::Ascending.reversed(),
            SortDirection::Descending
        );
        assert_eq!(
            SortDirection::Descending.reversed(),
            SortDirection::Ascending
        );
    }

    #[test]
    fn sort_indices_unsorted_state_is_a_noop() {
        let rows = vec![3, 1, 2];
        let mut idx = vec![0, 1, 2];
        // `SortState::new()` has no active column.
        sort_indices(&mut idx, &rows, &SortState::new(), &int_columns());
        assert_eq!(
            idx,
            vec![0, 1, 2],
            "unsorted leaves incoming order untouched"
        );
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
    fn sort_indices_applies_tiebreakers_in_priority_order() {
        // Two-field rows: primary "grp" (asc), tiebreak "val" (desc).
        #[derive(Clone)]
        struct Row {
            grp: i32,
            val: i32,
        }
        let cols: Vec<ColumnDef<Row, ()>> = vec![
            text_column::<Row, (), _>("grp", 10.0, CellAlign::End, |r: &Row| r.grp.to_string())
                .sortable_by_key(|r: &Row| r.grp),
            text_column::<Row, (), _>("val", 10.0, CellAlign::End, |r: &Row| r.val.to_string())
                .sortable_by_key(|r: &Row| r.val),
        ];
        // grp: 1,1,0,0 ; val: 5,9,2,7
        let rows = vec![
            Row { grp: 1, val: 5 }, // 0
            Row { grp: 1, val: 9 }, // 1
            Row { grp: 0, val: 2 }, // 2
            Row { grp: 0, val: 7 }, // 3
        ];
        let mut s = SortState::new();
        s.cycle(id("grp")); // primary: grp asc
        s.cycle_additive(id("val")); // tiebreak: val asc
        s.cycle_additive(id("val")); // → val desc
        let mut idx = vec![0, 1, 2, 3];
        sort_indices(&mut idx, &rows, &s, &cols);
        // grp asc → group 0 first {2,3}, then group 1 {0,1}. Within each,
        // val DESC: group0 → 7(3) then 2(2); group1 → 9(1) then 5(0).
        assert_eq!(idx, vec![3, 2, 1, 0]);
    }

    #[test]
    fn sort_indices_third_level_breaks_a_two_level_tie() {
        // Three sortable fields. Rows are constructed so the 1st and 2nd
        // levels leave a genuine tie that ONLY the 3rd level resolves —
        // proving the N-level loop consults all levels, not just two.
        #[derive(Clone)]
        struct Row {
            a: i32,
            b: i32,
            c: i32,
        }
        let cols: Vec<ColumnDef<Row, ()>> = vec![
            text_column::<Row, (), _>("a", 10.0, CellAlign::End, |r: &Row| r.a.to_string())
                .sortable_by_key(|r: &Row| r.a),
            text_column::<Row, (), _>("b", 10.0, CellAlign::End, |r: &Row| r.b.to_string())
                .sortable_by_key(|r: &Row| r.b),
            text_column::<Row, (), _>("c", 10.0, CellAlign::End, |r: &Row| r.c.to_string())
                .sortable_by_key(|r: &Row| r.c),
        ];
        // All share a=0, b=0 → 1st and 2nd levels tie on every row; only
        // `c` distinguishes them. Incoming order is c-descending, so a
        // correct asc-by-c sort must fully reverse it.
        let rows = vec![
            Row { a: 0, b: 0, c: 3 }, // 0
            Row { a: 0, b: 0, c: 2 }, // 1
            Row { a: 0, b: 0, c: 1 }, // 2
            Row { a: 0, b: 0, c: 0 }, // 3
        ];
        let mut s = SortState::new();
        s.cycle(id("a")); // a asc (all tie)
        s.cycle_additive(id("b")); // b asc (all tie)
        s.cycle_additive(id("c")); // c asc — the only discriminator
        let mut idx = vec![0, 1, 2, 3];
        sort_indices(&mut idx, &rows, &s, &cols);
        // Ordered by c ascending: 0(3),1(2),2(1),3(0) → 3,2,1,0.
        assert_eq!(idx, vec![3, 2, 1, 0], "3rd level resolves the 1st+2nd tie");
    }

    #[test]
    fn sort_indices_skips_an_unsortable_tiebreak_level() {
        // Primary "N" sortable; a tiebreak on "Plain" (no comparator) is
        // silently skipped, leaving the primary order intact.
        let rows = vec![30, 10, 20];
        let mut s = SortState::new();
        s.cycle(id("N")); // N asc
        s.cycle_additive(id("Plain")); // unsortable tiebreak
        let mut idx = vec![0, 1, 2];
        sort_indices(&mut idx, &rows, &s, &columns_with_unsortable());
        assert_eq!(idx, vec![1, 2, 0], "primary order holds; bad level skipped");
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
        assert_eq!(
            idx,
            vec![0, 1, 2],
            "unsortable column leaves order untouched"
        );
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
        assert_eq!(
            idx,
            vec![0, 1, 2],
            "absent sorted column leaves order untouched"
        );
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
