//! Slider demo panel used by the void-ui gallery.

use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};

use crate::label;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{range_slider, slider};
use crate::{LabelAlignment, Orientation, Theme};
use crate::components::ScrollBarVisibility;
use crate::scroll_container;
use crate::separator;
use crate::with_source;

#[derive(Debug, Clone)]
struct SliderDemo {
    continuous: f64,
    stepped: f64,
    range: f64,
    range_low: f64,
    range_high: f64,
    vertical: f64,
    vertical_low: f64,
    vertical_high: f64,
}

impl SliderDemo {
    fn new() -> Self {
        Self {
            continuous: 0.4,
            stepped: 30.0,
            range: 0.0,
            range_low: 34.0,
            range_high: 56.0,
            vertical: 0.6,
            vertical_low: 25.0,
            vertical_high: 75.0,
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

/// Pairs a vertical slider with a value readout below it, both inside a
/// fixed-height track so the slider's travel length stays constant.
fn vertical_column<V>(theme: &Theme, track: V, value_text: String) -> impl WidgetView<SliderDemo> + use<V>
where
    V: WidgetView<SliderDemo> + 'static,
{
    flex_col((
        sized_box(track).fixed_height(Length::px(160.0)),
        sized_box(
            label(value_text)
                .alignment(LabelAlignment::Center)
                .color(theme.palette.text_muted)
                .render(theme),
        )
        .fixed_width(Length::px(56.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0))
}

fn title_block(theme: &Theme) -> impl WidgetView<SliderDemo> + use<> {
    flex_col((
        label("Slider")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Draggable value control with single-thumb and dual-thumb range \
             modes. Value, range, and step are host-controlled — drive them \
             from app state and apply the emitted value in the change callback.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0))
}

fn disabled_demo(theme: &Theme) -> impl WidgetView<SliderDemo> + use<> {
    with_source!(theme, {
        flex_row((
            sized_box(slider(0.3, |_: &mut SliderDemo, _| {}).disabled(true).render(theme))
                .fixed_width(Length::px(280.0)),
            sized_box(slider(0.7, |_: &mut SliderDemo, _| {}).disabled(true).render(theme))
                .fixed_width(Length::px(280.0)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(24.0))
    })
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

    let range_mode = with_source!(theme, {
        row(
            theme,
            range_slider(state.range_low, state.range_high, |s: &mut SliderDemo, lo, hi| {
                s.range_low = lo;
                s.range_high = hi;
            })
            .range(0.0, 100.0)
            .render(theme),
            format!("{:.0}..{:.0}", state.range_low, state.range_high),
        )
    });

    let vertical = with_source!(theme, {
        flex_row((
            vertical_column(
                theme,
                slider(state.vertical, |s: &mut SliderDemo, v| s.vertical = v)
                    .orientation(Orientation::Vertical)
                    .render(theme),
                format!("{:.2}", state.vertical),
            ),
            vertical_column(
                theme,
                range_slider(state.vertical_low, state.vertical_high, |s: &mut SliderDemo, lo, hi| {
                    s.vertical_low = lo;
                    s.vertical_high = hi;
                })
                .range(0.0, 100.0)
                .orientation(Orientation::Vertical)
                .render(theme),
                format!("{:.0}..{:.0}", state.vertical_low, state.vertical_high),
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(48.0))
    });

    let disabled = disabled_demo(theme);

    scroll_container(
        flex_col((
            title_block(theme),
            separator().render(theme),
            header("Continuous (0.0 – 1.0)"),
            continuous,
            header("Stepped (0 – 100, step 10)"),
            stepped,
            header("Custom range (-50 – 50)"),
            custom_range,
            header("Range mode (dual thumb, 0 – 100)"),
            range_mode,
            header("Vertical orientation"),
            vertical,
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
