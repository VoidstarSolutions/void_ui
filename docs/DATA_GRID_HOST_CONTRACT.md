# `data_grid`: the host owns row order (team note)

**TL;DR for consumers:** the grid is presentation-only. *You* (the host)
own the row order and the selection's identity. The grid renders what you
hand it, emits intents (sort-click, filter-edit), and never reorders or
hides data itself. If you wire it up, you implement four small things:
a `rows` accessor, a stable `row_id`, a `sort` callback, and a `filter`
callback. This note explains the contract and why it's shaped this way.

## Why this shape

Sorting and filtering are **host-side and symmetric**. Filtering was
always host-side; as of this change sorting matches it, and selection is
keyed by a stable row id instead of slice position. This was a deliberate
architecture decision (see the commit + the design memo), validated
against three independent reference points that all agreed:

- **Headless web (`TanStack` Table):** `manualSorting`/`manualFiltering`
  → host passes pre-ordered rows; selection keyed by `getRowId`, never by
  array index. Canonical pipeline: **filter → sort → paginate**.
- **Production grids (AG Grid, Kendo):** the server-side row model has the
  host sort/filter and the grid request ordered data; selection persists
  across sort/filter *only* with a stable `getRowId`/`selectedKeys`.
  Index-keyed selection is documented as "static datasets only," and the
  exact failure (selection shifts to the wrong rows after sort/filter) is
  spelled out in Kendo's docs.
- **Rust GUIs (egui, gpui-component, xilem):** ordering is app-side
  universally. xilem's own `virtual_scroll` wants `func(state, i64) ->
  view` — a pure index→row projection over already-ordered state.

It also keeps `void_ui` honest to its charter: comparators, predicates,
and row identity are *domain* concerns that belong with the host's data,
not baked into a product-agnostic component.

## The four host responsibilities

```rust
data_grid(columns)
    // 1. rows: serve the rows IN DISPLAY ORDER. When you've reordered
    //    (filtered and/or sorted) serve the materialized view; otherwise
    //    serve your data directly (zero-copy).
    .rows(|s: &State| if s.view_is_materialized() { &s.visible[..] } else { &s.data[..] })
    .row_count(n)
    // 2. row_id: a STABLE, UNIQUE u64 per row (a DB key or a monotonic
    //    seq assigned at creation). Selection is keyed by this, so it
    //    follows rows across reordering. Omit it ONLY for a static grid.
    .row_id(|r: &Row| r.id)
    .selection(|s: &mut State| &mut s.selection)
    // 3. sort: cycle your SortState and re-derive your ordered view.
    //    The callback receives the clicked column's stable ColumnId.
    .sort(sort_snapshot, |s: &mut State, col: ColumnId| s.cycle_sort(col))
    // 4. filter: set your FilterState and re-derive your view (by ColumnId).
    .filter(filter_snapshot, |s: &mut State, col: ColumnId, query| s.set_filter(col, query))
    .render(&theme)
```

### Composing the view (filter, then sort)

Use the two mirror helpers over an index list, in the canonical order,
then materialize:

```rust
fn refresh_view(&mut self) {
    if !self.view_is_materialized() { self.visible.clear(); return; }
    let cols = columns();
    let mut idx = filtered_indices(&self.data, &self.filter, &cols); // 1. filter
    sort_indices(&mut idx, &self.data, &self.sort, &cols);           // 2. sort
    self.visible = idx.into_iter().map(|i| self.data[i].clone()).collect();
}
```

`view_is_materialized()` is your call: it's `true` when a filter is
active *or* a sort column is set. (See `demo.rs` for the reference host.)

## Columns are keyed by stable id — show/hide + reorder for free

Every column has a stable `ColumnId` (explicit via `ColumnDef::id(..)`,
or derived from its title). **Sort, filter, and width state are keyed by
that id, never by column position** — the column-level analogue of
`row_id`, and the `id`/`colId` model TanStack/AG Grid/Kendo all use. The
`.sort`, `.filter`, and `.on_column_resize` callbacks all hand you a
`ColumnId`.

The payoff: **show/hide and reorder cost you no grid feature** — express
them by *which* `ColumnDef`s you pass and *in what order*:

```rust
// Hide = omit the id; reorder = move it. Keep a Vec<ColumnId> in state.
let cols: Vec<ColumnDef<Row, _>> = your_layout         // Vec<ColumnId>
    .iter()
    .filter_map(|id| all_defs.iter().position(|d| &d.effective_id() == id)
                             .map(|i| all_defs.remove(i)))
    .collect();
data_grid(cols)/* … */
```

Because state is id-keyed, an active sort/filter/width **stays attached
to its column** across the rearrangement — hide an unrelated column and
your Price sort is untouched; move a column and its filter moves with it.
(See `demo.rs`: `arrange_columns`, `column_layout`, `toggle_column`,
`move_column_left`.) In-grid drag-to-reorder and a show/hide menu are
*not* built in — like headless TanStack, they're optional UI you layer
over your own order state.

## What you get for free

- **Selection follows rows** across any sort/filter — it's keyed by id.
- **Shift-extend follows on-screen order**, because the grid resolves the
  visual range from the ordered slice you serve.
- **Clipboard copy comes out in display order** (the grid walks your
  ordered slice), not in some internal index order.

## Gotchas

- **Omitting `row_id`** falls back to slice position as the id — correct
  ONLY for a static, unsorted, unfiltered grid. Supply one the moment you
  sort or filter.
- **Ids must be stable and unique.** Reusing an id for a different row, or
  changing a row's id, will misattribute selection.
- **Shift-extend when the anchor is filtered out** of the current view
  degrades to a single select (there's no on-screen range to span).
- **Column ids must be unique.** Two columns sharing an id share their
  sort/filter/width state; a debug build asserts uniqueness. Ids default
  to the column title, so give same-titled columns explicit `.id(..)`s.
- The grid is **read-only** — no cell editing (by design; see roadmap).

## Pointers

- Module docs: `src/components/data_grid/mod.rs` (the "host owns row
  order" + "stable row id" sections).
- Helpers: `sort::sort_indices`, `filter::filtered_indices`.
- Builder: `view::DataGrid` (`.row_id`, `.sort`, `.filter`,
  `.on_column_resize` — all `ColumnId`-keyed).
- Column identity: `column::ColumnId`, `ColumnDef::id`/`effective_id`.
- Reference host: `demo.rs` (`refresh_visible`, `cycle_sort`,
  `view_is_materialized`, `DemoTick.id`; `arrange_columns`,
  `column_layout`, `toggle_column`, `move_column_left` for show/hide +
  reorder).
- Backlog + decision trail: `docs/DATA_GRID_ROADMAP.md`.
