//! Clipboard demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::clipboard;
use crate::components::ScrollBarVisibility;
use crate::label as ui_label;
use crate::with_source;
use crate::{Theme, scroll_container, separator};

struct ClipboardDemoState {
    last_copied: Option<String>,
}

type InnerView = Box<AnyWidgetView<ClipboardDemoState>>;
type InnerViewState = <InnerView as View<ClipboardDemoState, (), ViewCtx>>::ViewState;

/// Opaque state owned by the clipboard demo panel.
pub struct ClipboardDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct ClipboardDemoPanelState {
    state: ClipboardDemoState,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Clipboard demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> ClipboardDemoPanel {
    ClipboardDemoPanel { theme: *theme }
}

fn build_inner(
    theme: &Theme,
    state: &ClipboardDemoState,
) -> impl WidgetView<ClipboardDemoState> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let row = |value: &'static str| {
        let value_label = label(value)
            .text_size(theme.typography.size_body)
            .color(theme.palette.text_muted);

        let btn = clipboard(value, |s: &mut ClipboardDemoState, text: &str| {
            s.last_copied = Some(text.to_owned());
        })
        .render(theme);

        flex_row((value_label, btn))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(8.0))
    };

    let examples = with_source!(theme, {
        flex_col((
            row("https://example.com/api/v1/token"),
            row("sk-proj-abc123xyz"),
            row("npm install void-ui"),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    });

    let feedback = {
        let msg = match &state.last_copied {
            Some(v) => format!("Copied: {v}"),
            None => "Click a clipboard button above".to_owned(),
        };
        label(msg)
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
    };

    let title_block = flex_col((
        ui_label("Clipboard")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        ui_label("Icon button that copies a value to the system clipboard and shows a brief checkmark on success.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            header("Clipboard button"),
            examples,
            feedback,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for ClipboardDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ClipboardDemoPanel {
    type ViewState = ClipboardDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = ClipboardDemoState { last_copied: None };
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            ClipboardDemoPanelState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut ClipboardDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.state));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.state,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut ClipboardDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut ClipboardDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
