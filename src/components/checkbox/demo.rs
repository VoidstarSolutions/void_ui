//! Checkbox demo panel used by the void-ui gallery.

use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};

use super::checkbox;
use crate::Theme;
use crate::with_source;

#[derive(Debug, Clone, Default)]
struct CheckboxDemo {
    bare: [bool; 2],
    labeled: [bool; 2],
}

impl CheckboxDemo {
    fn new() -> Self {
        Self {
            bare: [false, true],
            labeled: [false, true],
        }
    }
}

type InnerView = Box<AnyWidgetView<CheckboxDemo>>;
type InnerViewState = <InnerView as View<CheckboxDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the checkbox demo panel.
pub struct CheckboxDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct CheckboxDemoPanelState {
    checkbox: CheckboxDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Checkbox demo panel.
///
/// The returned view owns all toggle state internally; callers do not need to
/// store or lens into any checkbox state.
#[must_use]
pub fn panel(theme: &Theme) -> CheckboxDemoPanel {
    CheckboxDemoPanel { theme: *theme }
}

fn build_inner(theme: &Theme, state: &CheckboxDemo) -> impl WidgetView<CheckboxDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let [bare_a, bare_b] = state.bare;
    let [labeled_a, labeled_b] = state.labeled;

    // Box-only rows (no label)
    let bare = with_source!(theme, {
        flex_row((
            checkbox(bare_a, |s: &mut CheckboxDemo| s.bare[0] = !s.bare[0]).render(theme),
            checkbox(bare_b, |s: &mut CheckboxDemo| s.bare[1] = !s.bare[1]).render(theme),
            checkbox(false, |_: &mut CheckboxDemo| {})
                .disabled(true)
                .render(theme),
            checkbox(true, |_: &mut CheckboxDemo| {})
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
    });
    // Rows with labels
    let labeled = with_source!(theme, {
        flex_row((
            checkbox(labeled_a, |s: &mut CheckboxDemo| s.labeled[0] = !s.labeled[0])
                .label("Unchecked")
                .render(theme),
            checkbox(labeled_b, |s: &mut CheckboxDemo| s.labeled[1] = !s.labeled[1])
                .label("Checked")
                .render(theme),
            checkbox(false, |_: &mut CheckboxDemo| {})
                .label("Disabled off")
                .disabled(true)
                .render(theme),
            checkbox(true, |_: &mut CheckboxDemo| {})
                .label("Disabled on")
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    });

    flex_col((header("Box only"), bare, header("With label"), labeled))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0))
}

impl ViewMarker for CheckboxDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for CheckboxDemoPanel {
    type ViewState = CheckboxDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut checkbox = CheckboxDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &checkbox));
        let (element, inner_state) = inner_view.build(ctx, &mut checkbox);
        (element, CheckboxDemoPanelState { checkbox, inner_view, inner_state })
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut CheckboxDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.checkbox));
        new_inner.rebuild(&vs.inner_view, &mut vs.inner_state, ctx, element, &mut vs.checkbox);
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut CheckboxDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut CheckboxDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.checkbox)
    }
}
