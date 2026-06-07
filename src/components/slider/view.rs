//! Xilem views for the slider component.
//!
//! ```ignore
//! use void_ui::components::{slider, range_slider};
//!
//! // Single thumb:
//! slider(state.volume, |s: &mut State, v| s.volume = v)
//!     .range(0.0, 100.0)
//!     .step(1.0)
//!     .render(&theme)
//!
//! // Dual thumb — independent low/high bounds that cannot cross:
//! range_slider(state.lo, state.hi, |s: &mut State, lo, hi| {
//!     s.lo = lo;
//!     s.hi = hi;
//! })
//! .range(0.0, 100.0)
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::SliderWidget;
use super::{SliderChanged, SliderValue};
use crate::{Orientation, Theme};

/// Shared range/step/disabled/orientation configuration for both slider flavors.
#[derive(Clone, Copy, PartialEq)]
struct SliderConfig {
    min: f64,
    max: f64,
    step: f64,
    disabled: bool,
    orientation: Orientation,
}

impl Default for SliderConfig {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            step: 0.0,
            disabled: false,
            orientation: Orientation::Horizontal,
        }
    }
}

// --- MARK: SINGLE-THUMB SLIDER

/// Builder for an interactive single-thumb themed slider.
///
/// Created with [`slider`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Slider does nothing until rendered with .render(&theme)"]
pub struct Slider<F> {
    value: f64,
    config: SliderConfig,
    callback: F,
}

/// Create a new single-thumb slider with the given value and change callback.
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
        config: SliderConfig::default(),
        callback,
    }
}

impl<F> Slider<F> {
    /// Sets the inclusive value range. `min` must be less than `max`.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.config.min = min;
        self.config.max = max;
        self
    }

    /// Sets the snap increment. Values are rounded to the nearest multiple of
    /// `step` away from `min`. Pass `0.0` (the default) for continuous values.
    pub fn step(mut self, step: f64) -> Self {
        self.config.step = step;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.config.disabled = on;
        self
    }

    /// Sets the slider's layout axis. Defaults to [`Orientation::Horizontal`].
    ///
    /// A vertical slider travels bottom-to-top: `min` at the bottom, `max` at
    /// the top, matching the conventional orientation of vertical sliders.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.config.orientation = orientation;
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
            config: self.config,
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
    config: SliderConfig,
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
        let widget = SliderWidget::new_single(
            &self.theme,
            self.value,
            self.config.min,
            self.config.max,
            self.config.step,
            self.config.disabled,
            self.config.orientation,
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
        rebuild_shared(
            &self.theme,
            &prev.theme,
            &self.config,
            &prev.config,
            &mut element,
        );
        if (self.value - prev.value).abs() > f64::EPSILON {
            SliderWidget::set_value(&mut element, SliderValue::Single(self.value));
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
            Some(changed) => match changed.0 {
                SliderValue::Single(value) => {
                    MessageResult::Action((self.callback)(app_state, value))
                }
                SliderValue::Range(..) => MessageResult::Stale,
            },
            None => MessageResult::Stale,
        }
    }
}

// --- MARK: DUAL-THUMB RANGE SLIDER

/// Builder for an interactive dual-thumb themed range slider.
///
/// Created with [`range_slider`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "RangeSlider does nothing until rendered with .render(&theme)"]
pub struct RangeSlider<F> {
    low: f64,
    high: f64,
    config: SliderConfig,
    callback: F,
}

/// Create a new dual-thumb range slider with the given bounds and change callback.
///
/// `low` and `high` are host-controlled — the widget never mutates them, and
/// trusts that `low <= high`. The callback receives the updated `(low, high)`
/// pair (with thumbs clamped so they cannot cross) on drag, click-to-jump,
/// arrow-key nudges of the most recently touched thumb, and accessibility
/// actions; the host stores the pair and passes it back in on the next render.
///
/// Defaults to the range `0.0..=1.0` with continuous (unstepped) values.
pub fn range_slider<F>(low: f64, high: f64, callback: F) -> RangeSlider<F> {
    RangeSlider {
        low,
        high,
        config: SliderConfig::default(),
        callback,
    }
}

impl<F> RangeSlider<F> {
    /// Sets the inclusive value range. `min` must be less than `max`.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.config.min = min;
        self.config.max = max;
        self
    }

    /// Sets the snap increment. Values are rounded to the nearest multiple of
    /// `step` away from `min`. Pass `0.0` (the default) for continuous values.
    pub fn step(mut self, step: f64) -> Self {
        self.config.step = step;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.config.disabled = on;
        self
    }

    /// Sets the slider's layout axis. Defaults to [`Orientation::Horizontal`].
    ///
    /// A vertical slider travels bottom-to-top: `min` at the bottom, `max` at
    /// the top, matching the conventional orientation of vertical sliders.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.config.orientation = orientation;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> RangeSliderView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, f64, f64) -> Action + Send + Sync + 'static,
    {
        RangeSliderView {
            low: self.low,
            high: self.high,
            config: self.config,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`RangeSlider`].
///
/// Built only through [`RangeSlider::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct RangeSliderView<F, State, Action> {
    low: f64,
    high: f64,
    config: SliderConfig,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for RangeSliderView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for RangeSliderView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, f64, f64) -> Action + Send + Sync + 'static,
{
    type Element = Pod<SliderWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = SliderWidget::new_range(
            &self.theme,
            self.low,
            self.high,
            self.config.min,
            self.config.max,
            self.config.step,
            self.config.disabled,
            self.config.orientation,
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
        rebuild_shared(
            &self.theme,
            &prev.theme,
            &self.config,
            &prev.config,
            &mut element,
        );
        if (self.low - prev.low).abs() > f64::EPSILON
            || (self.high - prev.high).abs() > f64::EPSILON
        {
            SliderWidget::set_value(&mut element, SliderValue::Range(self.low, self.high));
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
            Some(changed) => match changed.0 {
                SliderValue::Range(low, high) => {
                    MessageResult::Action((self.callback)(app_state, low, high))
                }
                SliderValue::Single(_) => MessageResult::Stale,
            },
            None => MessageResult::Stale,
        }
    }
}

/// Applies theme/range/step/disabled/orientation changes shared by both slider flavors.
fn rebuild_shared(
    theme: &Theme,
    prev_theme: &Theme,
    config: &SliderConfig,
    prev_config: &SliderConfig,
    element: &mut Mut<'_, Pod<SliderWidget>>,
) {
    if theme != prev_theme {
        SliderWidget::set_theme(element, theme);
    }
    if (config.min - prev_config.min).abs() > f64::EPSILON
        || (config.max - prev_config.max).abs() > f64::EPSILON
    {
        SliderWidget::set_range(element, config.min, config.max);
    }
    if (config.step - prev_config.step).abs() > f64::EPSILON {
        SliderWidget::set_step(element, config.step);
    }
    if config.disabled != prev_config.disabled {
        SliderWidget::set_disabled(element, config.disabled);
    }
    if config.orientation != prev_config.orientation {
        SliderWidget::set_orientation(element, config.orientation);
    }
}
