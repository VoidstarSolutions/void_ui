//! Masonry widget for the date-picker calendar body.
//!
//! [`CalendarBodyWidget`] composes:
//! - A 4-button header row (prev | month | year | next)
//! - An optional weekday-label row (Day mode only)
//! - A [`CalendarGridWidget`] that fills the remainder
//!
//! Navigation (prev/next, mode switching) is handled entirely inside
//! `on_action` by mutating child pods via `ctx.mutate_child_later`.
//! The widget bubbles a single [`CalendarBodyAction::DateSelected`] when
//! the user picks a day cell.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
    MutateCtx, NewWidget, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty,
    TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length};
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label};

use crate::Theme;
use crate::components::button::ButtonVariant;
use crate::components::button::widget::ThemedButton;
use crate::components::date_picker::calendar_grid::{
    CalendarGridAction, CalendarGridHandle, CalendarGridWidget, CellDatum, cell_side,
};
use crate::components::date_picker::calendar_math::{
    WEEKDAY_LABELS, add_months, day_grid, day_grid_index_of, day_in_range, last_day_of_month,
    month_in_range, month_label, year_in_range, year_page_of, years_in_page,
};
use crate::components::date_picker::widget::DatePickerHandle;
use crate::components::item_list::index_f64;
use chrono::{Datelike, Duration, NaiveDate};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Which picker panel is currently displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewMode {
    /// 6×7 day grid for one month.
    Day,
    /// 3×4 month grid for one year.
    Month,
    /// 4×5 year grid for one page of 20 years.
    Year,
}

/// Action emitted by [`CalendarBodyWidget`] when the user selects a date.
#[derive(Debug)]
pub(crate) enum CalendarBodyAction {
    /// The user confirmed a specific calendar date.
    DateSelected(NaiveDate),
}

/// Keys routed from [`super::widget::ThemedDatePickerWidget::on_text_event`] to
/// the calendar body via `mutate_later` / `mutate_child_later`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CalendarNavKey {
    /// Arrow up — move focus up one row.
    Up,
    /// Arrow down — move focus down one row.
    Down,
    /// Arrow left — move focus left one cell.
    Left,
    /// Arrow right — move focus right one cell.
    Right,
    /// Home — jump to the first non-disabled cell.
    Home,
    /// End — jump to the last non-disabled cell.
    End,
    /// Page Up — step back one month (Day view only).
    PrevMonth,
    /// Page Down — step forward one month (Day view only).
    NextMonth,
    /// Shift+Page Up — step back one year (Day view only).
    PrevYear,
    /// Shift+Page Down — step forward one year (Day view only).
    NextYear,
    /// Enter — activate the focused cell.
    Activate,
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal nav state
// ─────────────────────────────────────────────────────────────────────────────

struct CalendarNavState {
    view_mode: ViewMode,
    current_year: i32,
    current_month: u32, // 1-indexed
    year_page: i32,
    day_grid: [NaiveDate; 42],
}

// ─────────────────────────────────────────────────────────────────────────────
// Widget struct
// ─────────────────────────────────────────────────────────────────────────────

/// Masonry widget for the calendar body: header nav + optional weekday row +
/// cell grid.
pub(crate) struct CalendarBodyWidget {
    nav: CalendarNavState,
    selected: Option<NaiveDate>,
    today: NaiveDate,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
    header_prev: WidgetPod<ThemedButton>,
    header_month: WidgetPod<ThemedButton>,
    header_year: WidgetPod<ThemedButton>,
    header_next: WidgetPod<ThemedButton>,
    weekday_row: [WidgetPod<Label>; 7],
    grid: WidgetPod<CalendarGridWidget>,
    theme: Theme,
    /// Keyboard-roving focus index within the current grid, driven by
    /// [`CalendarNavKey`] events routed from the parent date-picker widget.
    /// `None` when the keyboard focus is not inside the grid.
    focused_index: Option<usize>,
    /// Handle to the owning [`super::widget::ThemedDatePickerWidget`]'s
    /// widget id, used to return real masonry focus to the trigger when the
    /// calendar closes (Enter-select, Escape). `None` for `InTree` hosting
    /// and in isolated-widget tests — see [`Self::new_with_trigger_handle`].
    trigger_handle: Option<DatePickerHandle>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Cell builders
// ─────────────────────────────────────────────────────────────────────────────

/// Abbreviated month names used in the month picker grid.
const MONTH_ABBREVS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Builds 42 [`CellDatum`] entries for the day-view grid.
fn build_day_cells(
    day_grid_dates: &[NaiveDate; 42],
    current_month: u32,
    selected: Option<NaiveDate>,
    today: NaiveDate,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
) -> Vec<CellDatum> {
    day_grid_dates
        .iter()
        .map(|&date| CellDatum {
            label: ArcStr::from(date.day().to_string()),
            selected: selected == Some(date),
            today: date == today,
            disabled: !day_in_range(date, min_date, max_date),
            muted: date.month() != current_month,
        })
        .collect()
}

/// Builds 12 [`CellDatum`] entries for the month-view grid.
fn build_month_cells(
    current_year: i32,
    selected: Option<NaiveDate>,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
) -> Vec<CellDatum> {
    (1u32..=12)
        .map(|month| CellDatum {
            label: ArcStr::from(MONTH_ABBREVS[usize::try_from(month - 1).unwrap()]),
            selected: selected.is_some_and(|d| d.month() == month && d.year() == current_year),
            today: false,
            disabled: !month_in_range(current_year, month, min_date, max_date),
            muted: false,
        })
        .collect()
}

/// Builds 20 [`CellDatum`] entries for the year-view grid.
fn build_year_cells(
    year_page: i32,
    selected: Option<NaiveDate>,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
) -> Vec<CellDatum> {
    years_in_page(year_page)
        .into_iter()
        .map(|year| CellDatum {
            label: ArcStr::from(year.to_string()),
            selected: selected.is_some_and(|d| d.year() == year),
            today: false,
            disabled: !year_in_range(year, min_date, max_date),
            muted: false,
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Label strings for header buttons
// ─────────────────────────────────────────────────────────────────────────────

fn year_range_label(year_page: i32) -> String {
    let years = years_in_page(year_page);
    format!("{}–{}", years[0], years[19])
}

fn header_month_label(nav: &CalendarNavState) -> ArcStr {
    // In Month mode the button is disabled but retains its label so the user
    // can still read which month they are browsing.
    ArcStr::from(month_label(nav.current_month))
}

fn header_year_label(nav: &CalendarNavState) -> ArcStr {
    match nav.view_mode {
        ViewMode::Year => ArcStr::from(year_range_label(nav.year_page)),
        _ => ArcStr::from(nav.current_year.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Builds a ghost-variant `ThemedButton` wrapping a plain `Label`.
fn build_header_button(label: ArcStr, theme: &Theme) -> NewWidget<ThemedButton> {
    let lbl = Label::new(label)
        .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
        .prepare();
    let mut lbl = lbl.erased();
    lbl.properties.insert(ContentColor::new(theme.palette.text));
    NewWidget::new(ThemedButton::new(lbl, theme).with_variant(ButtonVariant::Ghost))
}

/// Builds a weekday header label.
fn build_weekday_label(text: &str, theme: &Theme) -> WidgetPod<Label> {
    let mut lbl = Label::new(ArcStr::from(text))
        .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
        .prepare();
    lbl.properties
        .insert(ContentColor::new(theme.palette.text_muted));
    lbl.to_pod()
}

/// Returns how many columns the grid uses for the given view mode.
fn cols_for(mode: ViewMode) -> usize {
    match mode {
        ViewMode::Day => 7,
        ViewMode::Month | ViewMode::Year => 4,
    }
}

/// Walks forward or backward from `start` by `step` until a non-disabled cell
/// is found (returning its index) or the grid boundary is reached (returning
/// `None`).
///
/// Uses checked arithmetic to avoid clippy cast lints; `step` may be negative.
fn nav_step_skip(start: usize, step: isize, len: usize, disabled: &[bool]) -> Option<usize> {
    let len_isize = isize::try_from(len).unwrap_or(isize::MAX);
    let mut idx: isize = isize::try_from(start).ok()?.checked_add(step)?;
    while idx >= 0 && idx < len_isize {
        let i = usize::try_from(idx).unwrap_or(usize::MAX);
        if !disabled.get(i).copied().unwrap_or(true) {
            return Some(i);
        }
        idx = idx.checked_add(step)?;
    }
    None
}

/// Like [`nav_step_skip`] but starts one full `step` before index 0 — used for
/// the initial Down keypress (no prior focus), which should land on the first
/// reachable cell in the first row.
fn nav_step_skip_from_before_start(step: isize, len: usize, disabled: &[bool]) -> Option<usize> {
    // Starting idx is `0 - step`, i.e. one stride before the grid begins.
    let start_isize: isize = isize::try_from(0usize).ok()?.checked_sub(step)?;
    let len_isize = isize::try_from(len).unwrap_or(isize::MAX);
    let mut idx: isize = start_isize.checked_add(step)?;
    while idx >= 0 && idx < len_isize {
        let i = usize::try_from(idx).unwrap_or(usize::MAX);
        if !disabled.get(i).copied().unwrap_or(true) {
            return Some(i);
        }
        idx = idx.checked_add(step)?;
    }
    None
}

/// Returns `start` if it's non-disabled, otherwise walks `direction`
/// (`+1` or `-1`) from `start` via [`nav_step_skip`]. `None` if no
/// non-disabled cell exists in `[0, len)` from `start` onward in that
/// direction, or if `start` is already out of bounds.
fn scan_for_enabled(
    start: usize,
    direction: isize,
    len: usize,
    disabled: &[bool],
) -> Option<usize> {
    if start < len && !disabled[start] {
        return Some(start);
    }
    nav_step_skip(start, direction, len, disabled)
}

/// Day-of-month step for arrow-key cross-month rollover. `None` for keys
/// with no rollover semantics (Home/End, and anything else routed to
/// `handle_nav_move`).
fn day_offset_for(key: CalendarNavKey) -> Option<i64> {
    match key {
        CalendarNavKey::Left => Some(-1),
        CalendarNavKey::Right => Some(1),
        CalendarNavKey::Up => Some(-7),
        CalendarNavKey::Down => Some(7),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared ctx abstraction (WidgetMut / ActionCtx)
// ─────────────────────────────────────────────────────────────────────────────

/// The subset of masonry context capabilities the day/month/year activation
/// logic needs, shimmed over the ctx types' inherent methods (they share no
/// trait upstream) so that one implementation of each activation function can
/// run from both the keyboard path (`MutateCtx`, via
/// `WidgetMut::handle_nav_key`) and the pointer path (`ActionCtx`, via
/// `on_action`). Mirrors `overlay::binding::PortalCtx`.
trait CalendarCtx {
    /// `ctx.mutate_later(target, f)`.
    fn queue_mutate(
        &mut self,
        target: WidgetId,
        f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static,
    );
    /// `ctx.submit_action::<CalendarBodyAction>(CalendarBodyAction::DateSelected(date))`.
    fn submit_date_selected(&mut self, date: NaiveDate);
    /// `ctx.request_layout()`.
    fn request_layout(&mut self);
    /// Transfers real focus to `target`, if this context supports it and a
    /// target is given. A documented no-op for `MutateCtx` — masonry only
    /// exposes `set_focus` on `ActionCtx`/`EventCtx` (confirmed by reading
    /// `masonry_core/src/core/contexts.rs`), and `MutateCtx`-driven paths
    /// (`InTree`'s keyboard-Enter path in particular) never need this
    /// anyway, since `InTree`'s trigger never loses real focus to begin
    /// with.
    fn set_focus_to(&mut self, target: Option<WidgetId>);
}

impl CalendarCtx for MutateCtx<'_> {
    fn queue_mutate(
        &mut self,
        target: WidgetId,
        f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static,
    ) {
        self.mutate_later(target, f);
    }

    fn submit_date_selected(&mut self, date: NaiveDate) {
        self.submit_action::<CalendarBodyAction>(CalendarBodyAction::DateSelected(date));
    }

    fn request_layout(&mut self) {
        self.request_layout();
    }

    fn set_focus_to(&mut self, _target: Option<WidgetId>) {
        // MutateCtx has no set_focus in this masonry version — see the
        // CalendarCtx::set_focus_to doc comment.
    }
}

macro_rules! impl_calendar_ctx_focusable {
    ($($ctx:ty),+ $(,)?) => {$(
        impl CalendarCtx for $ctx {
            fn queue_mutate(
                &mut self,
                target: WidgetId,
                f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static,
            ) {
                self.mutate_later(target, f);
            }

            fn submit_date_selected(&mut self, date: NaiveDate) {
                self.submit_action::<CalendarBodyAction>(CalendarBodyAction::DateSelected(date));
            }

            fn request_layout(&mut self) {
                self.request_layout();
            }

            fn set_focus_to(&mut self, target: Option<WidgetId>) {
                if let Some(id) = target {
                    self.set_focus(id);
                }
            }
        }
    )+};
}

impl_calendar_ctx_focusable!(ActionCtx<'_>, EventCtx<'_>);

// ─────────────────────────────────────────────────────────────────────────────
// Constructor
// ─────────────────────────────────────────────────────────────────────────────

impl CalendarBodyWidget {
    /// Creates a new calendar body widget anchored to `today`, optionally
    /// seeded from `selected`.
    #[must_use]
    pub(crate) fn new(
        selected: Option<NaiveDate>,
        min_date: Option<NaiveDate>,
        max_date: Option<NaiveDate>,
        theme: &Theme,
        grid_handle: CalendarGridHandle,
    ) -> Self {
        let today = chrono::Local::now().date_naive();
        let anchor = selected.unwrap_or(today);
        let current_year = anchor.year();
        let current_month = anchor.month();
        let year_page = year_page_of(current_year);
        let dg = day_grid(current_year, current_month);

        let nav = CalendarNavState {
            view_mode: ViewMode::Day,
            current_year,
            current_month,
            year_page,
            day_grid: dg,
        };

        let header_prev = build_header_button(ArcStr::from("‹"), theme).to_pod();
        let header_month =
            build_header_button(ArcStr::from(month_label(current_month)), theme).to_pod();
        let header_year =
            build_header_button(ArcStr::from(current_year.to_string()), theme).to_pod();
        let header_next = build_header_button(ArcStr::from("›"), theme).to_pod();

        let weekday_row = WEEKDAY_LABELS.map(|label| build_weekday_label(label, theme));

        let grid_data = build_day_cells(&dg, current_month, selected, today, min_date, max_date);
        let grid = WidgetPod::new(CalendarGridWidget::new(grid_data, 7, theme, grid_handle));

        Self {
            nav,
            selected,
            today,
            min_date,
            max_date,
            header_prev,
            header_month,
            header_year,
            header_next,
            weekday_row,
            grid,
            theme: *theme,
            focused_index: None,
            trigger_handle: None,
        }
    }

    /// Like [`Self::new`], but also wires a trigger-pointing handle so
    /// closing (Enter-select, Escape) can return real focus to the trigger —
    /// used by Portal mode only (`InTree`'s trigger never loses focus in the
    /// first place, so it has nothing to return to).
    #[must_use]
    pub(crate) fn new_with_trigger_handle(
        selected: Option<NaiveDate>,
        min_date: Option<NaiveDate>,
        max_date: Option<NaiveDate>,
        theme: &Theme,
        grid_handle: CalendarGridHandle,
        trigger_handle: Option<DatePickerHandle>,
    ) -> Self {
        let mut this = Self::new(selected, min_date, max_date, theme, grid_handle);
        this.trigger_handle = trigger_handle;
        this
    }

    /// Outer inset around the panel's header/weekday/grid content — this
    /// widget paints its own chrome (see [`Self::paint`]) and, per the
    /// `BareTrigger` overlay-content contract, must bake in its own padding
    /// rather than relying on `OverlaySurface` (which only wraps `Trigger`
    /// placements).
    fn pad(&self) -> f64 {
        f64::from(self.theme.density.pad)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WidgetMut setters
// ─────────────────────────────────────────────────────────────────────────────

impl CalendarBodyWidget {
    /// Updates the selected date and refreshes the grid.
    pub(crate) fn set_selected(this: &mut WidgetMut<'_, Self>, selected: Option<NaiveDate>) {
        this.widget.selected = selected;
        this.widget.push_grid_and_headers(&mut this.ctx, None);
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Updates the min/max bounds and refreshes the grid.
    pub(crate) fn set_min_max(
        this: &mut WidgetMut<'_, Self>,
        min: Option<NaiveDate>,
        max: Option<NaiveDate>,
    ) {
        this.widget.min_date = min;
        this.widget.max_date = max;
        this.widget.push_grid_and_headers(&mut this.ctx, None);
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Re-applies a new theme to the widget and all children.
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;

        // Update all 4 header buttons (each accessed separately to satisfy the
        // borrow checker — no unsafe needed).
        {
            let mut btn = this.ctx.get_mut(&mut this.widget.header_prev);
            ThemedButton::set_theme(&mut btn, theme);
        }
        {
            let mut btn = this.ctx.get_mut(&mut this.widget.header_month);
            ThemedButton::set_theme(&mut btn, theme);
        }
        {
            let mut btn = this.ctx.get_mut(&mut this.widget.header_year);
            ThemedButton::set_theme(&mut btn, theme);
        }
        {
            let mut btn = this.ctx.get_mut(&mut this.widget.header_next);
            ThemedButton::set_theme(&mut btn, theme);
        }

        // Update weekday labels — each accessed by index.
        let text_muted = theme.palette.text_muted;
        let ui_font_size = theme.density.ui_font_size;
        for i in 0..7 {
            let mut lbl = this.ctx.get_mut(&mut this.widget.weekday_row[i]);
            lbl.insert_prop(ContentColor::new(text_muted));
            Label::insert_style(&mut lbl, StyleProperty::FontSize(ui_font_size));
        }

        // Update grid (drop the mut ref before requesting layout).
        {
            let mut grid = this.ctx.get_mut(&mut this.widget.grid);
            CalendarGridWidget::set_theme(&mut grid, theme);
        }

        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Routes a keyboard navigation key to the calendar grid.
    ///
    /// Arrow keys move the keyboard-roving focus one cell in the requested
    /// direction, skipping disabled cells and crossing month boundaries when
    /// reaching the grid edge (Day view only). Home/End jump to the first/last
    /// non-disabled cell in the current month. PageUp/PageDown (Day view only)
    /// step back/forward one month; Shift+PageUp/PageDown step back/forward one
    /// year. Enter activates the focused cell (same logic as clicking it).
    pub(crate) fn handle_nav_key(this: &mut WidgetMut<'_, Self>, key: CalendarNavKey) {
        Self::handle_nav_key_impl(this.widget, &mut this.ctx, key);
    }

    /// Shared body of [`Self::handle_nav_key`] (the `WidgetMut`-based entry
    /// point used by `InTree` mode's `mutate_later` dispatch and the whole
    /// existing keyboard-nav test suite) and `on_text_event` (the new
    /// `EventCtx`-based entry point used once the grid holds real focus in
    /// Portal mode). Generic over [`CalendarCtx`] so both callers work
    /// unchanged.
    fn handle_nav_key_impl(widget: &mut Self, ctx: &mut impl CalendarCtx, key: CalendarNavKey) {
        match key {
            CalendarNavKey::PrevMonth
            | CalendarNavKey::NextMonth
            | CalendarNavKey::PrevYear
            | CalendarNavKey::NextYear => Self::handle_nav_page(widget, ctx, key),
            CalendarNavKey::Activate => {
                let disabled = Self::build_disabled_flags(widget);
                Self::handle_nav_activate(widget, ctx, &disabled);
            }
            _ => {
                let disabled = Self::build_disabled_flags(widget);
                Self::handle_nav_move(widget, ctx, key, &disabled);
            }
        }
    }

    /// Builds a flat `disabled` flag array from the current nav state, mirroring
    /// how [`build_day_cells`] / [`build_month_cells`] / [`build_year_cells`]
    /// compute the disabled flag, without borrowing into the child grid widget.
    fn build_disabled_flags(widget: &Self) -> Vec<bool> {
        match widget.nav.view_mode {
            ViewMode::Day => {
                let dg = widget.nav.day_grid;
                let min_date = widget.min_date;
                let max_date = widget.max_date;
                dg.iter()
                    .map(|&date| !day_in_range(date, min_date, max_date))
                    .collect()
            }
            ViewMode::Month => {
                let year = widget.nav.current_year;
                let min_date = widget.min_date;
                let max_date = widget.max_date;
                (1u32..=12)
                    .map(|month| !month_in_range(year, month, min_date, max_date))
                    .collect()
            }
            ViewMode::Year => {
                let year_page = widget.nav.year_page;
                let min_date = widget.min_date;
                let max_date = widget.max_date;
                years_in_page(year_page)
                    .into_iter()
                    .map(|year| !year_in_range(year, min_date, max_date))
                    .collect()
            }
        }
    }

    /// Handles arrow/Home/End movement within the grid.
    fn handle_nav_move(
        widget: &mut Self,
        ctx: &mut impl CalendarCtx,
        key: CalendarNavKey,
        disabled: &[bool],
    ) {
        let (cols, len) = match widget.nav.view_mode {
            ViewMode::Day => (7usize, 42usize),
            ViewMode::Month => (4, 12),
            ViewMode::Year => (4, 20),
        };
        let current = widget.focused_index;

        let new_index = match key {
            CalendarNavKey::Left => {
                let from = current.unwrap_or(0);
                nav_step_skip(from, -1, len, disabled)
            }
            CalendarNavKey::Right => {
                let from = current.unwrap_or_else(|| len.saturating_sub(1));
                nav_step_skip(from, 1, len, disabled)
            }
            CalendarNavKey::Up => current.and_then(|from| {
                let step = isize::try_from(cols).ok()?.checked_neg()?;
                nav_step_skip(from, step, len, disabled)
            }),
            CalendarNavKey::Down => {
                if let Some(from) = current {
                    let step = isize::try_from(cols).unwrap_or(isize::MAX);
                    nav_step_skip(from, step, len, disabled)
                } else {
                    // Initial Down: land on the first non-disabled cell by
                    // starting one row before index 0.
                    let step = isize::try_from(cols).unwrap_or(isize::MAX);
                    nav_step_skip_from_before_start(step, len, disabled)
                }
            }
            CalendarNavKey::Home => disabled.iter().position(|&d| !d),
            CalendarNavKey::End => disabled.iter().rposition(|&d| !d),
            // Activate/Page* are handled elsewhere, never reach here.
            CalendarNavKey::Activate
            | CalendarNavKey::PrevMonth
            | CalendarNavKey::NextMonth
            | CalendarNavKey::PrevYear
            | CalendarNavKey::NextYear => return,
        };

        if new_index.is_none()
            && widget.nav.view_mode == ViewMode::Day
            && let Some(offset) = day_offset_for(key)
        {
            Self::try_cross_month(widget, ctx, offset);
            // Rollover was attempted (this arrow key has day-offset
            // semantics) but if no reachable cell exists in the adjacent
            // month — e.g. it's entirely outside [min_date, max_date] —
            // focus stays where it was. Either the rollover succeeded and
            // try_cross_month landed focus in the adjacent month, or it
            // failed silently and this return keeps focus in place.
            return;
        }

        let new_index = new_index.filter(|&i| i < len);
        if new_index == widget.focused_index {
            return;
        }
        widget.focused_index = new_index;
        let grid_id = widget.grid.id();
        ctx.queue_mutate(grid_id, move |mut w| {
            let mut g = w.downcast::<CalendarGridWidget>();
            CalendarGridWidget::set_focused_index(&mut g, new_index);
        });
    }

    /// Rebuilds the Day-view grid for `(y, m)`, locates `target_date` in it,
    /// and — if the target month is in range and an enabled cell is
    /// reachable via [`scan_for_enabled`] from `direction` — commits the
    /// new grid/nav state and lands focus there. Returns `true` on success;
    /// `false` leaves `this` untouched (out-of-range month, target date not
    /// found in the rebuilt grid, or no enabled cell reachable in
    /// `direction`).
    fn jump_to_date(
        widget: &mut Self,
        ctx: &mut impl CalendarCtx,
        y: i32,
        m: u32,
        target_date: NaiveDate,
        direction: isize,
    ) -> bool {
        if !month_in_range(y, m, widget.min_date, widget.max_date) {
            return false;
        }
        let new_grid = day_grid(y, m);
        let Some(target_index) = day_grid_index_of(&new_grid, target_date) else {
            return false;
        };
        let new_disabled: Vec<bool> = new_grid
            .iter()
            .map(|&d| !day_in_range(d, widget.min_date, widget.max_date))
            .collect();
        let Some(landing) =
            scan_for_enabled(target_index, direction, new_grid.len(), &new_disabled)
        else {
            return false;
        };

        widget.nav.current_year = y;
        widget.nav.current_month = m;
        widget.nav.year_page = year_page_of(y);
        widget.nav.day_grid = new_grid;
        widget.focused_index = Some(landing);
        widget.push_grid_and_headers(ctx, Some(landing));
        true
    }

    /// Attempts to roll Day-view keyboard focus across a month boundary
    /// when [`Self::handle_nav_move`] finds no reachable cell in the
    /// current grid. `day_offset` is the arrow key's day step (see
    /// [`day_offset_for`]): its sign picks both the adjacent month
    /// (backward for negative, forward for positive) and the fallback scan
    /// direction. Returns `true` if focus moved.
    fn try_cross_month(widget: &mut Self, ctx: &mut impl CalendarCtx, day_offset: i64) -> bool {
        let Some(current) = widget.focused_index else {
            return false;
        };
        let Some(&reference) = widget.nav.day_grid.get(current) else {
            return false;
        };
        let target_date = reference + Duration::days(day_offset);
        let month_delta = if day_offset < 0 { -1 } else { 1 };
        let direction: isize = if day_offset < 0 { -1 } else { 1 };
        let (y, m) = add_months(
            widget.nav.current_year,
            widget.nav.current_month,
            month_delta,
        );
        Self::jump_to_date(widget, ctx, y, m, target_date, direction)
    }

    /// Handles PageUp/PageDown (month step) and Shift+PageUp/PageDown (year
    /// step). Day view only — no-op in Month/Year view, matching the
    /// existing guard on [`Self::handle_prev`]/[`Self::handle_next`].
    fn handle_nav_page(widget: &mut Self, ctx: &mut impl CalendarCtx, key: CalendarNavKey) {
        if widget.nav.view_mode != ViewMode::Day {
            return;
        }

        let reference = widget
            .focused_index
            .and_then(|i| widget.nav.day_grid.get(i).copied())
            .or(widget.selected)
            .unwrap_or(widget.today);

        let month_delta = match key {
            CalendarNavKey::PrevMonth => -1,
            CalendarNavKey::NextMonth => 1,
            CalendarNavKey::PrevYear => -12,
            CalendarNavKey::NextYear => 12,
            _ => return,
        };
        let direction: isize = if month_delta < 0 { -1 } else { 1 };
        let (y, m) = add_months(
            widget.nav.current_year,
            widget.nav.current_month,
            month_delta,
        );
        let target_day = reference.day().min(last_day_of_month(y, m).day());
        let target_date = NaiveDate::from_ymd_opt(y, m, target_day).unwrap();
        Self::jump_to_date(widget, ctx, y, m, target_date, direction);
    }

    /// Handles Enter — activates the currently focused cell.
    fn handle_nav_activate(widget: &mut Self, ctx: &mut impl CalendarCtx, disabled: &[bool]) {
        let Some(i) = widget.focused_index else {
            return;
        };
        if disabled.get(i).copied().unwrap_or(true) {
            return;
        }
        // Same activation logic as on_action/CalendarGridAction::CellActivated
        // (see `Self::activate_day` et al.), generic over `impl CalendarCtx` so
        // it runs unchanged from `MutateCtx`/`ActionCtx`/`EventCtx` alike.
        match widget.nav.view_mode {
            ViewMode::Day => Self::activate_day(widget, ctx, i),
            ViewMode::Month => Self::activate_month(widget, ctx, i),
            ViewMode::Year => Self::activate_year(widget, ctx, i),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Widget trait
// ─────────────────────────────────────────────────────────────────────────────

impl Widget for CalendarBodyWidget {
    type Action = CalendarBodyAction;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.header_prev);
        ctx.register_child(&mut self.header_month);
        ctx.register_child(&mut self.header_year);
        ctx.register_child(&mut self.header_next);
        for pod in &mut self.weekday_row {
            ctx.register_child(pod);
        }
        ctx.register_child(&mut self.grid);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::ChildFocusChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    /// Handles keyboard navigation once real masonry focus has moved into
    /// the calendar (Portal mode, after the Tab-interception in
    /// `ThemedDatePickerWidget::on_text_event` hands focus to the grid — see
    /// `CalendarGridHandle`). This is an additional path, not a replacement:
    /// `ThemedDatePickerWidget::on_text_event`'s own arrow/Home/End/Enter/
    /// PageUp/PageDown/Escape `mutate_later` dispatch (`InTree` mode, and
    /// Portal mode before any Tab press) is unchanged and unaffected.
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        if matches!(&key.key, Key::Named(NamedKey::Escape)) {
            if let Some(trigger_id) = self
                .trigger_handle
                .as_ref()
                .and_then(DatePickerHandle::widget_id)
            {
                ctx.set_focus_to(Some(trigger_id));
                ctx.mutate_later(trigger_id, |mut w| {
                    let mut picker = w.downcast::<crate::components::date_picker::widget::ThemedDatePickerWidget>();
                    crate::components::date_picker::widget::ThemedDatePickerWidget::close_from_calendar(&mut picker);
                });
            }
            ctx.set_handled();
            return;
        }
        let nav_key = match &key.key {
            Key::Named(NamedKey::ArrowUp) => Some(CalendarNavKey::Up),
            Key::Named(NamedKey::ArrowDown) => Some(CalendarNavKey::Down),
            Key::Named(NamedKey::ArrowLeft) => Some(CalendarNavKey::Left),
            Key::Named(NamedKey::ArrowRight) => Some(CalendarNavKey::Right),
            Key::Named(NamedKey::Home) => Some(CalendarNavKey::Home),
            Key::Named(NamedKey::End) => Some(CalendarNavKey::End),
            Key::Named(NamedKey::Enter) => Some(CalendarNavKey::Activate),
            Key::Named(NamedKey::PageUp) if key.modifiers.shift() => Some(CalendarNavKey::PrevYear),
            Key::Named(NamedKey::PageUp) => Some(CalendarNavKey::PrevMonth),
            Key::Named(NamedKey::PageDown) if key.modifiers.shift() => {
                Some(CalendarNavKey::NextYear)
            }
            Key::Named(NamedKey::PageDown) => Some(CalendarNavKey::NextMonth),
            _ => None,
        };
        let Some(nav) = nav_key else {
            return;
        };
        Self::handle_nav_key_impl(self, ctx, nav);
        ctx.set_handled();
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if let Some(&CalendarGridAction::CellActivated(i)) =
            action.downcast_ref::<CalendarGridAction>()
        {
            self.handle_cell_activated(ctx, i);
            return;
        }

        if action.downcast_ref::<ButtonPress>().is_some() {
            self.handle_header_press(ctx, source);
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let cell_px = cell_side(&self.theme);
        let cols = cols_for(self.nav.view_mode);
        let grid_w = index_f64(cols) * cell_px;
        let header_h = f64::from(self.theme.density.row_height);
        let weekday_h = if self.nav.view_mode == ViewMode::Day {
            cell_px
        } else {
            0.0
        };
        let n: usize = match self.nav.view_mode {
            ViewMode::Day => 42,
            ViewMode::Month => 12,
            ViewMode::Year => 20,
        };
        let rows = n.div_ceil(cols);
        let grid_h = index_f64(rows) * cell_px;
        let pad = self.pad();
        match axis {
            Axis::Horizontal => {
                // Sum of each header button's own natural (unclipped) label
                // width — buttons shrink to their content instead of all
                // sharing the width of the widest one (which used to stretch
                // "‹"/"›" out to match a label like "2000–2019").
                let cross = Some(Length::px(header_h));
                let mut header_w: f64 = 0.0;
                for pod in [
                    &mut self.header_prev,
                    &mut self.header_month,
                    &mut self.header_year,
                    &mut self.header_next,
                ] {
                    header_w += ctx
                        .compute_length(
                            pod,
                            len_req.into(),
                            LayoutSize::maybe(Axis::Vertical, cross),
                            Axis::Horizontal,
                            cross,
                        )
                        .get();
                }
                Length::px(grid_w.max(header_w) + 2.0 * pad)
            }
            Axis::Vertical => Length::px(header_h + weekday_h + grid_h + 2.0 * pad),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let cell_px = cell_side(&self.theme);
        let cols = cols_for(self.nav.view_mode);
        let grid_w = index_f64(cols) * cell_px;
        let header_h = f64::from(self.theme.density.row_height);
        let pad = self.pad();
        let content_w = (size.width - 2.0 * pad).max(0.0);

        // Header row: each button sized to its own natural width, spread
        // across the content width with equal gaps (prev/next pinned to the
        // edges, month/year evenly spaced between them).
        let cross = Some(Length::px(header_h));
        let mut btn_widths = [0.0_f64; 4];
        for (w, pod) in btn_widths.iter_mut().zip([
            &mut self.header_prev,
            &mut self.header_month,
            &mut self.header_year,
            &mut self.header_next,
        ]) {
            *w = ctx
                .compute_length(
                    pod,
                    LenDef::MinContent,
                    LayoutSize::maybe(Axis::Vertical, cross),
                    Axis::Horizontal,
                    cross,
                )
                .get();
        }
        let header_w: f64 = btn_widths.iter().sum();
        let gap = ((content_w - header_w) / 3.0).max(0.0);

        let mut x = pad;
        let header_pods = [
            &mut self.header_prev,
            &mut self.header_month,
            &mut self.header_year,
            &mut self.header_next,
        ];
        for (pod, w) in header_pods.into_iter().zip(btn_widths) {
            let btn_size = Size::new(w, header_h);
            ctx.run_layout(pod, btn_size);
            ctx.place_child(pod, Point::new(x, pad));
            x += w + gap;
        }

        // Weekday row: laid out in Day mode; stashed (excluded from layout
        // and paint) otherwise. A zero-height box alone doesn't stop a
        // `Label` from painting its text at natural size, so a collapsed
        // box isn't enough to hide these — they must be stashed.
        let day_mode = self.nav.view_mode == ViewMode::Day;
        let weekday_h = if day_mode { cell_px } else { 0.0 };
        for (i, pod) in self.weekday_row.iter_mut().enumerate() {
            ctx.set_stashed(pod, !day_mode);
            if !day_mode {
                continue;
            }
            let cell_size = Size::new(cell_px, cell_px);
            ctx.run_layout(pod, cell_size);
            ctx.place_child(
                pod,
                Point::new(pad + index_f64(i) * cell_px, pad + header_h),
            );
        }

        // Grid: fills the remaining space, stretched to the same content
        // width as the header row (see `CalendarGridWidget::layout`) so a
        // wide header label never leaves dead space to the grid's right.
        let grid_y = pad + header_h + weekday_h;
        let grid_h = (size.height - grid_y - pad).max(0.0);
        let grid_size = Size::new(content_w.max(grid_w), grid_h);
        ctx.run_layout(&mut self.grid, grid_size);
        ctx.place_child(&mut self.grid, Point::new(pad, grid_y));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box().size();
        let p = &self.theme.palette;
        let radius = f64::from(self.theme.radius.large);
        let rrect = RoundedRect::from_origin_size(Point::ORIGIN, size, radius);
        painter.fill(rrect, p.surface).draw();
        painter.stroke(rrect, &Stroke::new(1.0), p.border).draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        let mut ids = vec![
            self.header_prev.id(),
            self.header_month.id(),
            self.header_year.id(),
            self.header_next.id(),
        ];
        for pod in &self.weekday_row {
            ids.push(pod.id());
        }
        ids.push(self.grid.id());
        ChildrenIds::from_slice(&ids)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Navigation helpers (called from on_action)
// ─────────────────────────────────────────────────────────────────────────────

impl CalendarBodyWidget {
    /// Dispatches a grid-cell activation to the appropriate mode handler.
    ///
    /// Delegates to the same `Self::activate_day`/`activate_month`/
    /// `activate_year` used by [`Self::handle_nav_activate`] (the Enter-key
    /// path via `WidgetMut`) — see [`CalendarCtx`]. Only the event-specific
    /// bookkeeping (`set_handled`, paint/layout requests) differs per path
    /// and stays here.
    fn handle_cell_activated(&mut self, ctx: &mut ActionCtx<'_>, i: usize) {
        match self.nav.view_mode {
            ViewMode::Day => {
                Self::activate_day(self, ctx, i);
                ctx.request_paint_only();
            }
            ViewMode::Month => Self::activate_month(self, ctx, i),
            ViewMode::Year => Self::activate_year(self, ctx, i),
        }
        ctx.set_handled();
    }

    fn handle_header_press(&mut self, ctx: &mut ActionCtx<'_>, source: WidgetId) {
        if source == self.header_prev.id() {
            self.handle_prev(ctx);
        } else if source == self.header_next.id() {
            self.handle_next(ctx);
        } else if source == self.header_month.id() {
            self.handle_toggle_month(ctx);
        } else if source == self.header_year.id() {
            self.handle_toggle_year(ctx);
        }
        ctx.set_handled();
        ctx.request_paint_only();
    }

    fn handle_prev(&mut self, ctx: &mut ActionCtx<'_>) {
        if self.nav.view_mode == ViewMode::Month {
            return;
        }
        match self.nav.view_mode {
            ViewMode::Day => {
                // Step back one month.
                let (y, m) = add_months(self.nav.current_year, self.nav.current_month, -1);
                self.nav.current_year = y;
                self.nav.current_month = m;
                self.nav.year_page = year_page_of(y);
                self.nav.day_grid = day_grid(y, m);
            }
            ViewMode::Year => {
                // Step back one page of years.
                self.nav.year_page -= 1;
            }
            ViewMode::Month => unreachable!("guarded above"),
        }
        self.focused_index = None;
        self.push_grid_and_headers(ctx, None);
    }

    fn handle_next(&mut self, ctx: &mut ActionCtx<'_>) {
        if self.nav.view_mode == ViewMode::Month {
            return;
        }
        match self.nav.view_mode {
            ViewMode::Day => {
                let (y, m) = add_months(self.nav.current_year, self.nav.current_month, 1);
                self.nav.current_year = y;
                self.nav.current_month = m;
                self.nav.year_page = year_page_of(y);
                self.nav.day_grid = day_grid(y, m);
            }
            ViewMode::Year => {
                self.nav.year_page += 1;
            }
            ViewMode::Month => unreachable!("guarded above"),
        }
        self.focused_index = None;
        self.push_grid_and_headers(ctx, None);
    }

    fn handle_toggle_month(&mut self, ctx: &mut ActionCtx<'_>) {
        // Day → Month, anything else → Day.
        self.nav.view_mode = if self.nav.view_mode == ViewMode::Day {
            ViewMode::Month
        } else {
            ViewMode::Day
        };
        self.focused_index = None;
        self.push_grid_and_headers(ctx, None);
        ctx.request_layout();
    }

    fn handle_toggle_year(&mut self, ctx: &mut ActionCtx<'_>) {
        self.nav.view_mode = if self.nav.view_mode == ViewMode::Year {
            ViewMode::Day
        } else {
            ViewMode::Year
        };
        self.focused_index = None;
        self.push_grid_and_headers(ctx, None);
        ctx.request_layout();
    }

    /// Pushes updated grid data and header labels to children.
    ///
    /// Generic over [`CalendarCtx`] so it runs from both the `ActionCtx`
    /// click path and the `MutateCtx` keyboard path (`Self::set_selected`,
    /// `Self::set_min_max`).
    fn push_grid_and_headers(&self, ctx: &mut impl CalendarCtx, new_focused_index: Option<usize>) {
        let nav = &self.nav;
        let selected = self.selected;
        let today = self.today;
        let min_date = self.min_date;
        let max_date = self.max_date;

        let (new_data, new_cols): (Vec<CellDatum>, usize) = match nav.view_mode {
            ViewMode::Day => (
                build_day_cells(
                    &nav.day_grid,
                    nav.current_month,
                    selected,
                    today,
                    min_date,
                    max_date,
                ),
                7,
            ),
            ViewMode::Month => (
                build_month_cells(nav.current_year, selected, min_date, max_date),
                4,
            ),
            ViewMode::Year => (
                build_year_cells(nav.year_page, selected, min_date, max_date),
                4,
            ),
        };

        let grid_id = self.grid.id();
        ctx.queue_mutate(grid_id, move |mut w| {
            let mut g = w.downcast::<CalendarGridWidget>();
            CalendarGridWidget::set_data(&mut g, new_data, new_cols);
            CalendarGridWidget::set_focused_index(&mut g, new_focused_index);
        });

        // Header month label + disabled state.
        let month_lbl = header_month_label(nav);
        let month_disabled = nav.view_mode == ViewMode::Month;
        let month_btn_id = self.header_month.id();
        ctx.queue_mutate(month_btn_id, move |mut w| {
            let mut btn = w.downcast::<ThemedButton>();
            ThemedButton::set_disabled(&mut btn, month_disabled);
            let mut child = ThemedButton::child_mut(&mut btn);
            let mut lbl = child.downcast::<Label>();
            Label::set_text(&mut lbl, month_lbl);
        });

        // Header year label.
        let year_lbl = header_year_label(nav);
        let year_btn_id = self.header_year.id();
        ctx.queue_mutate(year_btn_id, move |mut w| {
            let mut btn = w.downcast::<ThemedButton>();
            let mut child = ThemedButton::child_mut(&mut btn);
            let mut lbl = child.downcast::<Label>();
            Label::set_text(&mut lbl, year_lbl);
        });

        // Prev/next disabled in Month mode.
        let nav_disabled = nav.view_mode == ViewMode::Month;
        let prev_id = self.header_prev.id();
        let next_id = self.header_next.id();
        ctx.queue_mutate(prev_id, move |mut w| {
            let mut btn = w.downcast::<ThemedButton>();
            ThemedButton::set_disabled(&mut btn, nav_disabled);
        });
        ctx.queue_mutate(next_id, move |mut w| {
            let mut btn = w.downcast::<ThemedButton>();
            ThemedButton::set_disabled(&mut btn, nav_disabled);
        });
    }

    /// Activates day cell `i`: selects the date, clears keyboard-roving
    /// focus, and refreshes the grid. Shared by the click path
    /// (`Self::handle_cell_activated`) and the Enter-key path
    /// (`Self::handle_nav_activate`).
    fn activate_day(widget: &mut Self, ctx: &mut impl CalendarCtx, i: usize) {
        let Some(&date) = widget.nav.day_grid.get(i) else {
            return;
        };
        if !day_in_range(date, widget.min_date, widget.max_date) {
            return;
        }
        widget.selected = Some(date);
        widget.focused_index = None;
        ctx.submit_date_selected(date);
        ctx.set_focus_to(
            widget
                .trigger_handle
                .as_ref()
                .and_then(DatePickerHandle::widget_id),
        );
        let new_data = build_day_cells(
            &widget.nav.day_grid,
            widget.nav.current_month,
            widget.selected,
            widget.today,
            widget.min_date,
            widget.max_date,
        );
        let grid_id = widget.grid.id();
        ctx.queue_mutate(grid_id, move |mut w| {
            let mut g = w.downcast::<CalendarGridWidget>();
            CalendarGridWidget::set_data(&mut g, new_data, 7);
            CalendarGridWidget::set_focused_index(&mut g, None);
        });
    }

    /// Activates month cell `i`: switches to that month in Day mode and
    /// refreshes the grid/headers. Shared by the click path and the
    /// Enter-key path (see [`Self::activate_day`]).
    fn activate_month(widget: &mut Self, ctx: &mut impl CalendarCtx, i: usize) {
        // Cell index 0..11 → month 1..12.
        let month = u32::try_from(i + 1).unwrap_or(1).clamp(1, 12);
        widget.nav.current_month = month;
        widget.nav.view_mode = ViewMode::Day;
        widget.nav.day_grid = day_grid(widget.nav.current_year, month);
        widget.focused_index = None;
        widget.push_grid_and_headers(ctx, None);
        ctx.request_layout();
    }

    /// Activates year cell `i`: switches to that year in Day mode and
    /// refreshes the grid/headers. Shared by the click path and the
    /// Enter-key path (see [`Self::activate_day`]).
    fn activate_year(widget: &mut Self, ctx: &mut impl CalendarCtx, i: usize) {
        let years = years_in_page(widget.nav.year_page);
        let Some(&year) = years.get(i) else {
            return;
        };
        widget.nav.current_year = year;
        widget.nav.view_mode = ViewMode::Day;
        widget.nav.day_grid = day_grid(year, widget.nav.current_month);
        widget.focused_index = None;
        widget.push_grid_and_headers(ctx, None);
        ctx.request_layout();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::NewWidget;
    use masonry::core::TextEvent;
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::kurbo::Point;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use xilem::view::PointerButton;

    #[test]
    fn clicking_a_day_cell_selects_it_without_panicking() {
        let theme = Theme::default();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let widget =
            CalendarBodyWidget::new(Some(date), None, None, &theme, CalendarGridHandle::new());
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        let dg = day_grid(2024, 6);
        let idx = dg.iter().position(|&d| d == date).unwrap();
        let cell_px = cell_side(&theme);
        let header_h = f64::from(theme.density.row_height);
        let weekday_h = cell_px; // Day mode
        let pad = f64::from(theme.density.pad);
        // Grid columns stretch to fill the panel's full content width (which
        // may be wider than `7 * cell_px` if the header's buttons need more
        // room), so derive the actual column width from the rendered widget
        // rather than assuming it's exactly `cell_px`.
        let total_w = h.root_widget().ctx().border_box().size().width;
        let col_w = ((total_w - 2.0 * pad) / 7.0).max(cell_px);
        let grid_y = pad + header_h + weekday_h;
        let col = idx % 7;
        let row = idx / 7;
        let x = pad + (index_f64(col) + 0.5) * col_w;
        let y = grid_y + (index_f64(row) + 0.5) * cell_px;

        h.mouse_move(Point::new(x, y));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        let action = h.pop_action::<CalendarBodyAction>();
        assert!(
            matches!(&action, Some((CalendarBodyAction::DateSelected(d), _)) if *d == date),
            "expected DateSelected({date:?}), got {action:?}"
        );
    }

    // ── on_text_event (Portal-mode key handling once real focus is inside) ────

    #[test]
    fn calendar_body_on_text_event_handles_arrow_right_directly() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            crate::components::date_picker::calendar_grid::CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        let root_id = h.root_id();

        h.edit_root_widget(|wm| {
            wm.widget.focused_index = Some(0);
        });
        h.focus_on(Some(root_id));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));

        h.edit_root_widget(|wm| {
            let idx = wm.widget.focused_index.expect("focus should have moved");
            assert_ne!(idx, 0, "ArrowRight should move the roving focus index");
        });
    }

    #[test]
    fn calendar_body_on_text_event_handles_enter_directly() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            crate::components::date_picker::calendar_grid::CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        let root_id = h.root_id();

        h.edit_root_widget(|wm| {
            wm.widget.focused_index = Some(0);
        });
        h.focus_on(Some(root_id));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));

        let action = h.pop_action::<CalendarBodyAction>();
        assert!(
            matches!(&action, Some((CalendarBodyAction::DateSelected(_), _))),
            "Enter should activate the focused cell and emit DateSelected, got {action:?}"
        );
    }

    // ── scan_for_enabled ──────────────────────────────────────────────────────

    #[test]
    fn scan_for_enabled_returns_start_when_already_enabled() {
        let disabled = [false, true, true];
        assert_eq!(scan_for_enabled(0, 1, 3, &disabled), Some(0));
    }

    #[test]
    fn scan_for_enabled_skips_forward_past_disabled() {
        let disabled = [true, true, false];
        assert_eq!(scan_for_enabled(0, 1, 3, &disabled), Some(2));
    }

    #[test]
    fn scan_for_enabled_skips_backward_past_disabled() {
        let disabled = [false, true, true];
        assert_eq!(scan_for_enabled(2, -1, 3, &disabled), Some(0));
    }

    #[test]
    fn scan_for_enabled_none_when_all_disabled() {
        let disabled = [true, true, true];
        assert_eq!(scan_for_enabled(0, 1, 3, &disabled), None);
    }

    #[test]
    fn scan_for_enabled_out_of_bounds_start_falls_through_to_skip() {
        // start == len: not enabled (out of bounds), falls through to
        // nav_step_skip which also immediately fails (idx == len is already
        // out of the `idx < len_isize` loop condition).
        let disabled = [false, false];
        assert_eq!(scan_for_enabled(2, 1, 2, &disabled), None);
    }

    // ── Cross-month arrow rollover ────────────────────────────────────────────

    #[test]
    fn rollover_left_crosses_into_previous_month() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.focused_index = Some(0); // grid[0] == 2024-02-25
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::Left);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 2);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 2, 24).unwrap()
            );
        });
    }

    #[test]
    fn rollover_up_crosses_into_previous_month() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.focused_index = Some(0); // grid[0] == 2024-02-25
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::Up);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 2);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 2, 18).unwrap()
            );
        });
    }

    #[test]
    fn rollover_right_crosses_into_next_month() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.focused_index = Some(41); // grid[41] == 2024-04-06
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::Right);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 4);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 4, 7).unwrap()
            );
        });
    }

    #[test]
    fn rollover_down_crosses_into_next_month() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.focused_index = Some(41); // grid[41] == 2024-04-06
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::Down);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 4);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 4, 13).unwrap()
            );
        });
    }

    #[test]
    fn rollover_dead_end_when_adjacent_month_fully_blocked() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let max_date = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            Some(max_date),
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.focused_index = Some(35); // grid[35] == 2024-03-31, last row
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::Down);
        });

        h.edit_root_widget(|wm| {
            // April is entirely past max_date, so the whole month is blocked
            // and the Down press must be a no-op.
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 3);
            assert_eq!(wm.widget.focused_index, Some(35));
        });
    }

    #[test]
    fn rollover_skips_disabled_landing_cell_to_nearest_enabled() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let min_date = NaiveDate::from_ymd_opt(2024, 4, 9).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            Some(min_date),
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.focused_index = Some(35); // grid[35] == 2024-03-31, last row
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::Down);
        });

        h.edit_root_widget(|wm| {
            // Target is 2024-04-07 (31 + 7 days), but April 1-8 are disabled
            // by min_date; the fallback scan (forward, same direction as
            // Down) must land on the nearest enabled cell: 2024-04-09.
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 4);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 4, 9).unwrap()
            );
        });
    }

    // ── PageUp/PageDown month & year jumps ────────────────────────────────────

    #[test]
    fn page_down_steps_month_preserving_day() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::NextMonth);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 4);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 4, 15).unwrap()
            );
        });
    }

    #[test]
    fn page_up_steps_month_preserving_day() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::PrevMonth);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 2);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 2, 15).unwrap()
            );
        });
    }

    #[test]
    fn page_down_clamps_day_to_shorter_leap_february() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::NextMonth);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_month, 2);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
            );
        });
    }

    #[test]
    fn page_down_clamps_day_non_leap_year() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2023, 1, 31).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::NextMonth);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_month, 2);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()
            );
        });
    }

    #[test]
    fn shift_page_down_steps_year_clamping_feb29_into_non_leap_year() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::NextYear);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2025);
            assert_eq!(wm.widget.nav.current_month, 2);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2025, 2, 28).unwrap()
            );
        });
    }

    #[test]
    fn shift_page_up_steps_year_preserving_month_day() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::PrevYear);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.current_year, 2023);
            assert_eq!(wm.widget.nav.current_month, 6);
            let idx = wm
                .widget
                .focused_index
                .expect("focus should land on a cell");
            assert_eq!(
                wm.widget.nav.day_grid[idx],
                NaiveDate::from_ymd_opt(2023, 6, 15).unwrap()
            );
        });
    }

    #[test]
    fn page_keys_are_noop_in_month_view() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.nav.view_mode = ViewMode::Month;
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::NextMonth);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.view_mode, ViewMode::Month);
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.nav.current_month, 3);
            assert_eq!(wm.widget.focused_index, None);
        });
    }

    #[test]
    fn page_keys_are_noop_in_year_view() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.nav.view_mode = ViewMode::Year;
            CalendarBodyWidget::handle_nav_key(&mut wm, CalendarNavKey::PrevYear);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.view_mode, ViewMode::Year);
            assert_eq!(wm.widget.nav.current_year, 2024);
            assert_eq!(wm.widget.focused_index, None);
        });
    }

    // ── Header button keyboard activation ─────────────────────────────────────

    #[test]
    fn header_month_button_enter_key_switches_to_month_view() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let month_btn_id = widget.header_month.id();
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.focus_on(Some(month_btn_id));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.view_mode, ViewMode::Month);
        });
    }

    #[test]
    fn header_year_button_enter_key_switches_to_year_view() {
        let theme = Theme::default();
        let selected = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let widget = CalendarBodyWidget::new(
            Some(selected),
            None,
            None,
            &theme,
            CalendarGridHandle::new(),
        );
        let year_btn_id = widget.header_year.id();
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.focus_on(Some(year_btn_id));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.nav.view_mode, ViewMode::Year);
        });
    }
}
