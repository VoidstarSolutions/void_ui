//! Stable-id keying and the integer-domain conversions shared by the
//! virtualized collection components.

use std::sync::Arc;

/// How a collection derives a row's stable id: the host's projector, or a
/// fallback to the item's current slice position.
pub(crate) enum IdSource<Item> {
    /// Host-supplied id projector.
    Explicit(Arc<dyn Fn(&Item) -> u64 + Send + Sync>),
    /// No projector: use the item's current slice position as its id.
    Position,
}

// Hand-written so the bound is on the `Arc` (always `Clone`), not on `Item` —
// a derived `Clone` would wrongly require `Item: Clone`.
impl<Item> Clone for IdSource<Item> {
    fn clone(&self) -> Self {
        match self {
            Self::Explicit(f) => Self::Explicit(Arc::clone(f)),
            Self::Position => Self::Position,
        }
    }
}

impl<Item> IdSource<Item> {
    /// Resolving needs both candidates available — the explicit projector reads
    /// the item, the fallback uses the position — so `id_of` takes both and picks.
    pub(crate) fn id_of(&self, pos: usize, item: &Item) -> u64 {
        match self {
            Self::Explicit(f) => f(item),
            Self::Position => position_fallback_id(pos),
        }
    }
}

/// Slice position (`usize`) → stable id (`u64`) for the position
/// fallback. Saturates to `u64::MAX`.
pub(crate) fn position_fallback_id(pos: usize) -> u64 {
    u64::try_from(pos).unwrap_or(u64::MAX)
}

/// `virtual_scroll` length bound: item count (`u64`) → `usize`. Saturates
/// to `usize::MAX`.
pub(crate) fn scroll_range_end(item_count: u64) -> usize {
    usize::try_from(item_count).unwrap_or(usize::MAX)
}

/// Lazy-load trigger predicate: `true` when the active range's end
/// (`active_end`, the exclusive end of the materialized window, same
/// `usize` domain as `scroll_range_end`) is within `threshold` items of the
/// data's end. The body's `message` handler peeks each `VirtualScrollAction`
/// and fires the host's lazy callback whenever this holds.
///
/// Saturating throughout: `item_count` past `usize::MAX` and an oversized
/// `threshold` both clamp to `usize::MAX`, and the distance uses
/// `saturating_sub` so an `active_end` past the (clamped) end reads as
/// distance 0 — i.e. "at the end", which always triggers.
pub(crate) fn nearing_end(item_count: u64, active_end: usize, threshold: u64) -> bool {
    let end = scroll_range_end(item_count);
    let threshold = usize::try_from(threshold).unwrap_or(usize::MAX);
    end.saturating_sub(active_end) <= threshold
}

/// Resolves the stable ids spanning the visual range between `anchor_id`
/// and `target_id` (inclusive) in current display order, or `None` when
/// either endpoint is absent. O(n) over the slice; called only on a
/// shift-click.
pub(crate) fn visual_range_ids<Item>(
    data: &[Item],
    id_source: &IdSource<Item>,
    anchor_id: u64,
    target_id: u64,
) -> Option<Vec<u64>> {
    let mut anchor_pos = None;
    let mut target_pos = None;
    for (pos, item) in data.iter().enumerate() {
        let id = id_source.id_of(pos, item);
        if id == anchor_id {
            anchor_pos = Some(pos);
        }
        if id == target_id {
            target_pos = Some(pos);
        }
    }
    let (a, t) = (anchor_pos?, target_pos?);
    let (lo, hi) = if a <= t { (a, t) } else { (t, a) };
    Some(
        (lo..=hi)
            .map(|pos| id_source.id_of(pos, &data[pos]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{nearing_end, visual_range_ids, IdSource};
    use std::sync::Arc;

    /// Row id == the row value itself, so test slices read naturally:
    /// `[10, 20, 30]` are rows with ids 10/20/30 in that display order.
    fn id_is_value() -> IdSource<u64> {
        IdSource::Explicit(Arc::new(|r: &u64| *r))
    }

    #[test]
    fn visual_range_spans_anchor_to_target_in_display_order() {
        let data = [10_u64, 20, 30, 40, 50];
        // Anchor at id 20 (pos 1), target id 40 (pos 3): inclusive span.
        let ids = visual_range_ids(&data, &id_is_value(), 20, 40);
        assert_eq!(ids, Some(vec![20, 30, 40]));
    }

    #[test]
    fn visual_range_is_orientation_independent() {
        let data = [10_u64, 20, 30, 40, 50];
        // Target *above* the anchor yields the same span, in display order.
        let ids = visual_range_ids(&data, &id_is_value(), 40, 20);
        assert_eq!(ids, Some(vec![20, 30, 40]));
    }

    #[test]
    fn visual_range_single_row_when_anchor_equals_target() {
        let data = [10_u64, 20, 30];
        assert_eq!(
            visual_range_ids(&data, &id_is_value(), 20, 20),
            Some(vec![20])
        );
    }

    #[test]
    fn visual_range_none_when_anchor_filtered_out() {
        // Simulate a filtered view: id 20 (a prior anchor) is no longer
        // present. The range can't be resolved → None, so the caller
        // reseats the anchor with a plain select instead of wedging.
        let data = [10_u64, 30, 40, 50];
        assert_eq!(visual_range_ids(&data, &id_is_value(), 20, 40), None);
    }

    #[test]
    fn visual_range_none_when_target_missing() {
        let data = [10_u64, 20, 30];
        assert_eq!(visual_range_ids(&data, &id_is_value(), 10, 99), None);
    }

    #[test]
    fn visual_range_position_fallback_ids_are_slice_positions() {
        // With no explicit projector, the id *is* the slice position.
        // Anchor pos 1, target pos 3 → ids [1, 2, 3].
        let data = ["a", "b", "c", "d", "e"];
        let ids = visual_range_ids(&data, &IdSource::Position, 1, 3);
        assert_eq!(ids, Some(vec![1, 2, 3]));
    }

    #[test]
    fn nearing_end_fires_only_within_threshold_of_the_end() {
        // 1000 items, fire within 20 of the end. An active window ending at
        // 979 is 21 away — too far. At 980 it's exactly 20 — fires.
        assert!(!nearing_end(1000, 979, 20));
        assert!(nearing_end(1000, 980, 20));
        // At or past the end always fires (distance saturates to 0).
        assert!(nearing_end(1000, 1000, 20));
        assert!(nearing_end(1000, 1001, 20));
    }

    #[test]
    fn nearing_end_with_zero_threshold_fires_only_at_the_very_end() {
        assert!(!nearing_end(10, 9, 0));
        assert!(nearing_end(10, 10, 0));
    }

    #[test]
    fn nearing_end_saturates_for_huge_counts_and_thresholds() {
        // item_count past i64::MAX clamps to i64::MAX; an active end near it
        // is still "far" unless the threshold is also huge.
        assert!(!nearing_end(u64::MAX, 0, 20));
        assert!(nearing_end(u64::MAX, 0, u64::MAX));
    }
}
