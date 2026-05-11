//! Tessera `.tb-btn` button — text label, optional `active` flag.
//!
//! Visual-only first cut: no hover, press, or click handling. The active
//! state is a flag the caller toggles (e.g. "Inspector" is active when the
//! inspector panel is open). When hover/press states land, they'll attach
//! to the same widget without changing the public API.

use masonry::core::ArcStr;
use masonry::properties::Padding;
use xilem::WidgetView;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{label, sized_box};

use crate::Theme;

/// Static visual state of a [`Button`].
///
/// In real usage the harness drives this — `Default` while resting,
/// `Hover` while the pointer is over the widget, `Active` while pressed
/// or when the host has marked the button as the currently-selected
/// toggle. While interactivity is not wired yet, callers can set the
/// state explicitly to render any of the three.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonState {
    /// Resting state — muted text on transparent chrome.
    #[default]
    Default,
    /// Pointer-hovered — full text, filled surface, no border.
    Hover,
    /// Selected / pressed — full text, filled surface, visible border.
    Active,
}

/// Builder for a Tessera-styled button.
///
/// Build with [`button`], chain modifiers, then call [`Button::render`] to
/// produce a xilem `WidgetView`.
#[derive(Clone, Debug)]
#[must_use = "Button does nothing until rendered with .render(&theme)"]
pub struct Button {
    label: ArcStr,
    state: ButtonState,
}

/// Start a new button with the given label.
pub fn button(label: impl Into<ArcStr>) -> Button {
    Button {
        label: label.into(),
        state: ButtonState::Default,
    }
}

impl Button {
    /// Mark this button as currently selected. Convenience for
    /// [`Self::state`] — `true` → `Active`, `false` → `Default`. Use
    /// [`Self::state`] directly if you need `Hover`.
    pub fn active(mut self, on: bool) -> Self {
        self.state = if on {
            ButtonState::Active
        } else {
            ButtonState::Default
        };
        self
    }

    /// Set the visual state explicitly.
    pub fn state(mut self, state: ButtonState) -> Self {
        self.state = state;
        self
    }

    /// Build the xilem view at the supplied theme.
    #[must_use]
    pub fn render<S: 'static>(&self, theme: &Theme) -> impl WidgetView<S> + use<S> {
        let p = &theme.palette;
        let (text_color, bg_color, border_color) = match self.state {
            ButtonState::Default => (p.text_muted, Color::TRANSPARENT, Color::TRANSPARENT),
            ButtonState::Hover => (p.text, p.surface_2, Color::TRANSPARENT),
            ButtonState::Active => (p.text, p.surface_2, p.border),
        };

        let text = label(self.label.clone())
            .text_size(theme.density.ui_font_size)
            .color(text_color);

        sized_box(text)
            .padding(Padding::from_vh(5.0, 9.0))
            .background_color(bg_color)
            .border(border_color, 1.0)
            .corner_radius(5.0)
    }
}
