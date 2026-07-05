//! Xilem view for the checkbox component.
//!
//! ```ignore
//! use void_ui::components::checkbox;
//! checkbox(state.enabled, |s: &mut State, checked: bool| s.enabled = checked)
//!     .label("Enable feature")
//!     .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::{ArcStr, StyleProperty, Widget as _};
use masonry::properties::ContentColor;
use masonry::widgets::Label;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::CheckboxPress;
use super::widget::CheckboxWidget;
use crate::Theme;

/// Builder for an interactive themed checkbox.
///
/// Created with [`checkbox`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Checkbox does nothing until rendered with .render(&theme)"]
pub struct Checkbox<F> {
    checked: bool,
    disabled: bool,
    label: Option<ArcStr>,
    callback: F,
}

/// Create a new checkbox with the given checked state and toggle callback.
///
/// The callback is invoked on primary-pointer release inside the widget and on
/// Space / Enter while the widget is focused. The callback receives the
/// **new** checked value (`!checked` at the time of the press); the host
/// stores it in its own state.
pub fn checkbox<F>(checked: bool, callback: F) -> Checkbox<F> {
    Checkbox {
        checked,
        disabled: false,
        label: None,
        callback,
    }
}

impl<F> Checkbox<F> {
    /// Attach a text label displayed to the right of the checkbox box.
    pub fn label(mut self, text: impl Into<ArcStr>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> CheckboxView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, bool) -> Action + Send + Sync + 'static,
    {
        CheckboxView {
            checked: self.checked,
            disabled: self.disabled,
            label: self.label,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Checkbox`].
///
/// Built only through [`Checkbox::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct CheckboxView<F, State, Action> {
    checked: bool,
    disabled: bool,
    label: Option<ArcStr>,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for CheckboxView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for CheckboxView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, bool) -> Action + Send + Sync + 'static,
{
    type Element = Pod<CheckboxWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let text_color = if self.disabled {
            self.theme.palette.text_faint
        } else {
            self.theme.palette.text
        };
        let mut widget = CheckboxWidget::new(&self.theme, self.checked, self.disabled);
        if let Some(text) = &self.label {
            let mut lbl = Label::new(text.clone())
                .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
                .prepare();
            lbl.properties.insert(ContentColor::new(text_color));
            widget = widget.with_label(lbl);
        }
        // `with_action_widget` registers the widget as an action source so that
        // `CheckboxPress` events bubble up to this view's `message` handler.
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
            CheckboxWidget::set_theme(&mut element, &self.theme);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else {
                self.theme.palette.text
            };
            if let Some(mut lbl) = CheckboxWidget::label_mut(&mut element) {
                lbl.insert_prop(ContentColor::new(text_color));
                let mut typed = lbl.downcast::<Label>();
                Label::insert_style(
                    &mut typed,
                    StyleProperty::FontSize(self.theme.density.ui_font_size),
                );
            }
        }
        if self.checked != prev.checked {
            CheckboxWidget::set_checked(&mut element, self.checked);
        }
        if self.disabled != prev.disabled {
            CheckboxWidget::set_disabled(&mut element, self.disabled);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else {
                self.theme.palette.text
            };
            if let Some(mut lbl) = CheckboxWidget::label_mut(&mut element) {
                lbl.insert_prop(ContentColor::new(text_color));
            }
        }
        if self.label != prev.label {
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else {
                self.theme.palette.text
            };
            match (&self.label, &prev.label) {
                (Some(text), Some(_)) => {
                    if let Some(mut lbl) = CheckboxWidget::label_mut(&mut element) {
                        lbl.insert_prop(ContentColor::new(text_color));
                        let mut typed = lbl.downcast::<Label>();
                        Label::insert_style(
                            &mut typed,
                            StyleProperty::FontSize(self.theme.density.ui_font_size),
                        );
                        Label::set_text(&mut typed, text.clone());
                    }
                }
                (Some(text), None) => {
                    let mut lbl = Label::new(text.clone())
                        .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
                        .prepare();
                    lbl.properties.insert(ContentColor::new(text_color));
                    CheckboxWidget::attach_label(&mut element, lbl);
                }
                (None, Some(_)) => {
                    CheckboxWidget::detach_label(&mut element);
                }
                (None, None) => {}
            }
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
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<CheckboxPress>() {
            // `checked` is the host-controlled prop this view was rendered
            // with; the widget never self-toggles, so the new value is its
            // negation by construction.
            Some(_) => MessageResult::Action((self.callback)(app_state, !self.checked)),
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::checkbox;
    use crate::Theme;

    /// Compile-level contract: the callback receives the NEW checked value.
    #[test]
    fn callback_takes_the_new_checked_value() {
        let theme = Theme::default();
        let _ = checkbox(false, |s: &mut bool, checked: bool| {
            *s = checked;
        })
        .label("check")
        .render::<bool, ()>(&theme);
    }
}
