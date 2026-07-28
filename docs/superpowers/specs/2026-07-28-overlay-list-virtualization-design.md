# Virtualize overlay suggestion/menu lists on the shared collection substrate

Closes #98. Approach C considered during design (promote virtualization into a
public `VirtualList` spanning `list`/`data_grid` too) was parked as #213 —
out of scope here.

## Problem

`autocomplete`'s `SuggestionList`/`LabelList` and `dropdown_button`'s
`MenuContent` (`src/components/autocomplete/widget.rs`,
`src/components/dropdown_button/widget.rs`) each materialize one widget per
item with no virtualization — `LabelList::set_items` and
`MenuContent::set_items` drain and rebuild every child from the full
`Vec<ArcStr>` on every call. Large filtered candidate sets don't scale. The
two widgets are also near-duplicates by their own doc comments (`LabelList`'s:
"Closely mirrors `MenuContent` from the dropdown button") — ~750 lines of
hand-synchronized hover/keyboard-highlight/scroll-into-view/click-to-select
logic.

The crate-internal `collection` substrate (`src/collection/`) already
virtualizes on masonry's `VirtualScroll` for `list`/`data_grid`, including
scroll-to-index, keyboard nav, and click routing. But its entry point,
`collection_body` (`src/collection/body_view.rs`), is a xilem **View**
(`impl WidgetView<State, Action>`, driven by per-frame render-row closures
against `State`). The overlay lists are driven **imperatively at the widget
layer**: `SuggestionListView`/`MenuContentView` are real `View`s but are empty
shells at `build()` — actual item population happens via `mutate_later`/direct
`WidgetMut` calls issued by `AutocompleteWidget`/`MenuContentView::rebuild` in
response to keystrokes or prop changes, not through `collection_body`'s
per-frame diffing. So `collection_body` is not a drop-in.

## Key insight

Everything `collection_body` *calls* is already non-generic over `State`/`Item`
and reusable as-is:

- `CollectionBodyWidget` (`src/collection/body.rs`) — nav precompute
  (`refresh_row_nav`), pure `WidgetId`/index bookkeeping.
- `RowClickable` (`src/collection/row_click.rs`) — hover/focus/keyboard
  nav/click-vs-activate detection, self-contained; emits `RowInteraction` via
  `ctx.submit_action`, a normal widget action any imperative parent can catch.
- `apply_row_click`/`apply_row_activate` (`src/collection/click.rs`) — id/modifier
  arithmetic, trivially usable with a plain `Vec<Item>`.
- `SelectionState` (`src/collection/selection.rs`), `ScrollState`,
  `clamp_scroll_index`, `nearing_end` (`src/collection/scroll.rs`,
  `src/collection/ids.rs`) — plain-data helpers.

What's missing is only the **orchestration** `collection_body`'s View
`rebuild`/`message` does: scroll-to-anchor application, lazy-load peeking, and
tree-depth refresh sequencing. Neither overlay list needs lazy-load (both
always receive an already-fully-filtered `Vec<ArcStr>`, never a
paginated/async source) or tree metadata (both are flat lists), so the
widget-imperative sibling needed here is strictly smaller than
`collection_body`'s full feature set: item diffing + nav refresh + selection,
nothing else.

One behavior is genuinely new, not inherited from the substrate: both overlay
lists currently **wrap** keyboard highlight at the list ends (`LabelList`'s doc:
"Moves the keyboard highlight by delta positions (wrapping)"; `MenuContent`'s:
identical wording). `RowClickable`'s nav no-ops at the materialized edge
instead of wrapping — correct for `list`/`data_grid`, which don't want
wrapping. Wrap-around stays local to the overlay-list wrappers, layered on top
of the substrate's edge-stop nav, rather than pushed into shared code neither
existing consumer wants.

## Design

### New: `src/collection/imperative_body.rs` — `CollectionListWidget<Item>`

A `pub(crate)` masonry `Widget`, generic over `Item: Clone + Send + Sync +
'static` (mirroring the existing item-generic shape of `RenderRow` elsewhere in
`collection`, not hardcoded to `ArcStr` — both current call sites happen to
use `ArcStr` today, but a future richer item type isn't precluded). Owns a
`CollectionBodyWidget` wrapping a `VirtualScroll`, plus:

- `new(render_row: impl Fn(&Item, bool, &Theme) -> NewWidget<W> + 'static, theme: &Theme) -> Self`
  — `render_row`'s `bool` is "is this row selected", matching `RenderRow`'s
  existing selected-flag convention.
- `set_items(this: &mut WidgetMut<'_, Self>, items: Vec<Item>)` — diffs
  `items` against `VirtualScroll`'s currently materialized window (add/remove
  via `VirtualScroll::add_child`/`remove_child`, same diff `collection_body`'s
  `virtual_scroll` closure already performs, just invoked explicitly instead
  of from a View rebuild), then calls `CollectionBodyWidget::refresh_row_nav`.
- `set_selected(this: &mut WidgetMut<'_, Self>, index: Option<usize>)` — single
  highlighted/selected index, not `SelectionState`'s `BTreeSet` (both overlay
  lists are single-select today; a `BTreeSet` would pull in multi-select
  modifier/shift-range machinery neither needs — see acceptance discussion).
  Re-renders the affected rows' `render_row` selected flag.
- `set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme)` — forwarded to
  materialized rows.
- `virtual_scroll_mut(this: &mut WidgetMut<'_, Self>) -> WidgetMut<'_, VirtualScroll>`
  — exposed (mirroring `CollectionBodyWidget::virtual_scroll_mut`'s existing
  pattern) so the owning `SuggestionList`/`MenuContent` can call
  `VirtualScroll::scroll_to` directly for wrap-around, without
  `CollectionListWidget` needing to know about wrapping itself.

Reuses `apply_row_click`/`apply_row_activate` and `clamp_scroll_index`
unchanged for click routing and index-shrink clamping.

### New: `src/collection/item_row.rs` — `OverlayListItem`

Replaces both `SuggestionItem` (autocomplete) and `MenuContent`'s current ad
hoc label widget — both are, today, a styled `ArcStr` label with
hover/keyboard-highlight paint, with no divergence found. `OverlayListItem`
wraps a `RowClickable` around that label+highlight paint. A single shared
`render_overlay_list_item(item: &ArcStr, selected: bool, theme: &Theme) ->
NewWidget<OverlayListItem>` function is the `render_row` closure both
`SuggestionList` and `MenuContent` pass to their `CollectionListWidget`.

### Rewritten: `SuggestionList` (`src/components/autocomplete/widget.rs`)

Becomes a thin wrapper around `CollectionListWidget<ArcStr>`:

- Row rendering: `render_overlay_list_item` (shared).
- Wrap-around: masonry's keyboard dispatch is bubble-only (no capture phase —
  `row_click.rs`'s own module doc notes this), so `RowClickable`'s unhandled
  Up/Down at the materialized edge bubbles up through `CollectionListWidget`
  to `SuggestionList`'s own `on_text_event` with no extra plumbing. On
  Down-at-last / Up-at-first, it calls `virtual_scroll_mut(...).scroll_to`
  (the new accessor above) to the opposite end and focuses the first/last
  materialized row once present.
- Action translation: `RowInteraction` (click/activate) → `SuggestionSelected`,
  forwarded exactly as `SuggestionListView` forwards it today — no change to
  `SuggestionList`'s or `SuggestionListView`'s public shape.

`LabelList` is deleted entirely; its virtualization/hover/highlight/scroll-into-view
logic is now `CollectionListWidget`'s job.

### Rewritten: `MenuContent` (`src/components/dropdown_button/widget.rs`)

Same shape as `SuggestionList` above: `CollectionListWidget<ArcStr>` +
`render_overlay_list_item` + wrap-around handling + `RowInteraction` →
dropdown's existing select action. No change to `MenuContent`'s or
`MenuContentView`'s public shape.

### Not touched

`collection_body`, `CollectionBodyWidget`'s existing methods/signature,
`RowClickable`'s core hover/focus/click logic, `SelectionState`, `list`,
`data_grid` — all reused or left exactly as they are.

## Data flow

1. **Population.** `AutocompleteWidget`/`MenuContentView` compute the full
   filtered/static `Vec<ArcStr>` exactly as today, then call
   `CollectionListWidget::set_items` (same call sites that currently call
   `LabelList::set_items`/`MenuContent::set_items`). Diffing adds/removes
   `OverlayListItem` rows for the materialized window; `refresh_row_nav`
   brings up/down neighbor targets current.
2. **Keyboard nav.** Arrow keys hit the focused `OverlayListItem`'s
   `RowClickable` first; Up/Down move to the precomputed neighbor and
   scroll-into-view if needed (unchanged substrate behavior). At a
   materialized edge, `RowClickable` reports unhandled; the owning
   `SuggestionList`/`MenuContent` catches that and performs the wrap.
3. **Click/activate.** Pointer press → `RowClickable` → `RowInteraction`
   widget action → caught by `SuggestionList`/`MenuContent`'s
   `on_widget_action` → translated to the component's own public action →
   forwarded up by the owning `*View` exactly as today.
4. **Selection highlight.** `CollectionListWidget::set_selected(Option<usize>)`
   — single index, no `SelectionState`.

## Edge cases / error handling

- **Empty item list.** `set_items(vec![])` removes all materialized rows; no
  focus target — matches today.
- **Selected/focused index past the new list's end** after items shrink.
  Clamp via `clamp_scroll_index`, reused rather than reimplemented.
- **Rapid re-`set_items` calls** (fast typing). Each call diffs against the
  *current* materialized window independently — no stale-state risk, same
  guarantee `LabelList::set_items`'s drain-and-rebuild gives today, just
  narrowed to a diff.
- **Row added/removed in the same rebuild pass a nav refresh runs.**
  `CollectionBodyWidget::refresh_row_nav`'s existing same-pass-add guard
  (regression-tested against #175) applies unchanged, since
  `CollectionListWidget` calls the same method the same way.
- **Wrap-around at a single-item list.** Wraps to itself — a no-op focus
  request, not a crash. This boundary isn't exercised by `list`/`data_grid`
  (which never wrap), so it needs an explicit new test.
- **`aria-activedescendant`.** Must keep pointing at the focused
  `OverlayListItem`'s id after the rewrite — concrete regression risk during a
  widget rewrite; verify explicitly rather than assuming it carries over.

## Testing plan

- **Existing autocomplete/dropdown_button test suites** (hover, keyboard
  highlight, scroll-into-view, click-to-select, `aria-activedescendant` in
  each `widget.rs`) must keep passing, re-pointed at the new thin wrappers
  without behavioral rewrites. A test needing a behavioral change is a
  regression signal, not something to relax.
- **New `CollectionListWidget` tests** (`src/collection/imperative_body.rs`),
  following `body.rs`'s existing `drive_to_fixpoint`-style harness pattern:
  `set_items` diffing (grow/shrink/replace), selected-index tracking,
  clamp-on-shrink, and the materialized-edge nav cases `body.rs` already
  covers (reused, not reinvented).
- **New wrap-around tests**, since this logic is genuinely new: Down-at-last
  wraps to first, Up-at-first wraps to last, single-item list wraps to itself,
  wrap issues a scroll request before focusing (mirroring `body.rs`'s existing
  `arrow_down_past_the_viewport_edge_scrolls_the_new_focus_into_view`-style
  assertions).
- **Manual gallery verification** (defer to the human per standing project
  practice — no claimed visual verification): run the gallery's autocomplete
  and dropdown_button demo panels with a large candidate list (hundreds of
  items) and confirm bounded materialized widget count, e.g. via a test
  harness assertion on `children_ids().len()` after `set_items` with a large
  `Vec`, or the `profiling` feature/Tracy.
- **Regression check for #175.** Existing `body.rs` test for the same-pass-add
  guard must keep passing unchanged.

## Acceptance criteria (from #98)

- [ ] Autocomplete + dropdown_button overlay lists virtualize (bounded widget
      count regardless of item count).
- [ ] `SuggestionList` and `MenuContent` share the virtualization/hover/keyboard/
      scroll-into-view/click substrate (`CollectionListWidget` +
      `OverlayListItem`) rather than duplicating it.
- [ ] Existing autocomplete + dropdown_button behavior/accessibility tests
      still pass.
- [ ] Keyboard-highlight wrap-around at list ends is preserved (new behavior
      relative to the shared substrate, owned locally by each wrapper).
