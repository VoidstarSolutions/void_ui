# Portal-overlay keyboard focus: date_picker and dropdown_button

## Context

A user reported that the date picker's header row (prev/month/year/next)
"should be in line to be tabbed to from the active interface, and they're
not coming up in the right order." Root-caused via systematic debugging
(see prior conversation, not reproduced here): this is not a code-ordering
mistake inside `CalendarBodyWidget` — its own child registration order
(`prev`, `month`, `year`, `next`, then the grid) is already correct and
already matches visual left-to-right order.

The real cause is architectural. `date_picker` (in Portal mode, i.e.
whenever a `overlay_scope` ancestor exists — which is the normal case for
any real app, and is what the gallery uses) mounts its calendar body into
the scope's `PortalSlot`. Per `overlay_scope.rs`'s own module doc,
`PortalSlot` is *always registered last* among the scope's children,
specifically to guarantee it paints on top of everything else
("`children register in the order content, portal slot`... `That
registration order *is* the entire z-mechanism`"). Masonry's Tab-key
traversal (`find_next_focusable` in `masonry_core/src/passes/update.rs`)
walks the tree in that exact same registration order. The practical
result: Tab from the date-picker trigger does not reach the calendar's
header row next — it continues into whatever comes after the trigger in
the rest of the app, and only reaches the calendar after tabbing through
everything else in the scope.

`dropdown_button` shares the identical architecture (a trigger holding
real masonry focus for the popup's entire lifetime, decoding keys in its
own `on_text_event`, driving a purely synthetic roving-highlight —
`highlighted: Option<usize>` in `MenuContent`, exactly mirroring
`focused_index` in `CalendarGridWidget`) and therefore has the identical
latent gap, confirmed by reading `menu_layer.rs`: `MenuContent` has no
`accepts_focus`/`on_text_event` override today.

`autocomplete` already solved this class of problem for its own
suggestion list (`TextAreaHandle`/`LabelListHandle` in
`components/autocomplete/widget.rs`, using an explicit Tab-interception +
`ctx.set_focus()` redirect).

**Revision, recorded after implementation started:** the original version
of this spec proposed moving real focus onto the calendar grid
automatically when the picker opens (bypassing Tab entirely), reasoning
that date_picker/dropdown_button's arrow keys — unlike autocomplete's,
which are blocked by `TextArea` claiming them for cursor movement — already
work immediately while the trigger holds focus, so an explicit Tab press
first seemed like an avoidable regression. Task 1's implementer hit this
exact case and, per instructions, did not patch around it: traced the
actual failure through masonry's pass-scheduling code
(`masonry_core/src/app/render_root.rs::run_rewrite_passes`,
`masonry_core/src/passes/update.rs::run_update_focus_pass`) and confirmed
it's a hard framework constraint, not a bug in this design's application:
`ctx.set_focus()` only exists on `ActionCtx`/`EventCtx`; a portal child's
un-stash happens inside `PortalSlot::layout`, which always runs *after*
`run_update_focus_pass` validates the pending focus target in the same
pass-loop iteration; nothing re-arms the focus request on a later
iteration once the target becomes interactive. There is no context type
anywhere in that call chain that can re-issue `set_focus` once the target
is confirmed ready — auto-focus-on-open is not achievable without either a
delayed/replayed-keypress workaround or an upstream masonry patch (this
repo tracks masonry's `main` branch rather than forking it, per
`CLAUDE.md` — a local fork was ruled out as disproportionate for this).

Given that, this design reverts to autocomplete's proven model after all:
**the trigger keeps real focus after opening (arrows/Home/End/Enter/
PageUp/PageDown/Escape keep working immediately, exactly as they do
today, via the trigger's existing key-decode-and-`mutate_later`
dispatch — completely unchanged, not removed)**, and **Tab is
additionally intercepted** to move real focus into the calendar, which is
mechanically sound with no race: by the time a user presses Tab, the
picker has already been open and rendered for at least one full pass-loop
cycle, so the target is definitely already un-stashed and interactive.
Once real focus has moved into the calendar this way, the calendar body
handles further keys itself (bubbling, no trigger involvement) — see
Design below.

`popover` has the same latent gap but is explicitly out of scope for this
spec (see Non-goals) — it hosts arbitrary caller-supplied content, so an
"auto-focus on open" feature for it needs a new kind of public API
(exposing a focus target across the crate boundary, which void_ui has no
precedent for today) and deserves its own separate design if wanted later.

## Goals

- Arrow keys/Home/End/Enter/PageUp/PageDown/Escape keep working
  immediately after opening `date_picker` — **no behavior change from
  today**, in either hosting mode. The trigger's existing key-decode-and-
  `mutate_later` dispatch is untouched.
- **Tab, pressed while the trigger holds focus and the calendar is open,
  moves real masonry focus onto `header_prev`, the first header button**
  (Portal mode; in `InTree` mode native Tab already reaches it correctly
  today, so no new interception is needed there — see the `InTree`
  section), matching natural top-to-bottom reading order rather than
  jumping into the grid. From `header_prev`, **native forward Tab walks
  the remaining header buttons in order** (prev → month → year → next)
  before reaching `CalendarGridWidget`, each independently focusable and
  keyboard-activatable via Enter/Space (confirmed already working once
  real focus reaches them, by the existing
  `header_month_button_enter_key_switches_to_month_view` /
  `header_year_button_enter_key_switches_to_year_view` tests).
- Once real focus has moved into the calendar (via Tab), it continues to
  handle arrow keys/Home/End/Enter/PageUp/PageDown/Escape itself —
  bubbling reaches `CalendarBodyWidget` directly, no trigger involvement.
- Closing the calendar (Enter-selecting a date, or Escape) returns real
  focus to the date-picker trigger — whether the calendar was closed while
  real focus was still on the trigger (the common case) or after Tab had
  moved it into the calendar.
- The identical fix applies to `dropdown_button`: Tab (while the trigger
  holds focus, menu open) moves focus onto `MenuContent`; closing
  (Enter-select or Escape) returns focus to the dropdown button trigger.
  Arrow keys/Home/End/Enter/Escape keep working immediately via the
  trigger's existing dispatch, unchanged, exactly as for date_picker.
- `InTree` mode (no scope ancestor) is unaffected — its Tab order is
  already correct today, since `AnchoredOverlay` registers the overlay
  content directly after the trigger.

## Non-goals

- `popover` is not touched by this spec. It has the identical latent gap,
  but fixing it requires a new public-facing "focus this arbitrary child"
  API (a `WidgetId`-based `initial_focus` parameter was discussed and
  intentionally deferred) — a separate spec if/when wanted.
- `autocomplete` is not touched. Its existing "Tab moves focus into the
  open listbox" model (deliberate, documented in
  `components/autocomplete/widget.rs`) is correct for its own UX (the
  listbox opens while the user is still typing) and is not being
  generalized to "auto-focus on open."
- No change to the disabled-date/highlight logic, day-grid math, or any
  of the keyboard-nav behavior added by the prior date-picker keyboard-nav
  work (`2026-07-16-date-picker-keyboard-nav-design.md`) — this spec only
  changes *which widget holds real masonry focus* and *where the
  key-decoding logic lives*, not what the decoded keys do once dispatched.
- No focus-trap behavior (e.g. wrapping Tab from the last focusable
  element back to the first). Forward-Tab past the grid/menu simply
  continues into whatever's next in the app, same as any other
  non-modal control.

## Design

### Shared mechanism

Both components keep the trigger as the real-focus holder immediately
after opening (its existing key-decode-and-`mutate_later` dispatch is
untouched), and **add Tab-interception** in the trigger's own
`on_text_event`, mirroring `autocomplete`'s proven
`on_text_event` Tab-handling block
(`components/autocomplete/widget.rs:1781-1811`) almost verbatim: if the
calendar/menu is open, Tab is pressed without Shift, and a handle to the
content's `WidgetId` is populated, call `ctx.set_focus(content_id)` and
`ctx.set_handled()` — real focus moves into the content, and masonry's
native Tab search takes over correctly from there (into the header row),
since that part was never broken — only the *initial* jump from trigger to
content was.

This is mechanically sound where auto-focus-on-open was not: masonry
bubbles unhandled `TextEvent`s up through the widget tree from wherever
real focus currently is (`masonry_core/src/passes/event.rs`'s
`run_event_pass`, confirmed by reading it directly), and by the time a
user can press Tab, the picker has already been open and rendered for at
least one full pass-loop cycle — the portal content is unconditionally
un-stashed well before any Tab keypress could arrive, so there's no
same-pass race with `PortalSlot::layout`'s un-stash timing (see the
Context section's revision note for why that race rules out doing this at
open time instead).

Three pieces, using the existing `widget_id_handle!` macro convention
already established by `TextAreaHandle`/`LabelListHandle`/
`DatePickerHandle`/`DropdownButtonHandle` (all `Arc<OnceLock<WidgetId>>`,
filled once by the widget reporting its own id, cheaply cloneable to share
across the portal boundary):

1. **Trigger → content handle** (new). Each trigger gains a handle to its
   content's `WidgetId`, filled by the content widget itself the first
   time it reports its id (`CalendarGridWidget`/`MenuContent`, at
   `Update::WidgetAdded`). Because `portal.register(...)` already happens
   unconditionally at `View::build()` regardless of `open` state (verified
   in `date_picker/view.rs:221-239` — the calendar body is mounted, just
   stashed, from the moment the date picker first appears on screen), this
   handle is reliably populated well before any Tab press could occur.
2. **Content → trigger handle** (already exists). `DatePickerHandle` and
   `DropdownButtonHandle` already point from content back to trigger and
   are already threaded into the content widgets for existing close-side
   callbacks — reused as-is for the new `ctx.set_focus(trigger_id)` calls.
3. **The Tab-interception and the return-focus-on-close calls** both run
   from `ActionCtx`/`EventCtx` contexts, since those are the only two
   masonry context types with `set_focus`/`request_focus` (confirmed by
   reading `masonry_core/src/core/contexts.rs` directly). `on_text_event`
   (`EventCtx`) is naturally where Tab-interception lives; the content's
   close paths (Enter-select, Escape) also run with `ActionCtx`/`EventCtx`
   already, so no new plumbing is needed for those either.

### `date_picker`

- `CalendarGridWidget` already `accepts_focus() -> true`; its cell focus
  ring is already driven purely by `focused_index` (confirmed in
  `calendar_grid.rs:321-325` — no dependency on real focus), so it renders
  correctly regardless of which widget holds real focus, and the header
  buttons correctly show no ring until real focus actually reaches one.
- `CalendarBodyWidget` does **not** need `accepts_focus() -> true` — only
  `CalendarGridWidget` needs to be a real focus target. What
  `CalendarBodyWidget` gains is its own `on_text_event`, decoding the same
  key set `ThemedDatePickerWidget::on_text_event` decodes today (Escape,
  arrows, Home, End, Enter, PageUp/PageDown, Shift+PageUp/PageDown) and
  dispatching to the existing `handle_nav_key`/`handle_nav_activate`/
  `handle_nav_page` functions — this is what handles those keys **once
  real focus has moved into the calendar via Tab**; it does not replace
  anything, it's an additional path.
- Those functions currently take `this: &mut WidgetMut<'_, Self>` because
  they're invoked via `ctx.mutate_later(...)` reaching in from the
  trigger. They're already generic over a `CalendarCtx` trait shimming
  `ActionCtx`/`MutateCtx` (`impl_calendar_ctx!(ActionCtx<'_>,
  MutateCtx<'_>)` in `calendar_body.rs`). Add `impl CalendarCtx for
  EventCtx<'_>` (mirroring the existing two impls — `queue_mutate` already
  matches `EventCtx::mutate_later`, `submit_date_selected` matches
  `EventCtx::submit_action`, `request_layout` matches directly) so the
  same functions run unchanged from the new `on_text_event`, called with
  `(&mut self, &mut EventCtx)` instead of unpacking a `WidgetMut`.
- `ThemedDatePickerWidget::on_text_event`'s existing arrow/Home/End/Enter/
  PageUp/PageDown decode-and-`mutate_later` logic is **kept exactly as-is,
  in both hosting modes** — it's what makes those keys work immediately
  after opening, which is the whole point of not auto-transferring focus.
  It gains one new, additional branch: Tab (no Shift), when the calendar
  is open and Portal-mode's grid handle is populated, calls
  `ctx.set_focus(grid_id)` and `ctx.set_handled()` instead of falling
  through to native Tab search (which, as established, wouldn't find
  anything useful in Portal mode). `InTree` mode doesn't need this branch
  at all — native Tab already works there — so it's gated to
  `Hosting::Portal` the same way the existing reanchor-loop logic already
  gates portal-only behavior in this file.
- Escape closing from *inside* the calendar (possible once Tab has moved
  real focus onto the grid) is added to `CalendarBodyWidget::on_text_event`,
  calling `ctx.set_focus(trigger_id)` via the existing `DatePickerHandle`
  before triggering the actual close (which itself must run through
  `mutate_later`/`MutateCtx`, since that's still the only way to reach the
  trigger widget's `open` state from the calendar body).
- Enter-selecting a day (`activate_day`, already closes and emits
  `DateChanged`) gains `ctx.set_focus(trigger_id)` alongside its existing
  close logic — this covers both the common case (day selected while the
  trigger still holds focus) and the Tab-then-select case (day selected
  after Tab moved focus into the grid) uniformly, since `activate_day` is
  the single shared activation path either way.
- New handle: `CalendarGridHandle` (or similarly named), filled by
  `CalendarGridWidget` at its own `Update::WidgetAdded`, held by
  `ThemedDatePickerWidget` (threaded down to `CalendarBodyView` the same
  way `DatePickerHandle` is threaded down today, just in the opposite
  direction).

### `dropdown_button`

Same shape, mapped onto the existing types:

- `MenuContent` gains `accepts_focus() -> true` and its own
  `on_text_event`, decoding the same arrow/Home/End/Enter/Escape set
  currently decoded in `DropdownButtonWidget::on_text_event`
  (`dropdown_button/widget.rs:662`), dispatching to whatever internal
  functions currently drive `highlighted` (mirroring `CalendarNavKey`'s
  relocation for date_picker — the exact function signatures will need
  the same `WidgetMut` → generic-ctx generalization if they're not
  already ctx-agnostic; this is a smaller surface than date_picker's,
  since `dropdown_button` has no month/year view-mode complexity). This is
  the additional path for once Tab has moved focus into the menu — it
  does not replace `DropdownButtonWidget::on_text_event`'s existing
  decode logic, which is kept exactly as-is in both hosting modes, same
  reasoning as date_picker's trigger above.
- `DropdownButtonWidget::on_text_event` gains the same new Tab-
  interception branch as date_picker's trigger: Tab (no Shift), menu
  open, `Hosting::Portal`, menu handle populated → `ctx.set_focus(menu_id)`
  + `ctx.set_handled()`.
- New handle from `DropdownButtonWidget` to `MenuContent`'s id, filled at
  `MenuContent`'s `Update::WidgetAdded`.
- `DropdownButtonHandle` (already exists, already points trigger-ward) is
  reused for the new `ctx.set_focus(trigger_id)` calls on Enter-select and
  Escape, added alongside `MenuContent`'s existing close logic.

### `InTree` mode

No changes needed for either component, and no new Tab-interception logic
is added there. `AnchoredOverlay` already registers `overlay` (the
calendar/menu content) directly after `primary` (the trigger) — Tab from
the trigger already reaches the content next, correctly, today. The new
Tab-interception branch is conditioned on `Hosting::Portal` only
(mirroring how `compose_reanchor`/`arm_reanchor_on_anim_frame` are already
conditioned on portal mode elsewhere in these same widgets) so `InTree`
mode's existing, already-correct native-Tab behavior is left untouched.
This must be verified — not just assumed — with an `InTree`-mode test
confirming Tab from the trigger still reaches the header row/menu in the
unmodified case.

## Testing plan

Per component, using `TestHarness` in Portal mode (mirroring the pattern
already established by the recent date-picker keyboard-nav test suite):

- Arrow keys/Home/End/Enter/PageUp/PageDown continue to work immediately
  after open, with **no Tab press required** — real regression check
  against the entire keyboard-nav test suite from the prior spec, since
  this is the behavior the reverted auto-focus-on-open design would have
  broken were it not for the pivot recorded above.
- Tab, while the trigger holds focus and the calendar/menu is open, moves
  real focus onto the grid/menu content — assert via the harness's focus
  state after driving a real Tab keypress, not merely that a handle was
  populated.
- From that focused state, Shift+Tab reaches each header button / first
  menu item in the correct order; each is independently
  Enter/Space-activatable (already covered for date_picker's header
  buttons by the existing Task 6 tests — confirm they still pass
  unchanged, since `ThemedButton`'s own keyboard handling isn't touched).
- Once focus is in the calendar/menu (post-Tab), arrow keys/Home/End/
  Enter/PageUp/PageDown/Escape still work, now via the content's own
  `on_text_event` and bubbling rather than the trigger's dispatch.
- Enter-selecting a date/menu item, and pressing Escape, both close and
  return real focus to the trigger — assert via harness focus state, in
  both the common case (closed while trigger still had focus) and the
  post-Tab case (closed while the calendar/menu held focus).
- `InTree` mode: Tab from the trigger still reaches the header row/menu
  correctly (unmodified behavior) — a regression test guarding the
  `Hosting::Portal`-only conditioning described above.

## Open questions

None — scope was narrowed deliberately during design: `popover`'s
`initial_focus` and `autocomplete`'s auto-focus-on-open were both
discussed and explicitly deferred (see Non-goals), not left ambiguous.
Auto-focus-on-open for date_picker/dropdown_button was also attempted and
reverted after a confirmed masonry framework constraint — see the Context
section's revision note; the Tab-to-enter model documented above is the
final design, not a remaining open question.
