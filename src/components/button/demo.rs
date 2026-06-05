//! Button demo panel used by the void-ui gallery.
//!
//! Each example block is wrapped in [`crate::with_source!`] so the code
//! snippet that produced the live output is shown directly below it. Adding
//! a new variant is: add a `header`, add an example block, wrap in
//! `with_source!`.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use crate::label;
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::button;
use crate::components::checkbox::checkbox;
use crate::components::{ButtonVariant, IconName, ScrollBarVisibility};
use crate::with_source;
use crate::{Theme, scroll_container};

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

fn variants_example(
    theme: &Theme,
    disabled_bool: bool,
    active_bool: bool,
) -> impl WidgetView<ButtonDemoState> + use<> {
    with_source!(theme, {
        flex_row((
            button(|_: &mut ButtonDemoState| {})
                .label("Default")
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Primary")
                .variant(ButtonVariant::Primary)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Secondary")
                .variant(ButtonVariant::Secondary)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Danger")
                .variant(ButtonVariant::Danger)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Warning")
                .variant(ButtonVariant::Warning)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Success")
                .variant(ButtonVariant::Success)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Info")
                .variant(ButtonVariant::Info)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Ghost")
                .variant(ButtonVariant::Ghost)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Link")
                .variant(ButtonVariant::Link)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Text")
                .variant(ButtonVariant::Text)
                .disabled(disabled_bool)
                .active(active_bool)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

fn controls_row(
    theme: &Theme,
    state: &ButtonDemoState,
) -> impl WidgetView<ButtonDemoState> + use<> {
    let disabled_toggle = flex_row(
        checkbox(state.disabled, |s: &mut ButtonDemoState| {
            s.disabled = !s.disabled;
        })
        .label("disabled_bool")
        .render(theme),
    );
    let active_toggle = flex_row(
        checkbox(state.active, |s: &mut ButtonDemoState| s.active = !s.active)
            .label("active_bool")
            .render(theme),
    );
    flex_row((disabled_toggle, active_toggle))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
}

fn icons_section(theme: &Theme, disabled: bool) -> impl WidgetView<ButtonDemoState> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let loading_example = with_source!(theme, {
        flex_row((
            button(|_: &mut ButtonDemoState| {})
                .label("Saving…")
                .variant(ButtonVariant::Primary)
                .loading(true)
                .disabled(disabled)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Adding")
                .icon(IconName::Plus)
                .variant(ButtonVariant::Primary)
                .loading(true)
                .disabled(disabled)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });
    let leading_icon_example = with_source!(theme, {
        flex_row((button(|_: &mut ButtonDemoState| {})
            .label("Create")
            .icon(IconName::Plus)
            .disabled(disabled)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });
    let trailing_icon_example = with_source!(theme, {
        flex_row((button(|_: &mut ButtonDemoState| {})
            .label("More options")
            .trailing_icon(IconName::ChevronDown)
            .disabled(disabled)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });
    let icon_only_example = with_source!(theme, {
        flex_row((
            button(|_: &mut ButtonDemoState| {})
                .icon(IconName::Plus)
                .accessible_name("Add")
                .disabled(disabled)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .trailing_icon(IconName::ChevronDown)
                .accessible_name("Open menu")
                .disabled(disabled)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    flex_col((
        header("Loading — spinner, interaction blocked"),
        loading_example,
        header("Leading icon"),
        leading_icon_example,
        header("Trailing icon"),
        trailing_icon_example,
        header("Icon only"),
        icon_only_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}

fn build_inner(theme: &Theme, state: &ButtonDemoState) -> impl WidgetView<ButtonDemoState> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Button")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Themed, interactive button with style variants, disabled/active states, and an optional loading spinner.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let variants_section = flex_col((
        header("Variants"),
        variants_example(theme, state.disabled, state.active),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0));

    scroll_container(
        flex_col((
            title_block,
            controls_row(theme, state),
            variants_section,
            icons_section(theme, state.disabled),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
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
