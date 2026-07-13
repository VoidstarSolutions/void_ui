//! Tessera `.tb-btn` button component.
//!
//! The xilem [`Button`] builder lives in [`view`]; the masonry widget that
//! owns the pointer state machine lives in [`widget`]. The widget is
//! exposed publicly so the [`ButtonView`]'s public `Element` associated
//! type can name it without leaking a private type through the public API.

mod content;
#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub mod widget;

pub use content::{ContentButton, ContentButtonView, content_button};
pub use view::{Button, ButtonView, button};

/// Visual style applied to a button — controls how background and border
/// colors are resolved from the theme palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Neutral — the default Tessera `.tb-btn` style.
    #[default]
    Default,
    /// Destructive action — danger accent tones on hover and active.
    Danger,
    /// Primary action — accent fill, always-visible background.
    Primary,
    /// Secondary action — secondary accent tones, less prominent than Primary.
    Secondary,
    /// Destructive-adjacent caution — warning accent tones.
    Warning,
    /// Positive confirmation — success accent tones.
    Success,
    /// Neutral information — info accent tones.
    Info,
    /// Subtle — always-visible border, no fill until hover.
    Ghost,
    /// Hyperlink style — accent text, no background or border.
    Link,
    /// Completely flat — no background, no border, no hover fill.
    Text,
}
