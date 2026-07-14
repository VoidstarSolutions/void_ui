//! Xilem view layer for the date picker component.
//!
//! `DatePicker<State, Action>` is the builder; `.render(&theme)` produces a
//! `DatePickerView`. Clicking the trigger opens or closes a calendar panel;
//! selecting a date fires `on_changed` with `Some(date)`, and clearing fires it
//! with `None`.
//!
//! ```ignore
//! use void_ui::components::date_picker::date_picker;
//! date_picker(self.selected_date, |s: &mut State, date| {
//!     s.selected_date = date;
//!     Action::DateChanged(date)
//! })
//! .placeholder("Pick a date…")
//! .cleanable(true)
//! .render(&theme)
//! ```

use std::marker::PhantomData;
use std::sync::Arc;

use chrono::NaiveDate;
use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::Theme;
use crate::components::date_picker::widget::{DatePickerAction, ThemedDatePickerWidget};

type OnChangedFn<State, Action> =
    Arc<dyn Fn(&mut State, Option<NaiveDate>) -> Action + Send + Sync>;
type OpenChangeFn<State, Action> = Arc<dyn Fn(&mut State, bool) -> Action + Send + Sync>;

/// Builder for a date picker.
///
/// Create with [`date_picker`]; configure via the builder methods.
/// Materialize as a xilem view via [`Self::render`].
#[must_use = "DatePicker does nothing until rendered with .render(&theme)"]
pub struct DatePicker<State, Action> {
    selected: Option<NaiveDate>,
    on_changed: OnChangedFn<State, Action>,
    placeholder: ArcStr,
    date_format: &'static str,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
    disabled: bool,
    cleanable: bool,
    open: Option<bool>,
    on_open_change: Option<OpenChangeFn<State, Action>>,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Construct a date picker bound to `selected`.
///
/// `on_changed` is called when the user picks a date (with `Some(date)`) or
/// clears the value (with `None`).
pub fn date_picker<State, Action, F>(
    selected: Option<NaiveDate>,
    on_changed: F,
) -> DatePicker<State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, Option<NaiveDate>) -> Action + Send + Sync + 'static,
{
    DatePicker {
        selected,
        on_changed: Arc::new(on_changed),
        placeholder: ArcStr::from(""),
        date_format: "%Y-%m-%d",
        min_date: None,
        max_date: None,
        disabled: false,
        cleanable: false,
        open: None,
        on_open_change: None,
        phantom: PhantomData,
    }
}

impl<State, Action> DatePicker<State, Action>
where
    State: 'static,
    Action: 'static,
{
    /// Set the placeholder text shown when no date is selected.
    pub fn placeholder(mut self, text: impl Into<ArcStr>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Set the `strftime`-style format string used to display the selected
    /// date. Defaults to `"%Y-%m-%d"`.
    pub fn date_format(mut self, fmt: &'static str) -> Self {
        self.date_format = fmt;
        self
    }

    /// Set the earliest selectable date (inclusive).
    pub fn min_date(mut self, date: NaiveDate) -> Self {
        self.min_date = Some(date);
        self
    }

    /// Set the latest selectable date (inclusive).
    pub fn max_date(mut self, date: NaiveDate) -> Self {
        self.max_date = Some(date);
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Show a clear button inside the trigger when a date is selected.
    pub fn cleanable(mut self, on: bool) -> Self {
        self.cleanable = on;
        self
    }

    /// Host-control the calendar's open state (controlled mode). See
    /// [`Self::on_open_change`]. Omit for the default uncontrolled behavior.
    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Observe open/close transitions (fires in both controlled and
    /// uncontrolled mode) with the new open state.
    pub fn on_open_change<G>(mut self, f: G) -> Self
    where
        G: Fn(&mut State, bool) -> Action + Send + Sync + 'static,
    {
        self.on_open_change = Some(Arc::new(f));
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> DatePickerView<State, Action> {
        DatePickerView {
            selected: self.selected,
            on_changed: self.on_changed,
            placeholder: self.placeholder,
            date_format: self.date_format,
            min_date: self.min_date,
            max_date: self.max_date,
            disabled: self.disabled,
            cleanable: self.cleanable,
            theme: *theme,
            open: self.open,
            on_open_change: self.on_open_change,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`DatePicker`].
///
/// Not constructed directly; use [`DatePicker::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct DatePickerView<State, Action> {
    selected: Option<NaiveDate>,
    on_changed: OnChangedFn<State, Action>,
    placeholder: ArcStr,
    date_format: &'static str,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
    disabled: bool,
    cleanable: bool,
    theme: Theme,
    open: Option<bool>,
    on_open_change: Option<OpenChangeFn<State, Action>>,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> ViewMarker for DatePickerView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for DatePickerView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ThemedDatePickerWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = ThemedDatePickerWidget::new(
            self.selected,
            self.placeholder.clone(),
            self.date_format,
            self.min_date,
            self.max_date,
            self.disabled,
            self.cleanable,
            &self.theme,
        )
        .with_open_state(self.open.unwrap_or(false), self.open.is_some());
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
            ThemedDatePickerWidget::set_theme(&mut element, &self.theme);
        }
        if self.selected != prev.selected {
            ThemedDatePickerWidget::set_selected(&mut element, self.selected);
        }
        if self.placeholder != prev.placeholder {
            ThemedDatePickerWidget::set_placeholder(&mut element, self.placeholder.clone());
        }
        if self.date_format != prev.date_format {
            ThemedDatePickerWidget::set_date_format(&mut element, self.date_format);
        }
        if self.min_date != prev.min_date {
            ThemedDatePickerWidget::set_min_date(&mut element, self.min_date);
        }
        if self.max_date != prev.max_date {
            ThemedDatePickerWidget::set_max_date(&mut element, self.max_date);
        }
        if self.disabled != prev.disabled {
            ThemedDatePickerWidget::set_disabled(&mut element, self.disabled);
        }
        if self.cleanable != prev.cleanable {
            ThemedDatePickerWidget::set_cleanable(&mut element, self.cleanable);
        }
        if self.open.is_some() != prev.open.is_some() {
            ThemedDatePickerWidget::set_controlled(&mut element, self.open.is_some());
        }
        if let Some(open) = self.open {
            if Some(open) != prev.open {
                ThemedDatePickerWidget::set_open(&mut element, open);
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
        match message.take_message::<DatePickerAction>() {
            Some(action) => match *action {
                DatePickerAction::DateChanged(date) => {
                    MessageResult::Action((self.on_changed)(app_state, date))
                }
                DatePickerAction::OpenChanged(open) => match &self.on_open_change {
                    Some(f) => MessageResult::Action(f(app_state, open)),
                    None => MessageResult::Nop,
                },
            },
            None => MessageResult::Stale,
        }
    }
}
