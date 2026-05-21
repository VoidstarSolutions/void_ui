//! Tessera `.tb-btn` button component.
//!
//! The xilem [`Button`] builder lives in [`view`]; the masonry widget that
//! owns the pointer state machine lives in [`widget`]. The widget is
//! exposed publicly so the [`ButtonView`]'s public `Element` associated
//! type can name it without leaking a private type through the public API.

pub mod demo;
mod view;
pub mod widget;

pub use view::{Button, ButtonView, button};

/// Visual style applied to a button — controls how background and border
/// colors are resolved from the theme palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Neutral — the default Tessera `.tb-btn` style.
    #[default]
    Default,
    /// Destructive action — coral accent tones on hover and active.
    Danger,
    /// Primary action — teal fill, always-visible background.
    Primary,
    /// Subtle — always-visible border, no fill until hover.
    Ghost,
    /// Cautionary action — amber accent tones on hover and active.
    Warning,
}
