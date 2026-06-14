//! Row-selection state for the virtualized collection components.
//!
//! Selection is row-only in v1. The model is the standard "anchor +
//! set" pattern used by every spreadsheet and file manager:
//!
//! - **Plain click** on a row replaces the selection with that one row
//!   and moves the anchor to it.
//! - **Ctrl/Cmd-click** toggles a single row's membership and moves the
//!   anchor.
//! - **Shift-click** extends the selection from the anchor to the
//!   clicked row inclusive. The anchor is *not* moved — successive
//!   shift-clicks pivot off the same anchor, matching native UI
//!   conventions.
//!
//! ## Keyed by stable row id, not position
//!
//! Each selected row is identified by a **stable row id** (`u64`)
//! supplied by the host's row-id projector — *not* by its
//! position in the row slice. This is the same `getRowId` contract every
//! production grid uses (`TanStack` Table, AG Grid, Kendo): because the
//! host now owns row order (sorting and filtering both reorder or trim the
//! slice), a positional key would point at a *different* row after any sort
//! or filter change. Keying on a stable id makes the selection **follow its
//! rows** across reordering for free — the documented failure mode of index
//! keying is exactly the bug this avoids.
//!
//! Ids are `u64` so 1M+ row grids fit; the underlying [`BTreeSet`] keeps
//! them sorted and de-duplicated. (The de-dup order is *id* order, which
//! is the host's responsibility to make meaningful for clipboard copy —
//! e.g. assign ids in natural row order.) We deliberately avoid
//! `HashSet`: deterministic iteration order matters for copy.

use std::collections::BTreeSet;

/// Stable ids of the selected rows plus the anchor id used to seed
/// shift-extend operations. The anchor is an *id*, so it stays pinned to
/// its row across sort/filter reordering.
#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    rows: BTreeSet<u64>,
    anchor: Option<u64>,
}

impl SelectionState {
    /// Creates an empty selection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes every selected row and clears the anchor.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.anchor = None;
    }

    /// Plain click: replace selection with `[id]`; anchor at `id`.
    pub fn replace_with(&mut self, id: u64) {
        self.rows.clear();
        self.rows.insert(id);
        self.anchor = Some(id);
    }

    /// Ctrl/Cmd-click: toggle membership of `id`; move anchor to
    /// `id` so a subsequent shift-extend pivots from there.
    pub fn toggle(&mut self, id: u64) {
        if !self.rows.insert(id) {
            self.rows.remove(&id);
        }
        self.anchor = Some(id);
    }

    /// Shift-click: replace the selection with the row ids spanning the
    /// **visual** range between the anchor and the clicked row.
    ///
    /// Because rows arrive in display order, the *grid* computes which
    /// ids fall between the anchor's on-screen position and the target's
    /// (inclusive) and passes them here — so the range follows what the
    /// user sees, regardless of sort or filter. The anchor is
    /// intentionally left in place: successive shift-clicks pivot off the
    /// original anchor, matching native UI conventions.
    ///
    /// Replacing (rather than unioning) matches the spreadsheet model:
    /// a shift-click defines the whole range from the anchor, it doesn't
    /// accrete. An empty iterator clears the row set (the anchor is left
    /// untouched — it's independent of which rows are selected).
    pub fn extend_range(&mut self, ids: impl IntoIterator<Item = u64>) {
        self.rows.clear();
        self.rows.extend(ids);
    }

    /// Returns whether the row with `id` is currently selected.
    #[must_use]
    pub fn contains(&self, id: u64) -> bool {
        self.rows.contains(&id)
    }

    /// Iterates the selected row ids in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.rows.iter().copied()
    }

    /// Number of selected rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// `true` when no rows are selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The current anchor, if any.
    #[must_use]
    pub fn anchor(&self) -> Option<u64> {
        self.anchor
    }
}

#[cfg(test)]
mod tests {
    use super::SelectionState;

    #[test]
    fn replace_with_resets_selection_and_anchor() {
        let mut s = SelectionState::new();
        s.replace_with(5);
        s.replace_with(10);
        assert!(s.contains(10));
        assert!(!s.contains(5));
        assert_eq!(s.anchor(), Some(10));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut s = SelectionState::new();
        s.toggle(3);
        s.toggle(7);
        s.toggle(3);
        assert!(!s.contains(3));
        assert!(s.contains(7));
        assert_eq!(s.anchor(), Some(3));
    }

    #[test]
    fn extend_range_replaces_with_the_supplied_ids() {
        let mut s = SelectionState::new();
        s.replace_with(10);
        // The grid resolves the visual range to a set of ids (here the
        // ids needn't be contiguous — they're whatever rows span the
        // on-screen range between anchor and target).
        s.extend_range([10, 42, 7, 99]);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![7, 10, 42, 99]);
        // Anchor stays put for further shift-extends.
        assert_eq!(s.anchor(), Some(10));
    }

    #[test]
    fn extend_range_supersedes_a_prior_range() {
        let mut s = SelectionState::new();
        s.replace_with(5);
        s.extend_range([5, 6, 7]);
        // A second shift-click redefines the whole range from the anchor
        // rather than accreting.
        s.extend_range([5, 4, 3]);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(s.anchor(), Some(5), "anchor unchanged across shift-clicks");
    }

    #[test]
    fn extend_range_empty_clears_rows_but_keeps_anchor() {
        let mut s = SelectionState::new();
        s.replace_with(8);
        s.extend_range(std::iter::empty());
        assert!(s.is_empty());
        // Anchor is independent of the row set.
        assert_eq!(s.anchor(), Some(8));
    }

    #[test]
    fn clear_removes_everything() {
        let mut s = SelectionState::new();
        s.replace_with(1);
        s.extend_range([1, 2, 3, 4, 5]);
        s.clear();
        assert!(s.is_empty());
        assert_eq!(s.anchor(), None);
    }
}
