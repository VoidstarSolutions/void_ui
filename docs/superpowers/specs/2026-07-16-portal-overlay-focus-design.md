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
`ctx.set_focus()` redirect) and is the reference implementation this
design generalizes from — but its exact model (stay on the trigger,
redirect only on an explicit Tab press) is deliberately *not* reused
verbatim, since autocomplete's listbox opens while the user is still
typing, and the two components (date_picker/dropdown_button) have a
different, more standard requirement: arrow keys should navigate
immediately once opened, without an extra Tab press first.

`popover` has the same latent gap but is explicitly out of scope for this
spec (see Non-goals) — it hosts arbitrary caller-supplied content, so an
"auto-focus on open" feature for it needs a new kind of public API
(exposing a focus target across the crate boundary, which void_ui has no
precedent for today) and deserves its own separate design if wanted later.

## Goals

- Opening `date_picker` (Portal mode) moves real masonry keyboard focus
  onto the calendar grid, so arrow keys/Home/End/Enter/PageUp/PageDown
  work immediately — no behavior change from today's "arrows work right
  away" experience.
- From that focused grid, **Shift+Tab reaches the header row buttons in
  the correct order** (next → year → month → prev), each independently
  focusable and keyboard-activatable via Enter/Space (confirmed already
  working once real focus reaches them, by the existing
  `header_month_button_enter_key_switches_to_month_view` /
  `header_year_button_enter_key_switches_to_year_view` tests).
- Closing the calendar (Enter-selecting a date, or Escape) returns real
  focus to the date-picker trigger.
- The identical fix applies to `dropdown_button`: opening the menu moves
  focus onto `MenuContent` (arrow keys/Home/End/Enter/Escape work
  immediately, matching today), and closing (Enter-select or Escape)
  returns focus to the dropdown button trigger.
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

Both components move from "trigger holds real focus, decodes keys itself,
drives a purely synthetic roving-highlight via `mutate_later`" to
"trigger *transfers* real focus into the content once, content decodes
its own keys thereafter." This works because masonry bubbles unhandled
`TextEvent`s up through the widget tree from wherever real focus
currently is (`masonry_core/src/passes/event.rs`'s `run_event_pass`,
confirmed by reading it directly) — so once the content widget holds real
focus, key events reach it and its own ancestors directly; no cross-widget
`mutate_later` indirection is needed for key routing anymore.

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
   handle is populated well before the very first user-initiated open, so
   there is no "first open has no target yet" edge case to handle.
2. **Content → trigger handle** (already exists). `DatePickerHandle` and
   `DropdownButtonHandle` already point from content back to trigger and
   are already threaded into the content widgets for existing close-side
   callbacks — reused as-is for the new `ctx.set_focus(trigger_id)` calls.
3. **The actual focus transfer** happens from an `ActionCtx`/`EventCtx`
   context, since those are the only two masonry context types with
   `set_focus`/`request_focus` (confirmed by reading
   `masonry_core/src/core/contexts.rs` directly — `UpdateCtx`,
   `ComposeCtx`, `LayoutCtx`, `MeasureCtx` do not have it, which rules out
   doing this from `Update::WidgetAdded`, `on_anim_frame`, or `compose`).
   Concretely: the trigger's existing `on_action` handler (already
   `ActionCtx`, already where `PortalBinding::open()` is called for the
   "open" action) additionally calls `ctx.set_focus(content_id)` right
   there, if the handle is populated. The content's existing close paths
   (Enter-select, Escape) additionally call `ctx.set_focus(trigger_id)`.

**Verification risk, called out explicitly:** whether `ctx.set_focus(id)`
called in the same action-handling turn that also un-stashes that same
widget (both happen while processing the "open" action) resolves
correctly, or needs a one-pass defer, could not be confirmed by reading
the framework source alone. This must be verified with a real
`TestHarness` test (open the picker/dropdown, assert real focus landed on
the grid/menu, not just that the handle was populated) during
implementation — not assumed.

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
  `handle_nav_page` functions.
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
  PageUp/PageDown decode-and-`mutate_later` logic is **only dead in Portal
  mode**, where the grid takes over key handling once it holds real focus.
  In `InTree` mode (see the `InTree` section below), the trigger keeps
  real focus for the calendar's entire lifetime exactly as it does today —
  none of that logic moves or changes there. Concretely: the existing
  decode-and-dispatch block in `on_text_event` stays exactly as-is, gated
  behind the `Hosting::InTree` branch it already switches on internally;
  the new `Hosting::Portal` branch has nothing left to do for those keys
  (the grid handles them directly), but keeps Escape handling for the
  brief window before the first open-time focus transfer completes.
  Escape closing from *inside* the calendar (now possible since the grid
  can hold real focus in Portal mode) is added to
  `CalendarBodyWidget::on_text_event`, calling `ctx.set_focus(trigger_id)`
  via the existing `DatePickerHandle`.
- Enter-selecting a day (`activate_day`, already closes and emits
  `DateChanged`) gains `ctx.set_focus(trigger_id)` alongside its existing
  close logic.
- New handle: `CalendarGridHandle` (or similarly named), filled by
  `CalendarGridWidget` at its own `Update::WidgetAdded`, held by
  `ThemedDatePickerWidget` (threaded down to `CalendarBodyView` the same
  way `DatePickerHandle` is threaded down today, just in the opposite
  direction).
- `ThemedDatePickerWidget`'s open-handling (`on_action`, wherever it calls
  `PortalBinding::open(...)` today) adds `ctx.set_focus(grid_id)` right
  after, if the handle is populated.

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
  since `dropdown_button` has no month/year view-mode complexity). As
  with date_picker, this only makes `DropdownButtonWidget::on_text_event`'s
  existing decode logic dead in Portal mode — its `Hosting::InTree` branch
  keeps that logic exactly as today, since the trigger keeps real focus
  for the menu's entire lifetime in that mode.
- New handle from `DropdownButtonWidget` to `MenuContent`'s id, filled at
  `MenuContent`'s `Update::WidgetAdded`.
- `DropdownButtonHandle` (already exists, already points trigger-ward) is
  reused for the new `ctx.set_focus(trigger_id)` calls on Enter-select and
  Escape, added alongside `MenuContent`'s existing close logic.
- `DropdownButtonWidget`'s open-handling adds `ctx.set_focus(menu_id)`
  after `PortalBinding::open(...)`, matching date_picker's trigger-side
  change.

### `InTree` mode

No changes needed for either component. `AnchoredOverlay` already
registers `overlay` (the calendar/menu content) directly after `primary`
(the trigger) — Tab from the trigger already reaches the content next,
correctly, today. The new focus-transfer-on-open logic should be
conditioned on `Hosting::Portal` only (mirroring how `compose_reanchor`/
`arm_reanchor_on_anim_frame` are already conditioned on portal mode
elsewhere in these same widgets) so `InTree` mode's existing, already-
correct behavior is left untouched. This must be verified — not just
assumed — with an `InTree`-mode test confirming Tab from the trigger still
reaches the header row/menu in the unmodified case.

## Testing plan

Per component, using `TestHarness` in Portal mode (mirroring the pattern
already established by the recent date-picker keyboard-nav test suite):

- Opening the picker/dropdown moves real focus onto the grid/menu content
  — assert via the harness's focus state after driving the open action,
  not merely that a handle was populated. This is the test that resolves
  the "same-pass set_focus-while-unstashing" risk flagged above.
- From that focused state, Shift+Tab reaches each header button /
  first menu item in the correct order; each is independently
  Enter/Space-activatable (already covered for date_picker's header
  buttons by the existing Task 6 tests — confirm they still pass
  unchanged, since `ThemedButton`'s own keyboard handling isn't touched).
- Arrow keys/Home/End/Enter/PageUp/PageDown continue to work immediately
  after open, with no extra Tab press required (regression check against
  the entire keyboard-nav test suite from the prior spec).
- Enter-selecting a date/menu item, and pressing Escape, both close and
  return real focus to the trigger — assert via harness focus state.
- `InTree` mode: Tab from the trigger still reaches the header row/menu
  correctly (unmodified behavior) — a regression test guarding the
  `Hosting::Portal`-only conditioning described above.

## Open questions

None — scope was narrowed deliberately during design: `popover`'s
`initial_focus` and `autocomplete`'s auto-focus-on-open were both
discussed and explicitly deferred (see Non-goals), not left ambiguous.
