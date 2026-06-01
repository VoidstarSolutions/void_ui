//! Themed text label component.
//!
//! Wraps `masonry::widgets::Label` with theme-aware defaults, optional
//! secondary (muted) text, masking, and multiline word-wrap.
//!
//! ```ignore
//! use void_ui::components::label;
//! label("API key")
//!     .secondary("sk-proj-abc123")
//!     .render(&theme)
//!
//! label("Redacted value")
//!     .masked(true)
//!     .render(&theme)
//!
//! label("A long paragraph that wraps at the container edge.")
//!     .multiline(true)
//!     .render(&theme)
//! ```

pub mod demo;
mod view;

pub use view::{Label, LabelView, LabelViewState, label};

/// Horizontal alignment for label text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelAlignment {
    /// Align to the leading edge — left in LTR scripts. Default.
    #[default]
    Start,
    /// Center horizontally.
    Center,
    /// Align to the trailing edge — right in LTR scripts.
    End,
}

impl LabelAlignment {
    #[must_use]
    pub(crate) fn into_text_align(self) -> xilem::TextAlign {
        use masonry::parley::Alignment;
        match self {
            Self::Start => Alignment::Start,
            Self::Center => Alignment::Center,
            Self::End => Alignment::End,
        }
    }
}
