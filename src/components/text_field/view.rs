//! Read-only highlighted text view.
//!
//! Wraps [`super::widget::CodeViewWidget`] in a xilem [`View`]. The view
//! stores the source text and an optional [`Highlighter`]; on `build` it
//! produces the spans and the brush palette, on `rebuild` it diffs against
//! the previous view and pushes only the deltas down to the widget.
//!
//! ```ignore
//! use void_ui::components::text_field::read_only_text;
//! read_only_text("fn main() {}\n").render::<MyState>(&theme)
//! ```
//!
//! By default `read_only_text` wires up [`RustHighlighter`]. Use
//! [`ReadOnlyText::highlighter`] to swap in a different one, or
//! [`ReadOnlyText::no_highlighter`] to render plain (single-color) text.

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::peniko::Brush;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::highlighter::{Highlighter, TokenSpan};
use super::rust::RustHighlighter;
use super::widget::{BRUSH_PALETTE_LEN, CodeViewWidget};
use crate::Theme;

/// Builder for a read-only highlighted text view.
///
/// Created with [`read_only_text`]. The highlighter defaults to
/// [`RustHighlighter`]; swap it via [`Self::highlighter`] or disable it
/// entirely via [`Self::no_highlighter`].
#[must_use = "ReadOnlyText does nothing until rendered with .render(&theme)"]
pub struct ReadOnlyText {
    text: String,
    highlighter: Option<Arc<dyn Highlighter>>,
}

/// Create a read-only highlighted text view with the default
/// [`RustHighlighter`].
pub fn read_only_text(text: impl Into<String>) -> ReadOnlyText {
    ReadOnlyText {
        text: text.into(),
        highlighter: Some(Arc::new(RustHighlighter)),
    }
}

impl ReadOnlyText {
    /// Replace the highlighter. Owned via `Arc` so the same instance can be
    /// shared across many views.
    pub fn highlighter(mut self, highlighter: impl Highlighter) -> Self {
        self.highlighter = Some(Arc::new(highlighter));
        self
    }

    /// Disable syntax highlighting entirely. The text renders with the
    /// `CodePalette::plain` color.
    pub fn no_highlighter(mut self) -> Self {
        self.highlighter = None;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State>(self, theme: &Theme) -> ReadOnlyTextView<State>
    where
        State: 'static,
    {
        ReadOnlyTextView {
            text: self.text,
            highlighter: self.highlighter,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`ReadOnlyText`].
///
/// Built only through [`ReadOnlyText::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ReadOnlyTextView<State> {
    text: String,
    highlighter: Option<Arc<dyn Highlighter>>,
    theme: Theme,
    phantom: PhantomData<fn(State)>,
}

impl<State> ViewMarker for ReadOnlyTextView<State> {}

impl<State> View<State, (), ViewCtx> for ReadOnlyTextView<State>
where
    State: 'static,
{
    type Element = Pod<CodeViewWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let spans = highlight(self.highlighter.as_ref(), &self.text);
        let brushes = build_brushes(&self.theme);
        let widget = CodeViewWidget::new(
            self.text.clone(),
            spans,
            brushes,
            self.theme.palette.bg_deep,
            self.theme.palette.border,
            1.0,
            self.theme.radius.small,
            12.0,
            self.theme.typography.size_caption,
        );
        (ctx.create_pod(widget), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        let highlighter_changed = match (&self.highlighter, &prev.highlighter) {
            (None, None) => false,
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        };
        let text_changed = self.text != prev.text;

        if text_changed {
            CodeViewWidget::set_text(&mut element, self.text.clone());
        }
        if text_changed || highlighter_changed {
            let spans = highlight(self.highlighter.as_ref(), &self.text);
            CodeViewWidget::set_spans(&mut element, spans);
        }
        if self.theme.code != prev.theme.code {
            CodeViewWidget::set_brushes(&mut element, build_brushes(&self.theme));
        }
        if self.theme.palette != prev.theme.palette || self.theme.radius != prev.theme.radius {
            CodeViewWidget::set_chrome(
                &mut element,
                self.theme.palette.bg_deep,
                self.theme.palette.border,
                1.0,
                self.theme.radius.small,
                12.0,
            );
        }
        if (self.theme.typography.size_caption - prev.theme.typography.size_caption).abs()
            > f32::EPSILON
        {
            CodeViewWidget::set_font_size(&mut element, self.theme.typography.size_caption);
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

/// Run the highlighter (if present); otherwise produce no spans, which
/// makes every byte fall through to the `plain` brush.
fn highlight(highlighter: Option<&Arc<dyn Highlighter>>, text: &str) -> Vec<TokenSpan> {
    highlighter.map_or_else(Vec::new, |h| h.highlight(text))
}

/// Build the brush palette in the exact slot order the widget expects.
/// Index 0 is the fallback color; indices 1..=9 follow
/// `widget::brush_index_for_kind`.
fn build_brushes(theme: &Theme) -> Vec<Brush> {
    let code = &theme.code;
    let brushes = vec![
        Brush::Solid(code.plain),
        Brush::Solid(code.keyword),
        Brush::Solid(code.type_name),
        Brush::Solid(code.function),
        Brush::Solid(code.identifier),
        Brush::Solid(code.string),
        Brush::Solid(code.number),
        Brush::Solid(code.comment),
        Brush::Solid(code.punctuation_or_plain()),
        Brush::Solid(code.operator),
    ];
    debug_assert_eq!(brushes.len(), BRUSH_PALETTE_LEN);
    brushes
}
