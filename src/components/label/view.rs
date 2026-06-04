//! Xilem view for the themed label component.
//!
//! Composes one or two `masonry::widgets::Label` widgets — a second one for
//! optional secondary text — configured from a [`Theme`] snapshot. There is
//! no custom masonry widget and no interaction; [`Label::render`] returns a
//! type-erased [`WidgetView`] so the single-label and two-label cases share
//! one return type, and `Box<AnyWidgetView>` handles the concrete type
//! changing across rebuilds (e.g. when `.secondary()` toggles).

use masonry::core::ArcStr;
use masonry::parley::{FontFamily, LineHeight};
use masonry::properties::LineBreaking;
use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_row, label as xl_label};
use xilem::{AnyWidgetView, WidgetView};

use super::LabelAlignment;
use crate::Theme;

/// Builder for a themed text label.
///
/// Created with [`label`]. Returns a xilem view via [`Self::render`].
#[must_use = "Label does nothing until rendered with .render(&theme)"]
pub struct Label {
    text: ArcStr,
    secondary: Option<ArcStr>,
    color: Option<Color>,
    text_size: Option<f32>,
    letter_spacing: f32,
    font: Option<FontFamily<'static>>,
    alignment: LabelAlignment,
    line_height: Option<f32>,
    multiline: bool,
    masked: bool,
}

/// Create a themed text label.
///
/// Defaults: body font size, `palette.text` color, start alignment,
/// no secondary text, single-line (clip on overflow), unmasked.
pub fn label(text: impl Into<ArcStr>) -> Label {
    Label {
        text: text.into(),
        secondary: None,
        color: None,
        text_size: None,
        letter_spacing: 0.0,
        font: None,
        alignment: LabelAlignment::default(),
        line_height: None,
        multiline: false,
        masked: false,
    }
}

impl Label {
    /// Append muted secondary text after the main text.
    ///
    /// Rendered as a separate label in `text_muted` color, inlined to the
    /// right with a theme-derived gap. Shares every other style knob with
    /// the main text.
    pub fn secondary(mut self, text: impl Into<ArcStr>) -> Self {
        self.secondary = Some(text.into());
        self
    }

    /// Override the main text color. Defaults to `palette.text`.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Override the font size in px. Defaults to `typography.size_body`.
    pub fn text_size(mut self, size: f32) -> Self {
        self.text_size = Some(size);
        self
    }

    /// Set letter spacing (tracking) in px. Defaults to `0.0`.
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Override the font family.
    ///
    /// Useful for monospace code labels; most UI labels should use the
    /// theme's sans-serif stack and do not need to call this.
    pub fn font(mut self, font: FontFamily<'static>) -> Self {
        self.font = Some(font);
        self
    }

    /// Set the horizontal text alignment. Defaults to [`LabelAlignment::Start`].
    pub fn alignment(mut self, alignment: LabelAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Set a relative line height multiplier (e.g. `1.5` = 1.5× the font size).
    pub fn line_height(mut self, factor: f32) -> Self {
        self.line_height = Some(factor);
        self
    }

    /// Enable word-wrap for multiline text. When `false` (default) text clips.
    pub fn multiline(mut self, on: bool) -> Self {
        self.multiline = on;
        self
    }

    /// Replace every character with `•`. Applies to both main and secondary text.
    pub fn masked(mut self, on: bool) -> Self {
        self.masked = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<State, Action>
    where
        State: 'static,
        Action: 'static,
    {
        let main_color = self.color.unwrap_or(theme.palette.text);
        let main_text = if self.masked {
            mask(&self.text)
        } else {
            self.text.clone()
        };

        let view: Box<AnyWidgetView<State, Action>> = match &self.secondary {
            None => self.single(main_text, main_color, theme),
            Some(sec) => {
                let sec_text = if self.masked { mask(sec) } else { sec.clone() };
                let main = self.single(main_text, main_color, theme);
                let secondary = self.single(sec_text, theme.palette.text_muted, theme);
                Box::new(
                    flex_row((main, secondary))
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .gap(Length::px(f64::from(theme.density.col))),
                )
            }
        };
        view
    }

    /// One styled `masonry::widgets::Label`. The main and secondary labels
    /// share every style knob except color.
    fn single<S: 'static, A: 'static>(
        &self,
        text: ArcStr,
        color: Color,
        theme: &Theme,
    ) -> Box<AnyWidgetView<S, A>> {
        let base = xl_label(text)
            .text_size(self.text_size.unwrap_or(theme.typography.size_body))
            .text_alignment(self.alignment.into_text_align())
            .letter_spacing(self.letter_spacing);
        let base = match self.font.clone() {
            Some(f) => base.font(f),
            None => base,
        };
        let base = match self.line_height {
            Some(lh) => base.line_height(LineHeight::FontSizeRelative(lh)),
            None => base,
        };
        if self.multiline {
            Box::new(
                base.color(color)
                    .line_break_mode(LineBreaking::WordWrap),
            )
        } else {
            Box::new(base.color(color))
        }
    }
}

fn mask(text: &str) -> ArcStr {
    ArcStr::from("•".repeat(text.chars().count()))
}
