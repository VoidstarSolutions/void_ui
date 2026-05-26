//! Text field component — selectable, syntax-highlighted text.
//!
//! The two-layer pattern: `view` will hold the xilem builder; `widget` will
//! hold the masonry widget that owns paint, layout, selection, and event
//! handling. This task only stubs in the [`Highlighter`] trait; the rest of
//! the module is built up in later tasks.

pub mod highlighter;

pub use highlighter::{Highlighter, TokenKind, TokenSpan};
