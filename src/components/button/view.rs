//! Tessera `.tb-btn` button — interactive, theme-driven.
//!
//! Wraps [`super::widget::ThemedButton`] in a xilem [`View`]. Pointer
//! state (hover, press) is tracked by the masonry widget; the `active`
//! flag is the host-controlled selected-toggle state.
//!
//! ```ignore
//! use void_ui::components::button;
//! button("Reset view", |s: &mut State| s.reset())
//!     .active(false)
//!     .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::{ArcStr, StyleProperty, Widget as _};
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::ThemedButton;
use crate::Theme;

/// Builder for an interactive themed button.
///
/// Created with [`button`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Button does nothing until rendered with .render(&theme)"]
pub struct Button<F> {
    label: ArcStr,
    active: bool,
    callback: F,
}

/// Create a new button with the given label and click callback.
///
/// The callback is invoked on primary-pointer release inside the widget
/// and on Space / Enter while the widget is focused.
pub fn button<F>(label: impl Into<ArcStr>, callback: F) -> Button<F> {
    Button {
        label: label.into(),
        active: false,
        callback,
    }
}

impl<F> Button<F> {
    /// Mark this button as the currently-selected toggle. Tessera's
    /// `.tb-btn.active` — filled background, visible border.
    pub fn active(mut self, on: bool) -> Self {
        self.active = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ButtonView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        ButtonView {
            label: self.label,
            active: self.active,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Button`].
///
/// Built only through [`Button::render`]; not constructed directly by
/// callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ButtonView<F, State, Action> {
    label: ArcStr,
    active: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for ButtonView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for ButtonView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedButton>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut label = Label::new(self.label.clone())
            .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
            .prepare();
        // Without this, the Label inherits masonry's stock dark-theme
        // text color and turns invisible against the light palette.
        label
            .properties
            .insert(ContentColor::new(self.theme.palette.text));
        let widget = ThemedButton::new(label, &self.theme).with_active(self.active);
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
            ThemedButton::set_theme(&mut element, &self.theme);
            let mut child = ThemedButton::child_mut(&mut element);
            child.insert_prop(ContentColor::new(self.theme.palette.text));
            let mut label = child.downcast::<Label>();
            Label::insert_style(
                &mut label,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.active != prev.active {
            ThemedButton::set_active(&mut element, self.active);
        }
        // Label text and font size are not re-applied here; the masonry
        // Label widget doesn't currently expose post-construction text
        // mutation in a way that's stable to depend on. If the label
        // string changes, the parent view should typically replace the
        // whole button, which xilem will do via teardown+build.
        let _ = &prev.label;
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
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<ButtonPress>() {
            Some(_press) => MessageResult::Action((self.callback)(app_state)),
            None => MessageResult::Stale,
        }
    }
}
