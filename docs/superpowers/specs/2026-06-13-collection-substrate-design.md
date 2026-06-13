# Collection substrate: unifying `list` and `data_grid`

**Date:** 2026-06-13
**Status:** Approved design — pending implementation plan
**Branches:** base = `collection-substrate`, branched off `feature/data-grid-scroll-to-index` (PR #71 — `main` does not yet have `data_grid`'s scroll-to-index feature, which this work unifies); `data_grid` refactor and `list` rebuild stack on top. When PR #71 lands on `main`, rebase the base onto `main`.

## Problem

`list` (PR #58) is, by its own documentation, "a stripped-down `data_grid`."
Today it re-implements — near-verbatim — a substantial amount of `data_grid`'s
row-virtualization machinery rather than sharing it. The two components carry
two hand-synced copies of:

| Concept | `list` | `data_grid` |
|---|---|---|
| Id source (`Explicit`/`Position`, hand-written `Clone`, `id_of`) | `ItemIdSource` (`list/body.rs`) | `RowIdSource` (`data_grid/view.rs`) |
| `visual_range_ids` + its 4 unit tests | `list/view.rs` | `data_grid/view.rs` |
| Shift/toggle/replace selection-click logic | `apply_row_click` (`list/view.rs`) | inline (`data_grid/view.rs`) |
| Scroll-to-anchor view wrapper (generation-tracked) | `ListBodyView` (`list/body.rs`) | `ScrollToView` (`data_grid/scroll.rs`) |
| `clamp_scroll_index` | `list/body.rs` | `data_grid/scroll.rs` |
| `scroll_range_end`, `scroll_idx_to_slice`, `position_fallback_id` | `list/body.rs` | `data_grid/view.rs` |

These copies will drift: the first time someone fixes a selection bug in one and
not the other, the components diverge silently. Separately, both components do
avoidable **per-element work** on every rebuild (see "Performance", below), which
the duplication makes harder to fix in one place.

## Goal

Extract the shared row-virtualization machinery into a single crate-internal
substrate that both `list` and `data_grid` build on as co-equal siblings.
`data_grid` does **not** depend on `list`'s public API. The substrate is carved
*from* `data_grid`'s proven implementations (the more mature, better-tested of
the two); `list`'s fresh copies are then deleted and rebuilt on the substrate.

## Non-goals / explicitly out of scope

- **No forced reuse of `resizable` or `separator`.** The library's `resizable`
  splits two *panels* with a draggable bar; column resize is a different
  interaction (grab zones, min-width clamping, authoritative-x layout shared
  across header/body/filter). The library's `separator` is a standalone divider
  view; the grid's column boundaries are painted inside `column_strip` with
  hover/drag state. Both stay as they are. (The `column_strip`-local
  `SeparatorStyle` name shadows the library `Separator` — noted, not changed
  here.)
- **`scroll_container` is already reused** by `data_grid` for horizontal scroll
  and is not in scope to change. `list` is vertical-only and does not need it.
- Column geometry, header, sort, filter, TSV copy stay in `data_grid`. Search
  input and spinners stay in `list`.

## Architecture

### Layering

A new **`pub(crate) mod collection`** (final name to confirm during
implementation; `virtual_collection` is the alternative) at the crate root,
owning the single source of truth:

- **Public types** (relocated here; re-exported from `data_grid`, `list`, and
  the crate root so no public name changes): `SelectionState`, `ScrollState`.
- **Crate-internal plumbing**: `IdSource<Item>` (`Explicit` | `Position`),
  `visual_range_ids`, `clamp_scroll_index`, the index/range math
  (`scroll_range_end`, `scroll_idx_to_slice`, `position_fallback_id`), the
  selection-click application, and the unified **body** (xilem `View` +
  masonry `Widget`).

`data_grid` and `list` become siblings on it. `list` stops importing from
`data_grid`.

The substrate module is **not public**: its only consumers are in-crate, and
downstream apps consume components rather than assembling masonry-level
primitives. If a third sibling (tree, kanban) ever needs it, it is made public
*then*, not speculatively.

### The body seam (closure-based)

The substrate exposes one builder:

```text
collection_body(
    item_count,
    id_source,                 // IdSource<Item>
    selection: Option<SelectionLens<State>>,
    scroll: ScrollState,
    lazy: Option<(threshold, Fn(&mut State))>,
    row_key: Option<Fn(&Item) -> K>,   // K: PartialEq + Clone + 'static — see Performance
    render_row: Fn(&mut State, idx) -> AnyWidgetView<State>,   // content only
) -> impl WidgetView<State>
```

`render_row` returns **only the row content**. The substrate wraps it in
`clickable_row`, and drives virtualization, scroll-to-anchor, lazy-load, and
arrow-key navigation. `list` passes content = one item view; `data_grid` passes
content = a `column_strip` of cells — column geometry is captured entirely
inside the grid's own closure, invisible to the substrate. The substrate never
learns about columns; the grid never re-implements virtualization.

**Row height stays component-side** (list wraps content in
`sized_box.fixed_height`; the grid's `column_strip` owns its row height) — it is
not part of the seam.

`data_grid`'s horizontal-scroll wrap is unaffected: it continues to
`assemble_grid_stack(header, filter, body)` and wrap the whole stack in a
horizontal-only `scroll_container`. The substrate owns only the `body` slot;
vertical virtualization stays inside it.

### Selection-click logic centralized

The shift/toggle/replace logic plus `visual_range_ids` move into the substrate
as one generic function (`apply_row_click`) over `IdSource` + the items
accessor — a single source of truth both components call.

**What is *not* centralized (and why):** the original design called for the
body *widget* to own click routing so individual rows would stop capturing the
selection lens / id source / items slice and stop boxing a per-row closure.
That was not implemented. `collection_body` still wraps each visible row in
`clickable_row` with a per-row closure that captures three `Arc` clones and
invokes the shared `apply_row_click`. Routing clicks centrally would require
`CollectionBodyView` to recover the clicked row's index by decoding
`virtual_scroll`'s **private** child-id encoding (`view_id_for_index` /
`index_for_view_id` are not exported) — fragile coupling to an upstream
internal that can change under `cargo update` (we track `main`; see
`CLAUDE.md`). So the per-row allocation is unchanged from today; this is
behavior-preserving, not a regression. Eliminating it folds into the deferred
memoization work, which restructures the per-row builder anyway (see
Performance).

### Keyboard navigation unified

`list`'s `ListBodyWidget` Up/Down arrow-key logic becomes the substrate body
widget. `data_grid` gains arrow-key row navigation (an approved behavior change
— a consistency/accessibility improvement).

## Performance

> DEFERRED: The opt-in memoization below (the `row_key` seam) is deferred to a
> follow-up, where the win can be measured against a real workload. Neither
> `data_grid` nor `list` consumes memoization yet, so the per-row allocation
> described below is **unchanged** on this branch — the once-hoped-for
> "unconditional central-click win" was not delivered (see *Selection-click
> logic centralized* for why). The `row_build_baseline` test captures today's
> cost so the future memoization win can be measured against it.

Both components today, per **visible** row, on **every rebuild** (every frame
state changes): re-borrow the items slice, `get(pos)`, run the id closure, a
`selection.contains(id)` lookup, then `Arc`-clone the selection lens + id source
+ items slice and **box a fresh per-row click closure**. This re-runs for all
visible rows on any state change — changing one row's selection re-runs and
re-allocates all ~40 visible row builders. The shared `apply_row_click` keeps
that logic in one place, but does not by itself remove the per-row clones.

The lever the substrate actually realizes:

1. **Per-row memoization (opt-in).** xilem's `memoize(data, |data| view)`
   requires `Data: PartialEq + 'static` and gives the view closure only
   `&Data` — not `&mut State`. So a memoizable row's render inputs must be
   owned, comparable data, decoupled from the live `&State` borrow. The seam
   takes an **optional** `row_key: Fn(&Item) -> K` (`K: PartialEq + Clone +
   'static`); when supplied, each row is memoized on `(row_key, selected,
   theme)`, so a selection change rebuilds only the rows whose
   key/selection/theme changed. When omitted, behavior matches today.

**Discipline: measure, don't assert.** The `row_build_baseline` test records a
before measurement on a wide grid for per-row build cost. The memoization win is
only *claimed* once an after-measurement shows it.

## Open risks (verify during planning, do not assume)

- **Memoization prototype first.** Memoization interacting with the `&mut State`
  row builder may require the substrate to snapshot row data into the
  `row_key`/`Data`. Prototype one memoized `data_grid` row before finalizing the
  seam's `row_key` shape.
- **TSV/copy path.** `data_grid`'s copy path also re-borrows the slice per
  rebuild. Confirm the substrate refactor does not disturb it.

## Success criteria

- All existing `data_grid` + `list` tests pass unchanged (behavior-preserving,
  except `data_grid` gains arrow-key nav).
- **Zero** duplicated helpers remain: `IdSource`, `visual_range_ids`, `clamp`,
  index math, the scroll-to wrapper, and selection-click each exist exactly
  once.
- `list` no longer imports from `data_grid`.
- `row_build_baseline` records today's per-row cost; the memoization follow-up
  measures its win against that baseline.
- Public API names unchanged (re-exports preserve `SelectionState` /
  `ScrollState` and the component entry points).

## Testing

- Shared unit tests (`visual_range_ids` ×4, `clamp`, scroll-to generation) move
  into the substrate; the duplicates are deleted.
- New substrate tests: central selection-click (shift/toggle/replace),
  arrow-key nav (moved from `list`), memoization (changed key rebuilds; unchanged
  key skips).
- `data_grid`'s and `list`'s existing widget/integration tests stay in place as
  the migration guardrail.

## Branch plan

Three branches; `data_grid` and `list` are siblings stacked on the substrate
base (not on each other).

1. **`collection-substrate`** (base, off `feature/data-grid-scroll-to-index` — PR #71, since `main` lacks the scroll-to-index feature this unifies): the substrate module + the
   relocated `SelectionState` / `ScrollState` (with re-exports keeping the
   public names), plus the substrate's own unit tests. Compiles green on its
   own.
   - *Planning note:* a `pub(crate)` substrate that no component uses yet will
     trip dead-code lints (workspace denies `clippy::pedantic`). The plan must
     decide how the base stays lint-clean before consumers land — e.g. land the
     `data_grid` migration in the same base, or gate the unused surface. Resolve
     this in the implementation plan.
2. **`data_grid` refactor** stacked on the base: migrate `data_grid` onto the
   body seam; delete its now-duplicated helpers. Existing grid tests are the
   guardrail.
3. **`list` rebuild** stacked on the base: delete `list`'s duplicated copies,
   rebuild on the substrate.

Validate all three together, then open the substrate PR with `data_grid` and
`list` stacked on top.
