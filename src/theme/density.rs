//! Layout density — three predefined steps mirroring Tessera's
//! `data-density` attribute (`compact` / `balanced` / `airy`).
//!
//! Density carries the small handful of sizes that scale together for the
//! whole UI: chart cell pitch, surface padding, and the UI control font
//! size. Component-specific values (a chip's vertical padding, an
//! inspector row's gap) compose from these.

/// Sizes that should scale together when a user picks a density step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Density {
    /// Chart row height (P&F box vertical pitch), in px.
    pub row: f32,
    /// Chart column width (P&F box horizontal pitch), in px.
    pub col: f32,
    /// Default inner padding for surfaces (cards, panels), in px.
    pub pad: f32,
    /// Base font size for UI controls (topbar buttons, inspector chips,
    /// segmented controls), in px.
    pub ui_font_size: f32,
}

impl Density {
    /// `data-density="compact"`.
    #[must_use]
    pub const fn compact() -> Self {
        Self {
            row: 14.0,
            col: 12.0,
            pad: 10.0,
            ui_font_size: 11.0,
        }
    }

    /// `data-density="balanced"` — Tessera's default.
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            row: 17.0,
            col: 15.0,
            pad: 12.0,
            ui_font_size: 12.0,
        }
    }

    /// `data-density="airy"`.
    #[must_use]
    pub const fn airy() -> Self {
        Self {
            row: 22.0,
            col: 19.0,
            pad: 16.0,
            ui_font_size: 13.0,
        }
    }
}

impl Default for Density {
    fn default() -> Self {
        Self::balanced()
    }
}
