//! Button demo panel used by the void-ui gallery.
//!
//! Each example block is wrapped in [`crate::with_source!`] so the code
//! snippet that produced the live output is shown directly below it. Adding
//! a new variant is: add a `header`, add an example block, wrap in
//! `with_source!`.

use masonry::kurbo::BezPath;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::button;
use crate::Theme;
use crate::components::ButtonVariant;
use crate::components::checkbox::checkbox;
use crate::with_source;

struct ButtonDemoState {
    disabled: bool,
    active: bool,
}

type InnerView = Box<AnyWidgetView<ButtonDemoState>>;
type InnerViewState = <InnerView as View<ButtonDemoState, (), ViewCtx>>::ViewState;

/// Opaque state owned by the button demo panel.
pub struct ButtonDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct ButtonDemoPanelState {
    state: ButtonDemoState,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Button demo panel.
///
/// Includes a "Disabled" checkbox that toggles all button variants between
/// enabled and disabled. Covers all 10 named variants plus loading and
/// trailing-icon states.
#[must_use]
pub fn panel(theme: &Theme) -> ButtonDemoPanel {
    ButtonDemoPanel { theme: *theme }
}

fn build_inner(theme: &Theme, state: &ButtonDemoState) -> impl WidgetView<ButtonDemoState> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let disabled_bool = state.disabled;

    let active_bool = state.active;

    let disabled_toggle = flex_row(
        checkbox(disabled_bool, |s: &mut ButtonDemoState| {
            s.disabled = !s.disabled
        })
        .label("disabled_bool")
        .render(theme),
    );

    let active_toggle = flex_row(
        checkbox(active_bool, |s: &mut ButtonDemoState| s.active = !s.active)
            .label("active_bool")
            .render(theme),
    );

    // --- variants ---

    let default_example = with_source!(theme, {
        flex_row((
            button("Default", |_: &mut ButtonDemoState| {})
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Primary", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Primary)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Secondary", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Secondary)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Danger", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Danger)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Warning", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Warning)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Success", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Success)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Info", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Info)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Ghost", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Ghost)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Link", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Link)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button("Text", |_: &mut ButtonDemoState| {})
                .variant(ButtonVariant::Text)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    // --- loading & trailing icon ---

    let loading_example = with_source!(theme, {
        flex_row((button("Saving…", |_: &mut ButtonDemoState| {})
            .variant(ButtonVariant::Primary)
            .loading(true)
            .disabled(disabled_bool)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let plus = {
        let mut p = BezPath::new();
        p.move_to((0.1, 0.4));
        p.line_to((0.4, 0.4));
        p.line_to((0.4, 0.1));
        p.line_to((0.6, 0.1));
        p.line_to((0.6, 0.4));
        p.line_to((0.9, 0.4));
        p.line_to((0.9, 0.6));
        p.line_to((0.6, 0.6));
        p.line_to((0.6, 0.9));
        p.line_to((0.4, 0.9));
        p.line_to((0.4, 0.6));
        p.line_to((0.1, 0.6));
        p.close_path();
        p
    };

    let caret = {
        let mut p = BezPath::new();
        p.move_to((0.2, 0.35));
        p.line_to((0.5, 0.65));
        p.line_to((0.8, 0.35));
        p
    };

    let leading_icon_example = with_source!(theme, {
        flex_row((button("Add item", |_: &mut ButtonDemoState| {})
            .icon(plus.clone())
            .disabled(disabled_bool)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let trailing_icon_example = with_source!(theme, {
        flex_row((button("More options", |_: &mut ButtonDemoState| {})
            .trailing_icon(caret.clone())
            .disabled(disabled_bool)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let top = flex_col((header("Variants"), default_example))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0));

    let bottom = flex_col((
        header("Loading — spinner, interaction blocked"),
        loading_example,
        header("Leading icon"),
        leading_icon_example,
        header("Trailing icon"),
        trailing_icon_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0));

    flex_col((flex_row((disabled_toggle, active_toggle)), top, bottom))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0))
}

impl ViewMarker for ButtonDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ButtonDemoPanel {
    type ViewState = ButtonDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = ButtonDemoState {
            disabled: false,
            active: false,
        };
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            ButtonDemoPanelState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut ButtonDemoPanelState,
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
        vs: &mut ButtonDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut ButtonDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
