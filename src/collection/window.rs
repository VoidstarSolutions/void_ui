//! Global-index <-> materialized-slot conversion for a `VirtualScroll`'s
//! currently materialized window.
//!
//! Shared by `CollectionBodyWidget` (`body.rs`) and `CollectionListWidget`
//! (`imperative_list.rs`), which each track a materialized window's
//! `active_start` their own way (a call parameter vs. a widget field — see
//! each type's own doc comment for why) but were each hand-rolling the same
//! bounds-check arithmetic to convert between a global item index and its
//! position among the currently materialized `VirtualScroll` children.

/// The currently materialized slice of a virtualized item list, described
/// by its starting global index (`VirtualScroll` reports only the
/// materialized *count*, not this offset — see `imperative_list.rs`'s
/// module doc for why it must be tracked externally).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MaterializedWindow {
    active_start: usize,
}

impl MaterializedWindow {
    /// A window starting at global index `active_start`.
    pub(crate) fn new(active_start: usize) -> Self {
        Self { active_start }
    }

    /// Updates the window's starting global index.
    pub(crate) fn set_active_start(&mut self, active_start: usize) {
        self.active_start = active_start;
    }

    /// The materialized slot for global index `idx`, given how many rows
    /// are currently materialized (`VirtualScroll::children_ids().len()`),
    /// or `None` if `idx` isn't currently materialized.
    pub(crate) fn slot_for(&self, materialized_count: usize, idx: usize) -> Option<usize> {
        if !(self.active_start..self.active_start + materialized_count).contains(&idx) {
            return None;
        }
        idx.checked_sub(self.active_start)
    }

    /// The global index at materialized slot `slot`.
    pub(crate) fn index_for_slot(&self, slot: usize) -> usize {
        self.active_start + slot
    }
}

#[cfg(test)]
mod tests {
    use super::MaterializedWindow;

    #[test]
    fn slot_for_returns_the_offset_within_the_window() {
        let window = MaterializedWindow::new(10);
        assert_eq!(window.slot_for(5, 10), Some(0));
        assert_eq!(window.slot_for(5, 12), Some(2));
        assert_eq!(window.slot_for(5, 14), Some(4));
    }

    #[test]
    fn slot_for_is_none_just_below_active_start() {
        let window = MaterializedWindow::new(10);
        assert_eq!(window.slot_for(5, 9), None);
    }

    #[test]
    fn slot_for_is_none_at_and_past_the_window_end() {
        let window = MaterializedWindow::new(10);
        assert_eq!(window.slot_for(5, 15), None);
        assert_eq!(window.slot_for(5, 100), None);
    }

    #[test]
    fn slot_for_is_none_for_an_empty_window_regardless_of_active_start() {
        let window = MaterializedWindow::new(10);
        assert_eq!(window.slot_for(0, 10), None);
        assert_eq!(window.slot_for(0, 0), None);
    }

    #[test]
    fn index_for_slot_offsets_from_active_start() {
        let window = MaterializedWindow::new(10);
        assert_eq!(window.index_for_slot(0), 10);
        assert_eq!(window.index_for_slot(4), 14);
    }

    #[test]
    fn index_for_slot_round_trips_with_slot_for() {
        let window = MaterializedWindow::new(7);
        for slot in 0..5 {
            let idx = window.index_for_slot(slot);
            assert_eq!(window.slot_for(5, idx), Some(slot));
        }
    }

    #[test]
    fn zero_active_start_is_the_common_case() {
        let window = MaterializedWindow::new(0);
        assert_eq!(window.slot_for(3, 0), Some(0));
        assert_eq!(window.slot_for(3, 2), Some(2));
        assert_eq!(window.slot_for(3, 3), None);
    }
}
