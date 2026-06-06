//! Sidebar navigation item and panel demo panels used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{sidebar_item, sidebar_panel};
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::label;
use crate::scroll_container;
use crate::separator;
use crate::with_source;

// --- MARK: LOCAL STATE

struct SidebarDemo {
    collapsed: bool,
}

impl SidebarDemo {
    fn new() -> Self {
        Self { collapsed: false }
    }
}

type InnerView = Box<AnyWidgetView<SidebarDemo>>;
type InnerViewState = <InnerView as View<SidebarDemo, (), ViewCtx>>::ViewState;

// --- MARK: PANEL VIEW STATE

/// Opaque state for the combined sidebar demo panel.
pub struct SidebarDemoPanelState {
    demo: SidebarDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Outer view returned by [`panel`]. Manages the collapsible demo state.
pub struct SidebarDemoPanel {
    theme: Theme,
}

// --- MARK: INNER VIEW BUILDER

fn build_inner(theme: &Theme, state: &SidebarDemo) -> impl WidgetView<SidebarDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    // --- static item examples ---

    let active_example = with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut SidebarDemo| {})
                .active(true)
                .render(theme),
            sidebar_item("Data Grid", |_: &mut SidebarDemo| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut SidebarDemo| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    });

    let default_example = with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut SidebarDemo| {}).render(theme),
            sidebar_item("Data Grid", |_: &mut SidebarDemo| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut SidebarDemo| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    });

    // --- interactive collapse demo ---

    let items = flex_col((
        sidebar_item("Dashboard", |_: &mut SidebarDemo| {})
            .active(true)
            .render(theme),
        sidebar_item("Charts", |_: &mut SidebarDemo| {}).render(theme),
        sidebar_item("Settings", |_: &mut SidebarDemo| {}).render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(2.0));

    let content = flex_col((label("Content area")
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_muted)
        .render(theme),))
    .cross_axis_alignment(CrossAxisAlignment::Start);

    let panel_row = with_source!(theme, {
        flex_row((
            sidebar_panel(items, |s: &mut SidebarDemo| s.collapsed = !s.collapsed)
                .collapsed(state.collapsed)
                .render(theme),
            content,
        ))
        .main_axis_alignment(MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
    });

    let title_block = flex_col((
        label("Sidebar")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Collapsible navigation panel with an animated horizontal slide and a built-in toggle strip.")
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
            header("Active — teal accent bar on the selected nav item"),
            active_example,
            header("Default — hover shows fill, label muted when inactive"),
            default_example,
            header("Collapsible panel — click the ‹ strip on the right to collapse, › to expand"),
            panel_row,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

// --- MARK: VIEW IMPL

impl ViewMarker for SidebarDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for SidebarDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = SidebarDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = SidebarDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            SidebarDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut SidebarDemoPanelState,
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
        vs: &mut SidebarDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut SidebarDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}

// --- MARK: PUBLIC ENTRY POINT

/// Renders the full Sidebar demo panel (items + collapsible panel).
#[must_use]
pub fn panel(theme: &Theme) -> SidebarDemoPanel {
    SidebarDemoPanel { theme: *theme }
}
