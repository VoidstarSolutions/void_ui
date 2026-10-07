//! Font stacks and the small type-size scale shared by `void_ui` components.
//!
//! The default stack is Geist / Geist Mono with a CSS-style fallback
//! chain. We don't load fonts here — the host application is responsible
//! for registering Geist with the masonry font collection if it wants it.
//! The fallback chain still applies: if Geist isn't present, parley walks
//! the list.

use std::borrow::Cow;

use masonry::parley::{FontFamily, FontFamilyName, GenericFamily};

/// Ordered family stack — first match wins, otherwise fall through.
///
/// Stored as a slice of `&'static str` so the canonical theme is fully
/// `const`. Convert to a parley `FontStack` at the widget boundary when
/// you actually mount text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontStack {
    pub families: &'static [&'static str],
}

impl FontStack {
    #[must_use]
    pub const fn new(families: &'static [&'static str]) -> Self {
        Self { families }
    }

    /// The stack as a parley [`FontFamily`], ready for
    /// `StyleProperty::FontFamily`. Names parley can't parse are dropped;
    /// an empty result falls back to the bare `fallback` generic.
    #[must_use]
    pub fn to_family(&self, fallback: GenericFamily) -> FontFamily<'static> {
        let names: Vec<FontFamilyName<'static>> = self
            .families
            .iter()
            .filter_map(|f| FontFamilyName::parse(f))
            .collect();
        if names.is_empty() {
            FontFamily::Single(FontFamilyName::Generic(fallback))
        } else {
            FontFamily::List(Cow::Owned(names))
        }
    }
}

/// The two font stacks (sans / mono) plus a tiny type scale.
///
/// Body / caption sizes are *not* density-driven — they're fixed
/// reference sizes. Density only moves the UI-control font size, which
/// lives on [`super::Density`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    pub sans: FontStack,
    pub mono: FontStack,
    /// Body / paragraph size, in px. 13px by default.
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
    // Keycap-symbol coverage tail: obscure glyphs like ⌤ (U+2324) aren't in the
    // mono faces above, and parley's script-bucketed fallback won't hunt them
    // down. These dedicated symbol fonts carry the full macOS/Windows keyboard
    // glyph set. Placed before the terminal `monospace` generic, which always
    // resolves and would otherwise end the search.
    "Apple Symbols",
    "Segoe UI Symbol",
    "monospace",
]);

impl Typography {
    /// The default stack — Geist / Geist Mono with the documented fallbacks.
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

    /// The sans stack as a parley family — the default face for every
    /// component's text.
    #[must_use]
    pub fn sans_family(&self) -> FontFamily<'static> {
        self.sans.to_family(GenericFamily::SansSerif)
    }

    /// The mono stack as a parley family.
    #[must_use]
    pub fn mono_family(&self) -> FontFamily<'static> {
        self.mono.to_family(GenericFamily::Monospace)
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::default_stack()
    }
}

#[cfg(test)]
mod tests {
    use masonry::parley::{FontFamily, FontFamilyName, GenericFamily};

    use super::{FontStack, Typography};

    #[test]
    fn to_family_keeps_stack_order_and_parses_generics() {
        let stack = FontStack::new(&["DM Sans", "system-ui", "sans-serif"]);
        let FontFamily::List(names) = stack.to_family(GenericFamily::SansSerif) else {
            panic!("a non-empty stack should become a family list");
        };
        assert_eq!(
            names.as_ref(),
            [
                FontFamilyName::Named("DM Sans".into()),
                FontFamilyName::Generic(GenericFamily::SystemUi),
                FontFamilyName::Generic(GenericFamily::SansSerif),
            ]
        );
    }

    #[test]
    fn empty_stack_falls_back_to_the_generic() {
        assert_eq!(
            FontStack::new(&[]).to_family(GenericFamily::Monospace),
            FontFamily::Single(FontFamilyName::Generic(GenericFamily::Monospace))
        );
    }

    #[test]
    fn default_stacks_lead_with_geist() {
        let t = Typography::default();
        for family in [t.sans_family(), t.mono_family()] {
            let FontFamily::List(names) = family else {
                panic!("default stacks are non-empty");
            };
            assert!(matches!(&names[0], FontFamilyName::Named(n) if n.starts_with("Geist")));
        }
    }
}
