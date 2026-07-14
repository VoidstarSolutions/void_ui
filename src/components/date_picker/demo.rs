//! Date picker demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::date_picker;
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::label;
use crate::overlay_scope::overlay_scope;
use crate::scroll_container;
use crate::separator;
use crate::with_source;

// --- MARK: LOCAL STATE

struct DatePickerDemo {
    selected: Option<chrono::NaiveDate>,
    bounded_selected: Option<chrono::NaiveDate>,
    controlled_open: bool,
    controlled_selected: Option<chrono::NaiveDate>,
}

impl DatePickerDemo {
    fn new() -> Self {
        Self {
            selected: None,
            bounded_selected: None,
            controlled_open: false,
            controlled_selected: None,
        }
    }
}

type InnerView = Box<AnyWidgetView<DatePickerDemo>>;
type InnerViewState = <InnerView as View<DatePickerDemo, (), ViewCtx>>::ViewState;

// --- MARK: PANEL VIEW STATE

pub struct DatePickerDemoPanelState {
    demo: DatePickerDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

pub struct DatePickerDemoPanel {
    theme: Theme,
}

// --- MARK: INNER VIEW BUILDER

fn basic_row(theme: &Theme, state: &DatePickerDemo) -> impl WidgetView<DatePickerDemo> + use<> {
    with_source!(theme, {
        date_picker(state.selected, |s: &mut DatePickerDemo, d| {
            s.selected = d;
        })
        .render(theme)
    })
}

fn placeholder_row(
    theme: &Theme,
    state: &DatePickerDemo,
) -> impl WidgetView<DatePickerDemo> + use<> {
    with_source!(theme, {
        date_picker(state.selected, |s: &mut DatePickerDemo, d| {
            s.selected = d;
        })
        .placeholder("Pick a date")
        .render(theme)
    })
}

fn cleanable_row(theme: &Theme, state: &DatePickerDemo) -> impl WidgetView<DatePickerDemo> + use<> {
    with_source!(theme, {
        date_picker(state.selected, |s: &mut DatePickerDemo, d| {
            s.selected = d;
        })
        .cleanable(true)
        .render(theme)
    })
}

fn bounded_row(theme: &Theme, state: &DatePickerDemo) -> impl WidgetView<DatePickerDemo> + use<> {
    with_source!(theme, {
        date_picker(state.bounded_selected, |s: &mut DatePickerDemo, d| {
            s.bounded_selected = d;
        })
        .min_date(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
        .max_date(chrono::NaiveDate::from_ymd_opt(2024, 12, 31).unwrap())
        .placeholder("2024 dates only")
        .render(theme)
    })
}

fn disabled_row(theme: &Theme) -> impl WidgetView<DatePickerDemo> + use<> {
    with_source!(theme, {
        date_picker(None, |_: &mut DatePickerDemo, _| {})
            .disabled(true)
            .render(theme)
    })
}

fn controlled_row(
    theme: &Theme,
    state: &DatePickerDemo,
) -> impl WidgetView<DatePickerDemo> + use<> {
    with_source!(theme, {
        flex_row((
            date_picker(state.controlled_selected, |s: &mut DatePickerDemo, d| {
                s.controlled_selected = d;
            })
            .open(state.controlled_open)
            .on_open_change(|s: &mut DatePickerDemo, open| {
                s.controlled_open = open;
            })
            .render(theme),
            label(if state.controlled_open {
                "Calendar: open"
            } else {
                "Calendar: closed"
            })
            .color(theme.palette.text_muted)
            .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

fn build_inner(theme: &Theme, state: &DatePickerDemo) -> impl WidgetView<DatePickerDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Date Picker")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Trigger that opens an inline calendar panel for selecting a date.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let inner = scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            header("Basic"),
            basic_row(theme, state),
            header("With placeholder"),
            placeholder_row(theme, state),
            header("Cleanable"),
            cleanable_row(theme, state),
            header("Min/max bounded (2024 only)"),
            bounded_row(theme, state),
            header("Controlled"),
            controlled_row(theme, state),
            header("Disabled"),
            disabled_row(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme);

    overlay_scope(inner)
}

// --- MARK: VIEW IMPL

impl ViewMarker for DatePickerDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for DatePickerDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = DatePickerDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = DatePickerDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            DatePickerDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut DatePickerDemoPanelState,
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
        vs: &mut DatePickerDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut DatePickerDemoPanelState,
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
pub fn panel(theme: &Theme) -> DatePickerDemoPanel {
    DatePickerDemoPanel { theme: *theme }
}
