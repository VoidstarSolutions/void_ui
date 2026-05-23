//! Tessera-derived color palette.
//!
//! Field names mirror the CSS custom-property names in `styles.css`
//! (`--bg`, `--surface-2`, `--teal-soft`, etc.) so swapping a token in the
//! design source maps to one rename here.

use masonry::peniko::Color;

use super::color::{oklch, oklcha};

/// Surface / text / accent colors for one theme variant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    // Surfaces & lines
    pub bg: Color,
    pub bg_deep: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub surface_hi: Color,
    pub border: Color,
    pub border_strong: Color,
    pub grid: Color,
    pub grid_major: Color,

    // Text
    pub text: Color,
    pub text_muted: Color,
    pub text_faint: Color,

    // Accents
    pub teal: Color,
    pub teal_soft: Color,
    pub teal_deep: Color,
    pub coral: Color,
    pub coral_soft: Color,
    pub coral_deep: Color,
    pub amber: Color,
    pub amber_soft: Color,
    pub amber_deep: Color,
    pub violet: Color,
    pub violet_soft: Color,
    pub violet_deep: Color,
    pub green: Color,
    pub green_soft: Color,
    pub green_deep: Color,
    pub blue: Color,
    pub blue_soft: Color,
    pub blue_deep: Color,

    // Domain semantics (mapped to accents in source, kept as fields for
    // future themes that may diverge)
    pub target: Color,
    pub target_soft: Color,
    pub compare: Color,
}

impl Palette {
    /// Dark variant — Tessera's default `data-theme="dark"`.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            bg: oklch(0.18, 0.008, 240.0),
            bg_deep: oklch(0.14, 0.008, 240.0),
            surface: oklch(0.22, 0.008, 240.0),
            surface_2: oklch(0.26, 0.008, 240.0),
            surface_hi: oklch(0.30, 0.008, 240.0),
            border: oklch(0.30, 0.008, 240.0),
            border_strong: oklch(0.40, 0.010, 240.0),
            grid: oklch(0.26, 0.006, 240.0),
            grid_major: oklch(0.32, 0.008, 240.0),

            text: oklch(0.94, 0.005, 240.0),
            text_muted: oklch(0.70, 0.006, 240.0),
            text_faint: oklch(0.52, 0.006, 240.0),

            teal: oklch(0.74, 0.090, 195.0),
            teal_soft: oklcha(0.74, 0.090, 195.0, 0.18),
            teal_deep: oklch(0.66, 0.090, 195.0),
            coral: oklch(0.74, 0.110, 30.0),
            coral_soft: oklcha(0.74, 0.110, 30.0, 0.18),
            coral_deep: oklch(0.66, 0.110, 30.0),
            amber: oklch(0.82, 0.130, 80.0),
            amber_soft: oklcha(0.82, 0.130, 80.0, 0.18),
            amber_deep: oklch(0.74, 0.130, 80.0),
            violet: oklch(0.72, 0.080, 295.0),
            violet_soft: oklcha(0.72, 0.080, 295.0, 0.18),
            violet_deep: oklch(0.64, 0.080, 295.0),
            green: oklch(0.72, 0.130, 145.0),
            green_soft: oklcha(0.72, 0.130, 145.0, 0.18),
            green_deep: oklch(0.64, 0.130, 145.0),
            blue: oklch(0.68, 0.090, 260.0),
            blue_soft: oklcha(0.68, 0.090, 260.0, 0.18),
            blue_deep: oklch(0.60, 0.090, 260.0),

            target: oklch(0.70, 0.060, 145.0),
            target_soft: oklcha(0.70, 0.060, 145.0, 0.18),
            compare: oklch(0.70, 0.050, 250.0),
        }
    }

    /// Light variant — Tessera's `data-theme="light"`.
    #[must_use]
    pub fn light() -> Self {
        Self {
            bg: oklch(0.985, 0.004, 85.0),
            bg_deep: oklch(0.965, 0.004, 85.0),
            surface: oklch(0.995, 0.003, 85.0),
            surface_2: oklch(0.970, 0.004, 85.0),
            surface_hi: oklch(0.940, 0.005, 85.0),
            border: oklch(0.900, 0.005, 85.0),
            border_strong: oklch(0.820, 0.006, 85.0),
            grid: oklch(0.930, 0.004, 85.0),
            grid_major: oklch(0.860, 0.005, 85.0),

            text: oklch(0.22, 0.010, 240.0),
            text_muted: oklch(0.45, 0.008, 240.0),
            text_faint: oklch(0.62, 0.006, 240.0),

            teal: oklch(0.50, 0.085, 195.0),
            teal_soft: oklcha(0.50, 0.085, 195.0, 0.14),
            teal_deep: oklch(0.46, 0.085, 195.0),
            coral: oklch(0.58, 0.115, 30.0),
            coral_soft: oklcha(0.58, 0.115, 30.0, 0.14),
            coral_deep: oklch(0.54, 0.115, 30.0),
            amber: oklch(0.62, 0.130, 65.0),
            amber_soft: oklcha(0.62, 0.130, 65.0, 0.14),
            amber_deep: oklch(0.58, 0.130, 65.0),
            violet: oklch(0.50, 0.085, 295.0),
            violet_soft: oklcha(0.50, 0.085, 295.0, 0.14),
            violet_deep: oklch(0.46, 0.085, 295.0),
            green: oklch(0.48, 0.120, 145.0),
            green_soft: oklcha(0.48, 0.120, 145.0, 0.14),
            green_deep: oklch(0.44, 0.120, 145.0),
            blue: oklch(0.46, 0.090, 260.0),
            blue_soft: oklcha(0.46, 0.090, 260.0, 0.14),
            blue_deep: oklch(0.42, 0.090, 260.0),

            target: oklch(0.50, 0.075, 145.0),
            target_soft: oklcha(0.50, 0.075, 145.0, 0.14),
            compare: oklch(0.50, 0.050, 250.0),
        }
    }

    /// Semantic alias — Tessera maps `--signal: var(--amber)`.
    #[must_use]
    pub fn signal(&self) -> Color {
        self.amber
    }

    /// Semantic alias for the translucent signal fill.
    #[must_use]
    pub fn signal_soft(&self) -> Color {
        self.amber_soft
    }

    /// Semantic alias — Tessera maps `--pattern: var(--violet)`.
    #[must_use]
    pub fn pattern(&self) -> Color {
        self.violet
    }

    /// Semantic alias for the translucent pattern fill.
    #[must_use]
    pub fn pattern_soft(&self) -> Color {
        self.violet_soft
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}
