# Fix #175: restore arrow-key row-focus navigation

## Problem

masonry's `VirtualScroll` (as of the pinned commit picked up while migrating
to its `usize`-indexed API — see PR #176) now implements its own
`on_text_event`, handling `ArrowUp`/`ArrowDown` as built-in scroll-by-arrow-key
gestures along its own scroll axis. masonry's keyboard dispatch is strictly
bubble-only (focused widget → its ancestors, stopping at the first one that
sets `is_handled`; confirmed in `masonry_core/src/passes/event.rs`, which has
no capture phase). `VirtualScroll` sits directly between a focused row and
`CollectionBodyWidget`, so its native arrow handling now claims `ArrowUp`/
`ArrowDown` before `CollectionBodyWidget`'s own row-focus-move logic ever runs.

Impact: keyboard row-to-row focus navigation (Up/Down) in `data_grid`/`list`
silently degrades to viewport scrolling. 5 tests in `collection::body::tests`
were left failing (not weakened) to flag this: `arrow_down_moves_focus_to_next_row`,
`arrow_up_moves_focus_to_previous_row`, `arrow_up_at_first_row_is_a_no_op`,
`arrow_down_at_last_materialized_row_is_a_no_op`, `arrow_keys_on_non_row_focus_are_unhandled`.

There is no opt-out flag on `VirtualScroll` for its built-in arrow handling,
and no capture-phase event mechanism in masonry to pre-empt it generically.

## Chosen approach

Preserve current keyboard UX exactly (arrow keys move row focus) by moving
Up/Down handling from `CollectionBodyWidget` down into `RowClickable` — the
only widget under our control that runs *before* `VirtualScroll` in the
bubble chain, since `RowClickable` is `VirtualScroll`'s direct child **and**
the actual focus target (rows take focus via `ctx.request_focus()` in
`RowClickable::on_pointer_event`).

`RowClickable` doesn't know its siblings, so `CollectionBodyWidget`/
`body_view.rs` precomputes each visible row's up/down neighbor `WidgetId`
whenever the materialized window settles, and pushes it down — the same
push-down pattern already used for `TreeRowMeta` (`RowClickable::tree_meta`
for local toggle decisions, vs. `CollectionBodyWidget::row_meta` for the
cross-row Left/Right focus moves it still owns).

Rejected alternative: drop Up/Down row-focus-move entirely and rely on Tab
(masonry's existing built-in traversal, unaffected by this bug) for
row-to-row keyboard movement, letting arrow keys become pure viewport
scroll. Simpler, but changes established keyboard UX for grids/lists.

## Design

### 1. `RowClickable` (`src/collection/row_click.rs`)

- New fields: `nav_up: Option<WidgetId>`, `nav_down: Option<WidgetId>`
  (default `None`), analogous to `tree_meta`.
- New setter: `set_nav(this: &mut WidgetMut<'_, Self>, up: Option<WidgetId>, down: Option<WidgetId>)`
  — cheap, no repaint/relayout (only affects the next key press), mirroring
  `set_tree_meta`.
- `on_text_event` gains `ArrowDown`/`ArrowUp` arms. Both **always** call
  `ctx.set_handled()` once a row has focus — this is what prevents
  `VirtualScroll`'s native scroll-by-arrow from ever running, even when
  there's no neighbor to move to. When the corresponding `nav_*` is `Some`,
  additionally call `ctx.set_focus(target)`.
- Behavior note: pressing Up at the first row (or Down at the last
  materialized row) is now a **positively handled** no-op, not a fully
  unhandled event — a deliberate, small semantic shift from today, required
  so the event doesn't fall through to `VirtualScroll`.

### 2. `CollectionBodyWidget` (`src/collection/body.rs`)

- Remove the `ArrowDown`/`ArrowUp` match arms from `on_text_event`; the
  top-level key filter narrows to `ArrowLeft | ArrowRight` only.
- `tree_first_child`/`tree_parent` (Right/Left tree nav) are unchanged —
  they were never intercepted by `VirtualScroll` (wrong scroll axis) and
  keep working exactly as today.
- Update the module doc comment, which currently claims Up/Down ownership.

### 3. Push mechanism (`src/collection/body_view.rs`)

- The `active_range` capture in `CollectionBodyView::message` (currently
  gated behind `self.lazy.is_some() || self.depth_at.is_some()`) becomes
  unconditional: every collection needs it now, not just lazy-load/tree
  ones. Rename the local `tree_active` variable to `active_range_update` to
  reflect the broadened purpose.
- New function `refresh_row_nav(element, active_start)`, called
  unconditionally in `rebuild()` (alongside the still-conditional
  `refresh_tree_row_meta`):
  - Reads `VirtualScroll::children_ids()` (confirmed `BTreeMap`-backed
    internally, so reliably ascending-index-ordered) into an owned `Vec<WidgetId>`.
  - For each visible row at slice position `k` (absolute index
    `active_start + k`), computes `up = ids.get(k - 1)`, `down = ids.get(k + 1)`.
  - Reaches the row directly via `VirtualScroll::child_mut(idx)` (existing
    public masonry API, keyed by absolute index) + `WidgetMut::downcast::<RowClickable>()`,
    then calls `RowClickable::set_nav(&mut row, up, down)`.
  - Safety: this runs at the same "materialization has caught up" point as
    `refresh_tree_row_meta`, so `ids.len() == active_range.len()` holds (no
    gaps) — the same invariant that function already relies on. A
    momentarily-stale `active_start` (mid-transition) at worst yields a
    transiently-wrong nav map that the next settled rebuild corrects.

### 4. Tests

- Move the 4 real regression tests off `collection::body::tests` (which
  drive plain `Label` rows inside a bare `VirtualScroll`, bypassing
  `RowClickable` entirely) into `collection::row_click::tests`, rewritten as
  direct `RowClickable` + `set_nav` unit tests — simpler than driving a full
  `VirtualScroll` harness, since the fix is now local to `RowClickable`:
  - `arrow_down_moves_focus_to_next_row`
  - `arrow_up_moves_focus_to_previous_row`
  - `arrow_up_at_first_row_is_a_no_op` — assertion flips to
    `handled.is_handled()` (see behavior note above), comment updated.
  - `arrow_down_at_last_materialized_row_is_a_no_op` — same assertion flip.
- `arrow_keys_on_non_row_focus_are_unhandled` stays in `body.rs`, assertion
  flips to `handled.is_handled()`: focus sitting directly on `VirtualScroll`
  (not a row) is a scenario this fix doesn't touch, and masonry's native
  scroll claiming it is correct, not a bug.
- `collection::body::tests` keeps its tree Left/Right tests unchanged (never
  regressed).

## Out of scope

- Filing an upstream masonry issue/PR for a `VirtualScroll` arrow-key
  opt-out flag — not needed given the above fix, but still a reasonable
  longer-term cleanup upstream could offer.
- Scrolling a newly-focused-but-just-outside-the-viewport row into view
  (tracked separately as #136).
