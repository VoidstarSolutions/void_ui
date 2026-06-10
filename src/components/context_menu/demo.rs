//! Context menu demo panel used by the void-ui gallery.
//!
//! Chunk 1 renders the [`MenuPanel`](super::widget::MenuPanel) inline so the
//! row rendering, hover, separators, disabled state, and click-to-select can be
//! exercised on their own — ahead of the right-click trigger.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::components::context_menu::{item, menu};
use crate::label;
use crate::scroll_container;
use crate::with_source;

#[derive(Debug, Default)]
struct ContextMenuDemo {
    last_action: String,
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
                .item(item("Cut").on_select(|s: &mut ContextMenuDemo| {
                    s.last_action = "Cut".into();
                }))
                .item(item("Copy").on_select(|s: &mut ContextMenuDemo| {
                    s.last_action = "Copy".into();
                }))
                .item(
                    item("Paste")
                        .disabled(true)
                        .on_select(|s: &mut ContextMenuDemo| {
                            s.last_action = "Paste".into();
                        }),
                )
                .separator()
                .item(item("Select All").on_select(|s: &mut ContextMenuDemo| {
                    s.last_action = "Select All".into();
                }))
                .render(theme),
        )
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
            "Rich menu surface — command rows, separators, and disabled items. \
             Hover to highlight; click an enabled row to select it. (Right-click \
             trigger and icon/shortcut/check columns land in later chunks.)",
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
            header("Menu"),
            basic_menu(theme),
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
