//! Dialog demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::dialog;
use crate::Theme;
use crate::components::ButtonVariant;
use crate::components::button::button;
use crate::label;
use crate::overlay_scope::overlay_scope;
use crate::separator;
use crate::with_source;

#[derive(Debug, Default)]
struct DialogDemo {
    open: bool,
}

type InnerView = Box<AnyWidgetView<DialogDemo>>;
type InnerViewState = <InnerView as View<DialogDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the dialog demo panel.
pub struct DialogDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct DialogDemoPanelState {
    state: DialogDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Dialog demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> DialogDemoPanel {
    DialogDemoPanel { theme: *theme }
}

fn dialog_content(theme: &Theme) -> impl WidgetView<DialogDemo> + use<> {
    flex_col((
        label("Dialog title")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Dialogs sit above everything else, centered and a quarter of the way down the screen. Use them for focused tasks or confirmations.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
        separator().render(theme),
        flex_row((
            button(|state: &mut DialogDemo| state.open = false)
                .label("Cancel")
                .variant(ButtonVariant::Secondary)
                .render(theme),
            button(|state: &mut DialogDemo| state.open = false)
                .label("Confirm")
                .render(theme),
        ))
        .main_axis_alignment(MainAxisAlignment::End)
        .gap(Length::px(8.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(12.0))
}

fn basic_row(theme: &Theme, open: bool) -> impl WidgetView<DialogDemo> + use<> {
    with_source!(theme, {
        flex_col((
            button(|state: &mut DialogDemo| state.open = true)
                .label("Open Dialog")
                .render(theme),
            dialog(open, dialog_content(theme))
                .show_close_button()
                .on_dismiss(|state: &mut DialogDemo| state.open = false)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    })
}

fn build_inner(theme: &Theme, open: bool) -> impl WidgetView<DialogDemo> + use<> {
    let title_block = flex_col((
        label("Dialog")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("A container for focused content, positioned above everything else: centered horizontally and a quarter of the way down the screen. Optionally dismissed via a close button or an outside click.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let inner = flex_col((title_block, separator().render(theme), basic_row(theme, open)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0));
    overlay_scope(inner)
}

impl ViewMarker for DialogDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for DialogDemoPanel {
    type ViewState = DialogDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = DialogDemo::default();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, state.open));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            DialogDemoPanelState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut DialogDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, vs.state.open));
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
        vs: &mut DialogDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut DialogDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
