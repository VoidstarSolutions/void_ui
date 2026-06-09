//! Toggle demo panel used by the void-ui gallery.

use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use crate::label;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::toggle;
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::scroll_container;
use crate::separator;
use crate::with_source;

#[derive(Debug, Clone, Default)]
struct ToggleDemo {
    bare: [bool; 2],
    labeled: [bool; 2],
}

impl ToggleDemo {
    fn new() -> Self {
        Self {
            bare: [false, true],
            labeled: [false, true],
        }
    }
}

type InnerView = Box<AnyWidgetView<ToggleDemo>>;
type InnerViewState = <InnerView as View<ToggleDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the toggle demo panel.
pub struct ToggleDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct ToggleDemoPanelState {
    toggle: ToggleDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Toggle demo panel.
///
/// The returned view owns all toggle state internally; callers do not need to
/// store or lens into any toggle state.
#[must_use]
pub fn panel(theme: &Theme) -> ToggleDemoPanel {
    ToggleDemoPanel { theme: *theme }
}

fn build_inner(theme: &Theme, state: &ToggleDemo) -> impl WidgetView<ToggleDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let [bare_a, bare_b] = state.bare;
    let [labeled_a, labeled_b] = state.labeled;

    let bare = with_source!(theme, {
        flex_row((
            toggle(bare_a, |s: &mut ToggleDemo| s.bare[0] = !s.bare[0]).render(theme),
            toggle(bare_b, |s: &mut ToggleDemo| s.bare[1] = !s.bare[1]).render(theme),
            toggle(false, |_: &mut ToggleDemo| {})
                .disabled(true)
                .render(theme),
            toggle(true, |_: &mut ToggleDemo| {})
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
    });

    let labeled = with_source!(theme, {
        flex_row((
            toggle(labeled_a, |s: &mut ToggleDemo| {
                s.labeled[0] = !s.labeled[0];
            })
            .label("Off")
            .render(theme),
            toggle(labeled_b, |s: &mut ToggleDemo| {
                s.labeled[1] = !s.labeled[1];
            })
            .label("On")
            .render(theme),
            toggle(false, |_: &mut ToggleDemo| {})
                .label("Disabled off")
                .disabled(true)
                .render(theme),
            toggle(true, |_: &mut ToggleDemo| {})
                .label("Disabled on")
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let title_block = flex_col((
        label("Toggle")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Two-state pill switch with optional label. State is host-controlled.")
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
            header("Switch only"),
            bare,
            header("With label"),
            labeled,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for ToggleDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ToggleDemoPanel {
    type ViewState = ToggleDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut toggle_state = ToggleDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &toggle_state));
        let (element, inner_state) = inner_view.build(ctx, &mut toggle_state);
        (
            element,
            ToggleDemoPanelState {
                toggle: toggle_state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut ToggleDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.toggle));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.toggle,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut ToggleDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut ToggleDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.toggle)
    }
}
