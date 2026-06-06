//! Collapsible section demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::collapsible;
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::components::checkbox::{checkbox};
use crate::label;
use crate::scroll_container;
use crate::with_source;

// --- MARK: LOCAL STATE

#[allow(clippy::struct_excessive_bools)]
struct CollapsibleDemo {
    first_open: bool,
    second_open: bool,
    third_open: bool,
    check_a: bool,
    check_b: bool,
}

impl CollapsibleDemo {
    fn new() -> Self {
        Self {
            first_open: true,
            second_open: false,
            third_open: true,
            check_a: false,
            check_b: true,
        }
    }
}

type InnerView = Box<AnyWidgetView<CollapsibleDemo>>;
type InnerViewState = <InnerView as View<CollapsibleDemo, (), ViewCtx>>::ViewState;

// --- MARK: PANEL VIEW STATE

pub struct CollapsibleDemoPanelState {
    demo: CollapsibleDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

pub struct CollapsibleDemoPanel {
    theme: Theme,
}

// --- MARK: INNER VIEW BUILDER

fn static_examples(
    theme: &Theme,
) -> (
    impl WidgetView<CollapsibleDemo> + use<>,
    impl WidgetView<CollapsibleDemo> + use<>,
) {
    let open_example = with_source!(theme, {
        collapsible(
            "Open section",
            flex_col((label("This body is visible.").color(theme.palette.text_muted).render(theme),))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(Length::px(8.0)),
            |_: &mut CollapsibleDemo| {},
        )
        .open(true)
        .render(theme)
    });

    let closed_example = with_source!(theme, {
        collapsible(
            "Closed section",
            flex_col((label("This body is hidden.").color(theme.palette.text_muted).render(theme),))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(Length::px(8.0)),
            |_: &mut CollapsibleDemo| {},
        )
        .open(false)
        .render(theme)
    });

    (open_example, closed_example)
}

fn interactive_examples(
    theme: &Theme,
    state: &CollapsibleDemo,
) -> (
    impl WidgetView<CollapsibleDemo> + use<>,
    impl WidgetView<CollapsibleDemo> + use<>,
    impl WidgetView<CollapsibleDemo> + use<>,
) {
    let basic = with_source!(theme, {
        collapsible(
            "Basic section",
            flex_col((
                label("Section body with some content.")
                    .color(theme.palette.text_muted)
                    .render(theme),
                label("A second line of content.")
                    .color(theme.palette.text_muted)
                    .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(8.0)),
            |s: &mut CollapsibleDemo| s.first_open = !s.first_open,
        )
        .open(state.first_open)
        .render(theme)
    });

    let with_inputs = with_source!(theme, {
        collapsible(
            "Section with inputs",
            flex_col((
                checkbox(state.check_a, |s: &mut CollapsibleDemo| {
                    s.check_a = !s.check_a;
                })
                .label("Option A")
                .render(theme),
                checkbox(state.check_b, |s: &mut CollapsibleDemo| {
                    s.check_b = !s.check_b;
                })
                .label("Option B")
                .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(8.0)),
            |s: &mut CollapsibleDemo| s.second_open = !s.second_open,
        )
        .open(state.second_open)
        .render(theme)
    });

    let nested = with_source!(theme, {
        collapsible(
            "Outer section",
            flex_col((collapsible(
                "Inner section",
                flex_col((
                    label("Nested body content.")
                        .color(theme.palette.text_muted)
                        .render(theme),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start),
                |s: &mut CollapsibleDemo| s.third_open = !s.third_open,
            )
            .open(state.third_open)
            .render(theme),))
            .cross_axis_alignment(CrossAxisAlignment::Stretch),
            |s: &mut CollapsibleDemo| s.first_open = !s.first_open,
        )
        .open(state.first_open)
        .render(theme)
    });

    (basic, with_inputs, nested)
}

fn build_inner(theme: &Theme, state: &CollapsibleDemo) -> impl WidgetView<CollapsibleDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let (open_example, closed_example) = static_examples(theme);
    let (basic, with_inputs, nested) = interactive_examples(theme, state);

    let title_block = flex_col((
        label("Collapsible")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Animated disclosure section. Click the header to show or hide the body content.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            header("Open — body fully visible"),
            open_example,
            header("Closed — body hidden"),
            closed_example,
            header("Interactive — click header to toggle"),
            basic,
            header("With interactive body content"),
            with_inputs,
            header("Nested collapsibles"),
            nested,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

// --- MARK: VIEW IMPL

impl ViewMarker for CollapsibleDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for CollapsibleDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = CollapsibleDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = CollapsibleDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            CollapsibleDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut CollapsibleDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.demo));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.demo,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut CollapsibleDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut CollapsibleDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}

// --- MARK: PUBLIC ENTRY POINT

#[must_use]
pub fn panel(theme: &Theme) -> CollapsibleDemoPanel {
    CollapsibleDemoPanel { theme: *theme }
}
