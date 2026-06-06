//! Popover demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::Theme;
use crate::components::IconName;
use crate::components::ScrollBarVisibility;
use crate::components::button::button;
use crate::components::popover::{PopoverAnchor, popover};
use crate::label;
use crate::scroll_container;
use crate::with_source;

#[derive(Debug, Default)]
struct PopoverDemo;

type InnerView = Box<AnyWidgetView<PopoverDemo>>;
type InnerViewState = <InnerView as View<PopoverDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the popover demo panel.
pub struct PopoverDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct PopoverDemoPanelState {
    state: PopoverDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Popover demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> PopoverDemoPanel {
    PopoverDemoPanel { theme: *theme }
}

fn basic_row(theme: &Theme) -> impl WidgetView<PopoverDemo> + use<> {
    with_source!(theme, {
        popover(
            button(|_: &mut PopoverDemo| {})
                .label("Show info")
                .render(theme),
            flex_col((
                label("Popover title")
                    .color(theme.palette.text)
                    .render(theme),
                label("This content appears above all other widgets in a floating layer.")
                    .color(theme.palette.text_muted)
                    .multiline(true)
                    .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(6.0)),
        )
        .render(theme)
    })
}

fn anchor_row(theme: &Theme) -> impl WidgetView<PopoverDemo> + use<> {
    with_source!(theme, {
        flex_col((
            flex_row((
                popover(
                    button(|_: &mut PopoverDemo| {})
                        .label("BottomStart")
                        .icon(IconName::ArrowDownLeft)
                        .render(theme),
                    label("BottomStart").render(theme),
                )
                .anchor(PopoverAnchor::BottomStart)
                .render(theme),
                popover(
                    button(|_: &mut PopoverDemo| {})
                        .label("BottomCenter")
                        .icon(IconName::ArrowDown)
                        .render(theme),
                    label("BottomCenter").render(theme),
                )
                .anchor(PopoverAnchor::BottomCenter)
                .render(theme),
                popover(
                    button(|_: &mut PopoverDemo| {})
                        .label("BottomEnd")
                        .trailing_icon(IconName::ArrowDownRight)
                        .render(theme),
                    label("BottomEnd").render(theme),
                )
                .anchor(PopoverAnchor::BottomEnd)
                .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::SpaceBetween),
            flex_row((
                popover(
                    button(|_: &mut PopoverDemo| {})
                        .label("TopStart")
                        .icon(IconName::ArrowUpLeft)
                        .render(theme),
                    label("TopStart").render(theme),
                )
                .anchor(PopoverAnchor::TopStart)
                .render(theme),
                popover(
                    button(|_: &mut PopoverDemo| {})
                        .label("TopCenter")
                        .icon(IconName::ArrowUp)
                        .render(theme),
                    label("TopCenter").render(theme),
                )
                .anchor(PopoverAnchor::TopCenter)
                .render(theme),
                popover(
                    button(|_: &mut PopoverDemo| {})
                        .label("TopEnd")
                        .trailing_icon(IconName::ArrowUpRight)
                        .render(theme),
                    label("TopEnd").render(theme),
                )
                .anchor(PopoverAnchor::TopEnd)
                .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_alignment(MainAxisAlignment::SpaceBetween),
        ))
        .gap(Length::px(48.0))
    })
}

fn build_inner(theme: &Theme) -> impl WidgetView<PopoverDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Popover")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("A trigger widget that opens a floating content panel anchored at one of six positions. Click outside or press Escape to dismiss.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            header("Basic"),
            basic_row(theme),
            header("Anchor Variants"),
            anchor_row(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for PopoverDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for PopoverDemoPanel {
    type ViewState = PopoverDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let state = PopoverDemo;
        let inner_view: InnerView = Box::new(build_inner(&self.theme));
        let (element, inner_state) = inner_view.build(ctx, &mut { state });
        (
            element,
            PopoverDemoPanelState {
                state: PopoverDemo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut PopoverDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme));
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
        vs: &mut PopoverDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut PopoverDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
