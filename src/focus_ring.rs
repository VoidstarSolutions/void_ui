//! Shared focus-ring stroke for interactive widgets.
//!
//! Several components (slider thumbs, radio circles, checkbox boxes, ...)
//! draw a themed ring around themselves when focused. Each computes its own
//! geometry — a circle outset from a thumb, a rounded rect inset around a
//! box — because that shape is intrinsic to the component. What's *not*
//! intrinsic, and was previously redeclared (and could silently drift) in
//! each widget, is the stroke width and color used to paint that shape once
//! it's computed. [`paint_focus_ring`] centralizes that so the design
//! system's focus treatment can be changed in one place.

use masonry::imaging::{PaintShape, Painter};
use masonry::kurbo::{Point, Rect, Size, Stroke};

use crate::Theme;

/// Stroke width of a focus ring, in logical pixels — a deliberate constant
/// accessibility affordance that must stay visible at compact density, so it
/// doesn't scale with density.
pub const FOCUS_RING_WIDTH: f64 = 1.5;

/// How far outside a widget's geometry the focus ring sits, in logical pixels.
/// Used by circular/tab-indicator shapes (slider thumbs, radio buttons, tab
/// pills) that draw the ring as an outset around the visual shape. Focus-ring
/// chrome, not density-scaled.
pub const FOCUS_RING_OUTSET: f64 = 2.0;

/// How far inside a widget's border-box the focus ring sits, in logical pixels.
/// Used by box-shaped form controls (checkboxes, toggles, row items) where the
/// ring is an inset overlay within the widget's own layout footprint.
/// Focus-ring chrome, not density-scaled.
pub const FOCUS_RING_INSET: f64 = 1.5;

/// Strokes `shape` as a focus ring in the theme's focus color.
///
/// Callers are responsible for both computing `shape` (a ring outset from a
/// circular thumb, an inset rounded rect around a box, ...) and gating the
/// call on `ctx.is_focus_target() && !disabled` — this only owns the paint
/// treatment shared across components, not the per-shape geometry or the
/// decision to draw at all.
pub fn paint_focus_ring<S>(painter: &mut Painter<'_>, shape: S, theme: &Theme)
where
    S: for<'a> PaintShape<'a>,
{
    painter
        .stroke(shape, &Stroke::new(FOCUS_RING_WIDTH), theme.palette.focus)
        .draw();
}

/// Paints a focus ring inset [`FOCUS_RING_INSET`] into a plain axis-aligned
/// `size` — the ring shape for widgets whose focusable area is their whole
/// (unrounded) border box. Callers still gate the call on
/// `ctx.is_focus_target() && !disabled`; widgets whose ring needs rounding
/// or a sub-rect (checkboxes, toggles, buttons) compute their own shape and
/// call [`paint_focus_ring`] directly instead.
pub fn paint_focus_ring_inset(painter: &mut Painter<'_>, size: Size, theme: &Theme) {
    let inset = FOCUS_RING_INSET;
    let rect = Rect::from_origin_size(
        Point::new(inset, inset),
        Size::new(
            (size.width - 2.0 * inset).max(0.0),
            (size.height - 2.0 * inset).max(0.0),
        ),
    );
    paint_focus_ring(painter, rect, theme);
}
