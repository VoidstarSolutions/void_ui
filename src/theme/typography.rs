//! Font stacks and the small type-size scale shared by `void_ui` components.
//!
//! Tessera ships with Geist / Geist Mono and a CSS fallback chain. We don't
//! load fonts here — the host application is responsible for registering
//! Geist with the masonry font collection if it wants it. The fallback
//! chain still applies: if Geist isn't present, parley walks the list.

/// Ordered family stack — first match wins, otherwise fall through.
///
/// Stored as a slice of `&'static str` so the canonical theme is fully
/// `const`. Convert to a parley `FontStack` at the widget boundary when
/// you actually mount text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontStack {
    pub families: &'static [&'static str],
}

impl FontStack {
    #[must_use]
    pub const fn new(families: &'static [&'static str]) -> Self {
        Self { families }
    }
}

/// The two font stacks (sans / mono) plus a tiny type scale.
///
/// Body / caption sizes are *not* density-driven — they're the fixed
/// reference sizes from Tessera. Density only moves the UI-control font
/// size, which lives on [`super::Density`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    pub sans: FontStack,
    pub mono: FontStack,
    /// Body / paragraph size, in px. Tessera `body { font-size: 13px }`.
    pub size_body: f32,
    /// Caption / label / chip size, in px. Used for axis labels,
    /// pin-rail labels, legend, meta-tags.
    pub size_caption: f32,
    pub size_title: f32,
}

const SANS: FontStack = FontStack::new(&[
    "Geist",
    "ui-sans-serif",
    "system-ui",
    "-apple-system",
    "Segoe UI",
    "sans-serif",
]);

const MONO: FontStack = FontStack::new(&[
    "Geist Mono",
    "ui-monospace",
    "SF Mono",
    "Menlo",
    "monospace",
]);

impl Typography {
    /// Tessera default — Geist / Geist Mono with the documented fallbacks.
    #[must_use]
    pub const fn default_stack() -> Self {
        Self {
            sans: SANS,
            mono: MONO,
            size_body: 13.0,
            size_caption: 10.0,
            size_title: 20.0,
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::default_stack()
    }
}
