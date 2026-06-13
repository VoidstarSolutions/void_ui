//! Xilem view layer for the dialog component.
//!
//! `Dialog<State, Action, ContentV, D>` is the builder; `.render(&theme)`
//! produces a `DialogView`.
//!
//! ```ignore
//! use void_ui::components::dialog;
//! dialog(state.dialog_open, my_content_view)
//!     .show_close_button()
//!     .on_dismiss(|s: &mut State| s.dialog_open = false)
//!     .render(&theme)
//! ```
//!
//! Unlike `popover`, a dialog's open/closed state is app-owned (there's no
//! trigger widget to toggle it internally), so [`Dialog::open`] is passed in
//! and [`Dialog::on_dismiss`] reports both outside-click dismissal and the
//! optional close button back to app state — mirroring
//! [`crate::components::alert::Alert::on_close`]'s `CloseCallback` pattern.
//!
//! `render` erases the content view into an `Arc` and registers it with the
//! ROOT [`crate::overlay_scope`]'s [`OverlayPortal`] — the outermost scope
//! ancestor, regardless of how deeply nested the dialog's own scope ancestor
//! is (see [`crate::overlay_scope::root_portal`]) — so the dialog is always
//! centered on the whole region the app wrapped in `overlay_scope`, not just
//! a smaller sub-region. `popover`, by contrast, uses the *nearest* scope.
//! There is no in-tree fallback: a dialog has no trigger rect to anchor an
//! `AnchoredOverlay` to, so an `overlay_scope` ancestor is required.

use std::marker::PhantomData;
use std::sync::Arc;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{DialogDismissed, DialogHost};
use crate::Theme;
use crate::components::button::{ButtonVariant, button};
use crate::components::icon::IconName;
use crate::components::popover::widget::SurfaceStyle;
use crate::overlay_portal::{OverlayPortal, PortalContentView, PortalPlacement};
use crate::overlay_scope::root_portal;

/// Implemented by `()` (no dismiss callback) and by `Fn(&mut State) ->
/// Action` closures, so [`Dialog::on_dismiss`] is optional without boxing the
/// callback. Mirrors [`crate::components::alert::CloseCallback`].
pub trait DismissCallback<State, Action>: Send + Sync + 'static {
    /// Whether outside-click dismissal and the close button should be active.
    #[must_use]
    fn enabled() -> bool {
        false
    }

    /// Invoke the callback. Only called when [`Self::enabled`] is `true`.
    fn call(&self, _state: &mut State) -> Action {
        unreachable!("DismissCallback::call on a disabled callback")
    }
}

impl<State: 'static, Action: 'static> DismissCallback<State, Action> for () {}

impl<State, Action, F> DismissCallback<State, Action> for F
where
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    fn enabled() -> bool {
        true
    }

    fn call(&self, state: &mut State) -> Action {
        self(state)
    }
}

/// Builder for a dialog.
///
/// Created with [`dialog`]; configure with builder methods; materialize as a
/// xilem view via [`Self::render`].
#[must_use = "Dialog does nothing until rendered with .render(&theme)"]
pub struct Dialog<State, Action, ContentV, D = ()> {
    open: bool,
    content: ContentV,
    show_close_button: bool,
    on_dismiss: D,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Construct a dialog showing `content` when `open` is `true`.
///
/// The dialog is mounted above everything else inside the ROOT
/// [`crate::overlay_scope`] ancestor — the outermost scope, regardless of how
/// deeply nested the dialog itself is — horizontally centered and a quarter
/// of the way down that container.
pub fn dialog<State, Action, ContentV>(
    open: bool,
    content: ContentV,
) -> Dialog<State, Action, ContentV>
where
    State: 'static,
    Action: 'static,
    ContentV: WidgetView<State, Action>,
{
    Dialog {
        open,
        content,
        show_close_button: false,
        on_dismiss: (),
        phantom: PhantomData,
    }
}

impl<State, Action, ContentV, D> Dialog<State, Action, ContentV, D>
where
    State: 'static,
    Action: 'static,
    ContentV: WidgetView<State, Action>,
{
    /// Report outside-click dismissal and the optional close button (see
    /// [`Self::show_close_button`]) back to app state.
    pub fn on_dismiss<F>(self, on_dismiss: F) -> Dialog<State, Action, ContentV, F>
    where
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        Dialog {
            open: self.open,
            content: self.content,
            show_close_button: self.show_close_button,
            on_dismiss,
            phantom: PhantomData,
        }
    }

    /// Show an X close button that invokes [`Self::on_dismiss`] when clicked.
    /// No-op unless [`Self::on_dismiss`] is also set.
    pub fn show_close_button(mut self) -> Self {
        self.show_close_button = true;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> DialogView<State, Action>
    where
        D: DismissCallback<State, Action>,
    {
        let has_dismiss = D::enabled();
        let on_dismiss = self.on_dismiss;
        let on_dismiss: Arc<dyn Fn(&mut State) -> Action + Send + Sync> =
            Arc::new(move |state: &mut State| on_dismiss.call(state));

        let close_button = (self.show_close_button && has_dismiss).then(|| {
            let on_dismiss = on_dismiss.clone();
            button(move |state: &mut State| on_dismiss(state))
                .icon(IconName::X)
                .variant(ButtonVariant::Text)
                .accessible_name("Close")
                .render(theme)
        });
        let header = close_button
            .map(|close_button| flex_row((close_button,)).main_axis_alignment(MainAxisAlignment::End));

        let composed = flex_col((header, self.content)).cross_axis_alignment(CrossAxisAlignment::Stretch);
        let content: Arc<PortalContentView<State, Action>> = Arc::new(composed);

        DialogView {
            open: self.open,
            content,
            on_dismiss,
            has_dismiss,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`Dialog`].
///
/// Not constructed directly; use [`Dialog::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct DialogView<State, Action> {
    open: bool,
    content: Arc<PortalContentView<State, Action>>,
    on_dismiss: Arc<dyn Fn(&mut State) -> Action + Send + Sync>,
    has_dismiss: bool,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> ViewMarker for DialogView<State, Action> {}

/// View state for `DialogView`: the portal registration and its key.
#[doc(hidden)]
pub struct DialogViewState<State: 'static, Action: 'static> {
    portal: OverlayPortal<State, Action>,
    key: u64,
}

impl<State, Action> View<State, Action, ViewCtx> for DialogView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<DialogHost>;
    type ViewState = DialogViewState<State, Action>;

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = root_portal::<State, Action>().expect(
            "dialog requires an overlay_scope ancestor — wrap the app root (or region) in overlay_scope(...)",
        );
        let key = portal.register(
            self.content.clone(),
            &self.theme,
            PortalPlacement::Trigger,
            SurfaceStyle::Dialog,
        );
        let widget = DialogHost::new(portal.scope().clone(), key, self.open);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, DialogViewState { portal, key })
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // Content rebuild happens when the scope's view diffs the registry
        // (after our subtree's rebuild returns) — we only refresh the
        // registered view value here.
        view_state.portal.update(
            view_state.key,
            self.content.clone(),
            &self.theme,
            PortalPlacement::Trigger,
            SurfaceStyle::Dialog,
        );
        if self.open != prev.open {
            DialogHost::set_open(&mut element, self.open);
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        // The scope's next rebuild (same pass) unmounts the slot child.
        view_state.portal.deregister(view_state.key);
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // Content messages route through the scope's slot path, never through
        // us; we're only the `DialogDismissed` action source for outside-click
        // dismissal (the close button, if any, invokes `on_dismiss` directly).
        match message.take_message::<DialogDismissed>() {
            Some(_) if self.has_dismiss => MessageResult::Action((self.on_dismiss)(app_state)),
            Some(_) => MessageResult::Nop,
            None => MessageResult::Stale,
        }
    }
}
