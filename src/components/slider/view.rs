//! Xilem view for the slider component.
//!
//! ```ignore
//! use void_ui::components::slider;
//! slider(state.volume, |s: &mut State, v| s.volume = v)
//!     .range(0.0, 100.0)
//!     .step(1.0)
//!     .render(&theme)
//! ```

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::SliderChanged;
use super::widget::SliderWidget;
use crate::Theme;

/// Builder for an interactive themed slider.
///
/// Created with [`slider`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Slider does nothing until rendered with .render(&theme)"]
pub struct Slider<F> {
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    disabled: bool,
    callback: F,
}

/// Create a new slider with the given value and change callback.
///
/// `value` is host-controlled — the widget never mutates it. The callback is
/// invoked with the new value on drag, click-to-jump, arrow-key nudges, and
/// accessibility actions; the host is responsible for storing the value and
/// passing it back in on the next render.
///
/// Defaults to the range `0.0..=1.0` with continuous (unstepped) values.
pub fn slider<F>(value: f64, callback: F) -> Slider<F> {
    Slider {
        value,
        min: 0.0,
        max: 1.0,
        step: 0.0,
        disabled: false,
        callback,
    }
}

impl<F> Slider<F> {
    /// Sets the inclusive value range. `min` must be less than `max`.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Sets the snap increment. Values are rounded to the nearest multiple of
    /// `step` away from `min`. Pass `0.0` (the default) for continuous values.
    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> SliderView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, f64) -> Action + Send + Sync + 'static,
    {
        SliderView {
            value: self.value,
            min: self.min,
            max: self.max,
            step: self.step,
            disabled: self.disabled,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Slider`].
///
/// Built only through [`Slider::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SliderView<F, State, Action> {
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    disabled: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for SliderView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for SliderView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, f64) -> Action + Send + Sync + 'static,
{
    type Element = Pod<SliderWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = SliderWidget::new(
            &self.theme,
            self.value,
            self.min,
            self.max,
            self.step,
            self.disabled,
        );
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
            SliderWidget::set_theme(&mut element, &self.theme);
        }
        if (self.min - prev.min).abs() > f64::EPSILON || (self.max - prev.max).abs() > f64::EPSILON {
            SliderWidget::set_range(&mut element, self.min, self.max);
        }
        if (self.step - prev.step).abs() > f64::EPSILON {
            SliderWidget::set_step(&mut element, self.step);
        }
        if (self.value - prev.value).abs() > f64::EPSILON {
            SliderWidget::set_value(&mut element, self.value);
        }
        if self.disabled != prev.disabled {
            SliderWidget::set_disabled(&mut element, self.disabled);
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
        match message.take_message::<SliderChanged>() {
            Some(changed) => MessageResult::Action((self.callback)(app_state, changed.0)),
            None => MessageResult::Stale,
        }
    }
}
