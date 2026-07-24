//! Xilem view layer for the dialog component.
//!
//! `Dialog<State, Action, ContentV, D>` is the builder; `.render(&theme)`
//! produces a `DialogView`.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { dialog_open: bool }
//! # let state = State { dialog_open: false };
//! # let my_content_view = void_ui::label("content").render(&theme);
//! use void_ui::components::dialog;
//! dialog(state.dialog_open, my_content_view)
//!     .show_close_button()
//!     .on_close(|s: &mut State| s.dialog_open = false)
//!     .render(&theme)
//! # ;
//! ```
//!
//! Unlike `popover`, a dialog's open/closed state is app-owned (there's no
//! trigger widget to toggle it internally), so [`Dialog::open`] is passed in
//! and [`Dialog::on_close`] reports both outside-click dismissal and the
//! optional close button back to app state — mirroring
//! [`crate::components::alert::Alert::on_close`]'s `CloseCallback` pattern.
//!
//! `render` erases the content view into an `Arc` and registers it with the
//! ROOT [`crate::overlay_scope`]'s [`OverlayPortal`] — the outermost scope
//! ancestor, regardless of how deeply nested the dialog's own scope ancestor
//! is (see [`crate::overlay_scope::root_portal_lookup`]) — so the dialog is always
//! centered on the whole region the app wrapped in `overlay_scope`, not just
//! a smaller sub-region. `popover`, by contrast, uses the *nearest* scope.
//! There is no in-tree fallback: a dialog has no trigger rect to anchor an
//! `AnchoredOverlay` to, so an `overlay_scope` ancestor is required.
//!
//! Because the root portal is looked up by downcasting a type-erased
//! `OverlayPortal<State, Action>`, a `dialog(...)` view's `State`/`Action`
//! must match the root `overlay_scope`'s exactly — typically the host
//! application's own state type. A `dialog` should not be wrapped in its own
//! (differently-typed) nested `overlay_scope`; demo panels or other
//! components with their own local state should instead embed their
//! open/closed flag in the host's state via an `AsMut<...>` host trait, the
//! way [`crate::components::notification::demo`] and
//! [`crate::components::dialog::demo`] do.

use std::marker::PhantomData;
use std::sync::Arc;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{DialogDismissed, DialogHost};
use crate::Theme;
use crate::components::button::{ButtonVariant, button};
use crate::components::icon::IconName;
use crate::overlay::SurfaceStyle;
use crate::overlay_portal::{OverlayPortal, PortalContentView, PortalPlacement};
use crate::overlay_scope::root_portal_lookup;

/// Erased close callback, present only when [`Dialog::on_close`] was set.
type OnClose<State, Action> = Arc<dyn Fn(&mut State) -> Action + Send + Sync>;

/// Implemented by `()` (no close callback) and by `Fn(&mut State) ->
/// Action` closures, so [`Dialog::on_close`] is optional without boxing the
/// callback. Mirrors [`crate::components::alert::CloseCallback`].
pub trait CloseCallback<State, Action>: Send + Sync + 'static {
    /// Whether outside-click dismissal and the close button should be active.
    #[must_use]
    fn enabled() -> bool {
        false
    }

    /// Invoke the callback. Only called when [`Self::enabled`] is `true`.
    fn call(&self, _state: &mut State) -> Action {
        unreachable!("CloseCallback::call on a disabled callback")
    }
}

impl<State: 'static, Action: 'static> CloseCallback<State, Action> for () {}

impl<State, Action, F> CloseCallback<State, Action> for F
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
    on_close: D,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Construct a dialog showing `content` when `open` is `true`.
///
/// The dialog is mounted above everything else inside the ROOT
/// [`crate::overlay_scope()`] ancestor — the outermost scope, regardless of how
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
        on_close: (),
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
    pub fn on_close<F>(self, on_close: F) -> Dialog<State, Action, ContentV, F>
    where
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        Dialog {
            open: self.open,
            content: self.content,
            show_close_button: self.show_close_button,
            on_close,
            phantom: PhantomData,
        }
    }

    /// Show an X close button that invokes [`Self::on_close`] when clicked.
    /// No-op unless [`Self::on_close`] is also set.
    pub fn show_close_button(mut self) -> Self {
        self.show_close_button = true;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> DialogView<State, Action>
    where
        D: CloseCallback<State, Action>,
    {
        let on_close: Option<OnClose<State, Action>> = D::enabled().then(|| {
            let on_close = self.on_close;
            Arc::new(move |state: &mut State| on_close.call(state)) as OnClose<State, Action>
        });

        let close_button = self
            .show_close_button
            .then(|| on_close.clone())
            .flatten()
            .map(|on_close| {
                button(move |state: &mut State| on_close(state))
                    .icon(IconName::X)
                    .variant(ButtonVariant::Text)
                    .accessible_name("Close")
                    .render(theme)
            });
        let header = close_button.map(|close_button| {
            flex_row((close_button,)).main_axis_alignment(MainAxisAlignment::End)
        });

        let composed =
            flex_col((header, self.content)).cross_axis_alignment(CrossAxisAlignment::Stretch);
        let content: Arc<PortalContentView<State, Action>> = Arc::new(composed);

        DialogView {
            open: self.open,
            content,
            on_close,
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
    on_close: Option<OnClose<State, Action>>,
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
        // Targets the outermost scope ancestor, and only succeeds if that scope
        // was published for the same `State`/`Action` pair as this dialog — so
        // dialogs must be rendered with the host app's own state type (see
        // module docs). `or_panic` distinguishes "no scope at all" from "a scope
        // that isn't yours", which have opposite fixes.
        let portal = root_portal_lookup::<State, Action>().or_panic("dialog");
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
        // dismissal (the close button, if any, invokes `on_close` directly).
        match (message.take_message::<DialogDismissed>(), &self.on_close) {
            (Some(_), Some(on_close)) => MessageResult::Action(on_close(app_state)),
            (Some(_), None) => MessageResult::Nop,
            (None, _) => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{CloseCallback, dialog};
    use crate::Theme;
    use crate::label::label;
    use crate::overlay_scope::overlay_scope;
    use crate::test_support;

    #[derive(Default)]
    struct AppState;

    /// A `dialog` rendered with the same `State`/`Action` pair as the root
    /// `overlay_scope` ancestor registers successfully — this is the only
    /// supported arrangement (see module docs): the dialog targets the
    /// outermost scope directly, so its types must match that scope's.
    #[test]
    fn dialog_builds_under_a_same_typed_root_overlay_scope() {
        let theme = Theme::default();
        let runtime = test_support::current_thread_runtime();
        let proxy = test_support::noop_proxy();

        let content = label("content").render::<AppState, ()>(&theme);
        let scope = overlay_scope(dialog(true, content).render(&theme));

        let mut ctx = ViewCtx::new(proxy, runtime);
        let mut state = AppState;
        let _ = scope.build(&mut ctx, &mut state);
    }

    #[test]
    fn closed_show_close_button_and_on_close_combinations_all_build() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        // Closed on mount.
        let content = label("content").render::<AppState, ()>(&theme);
        let scope = overlay_scope(dialog(false, content).render(&theme));
        let _ = scope.build(&mut ctx, &mut state);

        // `show_close_button` with no `on_close`: `CloseCallback::enabled()`
        // is `false` for `()`, so the button is a no-op per the doc comment
        // on `show_close_button` — must still build cleanly.
        let content = label("content").render::<AppState, ()>(&theme);
        let scope = overlay_scope(dialog(true, content).show_close_button().render(&theme));
        let _ = scope.build(&mut ctx, &mut state);

        // `on_close` without `show_close_button`: outside-click dismissal
        // wired, no header.
        let content = label("content").render::<AppState, ()>(&theme);
        let scope = overlay_scope(
            dialog(true, content)
                .on_close(|_: &mut AppState| ())
                .render(&theme),
        );
        let _ = scope.build(&mut ctx, &mut state);

        // Both set: header with a close button that invokes `on_close`.
        let content = label("content").render::<AppState, ()>(&theme);
        let scope = overlay_scope(
            dialog(true, content)
                .show_close_button()
                .on_close(|_: &mut AppState| ())
                .render(&theme),
        );
        let _ = scope.build(&mut ctx, &mut state);
    }

    #[test]
    fn show_close_button_sets_the_builder_flag() {
        let theme = Theme::default();
        let content = label("content").render::<AppState, ()>(&theme);
        let d = dialog(true, content).show_close_button();
        assert!(d.show_close_button);
    }

    fn assert_enabled<State: 'static, Action: 'static, D: CloseCallback<State, Action>>(
        expected: bool,
    ) {
        assert_eq!(D::enabled(), expected);
    }

    #[test]
    fn no_close_callback_is_disabled() {
        assert_enabled::<AppState, (), ()>(false);
    }

    #[test]
    fn a_close_callback_closure_is_enabled() {
        assert_enabled::<AppState, (), fn(&mut AppState)>(true);
    }

    #[test]
    #[should_panic(expected = "CloseCallback::call on a disabled callback")]
    fn calling_the_disabled_callback_panics() {
        let mut state = AppState;
        let _: () = <() as CloseCallback<AppState, ()>>::call(&(), &mut state);
    }
}
