//! Button group demo panel used by the void-ui gallery.
//!
//! The toggle group section uses a custom [`View`] whose [`ViewState`] holds
//! the selected index so the demo is fully self-contained — no gallery state
//! required.

use std::marker::PhantomData;

use masonry::widgets::Passthrough;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{button_group, toggle_button_group};
use crate::components::ButtonVariant;
use crate::components::scroll_container::ScrollBarVisibility;
use crate::with_source;
use crate::{Theme, scroll_container};

/// Renders the Button Group demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let horizontal_example = with_source!(theme, {
        flex_row((
            button_group(["Cut", "Copy", "Paste"], |_: &mut S, _i| {}).render(theme),
            button_group(["Bold", "Italic", "Underline"], |_: &mut S, _i| {})
                .variant(ButtonVariant::Ghost)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    });

    let vertical_example = with_source!(theme, {
        flex_row((
            button_group(["Top", "Middle", "Bottom"], |_: &mut S, _i| {})
                .vertical()
                .render(theme),
            button_group(["Day", "Week", "Month"], |_: &mut S, _i| {})
                .variant(ButtonVariant::Ghost)
                .vertical()
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0))
    });

    let variants_example = with_source!(theme, {
        flex_col((
            button_group(["A", "B", "C"], |_: &mut S, _i| {}).render(theme),
            button_group(["A", "B", "C"], |_: &mut S, _i| {})
                .variant(ButtonVariant::Primary)
                .render(theme),
            button_group(["A", "B", "C"], |_: &mut S, _i| {})
                .variant(ButtonVariant::Ghost)
                .render(theme),
            button_group(["A", "B", "C"], |_: &mut S, _i| {})
                .variant(ButtonVariant::Secondary)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    });

    let disabled_example = with_source!(theme, {
        flex_row((button_group(["Cut", "Copy", "Paste"], |_: &mut S, _i| {})
            .disabled(true)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let toggle_example = ToggleGroupDemo::new(*theme, 3);

    let toggle_vertical_example = ToggleGroupDemoVertical::new(*theme, 3);

    scroll_container(
        flex_col((
            header("Horizontal group"),
            horizontal_example,
            header("Vertical group"),
            vertical_example,
            header("Variants"),
            variants_example,
            header("Disabled"),
            disabled_example,
            header("Toggle group — click to change selection"),
            toggle_example,
            header("Toggle group — vertical"),
            toggle_vertical_example,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

// ---------------------------------------------------------------------------
// Self-contained horizontal toggle group demo
// ---------------------------------------------------------------------------

type ToggleDemoChildView<S> = Box<AnyWidgetView<S, usize>>;
type ToggleDemoChildState<S> = <ToggleDemoChildView<S> as View<S, usize, ViewCtx>>::ViewState;

/// Self-contained toggle group whose selected index lives in [`ViewState`].
pub struct ToggleGroupDemo<S: 'static> {
    theme: Theme,
    count: usize,
    phantom: PhantomData<fn() -> S>,
}

impl<S: 'static> ToggleGroupDemo<S> {
    #[must_use]
    pub fn new(theme: Theme, count: usize) -> Self {
        Self {
            theme,
            count,
            phantom: PhantomData,
        }
    }
}

#[doc(hidden)]
pub struct ToggleGroupDemoState<S: 'static> {
    selected: usize,
    prev_child: ToggleDemoChildView<S>,
    child_state: ToggleDemoChildState<S>,
}

fn build_toggle_child<S: 'static>(
    selected: usize,
    count: usize,
    theme: &Theme,
) -> ToggleDemoChildView<S> {
    let items: Vec<String> = (0..count).map(|i| format!("Option {}", i + 1)).collect();
    let t = *theme;
    Box::new(
        toggle_button_group(items, selected, move |_: &mut S, i| i)
            .variant(ButtonVariant::Primary)
            .render(&t),
    )
}

impl<S: 'static> ViewMarker for ToggleGroupDemo<S> {}

impl<S: 'static> View<S, (), ViewCtx> for ToggleGroupDemo<S> {
    type Element = Pod<Passthrough>;
    type ViewState = ToggleGroupDemoState<S>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut S) -> (Self::Element, Self::ViewState) {
        let selected = 0;
        let child = build_toggle_child(selected, self.count, &self.theme);
        let (element, child_state) = ctx.with_id(ViewId::new(0), |ctx| child.build(ctx, state));
        (
            element,
            ToggleGroupDemoState {
                selected,
                prev_child: child,
                child_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) {
        let new_child = build_toggle_child(vs.selected, self.count, &self.theme);
        ctx.with_id(ViewId::new(0), |ctx| {
            new_child.rebuild(&vs.prev_child, &mut vs.child_state, ctx, element, state);
        });
        vs.prev_child = new_child;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            vs.prev_child.teardown(&mut vs.child_state, ctx, element);
        });
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) -> MessageResult<()> {
        match message.take_first().map(ViewId::routing_id) {
            Some(0) => {
                let result = vs
                    .prev_child
                    .message(&mut vs.child_state, message, element, state);
                match result {
                    MessageResult::Action(idx) => {
                        vs.selected = idx;
                        MessageResult::RequestRebuild
                    }
                    MessageResult::RequestRebuild => MessageResult::RequestRebuild,
                    MessageResult::Nop => MessageResult::Nop,
                    MessageResult::Stale => MessageResult::Stale,
                }
            }
            _ => MessageResult::Stale,
        }
    }
}

// ---------------------------------------------------------------------------
// Self-contained vertical toggle group demo
// ---------------------------------------------------------------------------

/// Self-contained vertical toggle group whose selected index lives in [`ViewState`].
pub struct ToggleGroupDemoVertical<S: 'static> {
    theme: Theme,
    count: usize,
    phantom: PhantomData<fn() -> S>,
}

impl<S: 'static> ToggleGroupDemoVertical<S> {
    #[must_use]
    pub fn new(theme: Theme, count: usize) -> Self {
        Self {
            theme,
            count,
            phantom: PhantomData,
        }
    }
}

#[doc(hidden)]
pub struct ToggleGroupDemoVerticalState<S: 'static> {
    selected: usize,
    prev_child: ToggleDemoChildView<S>,
    child_state: ToggleDemoChildState<S>,
}

fn build_toggle_child_vertical<S: 'static>(
    selected: usize,
    count: usize,
    theme: &Theme,
) -> ToggleDemoChildView<S> {
    let items: Vec<String> = (0..count).map(|i| format!("Option {}", i + 1)).collect();
    let t = *theme;
    Box::new(
        toggle_button_group(items, selected, move |_: &mut S, i| i)
            .variant(ButtonVariant::Ghost)
            .vertical()
            .render(&t),
    )
}

impl<S: 'static> ViewMarker for ToggleGroupDemoVertical<S> {}

impl<S: 'static> View<S, (), ViewCtx> for ToggleGroupDemoVertical<S> {
    type Element = Pod<Passthrough>;
    type ViewState = ToggleGroupDemoVerticalState<S>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut S) -> (Self::Element, Self::ViewState) {
        let selected = 0;
        let child = build_toggle_child_vertical(selected, self.count, &self.theme);
        let (element, child_state) = ctx.with_id(ViewId::new(0), |ctx| child.build(ctx, state));
        (
            element,
            ToggleGroupDemoVerticalState {
                selected,
                prev_child: child,
                child_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) {
        let new_child = build_toggle_child_vertical(vs.selected, self.count, &self.theme);
        ctx.with_id(ViewId::new(0), |ctx| {
            new_child.rebuild(&vs.prev_child, &mut vs.child_state, ctx, element, state);
        });
        vs.prev_child = new_child;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            vs.prev_child.teardown(&mut vs.child_state, ctx, element);
        });
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) -> MessageResult<()> {
        match message.take_first().map(ViewId::routing_id) {
            Some(0) => {
                let result = vs
                    .prev_child
                    .message(&mut vs.child_state, message, element, state);
                match result {
                    MessageResult::Action(idx) => {
                        vs.selected = idx;
                        MessageResult::RequestRebuild
                    }
                    MessageResult::RequestRebuild => MessageResult::RequestRebuild,
                    MessageResult::Nop => MessageResult::Nop,
                    MessageResult::Stale => MessageResult::Stale,
                }
            }
            _ => MessageResult::Stale,
        }
    }
}
