//! Xilem view that wraps [`ClipboardWidget`] and forwards the copy action
//! to a caller-supplied callback.
//!
//! ```ignore
//! use void_ui::components::clipboard;
//! clipboard("https://example.com/api", |s: &mut State, text: &str| {
//!     s.write_to_clipboard(text);
//! })
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::{ClipboardPress, ClipboardWidget};
use crate::Theme;

/// Builder for a clipboard icon button.
///
/// Created with [`clipboard`]. Returns a xilem `View` via [`Self::render`].
#[must_use = "Clipboard does nothing until rendered with .render(&theme)"]
pub struct Clipboard<F> {
    value: ArcStr,
    callback: F,
}

/// Create a clipboard icon button.
///
/// `value` is the string passed to `callback` when the user activates the
/// button. The component does not write to the system clipboard itself —
/// the callback decides what to do with the value.
pub fn clipboard<F>(value: impl Into<ArcStr>, callback: F) -> Clipboard<F> {
    Clipboard {
        value: value.into(),
        callback,
    }
}

impl<F> Clipboard<F> {
    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ClipboardView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, &str) -> Action + Send + Sync + 'static,
    {
        ClipboardView {
            value: self.value,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Clipboard`].
///
/// Built only through [`Clipboard::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ClipboardView<F, State, Action> {
    value: ArcStr,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for ClipboardView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for ClipboardView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, &str) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ClipboardWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = ClipboardWidget::new(&self.theme);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            ClipboardWidget::set_theme(&mut element, &self.theme);
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<ClipboardPress>() {
            Some(_) => {
                ClipboardWidget::set_copied(&mut element, true);
                MessageResult::Action((self.callback)(app_state, &self.value))
            }
            None => MessageResult::Stale,
        }
    }
}
