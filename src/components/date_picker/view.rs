//! Xilem view layer for the date picker component.
//!
//! `DatePicker<State, Action>` is the builder; `.render(&theme)` produces a
//! `DatePickerView`. Clicking the trigger opens or closes a calendar panel;
//! selecting a date fires `on_changed` with `Some(date)`, and clearing fires it
//! with `None`.
//!
//! At `build`, the view looks for the nearest [`crate::overlay_scope`]'s
//! [`OverlayPortal`] in the xilem `Environment`: if present, the calendar body
//! is registered as a [`CalendarBodyView`] with
//! [`PortalPlacement::BareTrigger`] (the scope's own view mounts it in the
//! always-on-top `PortalSlot`) and the picker hosts only the trigger;
//! otherwise the calendar is built in-tree under the picker's
//! `AnchoredOverlay`, exactly as before.
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
use crate::components::date_picker::calendar_body::{CalendarBodyAction, CalendarBodyWidget};
use crate::components::date_picker::calendar_grid::CalendarGridHandle;
use crate::components::date_picker::widget::{
    DatePickerAction, DatePickerHandle, ThemedDatePickerWidget,
};
use crate::overlay::SurfaceStyle;
use crate::overlay_portal::{OverlayPortal, PortalContentView, PortalPlacement, portal_from_env};

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

/// Where this date picker's calendar body is bound: the nearest scope's portal
/// (registered by key; the scope's view mounts/rebuilds it), or in-tree under
/// our own `ThemedDatePickerWidget` (fallback, handled entirely by the widget).
enum PickerBinding<State: 'static, Action: 'static> {
    Portal {
        portal: OverlayPortal<State, Action>,
        key: u64,
        handle: DatePickerHandle,
        grid_handle: CalendarGridHandle,
    },
    InTree,
}

/// View state for `DatePickerView`: just the calendar binding (see
/// [`PickerBinding`]) — the trigger has no nested view-layer children of its
/// own (it's built directly by `ThemedDatePickerWidget`).
#[doc(hidden)]
pub struct DatePickerViewState<State: 'static, Action: 'static> {
    binding: PickerBinding<State, Action>,
}

impl<State, Action> View<State, Action, ViewCtx> for DatePickerView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ThemedDatePickerWidget>;
    type ViewState = DatePickerViewState<State, Action>;

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx);
        if let Some(portal) = portal {
            let handle = DatePickerHandle::new();
            let grid_handle = CalendarGridHandle::new();
            let body_view = CalendarBodyView {
                selected: self.selected,
                min_date: self.min_date,
                max_date: self.max_date,
                picker_handle: handle.clone(),
                grid_handle: grid_handle.clone(),
                on_changed: self.on_changed.clone(),
                theme: self.theme,
            };
            let content: Arc<PortalContentView<State, Action>> = Arc::new(body_view);
            let key = portal.register(
                content,
                &self.theme,
                PortalPlacement::BareTrigger,
                SurfaceStyle::Popover,
            );
            let widget = ThemedDatePickerWidget::new_portal(
                self.selected,
                self.placeholder.clone(),
                self.date_format,
                self.min_date,
                self.max_date,
                self.disabled,
                self.cleanable,
                &self.theme,
                handle.clone(),
                portal.scope().clone(),
                key,
                grid_handle.clone(),
            )
            .with_open_state(self.open.unwrap_or(false), self.open.is_some());
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                DatePickerViewState {
                    binding: PickerBinding::Portal {
                        portal,
                        key,
                        handle,
                        grid_handle,
                    },
                },
            )
        } else {
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
            (
                element,
                DatePickerViewState {
                    binding: PickerBinding::InTree,
                },
            )
        }
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
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
        if let Some(open) = self.open
            && prev.open != Some(open)
        {
            ThemedDatePickerWidget::set_open(&mut element, open);
        }

        if let PickerBinding::Portal {
            portal,
            key,
            handle,
            grid_handle,
        } = &mut view_state.binding
        {
            // Content rebuild happens when the scope's view diffs the
            // registry (after our subtree's rebuild returns) — we only
            // refresh the registered view value here, mirroring
            // `DropdownButtonView::rebuild`'s `MenuBinding::Portal` arm.
            if self.theme != prev.theme
                || self.selected != prev.selected
                || self.min_date != prev.min_date
                || self.max_date != prev.max_date
            {
                let body_view = CalendarBodyView {
                    selected: self.selected,
                    min_date: self.min_date,
                    max_date: self.max_date,
                    picker_handle: handle.clone(),
                    grid_handle: grid_handle.clone(),
                    on_changed: self.on_changed.clone(),
                    theme: self.theme,
                };
                let content: Arc<PortalContentView<State, Action>> = Arc::new(body_view);
                portal.update(
                    *key,
                    content,
                    &self.theme,
                    PortalPlacement::BareTrigger,
                    SurfaceStyle::Popover,
                );
            }
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        if let PickerBinding::Portal { portal, key, .. } = &mut view_state.binding {
            portal.deregister(*key);
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
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

/// The content view registered with the scope's [`OverlayPortal`] for a
/// portal-mode date picker — wraps [`CalendarBodyWidget`] and, on day
/// selection, both calls the `on_changed` callback (producing `Action`) and
/// notifies the owning [`ThemedDatePickerWidget`] (via [`DatePickerHandle`])
/// to close the picker and update the trigger display. The calendar body is not
/// a descendant of the picker in this mode, so normal action bubbling never
/// reaches `ThemedDatePickerWidget::on_action`.
pub(crate) struct CalendarBodyView<State, Action> {
    selected: Option<NaiveDate>,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
    picker_handle: DatePickerHandle,
    grid_handle: CalendarGridHandle,
    on_changed: OnChangedFn<State, Action>,
    theme: Theme,
}

impl<State, Action> ViewMarker for CalendarBodyView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for CalendarBodyView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<CalendarBodyWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = CalendarBodyWidget::new(
            self.selected,
            self.min_date,
            self.max_date,
            &self.theme,
            self.grid_handle.clone(),
        );
        // `CalendarBodyWidget` itself must be the widget registered here —
        // it's the one that calls `ctx.submit_action::<CalendarBodyAction>`.
        // Wrapping it in an extra `Passthrough` first (as a previous version
        // of this code did) registers the *wrapper's* id instead, so the
        // action's origin never matches and masonry drops it with "unknown
        // widget". The `Pod<Passthrough>` erasure `PortalContentView` needs
        // is applied automatically at the `AnyView` boundary — see
        // `dropdown_button`'s `MenuContentView`, which uses this same
        // direct-registration pattern.
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
            CalendarBodyWidget::set_theme(&mut element, &self.theme);
        }
        if self.selected != prev.selected {
            CalendarBodyWidget::set_selected(&mut element, self.selected);
        }
        if self.min_date != prev.min_date || self.max_date != prev.max_date {
            CalendarBodyWidget::set_min_max(&mut element, self.min_date, self.max_date);
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
        match message.take_message::<CalendarBodyAction>() {
            Some(boxed) => {
                let CalendarBodyAction::DateSelected(date) = *boxed;
                // Back-channel to the picker widget to close it and update the
                // trigger text — the calendar body is not a descendant of the
                // picker in portal mode, so normal action bubbling won't reach it.
                if let Some(picker_id) = self.picker_handle.widget_id() {
                    element.ctx.mutate_later(picker_id, move |mut w| {
                        let mut picker = w.downcast::<ThemedDatePickerWidget>();
                        ThemedDatePickerWidget::close_for_selection(&mut picker, date);
                    });
                }
                MessageResult::Action((self.on_changed)(app_state, Some(date)))
            }
            None => MessageResult::Stale,
        }
    }
}
