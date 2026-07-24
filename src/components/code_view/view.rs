//! Read-only highlighted text view.
//!
//! Wraps [`super::widget::CodeViewWidget`] in a xilem [`View`]. The view
//! stores the source text and an optional [`Highlighter`]; on `build` it
//! produces the spans and the brush palette, on `rebuild` it diffs against
//! the previous view and pushes only the deltas down to the widget.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct MyState;
//! use void_ui::components::code_view::read_only_text;
//! read_only_text("fn main() {}\n").render::<MyState>(&theme)
//! # ;
//! ```
//!
//! By default `read_only_text` wires up [`RustHighlighter`]. Use
//! [`ReadOnlyText::highlighter`] to swap in a different one, or
//! [`ReadOnlyText::no_highlighter`] to render plain (single-color) text.

use std::marker::PhantomData;
use std::sync::{Arc, LazyLock};

use masonry::layout::UnitPoint;
use masonry::peniko::Brush;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{ZStackExt as _, sized_box, zstack};
use xilem::{Pod, ViewCtx, WidgetView};

use super::highlighter::{Highlighter, TokenSpan};
use super::rust::RustHighlighter;
use super::widget::{BRUSH_PALETTE_LEN, CodeViewWidget};
use crate::Theme;
use crate::components::clipboard::clipboard;

/// Hairline border around the field. Component-local like every other
/// bordered widget (tooltip, button, checkbox); a 1px stroke doesn't scale
/// with density. Inner padding, by contrast, is read from `Theme.density.pad`.
const BORDER_WIDTH: f32 = 1.0;

static DEFAULT_RUST_HIGHLIGHTER: LazyLock<Arc<dyn Highlighter>> =
    LazyLock::new(|| Arc::new(RustHighlighter));

/// Builder for a read-only highlighted text view.
///
/// Created with [`read_only_text`]. The highlighter defaults to
/// [`RustHighlighter`]; swap it via [`Self::highlighter`] or disable it
/// entirely via [`Self::no_highlighter`].
#[must_use = "ReadOnlyText does nothing until rendered with .render(&theme)"]
pub struct ReadOnlyText {
    text: String,
    highlighter: Option<Arc<dyn Highlighter>>,
    copyable: bool,
}

/// Create a read-only highlighted text view with the default
/// [`RustHighlighter`].
pub fn read_only_text(text: impl Into<String>) -> ReadOnlyText {
    ReadOnlyText {
        text: text.into(),
        highlighter: Some(DEFAULT_RUST_HIGHLIGHTER.clone()),
        copyable: false,
    }
}

impl ReadOnlyText {
    /// Replace the highlighter. Wraps the value in a fresh `Arc`; to share
    /// one (possibly expensive) highlighter instance across many views, use
    /// [`Self::shared_highlighter`].
    pub fn highlighter(mut self, highlighter: impl Highlighter) -> Self {
        self.highlighter = Some(Arc::new(highlighter));
        self
    }

    /// Replace the highlighter with an already-shared instance.
    pub fn shared_highlighter(mut self, highlighter: Arc<dyn Highlighter>) -> Self {
        self.highlighter = Some(highlighter);
        self
    }

    /// Disable syntax highlighting entirely. The text renders with the
    /// `CodePalette::plain` color.
    pub fn no_highlighter(mut self) -> Self {
        self.highlighter = None;
        self
    }

    /// Overlay a copy-to-clipboard button at the right edge, vertically
    /// centered on the block.
    ///
    /// The button writes the full source text to the system clipboard (the
    /// [`clipboard`] component handles the write and the copied feedback).
    /// Hosts that need to react to the copy should compose [`clipboard`]
    /// alongside the view themselves instead.
    pub fn copyable(mut self) -> Self {
        self.copyable = true;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State>(self, theme: &Theme) -> impl WidgetView<State> + use<State>
    where
        State: 'static,
    {
        let copy_button = self.copyable.then(|| {
            // Top-right corner, inset matching the code chrome's inner padding.
            sized_box(clipboard(self.text.clone(), |_: &mut State, _: &str| {}).render(theme))
                .padding(Length::px(f64::from(theme.density.pad)))
                .alignment(UnitPoint::TOP_RIGHT)
        });
        // Reserve right-side space equal to the button width so the text
        // never flows under the clipboard button.
        let right_inset = if self.copyable {
            theme.density.ui_font_size + 2.0 * theme.density.button_pad_h
        } else {
            0.0
        };
        let code = ReadOnlyTextView {
            text: self.text,
            highlighter: self.highlighter,
            theme: *theme,
            right_inset,
            phantom: PhantomData,
        };
        zstack((code, copy_button))
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
    right_inset: f32,
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
            BORDER_WIDTH,
            self.theme.radius.small,
            self.theme.density.pad,
            self.right_inset,
            self.theme.typography.size_caption,
            self.theme.palette.accent_soft,
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
        if self.theme.palette != prev.theme.palette
            || self.theme.radius != prev.theme.radius
            || (self.theme.density.pad - prev.theme.density.pad).abs() > f32::EPSILON
        {
            CodeViewWidget::set_chrome(
                &mut element,
                self.theme.palette.bg_deep,
                self.theme.palette.border,
                BORDER_WIDTH,
                self.theme.radius.small,
                self.theme.density.pad,
                self.theme.palette.accent_soft,
            );
        }
        if (self.theme.typography.size_caption - prev.theme.typography.size_caption).abs()
            > f32::EPSILON
        {
            CodeViewWidget::set_font_size(&mut element, self.theme.typography.size_caption);
        }
        if (self.right_inset - prev.right_inset).abs() > f32::EPSILON {
            CodeViewWidget::set_right_inset(&mut element, self.right_inset);
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
        // Read-only widget emits no actions; any message routed here is a no-op.
        MessageResult::Nop
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
