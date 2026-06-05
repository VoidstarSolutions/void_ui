//! Code view component — read-only, selectable, syntax-highlighted text.
//!
//! Two-layer pattern: `view` holds the [`read_only_text`] builder and the
//! xilem view that diffs text/spans/theme on rebuild; `widget` holds
//! [`widget::CodeViewWidget`], the masonry widget that owns paint, layout,
//! selection, and clipboard copy. Highlighting is pluggable via the
//! [`Highlighter`] trait; [`RustHighlighter`] is the built-in default.
//! `.copyable()` overlays a copy-to-clipboard button at the right edge.

pub mod demo;
pub mod highlighter;
mod rust;
mod view;
pub mod widget;

pub use highlighter::{Highlighter, TokenKind, TokenSpan};
pub use rust::RustHighlighter;
pub use view::{ReadOnlyText, ReadOnlyTextView, read_only_text};
