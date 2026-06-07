//! Resizable split-panel demo used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, label as xl_label, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{h_resizable, v_resizable};
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::label;
use crate::scroll_container;
use crate::with_source;

// --- MARK: LOCAL STATE

struct ResizableDemo {
    h_ratio: f32,
    v_ratio: f32,
    nested_h: f32,
    nested_v: f32,
}

impl ResizableDemo {
    fn new() -> Self {
        Self {
            h_ratio: 0.3,
            v_ratio: 0.4,
            nested_h: 0.5,
            nested_v: 0.5,
        }
    }
}

type InnerView = Box<AnyWidgetView<ResizableDemo>>;
type InnerViewState = <InnerView as View<ResizableDemo, (), ViewCtx>>::ViewState;

// --- MARK: PANEL STATE

pub struct ResizableDemoPanelState {
    demo: ResizableDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

pub struct ResizableDemoPanel {
    theme: Theme,
}

// --- MARK: PANE HELPER
//
// Returns Box<AnyWidgetView<State>> so the widget type is the concrete
// `Passthrough`, which satisfies `Sized + FromDynWidget` as required by
// `ResizableView`'s `View` impl bounds.

fn pane(
    text: &'static str,
    bg: Color,
    text_color: Color,
    font_size: f32,
) -> Box<AnyWidgetView<ResizableDemo>> {
    Box::new(
        sized_box(
            flex_col((xl_label(text).text_size(font_size).color(text_color),))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_alignment(MainAxisAlignment::Center),
        )
        .background_color(bg),
    )
}

// --- MARK: INNER VIEW BUILDER

#[allow(clippy::too_many_lines)]
fn build_inner(theme: &Theme, state: &ResizableDemo) -> impl WidgetView<ResizableDemo> + use<> {
    let p = &theme.palette;
    let caption = theme.typography.size_caption;

    let h_example = with_source!(theme, {
        sized_box(
            h_resizable(
                pane("Left", p.surface, p.text_muted, caption),
                pane("Right", p.surface_2, p.text_muted, caption),
                |s: &mut ResizableDemo, ratio: f32| s.h_ratio = ratio,
            )
            .ratio(state.h_ratio)
            .render(theme),
        )
        .fixed_height(Length::px(160.0))
    });

    let v_example = with_source!(theme, {
        sized_box(
            v_resizable(
                pane("Top", p.surface, p.text_muted, caption),
                pane("Bottom", p.surface_2, p.text_muted, caption),
                |s: &mut ResizableDemo, ratio: f32| s.v_ratio = ratio,
            )
            .ratio(state.v_ratio)
            .render(theme),
        )
        .fixed_height(Length::px(160.0))
    });

    // Nested: H-split with a V-split in the right pane.
    let nested_example = with_source!(theme, {
        sized_box(
            h_resizable(
                pane("Left", p.surface, p.text_muted, caption),
                v_resizable(
                    pane("Top right", p.surface_2, p.text_muted, caption),
                    pane("Bottom right", p.bg_deep, p.text_faint, caption),
                    |s: &mut ResizableDemo, ratio: f32| s.nested_v = ratio,
                )
                .ratio(state.nested_v)
                .render(theme),
                |s: &mut ResizableDemo, ratio: f32| s.nested_h = ratio,
            )
            .ratio(state.nested_h)
            .render(theme),
        )
        .fixed_height(Length::px(200.0))
    });

    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Resizable")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Drag the handle between panes to redistribute space.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            header("Horizontal split — drag the vertical handle"),
            h_example,
            header("Vertical split — drag the horizontal handle"),
            v_example,
            header("Nested splits"),
            nested_example,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

// --- MARK: VIEW IMPL

impl ViewMarker for ResizableDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ResizableDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = ResizableDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = ResizableDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            ResizableDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut ResizableDemoPanelState,
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
        vs: &mut ResizableDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut ResizableDemoPanelState,
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
pub fn panel(theme: &Theme) -> ResizableDemoPanel {
    ResizableDemoPanel { theme: *theme }
}
