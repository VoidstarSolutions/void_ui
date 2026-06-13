//! Programmatic vertical scrolling for the virtualized collection
//! components.
//!
//! `ScrollState` is a host-owned request token: the host keeps one in
//! its app state, calls [`ScrollState::scroll_to_index`] from any
//! callback, and passes a snapshot to the grid via `DataGrid::scroll_to`.
//! The grid's body wrapper (`ScrollToView`) compares the snapshot's
//! generation against the last one it applied and, when they differ,
//! re-anchors masonry's `VirtualScroll` so the requested row's top aligns
//! with the top of the viewport.
//!
//! The index is a **display position** (slice position in the host's
//! ordered view, the same domain as `row_count`) — not a stable row id.

/// Host-owned scroll-request token for the virtualized collection
/// components.
///
/// The generation counter makes requests re-triggerable: calling
/// [`Self::scroll_to_index`] with the same index again still produces a
/// distinct request (user scrolled away; host snaps back to the same
/// row). The default value (generation 0) is "no request ever made", so
/// a grid that never receives a request never re-anchors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollState {
    /// Bumped on every request; the grid applies a snapshot when this
    /// differs from the generation it last applied.
    generation: u64,
    /// Requested row (display position). Clamped to the row range at
    /// apply time.
    index: u64,
}

impl ScrollState {
    /// An empty scroll state (no pending request).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests that row `index` (display position) be scrolled to the
    /// top of the viewport on the grid's next rebuild. Re-calling with
    /// the same index re-triggers. An out-of-range index clamps to the
    /// last row at apply time; a request against an empty grid is a
    /// no-op.
    pub fn scroll_to_index(&mut self, index: u64) {
        self.generation = self.generation.wrapping_add(1);
        self.index = index;
    }

    /// The current request generation. Bumped on every
    /// [`Self::scroll_to_index`]; the grid applies a snapshot when this
    /// differs from the generation it last applied.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// The requested row (display position). Clamped to the row range at
    /// apply time.
    pub(crate) fn index(&self) -> u64 {
        self.index
    }
}

/// Clamps a requested scroll index to `0..row_count`, converted to the
/// `i64` anchor domain masonry's `VirtualScroll` uses (saturating, same
/// rationale as `scroll_range_end` in `view.rs`). `None` when the grid
/// is empty — there is no row to anchor to.
pub(crate) fn clamp_scroll_index(index: u64, row_count: u64) -> Option<i64> {
    if row_count == 0 {
        return None;
    }
    let clamped = index.min(row_count - 1);
    Some(i64::try_from(clamped).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::{ScrollState, clamp_scroll_index};

    #[test]
    fn default_state_has_no_pending_request() {
        // Generation 0 == "never requested"; two defaults are equal, so
        // the grid's generation comparison sees no change.
        assert_eq!(ScrollState::new(), ScrollState::default());
        assert_eq!(ScrollState::new().generation, 0);
    }

    #[test]
    fn scroll_to_index_bumps_generation_and_stores_index() {
        let mut s = ScrollState::new();
        s.scroll_to_index(42);
        assert_eq!(s.generation, 1);
        assert_eq!(s.index, 42);
    }

    #[test]
    fn same_index_retriggers_via_new_generation() {
        // The re-trigger contract: requesting the same row twice must
        // still produce distinct snapshots (the user may have scrolled
        // away in between).
        let mut s = ScrollState::new();
        s.scroll_to_index(7);
        let first = s;
        s.scroll_to_index(7);
        assert_ne!(s, first);
        assert_eq!(s.index, first.index);
    }

    #[test]
    fn clamp_passes_in_range_index_through() {
        assert_eq!(clamp_scroll_index(10, 100), Some(10));
        assert_eq!(clamp_scroll_index(0, 1), Some(0));
    }

    #[test]
    fn clamp_caps_past_the_end_to_last_row() {
        assert_eq!(clamp_scroll_index(100, 100), Some(99));
        assert_eq!(clamp_scroll_index(u64::MAX, 5), Some(4));
    }

    #[test]
    fn clamp_is_none_for_empty_grid() {
        assert_eq!(clamp_scroll_index(0, 0), None);
        assert_eq!(clamp_scroll_index(123, 0), None);
    }
}
