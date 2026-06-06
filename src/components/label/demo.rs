//! Label demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{LabelAlignment, label};
use crate::components::ScrollBarVisibility;
use crate::components::checkbox::checkbox;
use crate::with_source;
use crate::{Theme, scroll_container, separator};

struct LabelDemoState {
    masked: bool,
}

type InnerView = Box<AnyWidgetView<LabelDemoState>>;
type InnerViewState = <InnerView as View<LabelDemoState, (), ViewCtx>>::ViewState;

/// Opaque state owned by the label demo panel.
pub struct LabelDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct LabelDemoPanelState {
    state: LabelDemoState,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Label demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> LabelDemoPanel {
    LabelDemoPanel { theme: *theme }
}

fn section_header(text: &'static str, theme: &Theme) -> impl WidgetView<LabelDemoState> + use<> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn sizes_section(theme: &Theme) -> impl WidgetView<LabelDemoState> + use<> {
    with_source!(theme, {
        flex_col((
            label("Caption text — 10 px")
                .text_size(theme.typography.size_caption)
                .render(theme),
            label("Body text — 13 px")
                .text_size(theme.typography.size_body)
                .render(theme),
            label("Custom size — 18 px").text_size(18.0).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(6.0))
    })
}

fn colors_section(theme: &Theme) -> impl WidgetView<LabelDemoState> + use<> {
    with_source!(theme, {
        flex_col((
            label("text — default").render(theme),
            label("text_muted")
                .color(theme.palette.text_muted)
                .render(theme),
            label("text_faint")
                .color(theme.palette.text_faint)
                .render(theme),
            label("teal accent").color(theme.palette.teal).render(theme),
            label("coral accent")
                .color(theme.palette.coral)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(6.0))
    })
}

fn secondary_section(theme: &Theme) -> impl WidgetView<LabelDemoState> + use<> {
    with_source!(theme, {
        flex_col((
            label("Primary text")
                .secondary("Secondary text")
                .render(theme),
            label("Status").secondary("active").render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(6.0))
    })
}

fn alignment_section(theme: &Theme) -> impl WidgetView<LabelDemoState> + use<> {
    with_source!(theme, {
        flex_col((
            label("Start-aligned (default)").render(theme),
            label("Center-aligned")
                .alignment(LabelAlignment::Center)
                .render(theme),
            label("End-aligned")
                .alignment(LabelAlignment::End)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(6.0))
    })
}

fn build_inner(theme: &Theme, state: &LabelDemoState) -> impl WidgetView<LabelDemoState> + use<> {
    let mask_toggle = checkbox(state.masked, |s: &mut LabelDemoState| {
        s.masked = !s.masked;
    })
    .label("Mask values")
    .render(theme);

    let masked = with_source!(theme, {
        flex_col((
            label("sk-proj-abc123xyz")
                .masked(state.masked)
                .render(theme),
            label("super-secret-token")
                .masked(state.masked)
                .secondary("hidden")
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(6.0))
    });

    let multiline = with_source!(theme, {
        label(
            "This is a longer paragraph that will wrap onto multiple lines \
             when the container is narrower than the text. Word-wrap is \
             enabled via .multiline(true).",
        )
        .multiline(true)
        .render(theme)
    });

    let title_block = flex_col((
        label("Label")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Themed text with optional secondary (muted) text, masking, word-wrap, and alignment.",
        )
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
            section_header("Sizes", theme),
            sizes_section(theme),
            section_header("Colors", theme),
            colors_section(theme),
            section_header("Secondary text", theme),
            secondary_section(theme),
            section_header("Masked", theme),
            mask_toggle,
            masked,
            section_header("Multiline", theme),
            multiline,
            section_header("Alignment", theme),
            alignment_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for LabelDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for LabelDemoPanel {
    type ViewState = LabelDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = LabelDemoState { masked: true };
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            LabelDemoPanelState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut LabelDemoPanelState,
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
        vs: &mut LabelDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut LabelDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
