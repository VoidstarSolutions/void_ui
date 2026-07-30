//! Sidebar navigation item and panel demo panels used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{SidebarNavItem, sidebar_item, sidebar_nav, sidebar_panel};
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::scroll_container;
use crate::separator;
use crate::with_source;
use crate::{ButtonVariant, IconName, button, label};

// --- MARK: LOCAL STATE

struct SidebarDemo {
    collapsed: bool,
    nav_selected: usize,
    /// Most recent row selection or action click in the row-actions
    /// example, echoed underneath it so the reveal + click paths are
    /// observable.
    last_action: String,
}

impl SidebarDemo {
    fn new() -> Self {
        Self {
            collapsed: false,
            nav_selected: 0,
            last_action: String::new(),
        }
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

/// One row in the row-actions example: a label that selects the row, plus a
/// gear revealed on hover / keyboard focus. Clicking the gear and selecting
/// the row write distinct `last_action` text so the two paths are visibly
/// different.
fn actions_row(theme: &Theme, name: &'static str) -> impl WidgetView<SidebarDemo> + use<> {
    sidebar_item(name, move |s: &mut SidebarDemo| {
        s.last_action = format!("Selected {name}");
    })
    .action(
        button(move |s: &mut SidebarDemo| s.last_action = format!("Settings · {name}"))
            .icon(IconName::Settings)
            .variant(ButtonVariant::Text)
            .accessible_name(format!("{name} settings"))
            .render(theme),
    )
    .render(theme)
}

/// "Active" example: static items showing the selected accent bar and a
/// disabled entry.
fn active_example(theme: &Theme) -> impl WidgetView<SidebarDemo> + use<> {
    with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut SidebarDemo| {})
                .selected(true)
                .render(theme),
            sidebar_item("Data Grid", |_: &mut SidebarDemo| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut SidebarDemo| {}).render(theme),
            sidebar_item("Disabled entry", |_: &mut SidebarDemo| {})
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    })
}

/// "Default" example: unselected items showing hover fill and muted label.
fn default_example(theme: &Theme) -> impl WidgetView<SidebarDemo> + use<> {
    with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut SidebarDemo| {}).render(theme),
            sidebar_item("Data Grid", |_: &mut SidebarDemo| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut SidebarDemo| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    })
}

/// Row-actions (hover-reveal) example, plus the label underneath that echoes
/// the most recent row selection or action click. Returned together since
/// they're always used adjacently.
fn row_actions_section(
    theme: &Theme,
    state: &SidebarDemo,
) -> (
    impl WidgetView<SidebarDemo> + use<>,
    impl WidgetView<SidebarDemo> + use<>,
) {
    let row_actions_example = with_source!(theme, {
        flex_col((
            actions_row(theme, "AAPL"),
            actions_row(theme, "MSFT"),
            actions_row(theme, "GOOG"),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    });

    let row_actions_echo = {
        let text = if state.last_action.is_empty() {
            "Hover a row (or Tab to it), then click the gear — it acts without \
             selecting the row."
                .to_string()
        } else {
            state.last_action.clone()
        };
        label(text)
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme)
    };

    (row_actions_example, row_actions_echo)
}

/// Interactive collapse demo: nav list embedded in a `sidebar_panel`, with a
/// content pane beside it.
fn collapsible_panel_section(
    theme: &Theme,
    state: &SidebarDemo,
) -> impl WidgetView<SidebarDemo> + use<> {
    let items = sidebar_nav(
        vec![
            SidebarNavItem::new("Dashboard"),
            SidebarNavItem::new("Charts"),
            SidebarNavItem::new("Settings"),
        ],
        state.nav_selected,
        |s: &mut SidebarDemo, i| s.nav_selected = i,
    )
    .render(theme);

    let content = flex_col((label("Content area")
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_muted)
        .render(theme),))
    .cross_axis_alignment(CrossAxisAlignment::Start);

    with_source!(theme, {
        flex_row((
            sidebar_panel(items, |s: &mut SidebarDemo| s.collapsed = !s.collapsed)
                .collapsed(state.collapsed)
                .render(theme),
            content,
        ))
        .main_axis_alignment(MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
    })
}

fn build_inner(theme: &Theme, state: &SidebarDemo) -> impl WidgetView<SidebarDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let active_example = active_example(theme);
    let default_example = default_example(theme);
    let (row_actions_example, row_actions_echo) = row_actions_section(theme, state);
    let panel_row = collapsible_panel_section(theme, state);

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
            header("Active — accent bar on the selected nav item"),
            active_example,
            header("Default — hover shows fill, label muted when inactive"),
            default_example,
            header(
                "Row actions — trailing gear revealed on hover / keyboard focus; \
                 clicking it does not select the row",
            ),
            row_actions_example,
            row_actions_echo,
            header(
                "Collapsible panel — click the ‹ strip on the right to collapse, › to expand. Tab to the list, Up/Down to move focus, Enter/Space to select",
            ),
            panel_row,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
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
