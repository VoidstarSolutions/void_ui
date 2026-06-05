//! Color slots for syntax-highlighted source code.
//!
//! One slot per [`TokenKind`](crate::components::code_view::TokenKind)
//! variant — except `Punctuation`, which aliases `plain` via
//! [`CodePalette::punctuation_or_plain`] — plus `plain` itself for
//! unspanned bytes (whitespace, fallback).

use masonry::peniko::Color;

use super::color::oklch;

/// Per-token-kind colors used by the `code_view` component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodePalette {
    /// Fallback color for whitespace and any byte not covered by a token span.
    pub plain: Color,
    /// Color for language keywords (e.g. `fn`, `let`, `if`).
    pub keyword: Color,
    /// Color for type names and type-like identifiers.
    pub type_name: Color,
    /// Color for function and method names at call and definition sites.
    pub function: Color,
    /// Color for plain identifiers — intentionally tracks `plain` so unhighlighted
    /// identifiers blend with surrounding text rather than competing for attention.
    pub identifier: Color,
    /// Color for string and character literals.
    pub string: Color,
    /// Color for numeric literals.
    pub number: Color,
    /// Color for comments.
    pub comment: Color,
    /// Color for operators. Punctuation does **not** use this slot — it
    /// aliases `plain` via [`CodePalette::punctuation_or_plain`].
    pub operator: Color,
}

impl CodePalette {
    /// Dark-variant palette — desaturated, muted accents on a dark surface.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            plain: oklch(0.88, 0.005, 240.0),
            keyword: oklch(0.72, 0.090, 295.0),
            type_name: oklch(0.78, 0.090, 195.0),
            function: oklch(0.80, 0.090, 80.0),
            identifier: oklch(0.88, 0.005, 240.0),
            string: oklch(0.74, 0.110, 145.0),
            number: oklch(0.78, 0.100, 30.0),
            comment: oklch(0.55, 0.006, 240.0),
            operator: oklch(0.70, 0.006, 240.0),
        }
    }

    /// Light-variant palette — saturated low-luma tones on a light surface.
    #[must_use]
    pub fn light() -> Self {
        Self {
            plain: oklch(0.25, 0.010, 240.0),
            keyword: oklch(0.45, 0.130, 295.0),
            type_name: oklch(0.45, 0.110, 195.0),
            function: oklch(0.55, 0.130, 65.0),
            identifier: oklch(0.25, 0.010, 240.0),
            string: oklch(0.48, 0.130, 145.0),
            number: oklch(0.52, 0.130, 30.0),
            comment: oklch(0.55, 0.008, 240.0),
            operator: oklch(0.45, 0.008, 240.0),
        }
    }

    /// Color used for `TokenKind::Punctuation`. Currently aliased to `plain`;
    /// kept as a method so we can split it out later without churning callers.
    #[must_use]
    pub fn punctuation_or_plain(&self) -> Color {
        self.plain
    }
}
