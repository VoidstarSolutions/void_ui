//! Xilem view for the themed label component.
//!
//! Wraps one or two `masonry::widgets::Label` widgets — a second one for
//! optional secondary text — configured from a [`Theme`] snapshot. All
//! interaction is absent; the element type is always [`Pod<Passthrough>`]
//! so the secondary-text (two-label) case can share the same type as the
//! single-label case.

use masonry::core::ArcStr;
use masonry::parley::{Alignment, FontFamily, LineHeight};
use masonry::properties::LineBreaking;
use masonry::widgets::Passthrough;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_row, label as xl_label};
use xilem::{AnyWidgetView, Pod, ViewCtx};

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
    /// right with a 4 px gap.
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
    pub fn render<State, Action>(self, theme: &Theme) -> LabelView<State, Action>
    where
        State: 'static,
        Action: 'static,
    {
        let color = self.color.unwrap_or(theme.palette.text);
        let text_size = self.text_size.unwrap_or(theme.typography.size_body);
        let secondary_color = theme.palette.text_muted;
        LabelView {
            text: self.text,
            secondary: self.secondary,
            color,
            secondary_color,
            text_size,
            letter_spacing: self.letter_spacing,
            font: self.font,
            alignment: self.alignment.into_text_align(),
            line_height: self.line_height,
            multiline: self.multiline,
            masked: self.masked,
            secondary_gap: theme.density.col,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Label`].
///
/// Built only through [`Label::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct LabelView<State, Action> {
    text: ArcStr,
    secondary: Option<ArcStr>,
    color: Color,
    secondary_color: Color,
    text_size: f32,
    letter_spacing: f32,
    font: Option<FontFamily<'static>>,
    alignment: Alignment,
    line_height: Option<f32>,
    multiline: bool,
    masked: bool,
    secondary_gap: f32,
    _phantom: std::marker::PhantomData<fn() -> (State, Action)>,
}

/// Opaque view state for [`LabelView`].
#[doc(hidden)]
pub struct LabelViewState<S: 'static, A: 'static> {
    inner: Box<AnyWidgetView<S, A>>,
    inner_state: <Box<AnyWidgetView<S, A>> as View<S, A, ViewCtx>>::ViewState,
}

fn mask(text: &str) -> ArcStr {
    ArcStr::from("•".repeat(text.chars().count()))
}

fn make_single<S: 'static, A: 'static>(
    text: ArcStr,
    color: Color,
    text_size: f32,
    letter_spacing: f32,
    font: Option<FontFamily<'static>>,
    alignment: Alignment,
    line_height: Option<f32>,
    multiline: bool,
) -> Box<AnyWidgetView<S, A>> {
    let base = xl_label(text)
        .text_size(text_size)
        .text_alignment(alignment)
        .letter_spacing(letter_spacing);
    let base = match font {
        Some(f) => base.font(f),
        None => base,
    };
    let base = match line_height {
        Some(lh) => base.line_height(LineHeight::FontSizeRelative(lh)),
        None => base,
    };
    if multiline {
        Box::new(base.color(color).line_break_mode(LineBreaking::WordWrap))
    } else {
        Box::new(base.color(color))
    }
}

fn build_inner<S: 'static, A: 'static>(view: &LabelView<S, A>) -> Box<AnyWidgetView<S, A>> {
    let main_text = if view.masked {
        mask(&view.text)
    } else {
        view.text.clone()
    };

    match &view.secondary {
        None => make_single(
            main_text,
            view.color,
            view.text_size,
            view.letter_spacing,
            view.font.clone(),
            view.alignment,
            view.line_height,
            view.multiline,
        ),
        Some(sec) => {
            let sec_text = if view.masked { mask(sec) } else { sec.clone() };
            let main_label = make_single(
                main_text,
                view.color,
                view.text_size,
                view.letter_spacing,
                view.font.clone(),
                view.alignment,
                view.line_height,
                view.multiline,
            );
            let sec_label: Box<AnyWidgetView<S, A>> = Box::new(
                xl_label(sec_text)
                    .text_size(view.text_size)
                    .color(view.secondary_color),
            );
            Box::new(
                flex_row((main_label, sec_label))
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .gap(Length::px(f64::from(view.secondary_gap))),
            )
        }
    }
}

impl<State, Action> ViewMarker for LabelView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for LabelView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<Passthrough>;
    type ViewState = LabelViewState<State, Action>;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let inner = build_inner(self);
        let (element, inner_state) =
            ctx.with_id(ViewId::new(0), |ctx| inner.build(ctx, state));
        (element, LabelViewState { inner, inner_state })
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        let new_inner = build_inner(self);
        ctx.with_id(ViewId::new(0), |ctx| {
            new_inner.rebuild(&vs.inner, &mut vs.inner_state, ctx, element, state);
        });
        vs.inner = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            vs.inner.teardown(&mut vs.inner_state, ctx, element);
        });
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_first().map(|id| id.routing_id()) {
            Some(0) => vs.inner.message(&mut vs.inner_state, message, element, state),
            _ => MessageResult::Stale,
        }
    }
}
