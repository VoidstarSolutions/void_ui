//! Xilem view that wraps a [`ThemedButton`] containing a [`ClipboardWidget`]
//! icon child, and forwards the copy action to a caller-supplied callback.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! # impl State { fn write_to_clipboard(&mut self, _text: &str) {} }
//! use void_ui::components::clipboard;
//! clipboard("https://example.com/api", |s: &mut State, text: &str| {
//!     s.write_to_clipboard(text);
//! })
//! .render(&theme)
//! # ;
//! ```
//!
//! [`ThemedButton`]: crate::components::button::widget::ThemedButton

use std::marker::PhantomData;

use masonry::core::{ArcStr, NewWidget};
use masonry::widgets::ButtonPress;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::ClipboardWidget;
use crate::Theme;
use crate::components::button::widget::ThemedButton;

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
/// `value` is written to the system clipboard when the user activates the
/// button. `callback` is called afterward so the host can react (e.g. update
/// UI state, show a toast).
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
    type Element = Pod<ThemedButton>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let icon = NewWidget::new(ClipboardWidget::new(&self.theme));
        let widget = ThemedButton::new(icon.erased(), &self.theme)
            .with_accessibility_label(Some("Copy to clipboard".into()))
            .with_clipboard_payload(Some(self.value.clone()));
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
        if self.value != prev.value {
            ThemedButton::set_clipboard_payload(&mut element, Some(self.value.clone()));
        }
        if self.theme != prev.theme {
            ThemedButton::set_theme(&mut element, &self.theme);
            let mut child = ThemedButton::child_mut(&mut element);
            let mut icon = child.downcast::<ClipboardWidget>();
            ClipboardWidget::set_theme(&mut icon, &self.theme);
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
        match message.take_message::<ButtonPress>() {
            Some(_) => {
                let mut child = ThemedButton::child_mut(&mut element);
                let mut icon = child.downcast::<ClipboardWidget>();
                ClipboardWidget::set_copied(&mut icon, true);
                MessageResult::Action((self.callback)(app_state, &self.value))
            }
            None => MessageResult::Stale,
        }
    }
}
