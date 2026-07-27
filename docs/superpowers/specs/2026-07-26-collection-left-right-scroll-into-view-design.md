# Collection: Left/Right tree nav should scroll the focused row into view

Closes #203 (split from #136 — Up/Down was fixed in PR for #136; this covers the Left/Right half that PR deliberately left out of scope).

## Problem

`CollectionBodyWidget::on_text_event`'s Left/Right tree navigation (`src/collection/body.rs`, `tree_first_child`/`tree_parent`) moves focus to a materialized parent/first-child row via `ctx.set_focus(target)`, but never requests a scroll. A target beyond the current viewport (but still inside `VirtualScroll`'s buffered materialized range) gets focused off-screen with no scroll-into-view.

Up/Down's existing fix (`RowClickable::request_neighbor_scroll_into_view`, `src/collection/row_click.rs`) doesn't drop in here because it relies on the target always being exactly one materialized row away — true for Up/Down and for Right→first-child (a parent's children are spliced immediately after it), but not for Left→parent, which can be an arbitrary number of materialized rows back (depth- and sibling-count-dependent).

## Key insight

The issue's own write-up assumed the widget computing the scroll-into-view request would have to be `CollectionBodyWidget` itself, and got stuck there: `CollectionBodyWidget` isn't positioned at a per-row offset within its own content, and it has no way to read a descendant row's absolute layout rect.

That assumption is avoidable. Masonry's scroll-request mechanism (`EventCtx::request_scroll_to`) walks *up* from the calling widget through its real ancestors, delivering `Update::RequestPanToChild` to each — and `VirtualScroll`, which owns the actual scroll position, is itself the one that handles that event. Up/Down's fix works because the *currently-focused row* (a descendant of `VirtualScroll`) issues the request from its own `EventCtx`, so the walk-up passes directly through `VirtualScroll`.

`CollectionBodyWidget` sits *above* `VirtualScroll`, not below it. If `CollectionBodyWidget` issues `request_scroll_to`, the walk-up starts at `CollectionBodyWidget` and climbs *past* `VirtualScroll` (which is its descendant, not an ancestor) — never reaching the widget that can act on it. That's the actual wall, not a missing masonry API.

The resolution: keep the *row* as the one issuing the scroll request, exactly as Up/Down already does, and generalize the offset from a fixed "±1 row" to "±N rows":

- **Right → first child** is *always* exactly 1 materialized row below (children are spliced immediately after their parent in the flattened row list). This is literally the same target `RowClickable::nav_down` already carries — no new data needed. `RowClickable` already receives `tree_meta` per row (pushed at view-build time, independent of `CollectionBodyWidget`'s row tracking), so it already knows locally whether it's an expanded parent.
- **Left → parent** can be N rows back. `CollectionBodyWidget` already tracks `row_meta` and already does this exact backward scan today (`tree_parent`, by depth) — it just needs to also record how many materialized rows back the match was, and push `(target_id, n)` onto the row, using the same precompute-and-push pattern `refresh_row_nav` already uses for `nav_up`/`nav_down`.

With the target and row-count in hand, the row computes `rect ± n * rect.height()` and calls `request_scroll_to` on its own `ctx` — same mechanism Up/Down uses, generalized to an integer multiplier instead of `±1`.

This closes the gap with no masonry/upstream change (the issue's alternative "give `EventCtx`/`ActionCtx` a way to request scroll-to for a specific descendant `WidgetId`" is unnecessary).

## Design

### `RowClickable` (`src/collection/row_click.rs`)

- Add field `nav_parent: Option<(WidgetId, u32)>` — the Left target and its distance in materialized rows. Defaults to `None` in `new`.
- Add setter `set_tree_parent_nav(this: &mut WidgetMut<'_, Self>, target: Option<(WidgetId, u32)>)`, mirroring `set_nav`'s "cheap, no repaint/relayout" contract.
- Generalize `request_neighbor_scroll_into_view(ctx, down: bool)` → `request_scroll_by_rows(ctx: &mut EventCtx<'_>, row_delta: i32)`:
  ```rust
  fn request_scroll_by_rows(ctx: &mut EventCtx<'_>, row_delta: i32) {
      let rect = ctx.border_box();
      ctx.request_scroll_to(rect + Vec2::new(0.0, f64::from(row_delta) * rect.height()));
  }
  ```
  Up/Down call it with `±1`; Left/Right call it with `±n`. One formula, one thing to get right.
- In `on_text_event`, ahead of the existing `toggles_on` check (toggle keeps priority, unchanged):
  - `ArrowRight`: if not `toggles_on(false)` and `tree_meta` indicates an expanded parent (`has_children && is_expanded`), and `nav_down` is `Some(target)`: `ctx.set_focus(target)`, `request_scroll_by_rows(ctx, 1)`, `ctx.set_handled()`, return. If `nav_down` is `None` (materialized edge), fall through unhandled — identical to today's no-op.
  - `ArrowLeft`: if not `toggles_on(true)` and `nav_parent` is `Some((target, n))`: `ctx.set_focus(target)`, `request_scroll_by_rows(ctx, -(i32::try_from(n).unwrap_or(i32::MAX)))`, `ctx.set_handled()`, return. `None` falls through unhandled, same as today.

### `CollectionBodyWidget` (`src/collection/body.rs`)

- Delete `on_text_event`, `tree_first_child`, `tree_parent` in full — that impl block's only job today is Left/Right handling, and all of it moves to `RowClickable`. Drop now-unused imports (`NamedKey`, `KeyState`, `TextEvent`, `ChildrenIds` if nothing else in the file needs them — verify at implementation time).
- Extend `refresh_row_nav`'s existing per-row loop — which already does the guarded "only touch rows registered as of the *previous* call" `WidgetMut` iteration for `nav_up`/`nav_down` (the #175 same-pass-add guard) — to also compute `nav_parent` from `self.row_meta`:
  - For row `k` (id `ids[k]`), look up its own depth by id in `row_meta` (tolerant of `row_meta` being a filtered subsequence, same tolerance `tree_parent` has today).
  - Scan `row_meta` backward from that position for the nearest shallower-depth entry; if found, record `(target_id, steps_back)` where `steps_back` is the count of materialized rows between the two (not tree-depth difference — matches `tree_parent`'s existing semantics, e.g. skipping a deeper sibling subtree still counts those intervening rows).
  - Push via `RowClickable::set_tree_parent_nav`. Rows with no tracked depth (flat collection, `row_meta` empty) get `None`, same as today's behavior for non-tree collections.

### `body_view.rs`

- Reorder `CollectionBodyView::rebuild`: call `refresh_tree_row_meta` (which calls `CollectionBodyWidget::set_row_meta`) *before* `CollectionBodyWidget::refresh_row_nav`, so the parent-distance computation above sees this rebuild's fresh `row_meta` rather than the previous rebuild's. (Today's order is the reverse, harmless before now since nothing consumed `row_meta` until event time; the reorder makes it strictly more current.)

## Testing plan

### Existing `body.rs` tests need adaptation, not just a touch-up

Several current Left/Right tests exercise `CollectionBodyWidget::on_text_event` directly and won't compile/pass unchanged once that logic moves:

- `right_on_expanded_parent_focuses_first_child`, `right_on_leaf_or_collapsed_parent_does_not_move`, `left_on_child_focuses_its_parent_skipping_deeper_siblings`, `left_on_depth_zero_row_does_not_move`, `left_right_are_no_ops_without_tree_metadata`, `tree_nav_to_an_unmaterialized_target_is_a_no_op` all use `harness_with_rows()` (the bare-`Label` fixture). They must switch to `harness_with_clickable_rows()` (the `RowClickable` fixture Up/Down's tests already use) — the handling logic now lives on the row itself, so the materialized rows must actually be `RowClickable`s.
- The `set_tree` test helper currently only calls `CollectionBodyWidget::set_row_meta`. It must also call `RowClickable::set_tree_meta` per row, since Right's "am I an expanded parent" check now reads the row's own `tree_meta` (production already wires this per-row via each row's view; this is test-fixture catch-up only).
- Each of these tests must add `CollectionBodyWidget::refresh_row_nav` call(s) after `set_tree` (mirroring the double-call pattern the existing Up/Down tests already use), since Right now depends on `nav_down` and Left on the new `nav_parent`, both pushed by that method — today's Right/Left tests don't call it at all, since the old code read `row_meta` live at event time.
- `tree_nav_to_an_unmaterialized_target_is_a_no_op`'s doc comment currently frames the missing-scroll behavior as an accepted, permanent limitation of this substrate. That framing is now wrong and must be corrected: the general N-row case works after this change; this test narrows to specifically cover the boundary case (target beyond the materialized/buffered window), which remains a no-op by design, matching Up/Down's own edge behavior and this issue's acceptance criteria (materialized targets scroll; a target past the buffer is still a no-op).

### New tests

- `row_click.rs`, unit level (isolated `RowClickable`, extending the existing `harness_with_nested_row` pattern): `nav_parent` set → `ArrowLeft` moves focus and is handled; unset → handled no-op. `ArrowRight` with `tree_meta` indicating an expanded parent and `nav_down` set → moves focus and is handled; toggle-eligible cases still take priority over focus-move.
- `body.rs`, integration level (real `VirtualScroll`, multiple materialized rows): a new test analogous to `arrow_down_past_the_viewport_edge_scrolls_the_new_focus_into_view`, but for `ArrowLeft` moving **more than one row** to a parent several materialized rows back, past the viewport edge — this is the actual regression check for the bug (the old code moved focus correctly but never scrolled for multi-row Left jumps).

### Regression check

Existing Up/Down scroll tests (`arrow_down_past_the_viewport_edge_scrolls_the_new_focus_into_view`, `arrow_up_before_the_viewport_edge_scrolls_the_new_focus_into_view`) must keep passing unchanged after the `request_scroll_by_rows` refactor — confirms the shared helper didn't regress the `±1` case.

## Acceptance criteria (from #203)

- [ ] Left/Right tree nav scrolls the newly-focused row into view when it's outside the viewport but within the materialized window.
- [ ] Minimal scroll (reveal), not a jarring pin-to-top — inherited for free from reusing `request_scroll_to`'s existing reveal semantics (the same mechanism Up/Down already uses).
- [ ] No regression to existing Left/Right nav semantics (toggle priority, depth-zero/leaf no-ops, materialized-edge no-ops) — covered by the adapted existing test suite above.
