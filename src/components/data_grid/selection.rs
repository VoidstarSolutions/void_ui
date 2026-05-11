//! Row-selection state for [`super::data_grid`].
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
//! Indices are `u64` so 1M+ row grids fit; the underlying
//! [`BTreeSet`] keeps them sorted and de-duplicated, which is also the
//! order the clipboard projection wants. We deliberately avoid
//! `HashSet`: deterministic iteration order matters for copy.

use std::collections::BTreeSet;

/// Indices of the selected rows plus the anchor used to seed
/// shift-extend operations.
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

    /// Plain click: replace selection with `[idx]`; anchor at `idx`.
    pub fn replace_with(&mut self, idx: u64) {
        self.rows.clear();
        self.rows.insert(idx);
        self.anchor = Some(idx);
    }

    /// Ctrl/Cmd-click: toggle membership of `idx`; move anchor to
    /// `idx` so a subsequent shift-extend pivots from there.
    pub fn toggle(&mut self, idx: u64) {
        if !self.rows.insert(idx) {
            self.rows.remove(&idx);
        }
        self.anchor = Some(idx);
    }

    /// Shift-click: replace the selection with the inclusive range
    /// `[anchor, idx]` (or `[idx, anchor]`, whichever is ordered
    /// correctly). If no anchor is set, behave like
    /// [`Self::replace_with`].
    pub fn extend_to(&mut self, idx: u64) {
        let Some(anchor) = self.anchor else {
            self.replace_with(idx);
            return;
        };
        let (lo, hi) = if anchor <= idx {
            (anchor, idx)
        } else {
            (idx, anchor)
        };
        self.rows.clear();
        for i in lo..=hi {
            self.rows.insert(i);
        }
        // Anchor is intentionally left in place — successive
        // shift-clicks should pivot off the original anchor, matching
        // native UI conventions.
    }

    /// Returns whether the row at `idx` is currently selected.
    #[must_use]
    pub fn contains(&self, idx: u64) -> bool {
        self.rows.contains(&idx)
    }

    /// Iterates the selected row indices in ascending order.
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
    fn extend_to_fills_inclusive_range() {
        let mut s = SelectionState::new();
        s.replace_with(10);
        s.extend_to(13);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![10, 11, 12, 13]);
        // Anchor stays put for further shift-extends.
        assert_eq!(s.anchor(), Some(10));
    }

    #[test]
    fn extend_to_orders_ascending_when_target_below_anchor() {
        let mut s = SelectionState::new();
        s.replace_with(10);
        s.extend_to(7);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![7, 8, 9, 10]);
    }

    #[test]
    fn extend_to_without_anchor_becomes_replace() {
        let mut s = SelectionState::new();
        s.extend_to(4);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![4]);
        assert_eq!(s.anchor(), Some(4));
    }

    #[test]
    fn clear_removes_everything() {
        let mut s = SelectionState::new();
        s.replace_with(1);
        s.extend_to(5);
        s.clear();
        assert!(s.is_empty());
        assert_eq!(s.anchor(), None);
    }
}
