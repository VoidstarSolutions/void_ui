//! Shared geometry helpers for list-shaped widgets (autocomplete, dropdown
//! menu, context menu) that row-stack uniform items, plus an axis-generic
//! placed-or-estimate helper shared by `tabs` and `sidebar`'s nav widget
//! (see [`item_rect_or_estimate`]).
//!
//! These formulas are in all three components and would silently diverge if a
//! density token were renamed or a padding policy were changed in only one
//! place.

use masonry::core::EventCtx;
use masonry::kurbo::{Axis, Point, Rect};

use crate::theme::Density;

/// Row height for a single-line list item using the current density tokens.
pub(crate) fn item_height(density: &Density) -> f64 {
    f64::from(density.ui_font_size) + 2.0 * f64::from(density.button_pad_v)
}

/// Horizontal inset (left/right padding) for list item content.
pub(crate) fn pad_h(density: &Density) -> f64 {
    f64::from(density.button_pad_h)
}

/// Vertical inset above the first and below the last row of a floating menu
/// surface (autocomplete suggestion list, dropdown menu, context menu):
/// two-thirds of the small gap token. Multiply-then-divide keeps the
/// balanced value exact (6 × 2 / 3 = the pre-token 4.0).
pub(crate) fn menu_pad_v(density: &Density) -> f64 {
    f64::from(density.gap) * 2.0 / 3.0
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

/// Geometry [`item_rect_or_estimate`] needs to place an item that hasn't been
/// laid out yet.
#[derive(Clone, Copy)]
pub(crate) struct EstimateGeometry {
    /// Axis items are stacked along (`tabs`: horizontal, sidebar nav: vertical).
    pub axis: Axis,
    /// Leading offset before the first item, along `axis`.
    pub outer: f64,
    /// Offset on the cross axis (perpendicular to `axis`).
    pub cross_offset: f64,
    /// Estimated extent along `axis` — typically the real average from the
    /// last layout, falling back to a theme-metric guess before the first
    /// layout has ever run.
    pub main_extent: f64,
    /// Estimated extent on the cross axis. `0.0` for callers that only need
    /// the main-axis scroll target and don't care about a cross-axis span.
    pub cross_extent: f64,
    /// Gap between adjacent items.
    pub gap: f64,
}

/// Item rect for `index`: `placed[index]` if layout has already run, else an
/// estimate from `geometry`.
///
/// Shared by `tabs` and sidebar nav — both row-stack variable-size items
/// (along opposite axes) and need this fallback for keyboard scroll-into-view
/// right after `set_items` clears `placed`, before the next layout rebuilds
/// it.
pub(crate) fn item_rect_or_estimate(
    placed: &[Rect],
    index: usize,
    geometry: EstimateGeometry,
) -> Rect {
    if let Some(&rect) = placed.get(index) {
        return rect;
    }
    let main_pos = geometry.outer + index_f64(index) * (geometry.main_extent + geometry.gap);
    match geometry.axis {
        Axis::Horizontal => Rect::new(
            main_pos,
            geometry.cross_offset,
            main_pos + geometry.main_extent,
            geometry.cross_offset + geometry.cross_extent,
        ),
        Axis::Vertical => Rect::new(
            geometry.cross_offset,
            main_pos,
            geometry.cross_offset + geometry.cross_extent,
            main_pos + geometry.main_extent,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::menu_pad_v;
    use crate::theme::Density;

    #[test]
    fn menu_pad_v_scales_with_density_and_preserves_balanced() {
        // 6 × 2 / 3 = the pre-token LIST_PAD_V / MENU_PAD_V of 4.0.
        assert!((menu_pad_v(&Density::balanced()) - 4.0).abs() < 1e-6);
        assert!(menu_pad_v(&Density::compact()) < menu_pad_v(&Density::balanced()));
        assert!(menu_pad_v(&Density::balanced()) < menu_pad_v(&Density::airy()));
    }
}
