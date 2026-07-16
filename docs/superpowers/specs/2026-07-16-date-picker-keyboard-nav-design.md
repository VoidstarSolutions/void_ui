# Date picker: cross-month arrow rollover + PageUp/PageDown jumps

## Context

`date_picker` already has substantial keyboard support: Space/Enter opens
the trigger, Escape closes it, and `CalendarNavKey` (arrows, Home, End,
Enter) drives a roving `focused_index` inside `CalendarBodyWidget`'s grid
(`calendar_body.rs`). Two gaps remain, both scoped to Day view:

1. Arrow keys clamp at the 42-cell grid boundary instead of crossing into
   the adjacent month. The existing doc comment on `handle_nav_key` calls
   this a "known v1 limitation," but the actual behavior is worse than a
   clamp: when `nav_step_skip` walks off the grid it returns `None`, and
   `handle_nav_move` sets `focused_index = None` — the focus ring silently
   disappears rather than stopping at the edge.
2. There's no keyboard shortcut to jump a month or a year at a time; the
   only way is repeated arrow presses or a pointer click on the header
   prev/next buttons.

A third candidate (Space/Enter on the month/year header buttons opening
the Month/Year picker) was investigated and found to already work:
`header_month`/`header_year` are `ThemedButton`s, which already handle
Space/Enter via `interaction::keyboard_activate` (`button/widget.rs:537`)
and route through `handle_toggle_month`/`handle_toggle_year`
(`calendar_body.rs:883-904`). `accepts_focus` returns `!self.disabled`, so
Tab should already reach them. No implementation is planned for this item —
only a confirming test.

## Goals

- Arrow key navigation off the Day-view grid edge rolls into the adjacent
  month (recomputing the grid) and lands focus on the correct date, instead
  of dropping focus.
- PageUp/PageDown step the focused date by one month; Shift+PageUp/
  Shift+PageDown step by one year. Day view only — no-op in Month/Year view.
- Both features respect `min_date`/`max_date`: if the exact target date is
  disabled, focus lands on the nearest enabled cell in the same direction
  within the new month; if the whole target month/year is out of range, the
  key press is a no-op (mirrors the existing disabled state on the header
  prev/next buttons).
- Add a widget test confirming Tab + Enter/Space on `header_month`/
  `header_year` already toggles Month/Year view (no source change expected).

## Non-goals

- Month view / Year view do not gain PageUp/PageDown semantics (e.g.
  step-by-year in Month view, step-by-decade in Year view). Out of scope.
- Home/End keep their current in-grid-only behavior; no cross-month
  semantics are defined for them.
- No change to the disabled-date model (still min/max bounds only, per the
  existing "known v1 limitation" note in `calendar_math.rs` about a general
  `Matcher` trait).

## Design

### New `CalendarNavKey` variants

`calendar_body.rs`'s `CalendarNavKey` enum gains four variants:

```rust
PrevMonth,  // PageUp
NextMonth,  // PageDown
PrevYear,   // Shift+PageUp
NextYear,   // Shift+PageDown
```

`widget.rs`'s `on_text_event` decodes them alongside the existing arrow/
Home/End/Enter mapping, using `key.modifiers.shift()` to distinguish the
month and year variants:

```rust
Key::Named(NamedKey::PageUp) if key.modifiers.shift() => Some(CalendarNavKey::PrevYear),
Key::Named(NamedKey::PageUp) => Some(CalendarNavKey::PrevMonth),
Key::Named(NamedKey::PageDown) if key.modifiers.shift() => Some(CalendarNavKey::NextYear),
Key::Named(NamedKey::PageDown) => Some(CalendarNavKey::NextMonth),
```

These route through the same `mutate_later` → `CalendarBodyWidget::
handle_nav_key` path as arrows (both `Hosting::InTree` and `Hosting::
Portal` branches), so no new plumbing is needed there.

### Shared helpers

**`day_grid_index_of(grid: &[NaiveDate; 42], date: NaiveDate) -> Option<usize>`**
(new, `calendar_body.rs` or `calendar_math.rs`) — linear scan for a date's
index in a day grid. Used by both features below.

**`last_day_of_month(year: i32, month: u32) -> NaiveDate`** (new,
`calendar_math.rs`) — extracted from the "last day of month" calculation
already inlined in `month_in_range` (`add_months(y, m, 1)` then minus one
day). `month_in_range` is refactored to call it. Used to clamp
day-of-month when stepping by month/year.

**`push_grid_and_headers`** (`calendar_body.rs`) gains a
`new_focused_index: Option<usize>` parameter. Existing call sites
(`handle_prev`, `handle_next`, `handle_toggle_month`, `handle_toggle_year`)
pass `None`, preserving today's reset-to-`None` behavior for mouse-driven
navigation. The two new keyboard paths pass `Some(index)`.

### Cross-month arrow rollover

In `handle_nav_move`, when `nav.view_mode == ViewMode::Day` and
`nav_step_skip`/`nav_step_skip_from_before_start` returns `None` (grid
boundary reached):

1. Resolve the reference date: `nav.day_grid[current_index]` if
   `focused_index` is `Some`, else bail (no reference — leave state
   unchanged). This can happen on a first keypress with no prior focus:
   Left/Right/Up all default `current` to an edge index (`0`, `len - 1`, or
   skip the lookup entirely for Up) and immediately under/overflow, so
   there's no cell to derive a reference date from. Down is the only
   direction that seeds a valid landing cell with no prior focus (via
   `nav_step_skip_from_before_start`), so it never reaches this bail path.
2. Compute the target date by offsetting the reference date: -1/+1 day for
   Left/Right, -7/+7 days for Up/Down.
3. Determine the target month via `add_months(current_year, current_month,
   -1)` (Left/Up) or `+1` (Right/Down) — a single month hop always
   suffices, since a ≤7-day step off a 6-week grid can't skip a whole
   month.
4. If the target month is entirely out of `[min_date, max_date]`
   (`!month_in_range(..)`), no-op: leave `focused_index` and the displayed
   month unchanged.
5. Otherwise rebuild `nav.day_grid`/`current_month`/`current_year`/
   `year_page` for the target month, find the target date's index via
   `day_grid_index_of`, and check its disabled flag (via
   `day_in_range`/`min_date`/`max_date`).
6. If disabled, scan forward (Right/Down) or backward (Left/Up) from that
   index within the new grid for the nearest enabled cell (same skip logic
   `nav_step_skip` already implements, reused rather than duplicated).
   If none found, no-op (leave the original month/focus unchanged — don't
   commit to a rebuilt grid with no valid landing cell).
7. On success, call `push_grid_and_headers(.., Some(landing_index))` and
   set `this.widget.focused_index = Some(landing_index)`.

Home/End are unaffected — they keep operating on the currently visible
grid only.

### PageUp/PageDown / Shift+PageUp/PageDown

New `handle_nav_page` function in `calendar_body.rs`, called from
`handle_nav_key` for the four new variants. No-ops immediately if
`nav.view_mode != ViewMode::Day`.

1. Resolve a reference date: focused cell's date if `focused_index` is
   `Some`, else `self.selected`, else `self.today`.
2. Compute the target `(year, month)`:
   - `PrevMonth`/`NextMonth`: `add_months(current_year, current_month, ∓1)`.
   - `PrevYear`/`NextYear`: `add_months(current_year, current_month, ∓12)`
     (equivalent to `(current_year ∓ 1, current_month)`, reusing
     `add_months` rather than a new year-stepping helper).
3. Clamp the reference date's day-of-month into the target month via
   `day.min(last_day_of_month(target_year, target_month).day())`, forming
   the target `NaiveDate`.
4. If the target month is entirely out of range (`!month_in_range(..)`),
   no-op.
5. Rebuild the grid for the target month, locate the target date's index,
   and apply the same disabled-cell fallback scan as the rollover path
   (direction: forward for Next*, backward for Prev*). No-op if no enabled
   cell is found.
6. On success, call `push_grid_and_headers(.., Some(landing_index))` and
   set `focused_index` accordingly.

### Header button keyboard activation (confirmation only)

Add a `TestHarness`-based test that Tab-focuses `header_month` (or
constructs the harness and directly targets its `WidgetId`), sends a
keyboard Enter, and asserts the view mode flips to `Month` (grid data
switches to `build_month_cells` output — e.g. 12 cells instead of 42).
Same for `header_year` → `Year`. If either fails, that's new information
requiring a design update, not a silent skip.

## Testing plan

All new tests live in `calendar_body.rs`'s existing `#[cfg(test)] mod
tests`, using the `TestHarness` + `process_text_event` pattern already
established by `clicking_a_day_cell_selects_it_without_panicking`.

- Rollover at each of the four grid edges (ArrowUp from row 0, ArrowDown
  from the last row, ArrowLeft from column 0, ArrowRight from the last
  column), asserting the new month's label and the correct landing date.
- Rollover dead-end: a `max_date` set such that the adjacent month is
  entirely disabled — asserts focus and displayed month are unchanged.
- Rollover landing-cell-disabled-but-month-partially-enabled: asserts the
  fallback scan lands on the nearest enabled cell.
- PageUp/PageDown from a mid-month date — asserts month changes, day
  preserved.
- PageUp/PageDown day-of-month clamping — e.g. focused on Jan 31, PageDown
  lands on Feb 28/29 (parametrize leap vs. non-leap year).
- Shift+PageUp/Shift+PageDown — asserts year changes, month/day preserved
  (with clamping test for Feb 29 crossing into a non-leap year).
- PageUp/PageDown/Shift variants are no-ops in Month and Year view.
- Header button Tab+Enter confirmation (see above).

## Open questions

None — scope confirmed with the user; Month/Year-view Page-key semantics
and Home/End cross-month behavior are explicitly out of scope (see
Non-goals).
