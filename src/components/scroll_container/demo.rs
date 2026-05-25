//! Scroll container demo panel used by the void-ui gallery.

use masonry::widgets::Passthrough;
use xilem::AnyWidgetView;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{
    AnyFlexChild, CrossAxisAlignment, FlexExt as _, flex_col, flex_row, label, sized_box,
};
use xilem::{Pod, ViewCtx, WidgetView};

use super::{ScrollBarVisibility, scroll_container};
use crate::Theme;
use crate::components::radio::radio;
use crate::with_source;

// --- Static scroll demos ------------------------------------------------

/// Renders the Scroll Container demo panel.
///
/// Displays fixed-size viewports with overflow content and a live visibility
/// switcher that exercises [`ScrollBarVisibility::AlwaysVisible`],
/// [`ScrollBarVisibility::OnActivity`], and [`ScrollBarVisibility::AlwaysHidden`].
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let both_axes = with_source!(theme, {
        sized_box(scroll_container(content_grid::<S>(theme, 12, 8)).render(theme))
            .fixed_width(Length::px(320.0))
            .fixed_height(Length::px(200.0))
    });

    let vertical_only = with_source!(theme, {
        sized_box(
            scroll_container(content_grid::<S>(theme, 12, 8))
                .constrain_horizontal(true)
                .render(theme),
        )
        .fixed_width(Length::px(320.0))
        .fixed_height(Length::px(200.0))
    });

    sized_box(
        scroll_container(
            flex_col((
                header("Both axes — 12 × 8 grid in a 320 × 200 viewport"),
                both_axes,
                header("Vertical only — constrain_horizontal(true)"),
                vertical_only,
                header("Scrollbar visibility — switch to try each mode"),
                ScrollContainerDemoPanel { theme: *theme },
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(16.0)),
        )
        .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
        .render(theme),
    )
}

fn content_grid<S: 'static>(theme: &Theme, cols: u32, rows: u32) -> impl WidgetView<S> + use<S> {
    let cell_w = 80.0_f64;
    let cell_h = 32.0_f64;
    let bg_a = theme.palette.surface;
    let bg_b = theme.palette.surface_2;
    let caption = theme.typography.size_caption;
    let text_muted = theme.palette.text_muted;

    let row_views: Vec<AnyFlexChild<S, ()>> = (0..rows)
        .map(|r| {
            let cells: Vec<AnyFlexChild<S, ()>> = (0..cols)
                .map(|c| {
                    let bg = if (r + c) % 2 == 0 { bg_a } else { bg_b };
                    sized_box(
                        label(format!("{r},{c}"))
                            .text_size(caption)
                            .color(text_muted),
                    )
                    .fixed_width(Length::px(cell_w))
                    .fixed_height(Length::px(cell_h))
                    .background_color(bg)
                    .into_any_flex()
                })
                .collect();
            flex_row(cells)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .into_any_flex()
        })
        .collect();

    flex_col(row_views).cross_axis_alignment(CrossAxisAlignment::Start)
}

// --- Interactive visibility switcher ------------------------------------

type DemoState = ScrollBarVisibility;
type InnerView = Box<AnyWidgetView<DemoState>>;
type InnerViewState = <InnerView as View<DemoState, (), ViewCtx>>::ViewState;

/// Interactive demo panel that owns its scrollbar-visibility selection state.
pub struct ScrollContainerDemoPanel {
    theme: Theme,
}

/// View state for [`ScrollContainerDemoPanel`]. Public due to trait visibility;
/// treat as an implementation detail.
#[doc(hidden)]
pub struct ScrollContainerDemoPanelState {
    local: DemoState,
    view: InnerView,
    view_state: InnerViewState,
}

impl ViewMarker for ScrollContainerDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ScrollContainerDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = ScrollContainerDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut local: DemoState = DemoState::default();
        let view: InnerView = make_inner_view(&self.theme,local).boxed();
        let (element, view_state) = view.build(ctx, &mut local);
        (
            element,
            ScrollContainerDemoPanelState {
                local,
                view,
                view_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        _: &mut S,
    ) {
        let new_view: InnerView = make_inner_view(&self.theme,vs.local).boxed();
        new_view.rebuild(&vs.view, &mut vs.view_state, ctx, element, &mut vs.local);
        vs.view = new_view;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        vs.view.teardown(&mut vs.view_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.view
            .message(&mut vs.view_state, message, element, &mut vs.local)
    }
}

fn make_inner_view(theme: &Theme, vis: DemoState) -> impl WidgetView<DemoState> + use<> {
    let controls = flex_row((
        radio("Always visible", move |s: &mut DemoState| {
            *s = ScrollBarVisibility::AlwaysVisible;
        })
        .active(vis == ScrollBarVisibility::AlwaysVisible)
        .render(theme),
        radio("On activity", move |s: &mut DemoState| {
            *s = ScrollBarVisibility::OnActivity;
        })
        .active(vis == ScrollBarVisibility::OnActivity)
        .render(theme),
        radio("Always hidden", move |s: &mut DemoState| {
            *s = ScrollBarVisibility::AlwaysHidden;
        })
        .active(vis == ScrollBarVisibility::AlwaysHidden)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(16.0));

    let scroll = sized_box(
        scroll_container(content_grid::<DemoState>(theme, 12, 8))
            .constrain_horizontal(true)
            .scroll_bar_visibility(vis)
            .render(theme),
    )
    .fixed_width(Length::px(320.0))
    .fixed_height(Length::px(200.0));

    flex_col((controls, scroll))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(12.0))
}
