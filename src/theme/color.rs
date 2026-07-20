//! Color helpers used by the `void_ui` theme.
//!
//! Theme colors are authored in OKLCH (`oklch(l c h / a)`).
//! Linebender's `color` crate (re-exported through `masonry::peniko::color`)
//! converts that to sRGB for us; these wrappers exist so palette tables read
//! the same shape as the CSS — `oklch(0.74, 0.090, 195.0)` — without
//! repeating the `OpaqueColor::<Oklch>::new([..]).convert::<Srgb>()` dance.

use masonry::peniko::Color;
use masonry::peniko::color::{Oklch, OpaqueColor, Srgb};

/// Fully opaque sRGB color from OKLCH components.
///
/// `l` is lightness in `[0, 1]`, `c` is chroma (typically `0.0..=0.4` for
/// in-gamut colors), `h` is hue in degrees `[0, 360)`. Mirrors CSS
/// `oklch(l c h)`.
#[must_use]
pub fn oklch(l: f32, c: f32, h: f32) -> Color {
    OpaqueColor::<Oklch>::new([l, c, h])
        .convert::<Srgb>()
        .with_alpha(1.0)
}

/// Translucent sRGB color from OKLCH components and alpha in `[0, 1]`.
/// Mirrors CSS `oklch(l c h / a)`.
#[must_use]
pub fn oklcha(l: f32, c: f32, h: f32, a: f32) -> Color {
    OpaqueColor::<Oklch>::new([l, c, h])
        .convert::<Srgb>()
        .with_alpha(a)
}
