//! Per-column width overrides for [`super::data_grid`].
//!
//! Columns have a default pixel width on their [`ColumnDef`](super::column::ColumnDef).
//! [`ColumnWidths`] holds optional *overrides* keyed by
//! [`ColumnId`]; the grid resolves an
//! **effective width** for each column as `override(id)` if present, else
//! the column's default. This is the shared model behind both horizontal
//! scroll (total content width) and drag-to-resize (resize mutates an
//! override).
//!
//! Keyed by [`ColumnId`] rather than position so
//! a width override stays attached to its column when the host reorders
//! or hides columns (the same stable-identity contract as
//! [`SortState`](super::sort::SortState) /
//! [`FilterState`](super::filter::FilterState) /
//! [`SelectionState`](super::SelectionState)). Held in host
//! state and read by the grid through a snapshot: the grid never owns the
//! data, it owns the *interaction* state via a lens.
//!
//! Widths are clamped to [`MIN_COLUMN_WIDTH`] so a column can't be
//! resized to an unusable sliver (or negative).

use std::collections::BTreeMap;

use super::column::ColumnId;

/// Smallest width a column may be resized to, in pixels. Keeps a column
/// from collapsing to an unclickable sliver.
pub const MIN_COLUMN_WIDTH: f64 = 32.0;

/// Per-column width overrides ([`ColumnId`] → width in px). A column
/// with no entry uses its [`ColumnDef`](super::column::ColumnDef)
/// default width. Empty ⇒ every column is at its default (the
/// no-resize baseline).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColumnWidths {
    overrides: BTreeMap<ColumnId, f64>,
}

impl ColumnWidths {
    /// An empty override set — every column renders at its default width.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `column`'s width override, clamped to at least
    /// [`MIN_COLUMN_WIDTH`]. Non-finite values are ignored.
    pub fn set(&mut self, column: ColumnId, width: f64) {
        if !width.is_finite() {
            return;
        }
        self.overrides.insert(column, width.max(MIN_COLUMN_WIDTH));
    }

    /// Adjusts `column`'s width by `delta` px, starting from `base`
    /// (the column's current effective width). The result is clamped to
    /// [`MIN_COLUMN_WIDTH`]. This is the resize entry point: the handle
    /// reports a drag delta and the column's pre-drag width.
    pub fn resize_by(&mut self, column: ColumnId, base: f64, delta: f64) {
        self.set(column, base + delta);
    }

    /// Clears `column`'s override, restoring its default width.
    pub fn clear(&mut self, column: &ColumnId) {
        self.overrides.remove(column);
    }

    /// Clears every override.
    pub fn clear_all(&mut self) {
        self.overrides.clear();
    }

    /// The override for `column`, if any (already clamped).
    #[must_use]
    pub fn get(&self, column: &ColumnId) -> Option<f64> {
        self.overrides.get(column).copied()
    }

    /// The effective width for `column`: its override if set, else
    /// `default_width` — clamped to at least [`MIN_COLUMN_WIDTH`], so a
    /// `ColumnDef` default below the floor (overrides are clamped at
    /// [`Self::set`] already) can't produce an unusably-thin column.
    /// A NaN default also resolves to the floor.
    #[must_use]
    pub fn effective(&self, column: &ColumnId, default_width: f64) -> f64 {
        self.get(column)
            .unwrap_or(default_width)
            .max(MIN_COLUMN_WIDTH)
    }

    /// `true` when no column has an override (the default layout).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ColumnId, ColumnWidths, MIN_COLUMN_WIDTH};

    /// Float comparison with a tolerance — widths are computed via
    /// `max`/addition so exact bit-equality isn't guaranteed, and
    /// clippy's `float_cmp` (pedantic) forbids `==` on f64 anyway.
    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn id(s: &str) -> ColumnId {
        ColumnId::from(s)
    }

    #[test]
    fn effective_falls_back_to_default() {
        let w = ColumnWidths::new();
        assert!(w.is_empty());
        assert!(approx(w.effective(&id("price"), 90.0), 90.0));
    }

    #[test]
    fn set_overrides_default() {
        let mut w = ColumnWidths::new();
        w.set(id("size"), 150.0);
        assert!(approx(w.effective(&id("size"), 90.0), 150.0));
        assert!(
            approx(w.effective(&id("price"), 90.0), 90.0),
            "other columns untouched"
        );
    }

    #[test]
    fn effective_clamps_default_below_min() {
        // A ColumnDef default below the floor resolves to
        // MIN_COLUMN_WIDTH — same clamp `set` applies to overrides, so
        // no path can produce an unusably-thin (or invisible) column.
        let w = ColumnWidths::new();
        assert!(approx(w.effective(&id("tiny"), 4.0), MIN_COLUMN_WIDTH));
    }

    #[test]
    fn set_clamps_to_min() {
        let mut w = ColumnWidths::new();
        w.set(id("price"), 5.0);
        assert!(approx(w.effective(&id("price"), 90.0), MIN_COLUMN_WIDTH));
    }

    #[test]
    fn set_ignores_non_finite() {
        let mut w = ColumnWidths::new();
        w.set(id("price"), f64::NAN);
        w.set(id("price"), f64::INFINITY);
        assert!(w.is_empty(), "non-finite widths are rejected");
    }

    #[test]
    fn resize_by_delta_from_base_then_clamps() {
        let mut w = ColumnWidths::new();
        w.resize_by(id("price"), 100.0, 40.0);
        assert!(approx(w.effective(&id("price"), 90.0), 140.0));
        // Dragging far left clamps rather than going below the minimum.
        w.resize_by(id("price"), 100.0, -500.0);
        assert!(approx(w.effective(&id("price"), 90.0), MIN_COLUMN_WIDTH));
    }

    #[test]
    fn clear_restores_default() {
        let mut w = ColumnWidths::new();
        w.set(id("price"), 200.0);
        w.clear(&id("price"));
        assert!(approx(w.effective(&id("price"), 90.0), 90.0));
        assert!(w.is_empty());
    }
}
