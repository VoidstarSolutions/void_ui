//! Slider demo panel used by the void-ui gallery.

use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};

use crate::label;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::slider;
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::scroll_container;
use crate::separator;
use crate::with_source;

#[derive(Debug, Clone)]
struct SliderDemo {
    continuous: f64,
    stepped: f64,
    range: f64,
}

impl SliderDemo {
    fn new() -> Self {
        Self {
            continuous: 0.4,
            stepped: 30.0,
            range: 0.0,
        }
    }
}

type InnerView = Box<AnyWidgetView<SliderDemo>>;
type InnerViewState = <InnerView as View<SliderDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the slider demo panel.
pub struct SliderDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct SliderDemoPanelState {
    slider: SliderDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Slider demo panel.
///
/// The returned view owns all slider state internally; callers do not need to
/// store or lens into any slider state.
#[must_use]
pub fn panel(theme: &Theme) -> SliderDemoPanel {
    SliderDemoPanel { theme: *theme }
}

/// Pairs a row's slider with a value readout, both inside a fixed-width track
/// so the value label doesn't shift the layout as it changes width.
fn row<V>(theme: &Theme, track: V, value_text: String) -> impl WidgetView<SliderDemo> + use<V>
where
    V: WidgetView<SliderDemo> + 'static,
{
    flex_row((
        sized_box(track).fixed_width(Length::px(280.0)),
        sized_box(label(value_text).color(theme.palette.text_muted).render(theme))
            .fixed_width(Length::px(56.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(12.0))
}

fn build_inner(theme: &Theme, state: &SliderDemo) -> impl WidgetView<SliderDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let continuous = with_source!(theme, {
        row(
            theme,
            slider(state.continuous, |s: &mut SliderDemo, v| s.continuous = v).render(theme),
            format!("{:.2}", state.continuous),
        )
    });

    let stepped = with_source!(theme, {
        row(
            theme,
            slider(state.stepped, |s: &mut SliderDemo, v| s.stepped = v)
                .range(0.0, 100.0)
                .step(10.0)
                .render(theme),
            format!("{:.0}", state.stepped),
        )
    });

    let custom_range = with_source!(theme, {
        row(
            theme,
            slider(state.range, |s: &mut SliderDemo, v| s.range = v)
                .range(-50.0, 50.0)
                .render(theme),
            format!("{:.1}", state.range),
        )
    });

    let disabled = with_source!(theme, {
        flex_row((
            sized_box(slider(0.3, |_: &mut SliderDemo, _| {}).disabled(true).render(theme))
                .fixed_width(Length::px(280.0)),
            sized_box(slider(0.7, |_: &mut SliderDemo, _| {}).disabled(true).render(theme))
                .fixed_width(Length::px(280.0)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(24.0))
    });

    let title_block = flex_col((
        label("Slider")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Draggable single-value range control. Value, range, and step are \
             host-controlled — drive them from app state and apply the emitted \
             value in the change callback.",
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
            header("Continuous (0.0 – 1.0)"),
            continuous,
            header("Stepped (0 – 100, step 10)"),
            stepped,
            header("Custom range (-50 – 50)"),
            custom_range,
            header("Disabled"),
            disabled,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for SliderDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for SliderDemoPanel {
    type ViewState = SliderDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut slider = SliderDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &slider));
        let (element, inner_state) = inner_view.build(ctx, &mut slider);
        (
            element,
            SliderDemoPanelState {
                slider,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut SliderDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.slider));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.slider,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut SliderDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut SliderDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.slider)
    }
}
