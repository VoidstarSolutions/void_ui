//! Xilem views for the slider component.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { volume: f64, lo: f64, hi: f64 }
//! # let state = State { volume: 50.0, lo: 20.0, hi: 80.0 };
//! use void_ui::components::{slider, range_slider};
//!
//! // Single thumb:
//! slider(state.volume, |s: &mut State, v| s.volume = v)
//!     .range(0.0, 100.0)
//!     .step(1.0)
//!     .render(&theme);
//!
//! // Dual thumb — independent low/high bounds that cannot cross:
//! range_slider(state.lo, state.hi, |s: &mut State, lo, hi| {
//!     s.lo = lo;
//!     s.hi = hi;
//! })
//! .range(0.0, 100.0)
//! .render(&theme)
//! # ;
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

/// Generates the `range`/`step`/`disabled`/`orientation` builder methods
/// shared by [`Slider`] and [`RangeSlider`] — both flavors mutate the same
/// [`SliderConfig`] field and only differ in their value payload.
macro_rules! slider_config_methods {
    () => {
        /// Sets the inclusive value range. `min` must be less than `max`.
        pub fn range(mut self, min: f64, max: f64) -> Self {
            self.config.min = min;
            self.config.max = max;
            self
        }

        /// Sets the snap increment. Values are rounded to the nearest multiple
        /// of `step` away from `min`. Pass `0.0` (the default) for continuous
        /// values.
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
        /// A vertical slider travels bottom-to-top: `min` at the bottom, `max`
        /// at the top, matching the conventional orientation of vertical
        /// sliders.
        pub fn orientation(mut self, orientation: Orientation) -> Self {
            self.config.orientation = orientation;
            self
        }
    };
}

// --- MARK: VALUE-SHAPE ABSTRACTION

/// The value payload a slider flavor reads from and writes to its widget —
/// `f64` for [`Slider`], `(f64, f64)` for [`RangeSlider`]. Encapsulates every
/// place the two flavors' [`SliderValue`] shape would otherwise leak into a
/// shared view: widget construction, change detection, and message
/// extraction.
trait SliderPayload: Copy + Send + Sync + 'static {
    /// Builds a [`SliderWidget`] in this payload's mode (single or range).
    fn build_widget(self, theme: &Theme, config: &SliderConfig) -> SliderWidget;

    /// Whether `self` differs from `prev` enough to push to the widget —
    /// epsilon-based, matching [`SliderWidget`]'s own float comparisons.
    fn changed_from(self, prev: Self) -> bool;

    /// Wraps `self` in the [`SliderValue`] variant this payload represents.
    fn into_value(self) -> SliderValue;

    /// Extracts a payload of this shape from a [`SliderChanged`] value, or
    /// `None` if the widget reported the other flavor's shape.
    fn from_value(value: SliderValue) -> Option<Self>;
}

impl SliderPayload for f64 {
    fn build_widget(self, theme: &Theme, config: &SliderConfig) -> SliderWidget {
        SliderWidget::new_single(
            theme,
            self,
            config.min,
            config.max,
            config.step,
            config.disabled,
            config.orientation,
        )
    }

    fn changed_from(self, prev: Self) -> bool {
        (self - prev).abs() > f64::EPSILON
    }

    fn into_value(self) -> SliderValue {
        SliderValue::Single(self)
    }

    fn from_value(value: SliderValue) -> Option<Self> {
        match value {
            SliderValue::Single(v) => Some(v),
            SliderValue::Range(..) => None,
        }
    }
}

impl SliderPayload for (f64, f64) {
    fn build_widget(self, theme: &Theme, config: &SliderConfig) -> SliderWidget {
        SliderWidget::new_range(
            theme,
            self.0,
            self.1,
            config.min,
            config.max,
            config.step,
            config.disabled,
            config.orientation,
        )
    }

    fn changed_from(self, prev: Self) -> bool {
        (self.0 - prev.0).abs() > f64::EPSILON || (self.1 - prev.1).abs() > f64::EPSILON
    }

    fn into_value(self) -> SliderValue {
        SliderValue::Range(self.0, self.1)
    }

    fn from_value(value: SliderValue) -> Option<Self> {
        match value {
            SliderValue::Range(low, high) => Some((low, high)),
            SliderValue::Single(_) => None,
        }
    }
}

/// Dispatches a [`SliderChanged`] payload to a host callback.
///
/// Implemented for the single-value `Fn(&mut State, f64) -> Action` callbacks
/// taken by [`slider`] and the two-value `Fn(&mut State, f64, f64) -> Action`
/// callbacks taken by [`range_slider`], so [`SliderViewImpl`] can stay generic
/// over both shapes without forcing either flavor's callback into the other's
/// arity.
trait SliderCallback<P, State, Action>: Send + Sync + 'static {
    fn call(&self, state: &mut State, payload: P) -> Action;
}

impl<F, State, Action> SliderCallback<f64, State, Action> for F
where
    F: Fn(&mut State, f64) -> Action + Send + Sync + 'static,
{
    fn call(&self, state: &mut State, payload: f64) -> Action {
        self(state, payload)
    }
}

impl<F, State, Action> SliderCallback<(f64, f64), State, Action> for F
where
    F: Fn(&mut State, f64, f64) -> Action + Send + Sync + 'static,
{
    fn call(&self, state: &mut State, payload: (f64, f64)) -> Action {
        self(state, payload.0, payload.1)
    }
}

// --- MARK: SHARED VIEW

/// The materialized [`View`] backing both slider flavors.
///
/// Aliased as [`SliderView`] (`P = f64`, single thumb) and
/// [`RangeSliderView`] (`P = (f64, f64)`, dual thumb) — the [`SliderPayload`]
/// and [`SliderCallback`] abstractions absorb every place those two shapes
/// differ, so this `View` impl is written once for both. Built only through
/// [`Slider::render`] / [`RangeSlider::render`]; not constructed directly by
/// callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SliderViewImpl<P, F, State, Action> {
    value: P,
    config: SliderConfig,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<P, F, State, Action> ViewMarker for SliderViewImpl<P, F, State, Action> {}

impl<P, F, State, Action> View<State, Action, ViewCtx> for SliderViewImpl<P, F, State, Action>
where
    P: SliderPayload,
    F: SliderCallback<P, State, Action>,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<SliderWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = self.value.build_widget(&self.theme, &self.config);
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
        if (self.config.min - prev.config.min).abs() > f64::EPSILON
            || (self.config.max - prev.config.max).abs() > f64::EPSILON
        {
            SliderWidget::set_range(&mut element, self.config.min, self.config.max);
        }
        if (self.config.step - prev.config.step).abs() > f64::EPSILON {
            SliderWidget::set_step(&mut element, self.config.step);
        }
        if self.config.disabled != prev.config.disabled {
            SliderWidget::set_disabled(&mut element, self.config.disabled);
        }
        if self.config.orientation != prev.config.orientation {
            SliderWidget::set_orientation(&mut element, self.config.orientation);
        }
        if self.value.changed_from(prev.value) {
            SliderWidget::set_value(&mut element, self.value.into_value());
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
            Some(changed) => match P::from_value(changed.0) {
                Some(payload) => MessageResult::Action(self.callback.call(app_state, payload)),
                None => MessageResult::Stale,
            },
            None => MessageResult::Stale,
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
    slider_config_methods!();

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> SliderView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, f64) -> Action + Send + Sync + 'static,
    {
        SliderViewImpl {
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
pub type SliderView<F, State, Action> = SliderViewImpl<f64, F, State, Action>;

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
    slider_config_methods!();

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> RangeSliderView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, f64, f64) -> Action + Send + Sync + 'static,
    {
        SliderViewImpl {
            value: (self.low, self.high),
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
pub type RangeSliderView<F, State, Action> = SliderViewImpl<(f64, f64), F, State, Action>;
