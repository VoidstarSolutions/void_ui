//! Dialog demo panel used by the void-ui gallery.
//!
//! A dialog registers with the *root* [`crate::overlay_scope`] (see
//! [`crate::overlay_scope::root_portal_lookup`]) so it's centered over the whole
//! window, not just this panel — which means it needs `State`/`Action` type
//! parameters matching that root scope's. So, like the notification demo (see
//! `project_xilem_local_state` in memory), this panel's open/closed flag
//! lives in the host application's state rather than being owned locally: the
//! host's state must implement [`AsMut<DialogDemoState>`].

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
use crate::separator;
use crate::with_source;

/// Open/closed flag for the dialog demo.
///
/// Owned by the host application's state (e.g. `examples/gallery.rs`'s
/// `State`) so the dialog can register with the root `overlay_scope`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DialogDemoState {
    pub open: bool,
}

/// Implemented by host state types that embed a [`DialogDemoState`].
pub trait DialogDemoHost: AsMut<DialogDemoState> + 'static {}

impl<S: AsMut<DialogDemoState> + 'static> DialogDemoHost for S {}

fn dialog_content<S: DialogDemoHost>(theme: &Theme) -> impl WidgetView<S, ()> + use<S> {
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
            button(|s: &mut S| s.as_mut().open = false)
                .label("Cancel")
                .variant(ButtonVariant::Secondary)
                .render(theme),
            button(|s: &mut S| s.as_mut().open = false)
                .label("Confirm")
                .render(theme),
        ))
        .main_axis_alignment(MainAxisAlignment::End)
        .gap(Length::px(8.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(12.0))
}

fn basic_row<S: DialogDemoHost>(theme: &Theme, open: bool) -> impl WidgetView<S, ()> + use<S> {
    with_source!(theme, {
        flex_col((
            button(|s: &mut S| s.as_mut().open = true)
                .label("Open Dialog")
                .render(theme),
            dialog(open, dialog_content::<S>(theme))
                .show_close_button()
                .on_close(|s: &mut S| s.as_mut().open = false)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    })
}

fn build_inner<S: DialogDemoHost>(theme: &Theme, open: bool) -> impl WidgetView<S, ()> + use<S> {
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

    flex_col((
        title_block,
        separator().render(theme),
        basic_row::<S>(theme, open),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}

type InnerView<S> = Box<AnyWidgetView<S, ()>>;
type InnerViewState<S> = <InnerView<S> as View<S, (), ViewCtx>>::ViewState;

/// Opaque state owned by the dialog demo panel.
pub struct DialogDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct DialogDemoPanelState<S: DialogDemoHost> {
    inner_view: InnerView<S>,
    inner_state: InnerViewState<S>,
}

/// Renders the Dialog demo panel.
///
/// `S` must implement [`AsMut<DialogDemoState>`]; the registered dialog
/// content is centered over the whole window via the root `overlay_scope`
/// (see [`crate::overlay_scope::root_portal_lookup`]).
#[must_use]
pub fn panel(theme: &Theme) -> DialogDemoPanel {
    DialogDemoPanel { theme: *theme }
}

impl ViewMarker for DialogDemoPanel {}

impl<S: DialogDemoHost> View<S, (), ViewCtx> for DialogDemoPanel {
    type ViewState = DialogDemoPanelState<S>;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut S) -> (Self::Element, Self::ViewState) {
        let open = app_state.as_mut().open;
        let inner_view: InnerView<S> = Box::new(build_inner::<S>(&self.theme, open));
        let (element, inner_state) = inner_view.build(ctx, app_state);
        (
            element,
            DialogDemoPanelState {
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut DialogDemoPanelState<S>,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        app_state: &mut S,
    ) {
        let open = app_state.as_mut().open;
        let new_inner: InnerView<S> = Box::new(build_inner::<S>(&self.theme, open));
        new_inner.rebuild(&vs.inner_view, &mut vs.inner_state, ctx, element, app_state);
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut DialogDemoPanelState<S>,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut DialogDemoPanelState<S>,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        app_state: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, app_state)
    }
}
