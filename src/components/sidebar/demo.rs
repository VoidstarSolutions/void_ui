//! Sidebar navigation item and panel demo panels used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::label;

use super::{sidebar_collapse_button, sidebar_item, sidebar_panel};
use crate::Theme;
use crate::with_source;

// --- MARK: ITEM DEMO (stateless)

fn item_demo_view<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let active_example = with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut S| {})
                .active(true)
                .render(theme),
            sidebar_item("Data Grid", |_: &mut S| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut S| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    });

    let default_example = with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut S| {}).render(theme),
            sidebar_item("Data Grid", |_: &mut S| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut S| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    });

    flex_col((
        header("Active — teal accent bar on the selected nav item"),
        active_example,
        header("Default — hover shows fill, label muted when inactive"),
        default_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}

// --- MARK: PANEL DEMO (stateful, local state via custom View)

struct CollapseDemoState {
    collapsed: bool,
}

type InnerView = Box<AnyWidgetView<CollapseDemoState>>;
type InnerViewState = <InnerView as View<CollapseDemoState, (), ViewCtx>>::ViewState;

/// Opaque state owned by the panel demo.
pub struct SidebarPanelDemoState {
    state: CollapseDemoState,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Holds theme for the stateful panel demo panel.
pub struct SidebarPanelDemo {
    theme: Theme,
}

fn build_panel_inner(
    theme: &Theme,
    state: &CollapseDemoState,
) -> impl WidgetView<CollapseDemoState> + use<> {
    let items = flex_col((
        sidebar_collapse_button(|s: &mut CollapseDemoState| s.collapsed = true).render(theme),
        sidebar_item("Dashboard", |_: &mut CollapseDemoState| {})
            .active(true)
            .render(theme),
        sidebar_item("Charts", |_: &mut CollapseDemoState| {}).render(theme),
        sidebar_item("Settings", |_: &mut CollapseDemoState| {}).render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(2.0));

    let sidebar = sidebar_panel(items, state.collapsed).render(theme);

    let reopen_btn = sidebar_item("›  Expand", |s: &mut CollapseDemoState| s.collapsed = false)
        .render(theme);

    let content_area = flex_col((
        label("Content area")
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
        if state.collapsed { Some(reopen_btn) } else { None },
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(8.0));

    flex_row((sidebar, content_area))
        .main_axis_alignment(MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
}

impl ViewMarker for SidebarPanelDemo {}

impl<S: 'static> View<S, (), ViewCtx> for SidebarPanelDemo {
    type Element = Pod<Passthrough>;
    type ViewState = SidebarPanelDemoState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = CollapseDemoState { collapsed: false };
        let inner_view: InnerView = Box::new(build_panel_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            SidebarPanelDemoState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut SidebarPanelDemoState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_panel_inner(&self.theme, &vs.state));
        new_inner.rebuild(&vs.inner_view, &mut vs.inner_state, ctx, element, &mut vs.state);
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut SidebarPanelDemoState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut SidebarPanelDemoState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}

// --- MARK: COMBINED GALLERY PANEL

/// Renders the full Sidebar demo panel (items + collapsible panel).
///
/// Shown in the gallery under the "Sidebar" entry. Static item examples appear
/// above; the interactive collapsible panel demo appears below.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    flex_col((
        header("Items — active and default states"),
        item_demo_view(theme),
        header("Collapsible panel — click ‹ to collapse, › Expand to reopen"),
        SidebarPanelDemo { theme: *theme },
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}
