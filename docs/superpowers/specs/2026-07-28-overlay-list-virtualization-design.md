# Virtualize overlay suggestion/menu lists on the shared collection substrate

Closes #98. Approach C considered during design (promote virtualization into a
public `VirtualList` spanning `list`/`data_grid` too) was parked as #213 —
out of scope here.

> **Revision note (post-research correction):** the first version of this
> spec proposed reusing `RowClickable`/`CollectionBodyWidget` (the
> `list`/`data_grid` substrate). Research done while writing the
> implementation plan found that premise wrong — see "Key insight" below.
> This revision replaces that architecture; the Problem/scope stays the same.

## Problem

`autocomplete`'s `SuggestionList`/`LabelList` and `dropdown_button`'s
`MenuContent` (`src/components/autocomplete/widget.rs`,
`src/components/dropdown_button/menu_layer.rs`) each materialize one widget per
item with no virtualization — `LabelList::set_items` and
`MenuContent::set_items` drain and rebuild every child from the full
`Vec<ArcStr>` on every call. The two widgets are also near-duplicates: both
are a single always-focused container tracking hover/highlight state over a
flat `Vec` of bare label rows, hit-tested via a parallel `item_rects: Vec<Rect>`
built at layout time — hand-synchronized hover/keyboard-highlight/scroll-into-view/
click-to-select logic that will drift apart if changed independently.

**Scope correction found during research:** `autocomplete`'s
`compute_filtered` (`widget.rs:905-916`) already caps *both* the empty-query
and filtered branches at `MAX_SUGGESTIONS = 20` — its own doc comment names
this as issue #98's stopgap. So today, `SuggestionList` never sees more than
20 items; virtualizing it alone produces no observable benefit unless this
cap is also removed. `dropdown_button` has no equivalent cap, so unbounded
item lists there hit the real problem today. **This issue also removes
`MAX_SUGGESTIONS` entirely** (decision confirmed with the user), relying on
the new substrate to bound actual widget count instead of a hard item-count
ceiling.

`dropdown_button`'s `MenuContent` also has **no height cap or scrolling at
all** today (`measure()` sizes vertically to `item_height * item_count`,
unbounded) — a menu with many items just grows arbitrarily tall. Virtualizing
via masonry's `VirtualScroll` requires a **bounded viewport**: `VirtualScroll`
materializes what's visible within a fixed-height viewport, so an
unbounded-height container defeats the point (it would just ask for
everything). This issue therefore also gives `MenuContent` a capped,
scrollable viewport for the first time, matching `SuggestionList`'s existing
`MAX_LIST_HEIGHT` (200px) constant/value for consistency between the two.

## Key insight (revised)

The original plan for this issue proposed reusing the `list`/`data_grid`
substrate — `CollectionBodyWidget` + `RowClickable` (`src/collection/`) —
imperatively. Research done before writing the implementation plan found this
doesn't fit:

- **`RowClickable`'s model is per-row real focus** — `accepts_focus() == true`
  on every row, keyboard focus physically moves between rows via
  `ctx.set_focus(target)` (a roving-tabindex ARIA pattern). `SuggestionList`/
  `LabelList` and `MenuContent` use a **different, equally-valid ARIA
  pattern**: a single always-focused container (`LabelList` itself, or the
  dropdown's trigger button) plus a virtual `highlighted: Option<usize>`
  index, surfaced to assistive tech via accesskit's active-descendant
  relationship (`node.set_active_descendant(...)`, `LabelList::widget.rs:755-766`).
  Migrating onto `RowClickable` would move real keyboard focus between rows —
  a legitimate but *different*, more invasive pattern — and would require
  rewriting (not adapting) `tab_into_listbox_and_arrow_keys_set_active_descendant`
  (`autocomplete/widget.rs:2423-2518`), since what it asserts (Tab lands on a
  `Role::ListBox` container; active-descendant tracks the highlight) would no
  longer hold. **Confirmed with the user: preserve the existing single-focus
  pattern instead.**
- **Wrap-around isn't a gap.** `LabelList::move_highlight`
  (`widget.rs:404-424`) already wraps via `rem_euclid` over the *full* item
  count, not just the materialized window — this ports forward almost
  verbatim; no new wrap-around logic or tests are needed for the mechanism
  itself, only for the widget doing the materializing/scrolling underneath it.
- **`CollectionBodyWidget`/`apply_row_click`/`SelectionState` don't apply
  either** — those exist to serve `list`/`data_grid`'s multi-select,
  tree-nav, per-row-focus model. None of that machinery serves a
  single-highlighted-index, single-focus-target list.

What genuinely is reusable: **masonry's `VirtualScroll` widget itself**
(`masonry_core::widgets::VirtualScroll`, re-exported via `masonry::widgets`),
used directly rather than through `collection`'s `CollectionBodyWidget`
wrapper. Its real imperative API (verified against the pinned commit,
`masonry/src/widgets/virtual_scroll.rs` at the `c5950bc` checkout
`Cargo.lock` resolves to):

- `VirtualScroll::new(initial_anchor: usize, len: usize) -> Self`; runtime
  resize via `VirtualScroll::set_len(this: &mut WidgetMut<'_, Self>, len: usize)`
  — no reconstruction needed when the item count changes.
- `VirtualScroll::add_child(this: &mut WidgetMut<'_, Self>, idx: usize, child: NewWidget<dyn Widget>)` /
  `remove_child(this: &mut WidgetMut<'_, Self>, idx: usize)` — both
  `debug_assert!` that `will_handle_action` was already called for the
  in-flight `Fetch` action; calling either outside that reaction is a
  contract violation.
- `VirtualScroll::will_handle_action(this: &mut WidgetMut<'_, Self>, action: &VirtualScrollFetchAction)`,
  `scroll_to(this: &mut WidgetMut<'_, Self>, idx: usize)`.
- `VirtualScrollAction` has exactly two variants: `Fetch(VirtualScrollFetchAction)`
  (`.old_active() -> &Range<usize>`, `.target() -> &Range<usize>`) and
  `Scroll(VirtualScrollScrollAction)` (`.range_in_viewport() -> &Range<usize>`).
- `children_ids(&self) -> ChildrenIds` and
  `child_mut(this: &mut WidgetMut<'_, Self>, idx: usize) -> WidgetMut<'_, dyn Widget>`.
- **The critical mechanic**: `VirtualScroll` decides materialization itself
  from viewport/overscan geometry during its own `layout()`, and — only when
  the computed range differs from its stored active range — submits
  `VirtualScrollAction::Fetch` as a widget action via `ctx.submit_action`. A
  driver **cannot** proactively call `add_child`/`remove_child` from
  `set_items`; it must react to `Fetch` as it bubbles up. Masonry's `Widget`
  trait has an `on_action` hook for exactly this
  (`core/widget.rs:218-225`: `fn on_action(&mut self, ctx: &mut ActionCtx<'_>, props: &mut PropertiesMut<'_>, action: &ErasedAction, source: WidgetId)`;
  unhandled actions bubble further up the tree) — any plain widget ancestor
  can catch a child's action this way, no View layer required. This is
  exactly the mechanism `SuggestionList::on_action` already uses today
  (`widget.rs:197-218`) to re-emit `LabelList`'s `SuggestionSelected` from its
  own id.
- Resizing alone (`set_len`) does **not** itself add/remove anything or
  refresh already-materialized rows' *content* — only a `Fetch` reaction does
  that for rows entering/leaving the window. A same-length `set_items` call
  (new keystroke, same filtered count) triggers no `Fetch` at all, so
  `CollectionListWidget::set_items` must explicitly refresh every
  currently-materialized row's content itself in that case — nothing else
  will.

## Design

### New: `src/collection/imperative_list.rs` — `CollectionListWidget<Item>`

A `pub(crate)` masonry `Widget`, generic over `Item: Clone + Send + Sync +
'static` (both current call sites use `ArcStr`). Ports `LabelList`'s model
forward, backed by `VirtualScroll` instead of an unbounded `Vec<WidgetPod<_>>`:

- Fields: owned `WidgetPod<VirtualScroll>` child; `items: Vec<Item>` (the full
  list — needed to resolve content for newly-materialized rows and to compute
  `highlighted`'s wrap bound); `item_rects: Vec<Rect>` for the *currently
  materialized* window (built at layout, same hit-testing approach
  `LabelList`/`MenuContent` already use); `hover_index: Option<usize>`;
  `highlighted: Option<usize>`; `theme: Theme`; a stored `render_row: Arc<dyn Fn(&Item, bool, &Theme) -> NewWidget<W> + Send + Sync>`
  (the `bool` is "is this row highlighted," matching `RenderRow`'s existing
  selected-flag convention elsewhere in `collection`).
- `accepts_focus() -> true` — **this widget itself is the single focus
  target**, exactly like `LabelList` today. Item rows are plain,
  non-interactive, non-focusable content widgets.
- `set_items(this: &mut WidgetMut<'_, Self>, items: Vec<Item>)`: stores the
  new `Vec`; calls `VirtualScroll::set_len` if the length changed; clamps
  `highlighted`/`hover_index` past the new end (reusing
  `clamp_scroll_index`'s pattern — see edge cases); then, regardless of
  whether length changed, **refreshes every currently-materialized row's
  content** in place (walk `children_ids()`, rebuild each via `render_row`) —
  the length-unchanged case that `set_len` alone won't trigger a `Fetch` for.
- `set_theme`, `set_selected_theme` forwarded to materialized rows, mirroring
  `LabelList::set_theme`.
- `move_highlight(this: &mut WidgetMut<'_, Self>, delta: isize)`: ports
  `LabelList::move_highlight` (`widget.rs:404-424`) essentially verbatim —
  `rem_euclid` wrap over `self.items.len()`, not just the materialized count.
- `set_highlight(this: &mut WidgetMut<'_, Self>, index: Option<usize>)`: sets
  `highlighted`, requests a repaint/accessibility update. Scroll-into-view for
  the new highlight uses **index arithmetic** (`index * row_height`) rather
  than reading `item_rects` (which only covers the current window) — every
  row in this substrate is fixed-height, the same assumption
  `RowClickable::request_scroll_by_rows` relies on elsewhere in `collection`
  — then calls `VirtualScroll::scroll_to` if the target isn't already
  materialized, or `ctx.request_scroll_to` directly if it is.
- `on_action`: catches `VirtualScrollAction` from its `VirtualScroll` child.
  On `Fetch`: `will_handle_action`, diff `old_active()`/`target()` via
  `remove_child`/`add_child` (building new rows from `self.items[idx]` via
  `render_row`), rebuild `item_rects` for the new window, mark handled (does
  not bubble further). `Scroll` needs no reaction here (viewport-only
  movement within the already-materialized window).
- Click routing: `on_pointer_event` hit-tests via `item_rects` exactly as
  `LabelList`/`MenuContent` already do (hover tracking, primary-up on a hit
  row submits this widget's own selection action carrying the item — mirrors
  `SuggestionSelected(ArcStr)`'s "carry the value, not a stale index"
  rationale).
- Keyboard: `on_text_event` ports `LabelList::on_text_event`'s
  ArrowDown/ArrowUp (→ `move_highlight`), Home/End (→ `set_highlight(Some(0))`/
  `set_highlight(Some(len-1))`), Enter (submit selection for `highlighted`).
  Escape/Tab-interception and refocus-to-input/request-close plumbing stay
  with the owning `SuggestionList`/`MenuContent`, not this widget — see below.
- Accessibility: `accessibility_role() -> Role::ListBox`; `set_active_descendant`
  when `highlighted` is `Some`, exactly as `LabelList` does today.

No `RowClickable`, `CollectionBodyWidget`, `SelectionState`, or
`apply_row_click`/`apply_row_activate` involved — none of that machinery
serves this single-focus-target model. `clamp_scroll_index`
(`src/collection/scroll.rs`) is reused for the highlighted-index-past-the-end
case.

### New: `src/collection/item_row.rs` — `OverlayListItem`

Replaces `SuggestionItem` and gives `MenuContent`'s bare `Label` rows the same
shape. Both are, today, a paint-only wrapper around a `Label` — `SuggestionItem::paint`
is already a no-op ("purely structural, the inner Label paints itself";
`autocomplete/widget.rs:863-870`), with all hover/highlight painting done by
the *parent* (`LabelList::paint`, from `item_rects`/`hover_index`/`highlighted`)
— so unifying is a small, low-risk change: `MenuContent` gains the same thin
wrapper `SuggestionItem` already has. A single
`render_overlay_list_item(item: &ArcStr, highlighted: bool, theme: &Theme) -> NewWidget<OverlayListItem>`
function is the `render_row` closure both `SuggestionList` and `MenuContent`
pass to their `CollectionListWidget`. `CollectionListWidget` itself keeps
owning the actual hover/highlight paint (against its own `item_rects`), same
division of responsibility as today.

### Rewritten: `SuggestionList` (`src/components/autocomplete/widget.rs`)

`LabelList` is deleted; `SuggestionList` wraps `CollectionListWidget<ArcStr>`
directly, **dropping the intermediate `ScrollView`** — `VirtualScroll` is
itself a scrollable viewport (same as `list`/`data_grid`'s usage), so the
extra `ScrollView<LabelList>` layer `SuggestionList` uses today
(`widget.rs:128-153`) is no longer needed. `SuggestionList` keeps: its chrome
(`paint`'s rounded-rect background/border), its `MAX_LIST_HEIGHT` (200px)
measure cap, its `on_action` re-emission for portal routing (now catching
`CollectionListWidget`'s selection action instead of `LabelList`'s), and the
`text_area_handle`/`autocomplete_handle` state currently owned by `LabelList`
(needed for `refocus_input`/`request_close` — relocates to `SuggestionList`
since `LabelList` no longer exists to hold it) plus the Escape/Tab
interception currently in `LabelList::on_text_event`.

### Rewritten: `MenuContent` (`src/components/dropdown_button/menu_layer.rs`)

Rewritten around `CollectionListWidget<ArcStr>` the same way. Gains a height
cap (`MAX_LIST_HEIGHT`, matching autocomplete's value) and real scrolling for
the first time — previously `measure()` sized unbounded to
`item_height * item_count`. `MenuItemSelected(usize)` stays the emitted
action shape (index-based, as today); the index is resolved against
`CollectionListWidget`'s stored `items` at the point of selection, same
timing guarantee `apply_row_activate` elsewhere in `collection` relies on
(resolve at the moment of the click, not deferred).

### Not touched

`collection_body`, `CollectionBodyWidget`, `RowClickable`, `SelectionState`,
`apply_row_click`/`apply_row_activate`, `list`, `data_grid` — none of this
substrate is used by the new widget. `ThemedDropdownButton`'s in-tree
fallback path (no portal scope ancestor — `widget.rs:178-208`) still
constructs `MenuContent` directly as a permanent child of the trigger and
calls its `set_items`; that call site's shape doesn't change, only what's on
the other end of it.

## Data flow

1. **Population.** `AutocompleteWidget`/`MenuContentView`/`ThemedDropdownButton`
   compute the full filtered/static `Vec<ArcStr>` (no longer capped at 20 for
   autocomplete) exactly as today, then call `CollectionListWidget::set_items`
   (same call sites that currently call `LabelList::set_items`/
   `MenuContent::set_items`). Length-changed: `VirtualScroll::set_len`, then
   whatever `Fetch` that triggers is handled reactively via `on_action`.
   Length-unchanged: every currently-materialized row's content is refreshed
   directly (no `Fetch` fires for this case).
2. **Keyboard nav.** `CollectionListWidget` itself is focused throughout;
   ArrowDown/Up call `move_highlight` (wraps via `rem_euclid` over the full
   item count — no edge case, no substrate coordination needed). Highlight
   change requests scroll-into-view by index arithmetic, re-anchoring
   `VirtualScroll` if the target isn't already materialized.
3. **Click/select.** Pointer press hit-tests `item_rects`; a hit on
   primary-up submits this widget's own selection action carrying the item
   value, caught by the owning `SuggestionList`/`MenuContent`'s `on_action`
   and translated/forwarded exactly as today.
4. **Virtualization.** `VirtualScroll` computes its own materialized range
   during `layout()` and submits `Fetch` only when that range changes;
   `CollectionListWidget::on_action` reacts by adding/removing rows and
   rebuilding `item_rects`, never proactively.

## Edge cases / error handling

- **Empty item list.** `set_items(vec![])`: `set_len(0)` removes all
  materialized rows via the resulting `Fetch`; `highlighted`/`hover_index`
  clamp to `None`.
- **Highlighted/hovered index past the new list's end** after items shrink.
  Clamp via the same pattern `clamp_scroll_index` already provides.
- **Rapid re-`set_items` calls** (fast typing). Each call is a complete,
  synchronous content refresh of the current window plus (if needed) a
  `set_len` — no stale-state risk, matching `LabelList::set_items`'s current
  drain-and-rebuild guarantee.
- **`Fetch` reaction ordering.** `will_handle_action` must run before any
  `add_child`/`remove_child` for that action (masonry's own
  `debug_assert!`s enforce this) — the plan must get this ordering right in
  `on_action`, not just "eventually call both."
- **Highlight moved to an unmaterialized target** (a large list, highlight
  jumps beyond the current window via Home/End or fast repeated arrow
  presses). `set_highlight`'s scroll-into-view must use index arithmetic
  (not `item_rects`, which doesn't cover the target yet) and call
  `VirtualScroll::scroll_to` to bring it into the materialized window before
  the accessibility/active-descendant update references it.
- **`MenuContent`'s new height cap** changes previously-unbounded-height
  dropdowns to a capped, scrollable viewport — a visible behavior change,
  flagged here explicitly rather than as an incidental side effect.

## Testing plan

- **Existing autocomplete test suite** must keep passing after relocating
  `LabelList`'s state/logic into `SuggestionList` + `CollectionListWidget` —
  except **`tab_into_listbox_and_arrow_keys_set_active_descendant`
  (`widget.rs:2423-2518`) and `enter_in_listbox_selects_closes_and_returns_focus_to_input`
  (`widget.rs:2620-2643`) need adaptation, not just a fixture swap**: both
  assume `LabelList` is the focus target and hold
  `text_area_handle`/`autocomplete_handle` directly — after the rewrite,
  `CollectionListWidget` is the focus target (same role, same
  active-descendant mechanism, just relocated) and `SuggestionList` holds the
  handles. The *assertions* (Tab reaches a `Role::ListBox`, active-descendant
  tracks highlight, Enter refocuses the input) should hold unchanged in
  spirit; only the fixture wiring changes. `compute_filtered`'s existing
  tests need updating for the removed `MAX_SUGGESTIONS` cap.
- **New `CollectionListWidget` tests** (`src/collection/imperative_list.rs`):
  `set_items` diffing (grow/shrink/replace, both same-length-content-refresh
  and length-changed-via-Fetch paths exercised separately), `move_highlight`
  wrap-around (already-correct logic, ported — port its existing implicit
  coverage forward, no *new* wrap behavior to invent), highlight-past-the-end
  clamp-on-shrink, scroll-into-view when the highlight jumps to an
  unmaterialized target (the one genuinely new scroll case — mirroring
  `body.rs`'s `arrow_down_past_the_viewport_edge_scrolls_the_new_focus_into_view`-style
  assertions), and the `Fetch`-reaction ordering (`will_handle_action` before
  add/remove).
- **New `MenuContent` height-cap test**: menu with many items measures to
  `MAX_LIST_HEIGHT`, not `item_height * item_count`.
- **Manual gallery verification** (defer to the human — no claimed visual
  verification): run the gallery's autocomplete and dropdown_button demo
  panels with a large candidate list (hundreds of items) and confirm bounded
  materialized widget count via a test harness assertion on
  `VirtualScroll::children_ids().len()` (or `len()`) after `set_items` with a
  large `Vec`, or the `profiling` feature/Tracy.

## Acceptance criteria (from #98, revised)

- [ ] Autocomplete + dropdown_button overlay lists virtualize (bounded widget
      count regardless of item count) — verified with `MAX_SUGGESTIONS`
      removed, so this is a real test of unbounded input, not masked by the
      existing cap.
- [ ] `SuggestionList` and `MenuContent` share the virtualization/hover/
      highlight/scroll-into-view/click substrate (`CollectionListWidget` +
      `OverlayListItem`) rather than duplicating it.
- [ ] Existing autocomplete + dropdown_button behavior/accessibility tests
      still pass (with the two noted adaptations, not behavior changes).
- [ ] Keyboard-highlight wrap-around at list ends is preserved (already
      correct today; ported forward, not reimplemented).
- [ ] `dropdown_button`'s menu gains a bounded, scrollable viewport
      (previously unbounded height) — a new, intentional behavior change.
