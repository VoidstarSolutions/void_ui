//! Dropdown button demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::components::ScrollBarVisibility;
use crate::components::button::ButtonVariant;
use crate::components::dropdown_button::dropdown_button;
use crate::label;
use crate::overlay_scope::overlay_scope;
use crate::scroll_container;
use crate::with_source;
use crate::{Theme, separator};

#[derive(Debug, Default)]
struct DropdownDemo {
    last_action: String,
    menu_open: bool,
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

fn controlled_row(theme: &Theme, open: bool) -> impl WidgetView<DropdownDemo> + use<> {
    with_source!(theme, {
        flex_row((
            dropdown_button("Controlled")
                .item("Option A", |s: &mut DropdownDemo| {
                    s.last_action = "Controlled A".into();
                })
                .open(open)
                .on_open_change(|s: &mut DropdownDemo, open| s.menu_open = open)
                .render(theme),
            label(if open { "Menu: open" } else { "Menu: closed" })
                .color(theme.palette.text_muted)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
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

    let inner = scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            status,
            header("Variants"),
            variants_row(theme),
            header("Controlled"),
            controlled_row(theme, state.menu_open),
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
