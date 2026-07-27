//! Shared text-styling types usable across text-bearing components.

/// A single text decoration rule: underline, strikethrough, or none.
///
/// Renders as a real parley-native decoration line rather than a
/// font/renderer-dependent character-composition workaround (e.g. Unicode
/// combining marks). Always matches the decorated text's own color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDecoration {
    /// No decoration. Default.
    #[default]
    None,
    /// A line under the text.
    Underline,
    /// A line through the text.
    Strikethrough,
}

#[cfg(test)]
mod tests {
    use super::TextDecoration;

    #[test]
    fn default_text_decoration_is_none() {
        assert_eq!(TextDecoration::default(), TextDecoration::None);
    }
}
