//! Dropdown button demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::components::button::ButtonVariant;
use crate::components::dropdown_button::dropdown_button;
use crate::label;
use crate::scroll_container;
use crate::with_source;

#[derive(Debug, Default)]
struct DropdownDemo {
    last_action: String,
}

type InnerView = Box<AnyWidgetView<DropdownDemo>>;
type InnerViewState = <InnerView as View<DropdownDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the dropdown button demo panel.
pub struct DropdownDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct DropdownDemoPanelState {
    state: DropdownDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Dropdown Button demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> DropdownDemoPanel {
    DropdownDemoPanel { theme: *theme }
}

fn variants_row(theme: &Theme) -> impl WidgetView<DropdownDemo> + use<> {
    with_source!(theme, {
        flex_row((
            dropdown_button("Default")
                .item("Option A", |s: &mut DropdownDemo| {
                    s.last_action = "Option A".into();
                })
                .item("Option B", |s: &mut DropdownDemo| {
                    s.last_action = "Option B".into();
                })
                .render(theme),
            dropdown_button("Primary")
                .item("Option A", |s: &mut DropdownDemo| {
                    s.last_action = "Primary A".into();
                })
                .item("Option B", |s: &mut DropdownDemo| {
                    s.last_action = "Primary B".into();
                })
                .variant(ButtonVariant::Primary)
                .render(theme),
            dropdown_button("Danger")
                .item("Delete", |s: &mut DropdownDemo| {
                    s.last_action = "Delete".into();
                })
                .item("Archive", |s: &mut DropdownDemo| {
                    s.last_action = "Archive".into();
                })
                .variant(ButtonVariant::Danger)
                .render(theme),
            dropdown_button("Ghost")
                .item("Option A", |s: &mut DropdownDemo| {
                    s.last_action = "Ghost A".into();
                })
                .item("Option B", |s: &mut DropdownDemo| {
                    s.last_action = "Ghost B".into();
                })
                .variant(ButtonVariant::Ghost)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

fn disabled_row(theme: &Theme) -> impl WidgetView<DropdownDemo> + use<> {
    with_source!(theme, {
        flex_row(
            dropdown_button("Disabled")
                .item("Option A", |s: &mut DropdownDemo| {
                    s.last_action = "Disabled A".into();
                })
                .disabled(true)
                .render(theme),
        )
        .cross_axis_alignment(CrossAxisAlignment::Center)
    })
}

/// Nests a dropdown — with enough items that its menu is taller than its
/// host — inside an undersized scroll viewport, to confirm the menu is
/// clipped at the viewport's edge rather than painting over the rest of the
/// gallery (the headline behavior of the in-tree `AnchoredOverlay` hosting).
fn confinement_demo(theme: &Theme) -> impl WidgetView<DropdownDemo> + use<> {
    with_source!(theme, {
        sized_box(
            scroll_container(
                flex_col((
                    label("Scroll down, then open the menu — it's clipped by this box.")
                        .color(theme.palette.text_muted)
                        .multiline(true)
                        .render(theme),
                    dropdown_button("Actions")
                        .item("Alpha", |s: &mut DropdownDemo| {
                            s.last_action = "Alpha".into();
                        })
                        .item("Bravo", |s: &mut DropdownDemo| {
                            s.last_action = "Bravo".into();
                        })
                        .item("Charlie", |s: &mut DropdownDemo| {
                            s.last_action = "Charlie".into();
                        })
                        .item("Delta", |s: &mut DropdownDemo| {
                            s.last_action = "Delta".into();
                        })
                        .item("Echo", |s: &mut DropdownDemo| {
                            s.last_action = "Echo".into();
                        })
                        .item("Foxtrot", |s: &mut DropdownDemo| {
                            s.last_action = "Foxtrot".into();
                        })
                        .render(theme),
                    label("More content below the trigger…")
                        .color(theme.palette.text_faint)
                        .render(theme),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(Length::px(120.0)),
            )
            .render(theme),
        )
        .fixed_width(Length::px(280.0))
        .fixed_height(Length::px(160.0))
    })
}

fn build_inner(theme: &Theme, state: &DropdownDemo) -> impl WidgetView<DropdownDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Dropdown Button")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Button with trailing chevron — opens a menu anchored beneath it that's confined to its container's clip bounds.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let status = label(format!("Last: {}", state.last_action))
        .color(theme.palette.text_muted)
        .render(theme);

    scroll_container(
        flex_col((
            title_block,
            status,
            header("Variants"),
            variants_row(theme),
            header("Disabled"),
            disabled_row(theme),
            header("Confined to its container"),
            confinement_demo(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for DropdownDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for DropdownDemoPanel {
    type ViewState = DropdownDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = DropdownDemo::default();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            DropdownDemoPanelState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut DropdownDemoPanelState,
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
        vs: &mut DropdownDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut DropdownDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
