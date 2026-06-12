//! Context menu demo panel used by the void-ui gallery.
//!
//! Shows the [`MenuPanel`](super::widget::MenuPanel) two ways: rendered inline
//! (row rendering, hover, separators, disabled, click-to-select), and as a
//! right-click [`context_menu_area`] that pops the menu at the cursor.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::components::context_menu::{context_menu_area, item, menu, submenu};
use crate::components::icon::IconName;
use crate::label;
use crate::scroll_container;
use crate::with_source;

#[derive(Debug, Default)]
struct ContextMenuDemo {
    last_action: String,
    word_wrap: bool,
}

type InnerView = Box<AnyWidgetView<ContextMenuDemo>>;
type InnerViewState = <InnerView as View<ContextMenuDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the context menu demo panel.
pub struct ContextMenuDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct ContextMenuDemoPanelState {
    state: ContextMenuDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Context Menu demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> ContextMenuDemoPanel {
    ContextMenuDemoPanel { theme: *theme }
}

fn basic_menu(theme: &Theme) -> impl WidgetView<ContextMenuDemo> + use<> {
    with_source!(theme, {
        flex_row(
            menu()
                .section("Edit")
                .submenu(
                    submenu("Open Recent")
                        .icon(IconName::Copy)
                        .item(item("project-a").on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Recent: project-a".into();
                        }))
                        .item(item("project-b").on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Recent: project-b".into();
                        }))
                        .separator()
                        .item(item("Clear Recent").on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Clear Recent".into();
                        })),
                )
                .separator()
                .item(
                    item("Cut")
                        .shortcut("Ctrl+X")
                        .on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Cut".into();
                        }),
                )
                .item(
                    item("Copy")
                        .icon(IconName::Copy)
                        .shortcut("Ctrl+C")
                        .on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Copy".into();
                        }),
                )
                .item(
                    item("Paste")
                        .shortcut("Ctrl+V")
                        .disabled(true)
                        .on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Paste".into();
                        }),
                )
                .separator()
                .section("View")
                .item(
                    item("Word Wrap")
                        .checked(true)
                        .on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Word Wrap".into();
                        }),
                )
                .item(
                    item("Minimap")
                        .subtitle("Show a code overview on the right")
                        .checked(false)
                        .on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Minimap".into();
                        }),
                )
                .render(theme),
        )
    })
}

fn right_click_area(
    theme: &Theme,
    state: &ContextMenuDemo,
) -> impl WidgetView<ContextMenuDemo> + use<> {
    let wrap = state.word_wrap;
    with_source!(theme, {
        context_menu_area(
            sized_box(
                label("Right-click anywhere in this box")
                    .color(theme.palette.text_muted)
                    .render(theme),
            )
            .width(Length::px(320.0))
            .height(Length::px(140.0))
            .background_color(theme.palette.surface_2)
            .border_color(theme.palette.border)
            .border_width(Length::px(1.0)),
        )
        .section("Edit")
        .item(
            item("Cut")
                .shortcut("Ctrl+X")
                .on_select(|s: &mut ContextMenuDemo| {
                    s.last_action = "Cut".into();
                }),
        )
        .item(
            item("Copy")
                .icon(IconName::Copy)
                .shortcut("Ctrl+C")
                .on_select(|s: &mut ContextMenuDemo| {
                    s.last_action = "Copy".into();
                }),
        )
        .item(
            item("Paste")
                .shortcut("Ctrl+V")
                .disabled(true)
                .on_select(|s: &mut ContextMenuDemo| {
                    s.last_action = "Paste".into();
                }),
        )
        .separator()
        .section("View")
        // Live checkable: toggles on each selection so the gutter check
        // appears/disappears on the next open.
        .item(
            item("Word Wrap")
                .checked(wrap)
                .on_select(|s: &mut ContextMenuDemo| {
                    s.word_wrap = !s.word_wrap;
                    s.last_action = "Word Wrap".into();
                }),
        )
        .render(theme)
    })
}

fn build_inner(
    theme: &Theme,
    state: &ContextMenuDemo,
) -> impl WidgetView<ContextMenuDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Context Menu")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Rich menu surface — command rows with optional icons, keyboard \
             shortcuts, checkable state, sub-titles and hover-open submenus, \
             plus separators, section headers and disabled items. Right-click \
             the box below to open a menu at the cursor; click or use the \
             keyboard (arrows / Home / End / Enter / Esc) to select.",
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
            header("Menu (inline)"),
            basic_menu(theme),
            header("Right-click trigger"),
            right_click_area(theme, state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for ContextMenuDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ContextMenuDemoPanel {
    type ViewState = ContextMenuDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = ContextMenuDemo::default();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            ContextMenuDemoPanelState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut ContextMenuDemoPanelState,
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
        vs: &mut ContextMenuDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut ContextMenuDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
