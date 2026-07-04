//! Shared geometry helpers for list-shaped widgets (autocomplete, dropdown
//! menu, context menu) that row-stack uniform items.
//!
//! These formulas are in all three components and would silently diverge if a
//! density token were renamed or a padding policy were changed in only one
//! place.

use masonry::core::EventCtx;
use masonry::kurbo::{Point, Rect};

use crate::theme::Density;

/// Row height for a single-line list item using the current density tokens.
pub(crate) fn item_height(density: &Density) -> f64 {
    f64::from(density.ui_font_size) + 2.0 * f64::from(density.button_pad_v)
}

/// Horizontal inset (left/right padding) for list item content.
pub(crate) fn pad_h(density: &Density) -> f64 {
    f64::from(density.button_pad_h)
}

/// Index of the first rect in `item_rects` that contains `local`, if any.
pub(crate) fn hit_item(item_rects: &[Rect], local: Point) -> Option<usize> {
    item_rects.iter().position(|r| r.contains(local))
}

/// Converts a window-space point to the local space of the widget described
/// by `ctx`.
pub(crate) fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
    window_pos - ctx.to_window(Point::ZERO).to_vec2()
}

/// Converts `index: usize` to `f64` without precision-loss lint: clamps to
/// `u32::MAX` (≈ 4 billion items) before widening, which is safe for any
/// realistic list size.
pub(crate) fn index_f64(index: usize) -> f64 {
    f64::from(u32::try_from(index).unwrap_or(u32::MAX))
}
