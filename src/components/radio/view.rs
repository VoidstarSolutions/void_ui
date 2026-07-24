//! Themed radio button — interactive, host-managed selection state.
//!
//! Wraps [`super::widget::ThemedRadio`] in a xilem [`View`]. Pointer state
//! (hover, press) is tracked by the masonry widget; the `selected` flag is the
//! host-controlled selected state.
//!
//! Group mutual-exclusion is host-managed: render each option with
//! `.selected(selected == this_value)` and in the callback update `selected`.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # #[derive(PartialEq)]
//! # enum Choice { A, B }
//! # struct State { selected: Choice }
//! # let s = &State { selected: Choice::A };
//! use void_ui::components::radio;
//! radio("Option A", |s: &mut State| s.selected = Choice::A)
//!     .selected(s.selected == Choice::A)
//!     .render(&theme)
//! # ;
//! ```

use std::marker::PhantomData;

use masonry::core::{ArcStr, StyleProperty, Widget as _};
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::ThemedRadio;
use crate::Theme;

/// Builder for a themed radio button.
///
/// Created with [`radio`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Radio does nothing until rendered with .render(&theme)"]
pub struct Radio<F> {
    label: ArcStr,
    selected: bool,
    disabled: bool,
    callback: F,
}

/// Create a new radio button with the given label and selection callback.
///
/// The callback is invoked on primary-pointer release inside the widget and on
/// Space while the widget is focused. The host should update the selected value
/// in app state so this radio gets `.selected(true)` and siblings get `.selected(false)`.
pub fn radio<F>(label: impl Into<ArcStr>, callback: F) -> Radio<F> {
    Radio {
        label: label.into(),
        selected: false,
        disabled: false,
        callback,
    }
}

impl<F> Radio<F> {
    /// Mark this radio as the currently-selected option.
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    /// Deprecated alias of [`Self::selected`].
    #[deprecated(
        since = "0.1.0",
        note = "renamed to `selected` to match tabs/button_group vocabulary"
    )]
    pub fn active(self, on: bool) -> Self {
        self.selected(on)
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> RadioView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        RadioView {
            label: self.label,
            selected: self.selected,
            disabled: self.disabled,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Radio`].
///
/// Built only through [`Radio::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct RadioView<F, State, Action> {
    label: ArcStr,
    selected: bool,
    disabled: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for RadioView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for RadioView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedRadio>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let text_color = if self.disabled {
            self.theme.palette.text_faint
        } else {
            self.theme.palette.text
        };
        let mut label = Label::new(self.label.clone())
            .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
            .prepare();
        label.properties.insert(ContentColor::new(text_color));
        let widget = ThemedRadio::new(label, &self.theme)
            .with_selected(self.selected)
            .with_disabled(self.disabled);
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
            ThemedRadio::set_theme(&mut element, &self.theme);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else {
                self.theme.palette.text
            };
            let mut child = ThemedRadio::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
            let mut lbl = child.downcast::<Label>();
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.selected != prev.selected {
            ThemedRadio::set_selected(&mut element, self.selected);
        }
        if self.disabled != prev.disabled {
            ThemedRadio::set_disabled(&mut element, self.disabled);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else {
                self.theme.palette.text
            };
            let mut child = ThemedRadio::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
        }
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

#[cfg(test)]
mod tests {
    use super::radio;
    use crate::Theme;

    #[test]
    fn selected_is_the_canonical_builder_name() {
        let theme = Theme::default();
        let _ = radio("Option A", |_: &mut u8| {})
            .selected(true)
            .render::<u8, ()>(&theme);
    }

    /// The deprecated `.active` shim must still compile and delegate to
    /// `.selected` — a host mid-migration off the old name must see identical
    /// behavior.
    #[test]
    #[allow(deprecated)]
    fn deprecated_active_delegates_to_selected() {
        let theme = Theme::default();
        let view = radio("Option A", |_: &mut u8| {})
            .active(true)
            .render::<u8, ()>(&theme);
        assert!(view.selected);
    }
}
