//! Text-width measurement for drag-to-resize *content floors*.
//!
//! Drag-to-resize lets a column be dragged narrower than its content,
//! which clips the cells (see the resize adapter in [`super::view`]). To
//! stop that, the adapter floors each column at the width of its widest
//! cell. Computing that floor needs the *rendered* width of every cell's
//! text — not a glyph-count estimate — so this module lays the text out
//! with parley at the cell's font size, exactly as the `text_column`
//! label does, and reads the resulting advance.
//!
//! Measurement is authoritative but not free, so two thread-local caches
//! keep it off the hot path:
//!
//! - [`text_width`] memoizes `(font_size, text) -> width`, so a value
//!   that recurs across rows (or across successive drags) is laid out
//!   once.
//! - [`column_floor`] memoizes the finished per-column floor keyed by
//!   `(ColumnId, row_count)`. The row count makes it self-invalidate when
//!   the dataset grows or shrinks, while an in-flight drag (row count
//!   fixed) reuses an `O(1)` value instead of re-scanning every row on
//!   each pointer-move. This mirrors the grid's existing rule of keeping
//!   `O(rows)` work off the per-rebuild path (see `CopyOnShortcutView`).

use std::cell::RefCell;
use std::collections::HashMap;

use masonry::core::{BrushIndex, StyleProperty};
use masonry::parley::{FontContext, Layout, LayoutContext};

use super::column::ColumnId;

/// Per-thread parley contexts plus the two memo tables. Parley's
/// `FontContext`/`LayoutContext` are reused across measurements (building
/// them is expensive); a `thread_local` keeps them off any shared-state
/// path and matches how masonry drives text layout on the UI thread.
struct Measurer {
    fonts: FontContext,
    layouts: LayoutContext<BrushIndex>,
    /// `(font_size_bits, text) -> width_px`.
    widths: HashMap<(u32, String), f32>,
    /// `(column, row_count) -> floor_px`.
    floors: HashMap<(ColumnId, u64), f64>,
}

thread_local! {
    static MEASURER: RefCell<Measurer> = RefCell::new(Measurer {
        fonts: FontContext::new(),
        layouts: LayoutContext::new(),
        widths: HashMap::new(),
        floors: HashMap::new(),
    });
}

/// Rendered width, in px, of `text` laid out on a single line at
/// `font_size` in the default font family — matching a `text_column`
/// cell label. Empty text is `0.0`. Memoized per `(font_size, text)`.
pub(crate) fn text_width(text: &str, font_size: f32) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    MEASURER.with_borrow_mut(|m| {
        let key = (font_size.to_bits(), text.to_owned());
        if let Some(w) = m.widths.get(&key) {
            return f64::from(*w);
        }
        let Measurer {
            fonts,
            layouts,
            widths,
            ..
        } = m;
        let mut layout: Layout<BrushIndex> = Layout::default();
        let mut builder = layouts.ranged_builder(fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::Brush(BrushIndex(0)));
        builder.build_into(&mut layout, text);
        // `None` = no wrap width: a single line whose `width()` is the
        // full advance, which is exactly the content width we floor to.
        layout.break_all_lines(None);
        let w = layout.width();
        widths.insert(key, w);
        f64::from(w)
    })
}

/// The drag-resize floor for `column` at the current `row_count`,
/// computing it via `compute` on a cache miss and memoizing the result.
///
/// `compute` returns the finished floor (widest cell width plus chrome)
/// and runs at most once per `(column, row_count)` — so a drag, which
/// fires many pointer-moves at a fixed row count, pays the `O(rows)` scan
/// only on its first move. The borrow is released before `compute` runs,
/// so `compute` may call [`text_width`] without a re-entrant borrow.
pub(crate) fn column_floor(
    column: &ColumnId,
    row_count: u64,
    compute: impl FnOnce() -> f64,
) -> f64 {
    let key = (column.clone(), row_count);
    if let Some(f) = MEASURER.with_borrow(|m| m.floors.get(&key).copied()) {
        return f;
    }
    let floor = compute();
    MEASURER.with_borrow_mut(|m| {
        m.floors.insert(key, floor);
    });
    floor
}

#[cfg(test)]
mod tests {
    use super::{column_floor, text_width};
    use crate::components::data_grid::column::ColumnId;

    #[test]
    fn empty_text_is_zero_width() {
        assert!((text_width("", 14.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn wider_text_measures_wider() {
        // A longer string of the same characters must be at least as wide;
        // in practice strictly wider. Uses a fresh, deterministic default
        // font context — no assertion on absolute pixels (font-dependent).
        let short = text_width("$1.72", 14.0);
        let long = text_width("$1239.91", 14.0);
        assert!(short > 0.0, "non-empty text has positive width");
        assert!(long > short, "more glyphs => wider: {long} !> {short}");
    }

    #[test]
    fn column_floor_memoizes_by_row_count() {
        use std::cell::Cell;

        // A unique id keeps this independent of any other test's cache
        // entries sharing the process-wide thread-local.
        let id = ColumnId::from("column_floor_memoizes_by_row_count");
        let calls = Cell::new(0u32);
        let compute = || {
            calls.set(calls.get() + 1);
            100.0
        };

        assert!((column_floor(&id, 3, compute) - 100.0).abs() < f64::EPSILON);
        // Same (column, row_count) => cache hit, `compute` not re-run.
        assert!((column_floor(&id, 3, compute) - 100.0).abs() < f64::EPSILON);
        assert_eq!(calls.get(), 1, "second call with same row_count is cached");
        // A different row count recomputes (dataset changed size).
        let _ = column_floor(&id, 4, compute);
        assert_eq!(calls.get(), 2, "new row_count recomputes the floor");
    }
}
