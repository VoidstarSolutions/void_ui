//! Pluggable syntax-highlighter abstraction for the `text_field` component.
//!
//! A [`Highlighter`] is given a borrowed source string and returns a list of
//! [`TokenSpan`]s. Each span is a byte range into the source and a
//! [`TokenKind`] that the renderer maps to a color via
//! [`CodePalette`](crate::theme::CodePalette).
//!
//! Spans need not cover every byte. Uncovered bytes paint with
//! `CodePalette::plain`. Spans must not overlap.

use std::ops::Range;

/// One contiguous run of source with a single semantic kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenSpan {
    /// Byte range into the source text.
    pub range: Range<usize>,
    /// Semantic kind used to pick a color from `CodePalette`.
    pub kind: TokenKind,
}

/// Semantic kinds the renderer can color independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenKind {
    Keyword,
    Type,
    Function,
    Identifier,
    String,
    Number,
    Comment,
    Punctuation,
    Operator,
}

/// Produces token spans for a source string.
///
/// Implementations must be `Send + Sync + 'static` so the same highlighter
/// instance can be shared across views and rebuild cycles.
pub trait Highlighter: Send + Sync + 'static {
    fn highlight(&self, source: &str) -> Vec<TokenSpan>;
}
