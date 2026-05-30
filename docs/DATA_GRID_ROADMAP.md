# `data_grid` Roadmap

A living, prioritized plan for growing the generic `data_grid` component
(`src/components/data_grid/`). We refer to this the way we used the
Chunk 1–4 checklist: pick the next bite-sized, independently-reviewable
checkin from the top, verify, commit, repeat.

## Ground rules (unchanged)

- **100% generic & presentation-only.** No business logic, no network,
  no persistence, no market-data types in the component. State comes in
  via lenses/snapshots; events go out. (See `CLAUDE.md`.)
- **Value lens:** we *pretend* the eventual consumer is a financial
  chart / stock viewer (Symbol, Last, Δ, %Δ, Volume, Bid/Ask, …) and
  prioritize accordingly — but nothing financial leaks into the API.
- **Persistence stays out:** where a feature implies "remembering" a
  layout, the component *exposes* serializable state and the host
  persists it.

## North Stars

- Kendo UI Grid capability list (the canonical checklist) —
  <https://www.telerik.com/kendo-jquery-ui/documentation/controls/grid/overview>
- Longbridge `gpui-component` (Rust/GPUI table & gallery) —
  <https://github.com/longbridge/gpui-component> ·
  <https://longbridge.github.io/gpui-component/gallery/>
- IronCalc (spreadsheet engine — cell/number formatting, data semantics) —
  <https://www.ironcalc.com/>

## Done

- Row virtualization over `&[R]` (very large/append-only streams)
- Row selection (anchor + shift-range + ctrl/cmd-toggle), source-indexed
- TSV clipboard copy of the selection (Ctrl/Cmd+C)
- **Single-column sorting**: click-to-cycle asc→desc→off, ▲/▼ arrow,
  hover affordance, numeric-correct ordering, selection stable across sorts
- Grid fills its container height; one-shot overflow warning
- Rich (widget-returning) cell renderers — partial "templates"
- **Fluent `DataGrid` builder** (`new` + chained setters + `.render`),
  boxed lenses, optional selection/sort/filter — replaced the wide free
  fn and cleared the `too_many_arguments` debt *(was Tier 1.1)*
- **Column filtering** (host-filters / grid-drives-UI): `FilterState` +
  `filtered_indices` + per-column `filterable_by_text`; an in-grid
  per-column filter-input row; a persistent accent + ● marker on
  filtered columns so a filtered view is never mistaken for the full
  set *(was Tier 1.2)*
- **Conditional cell formatting**: `colored_text_column(fmt, color)` —
  per-row label color from `(&R, &Theme)`, theme-aware across variants
  (demo Side column: buys green / sells coral) *(was Tier 1.3)*

## ⚠️ ARCHITECT REVIEW REQUESTED

- **`DataGrid::render` returns `impl WidgetView<State, ()>`, not a named
  view type.** Every *other* component's `render` returns a concrete
  named view (`ButtonView`, `CheckboxView`, …). The grid differs because
  it is a *composition* (CopyOnShortcut → OverflowWarn → flex_col → …)
  rather than a wrapper around a single widget, so a named `DataGridView`
  would be an awkward, churn-prone alias. **Decision needed:** accept the
  `impl Trait` return for the grid, or introduce a named `DataGridView`
  for strict parity with the other components. *(The entry-point gap —
  missing a `data_grid()` free-fn constructor — has been fixed to match
  the `button()`/`checkbox()` convention.)*

## Deferred polish (tracked, not yet scheduled)

- **Filter-input discoverability** — make the filter row obviously a
  filter affordance (placeholder/funnel glyph or distinct look).
- **Global UI zoom/scaling** — a theme/density-driven scale applied
  across *all* components (supersedes component-local size constants,
  e.g. the filter input). Cross-cutting, not a data_grid-only change.
- **Filter UX**: shift-extend selection + clipboard copy currently use
  source-index / source order, not the on-screen order (see module
  "Known limitations").

## Prioritized backlog

Each item: **value justification (≤1 sentence)** · rough size · depends-on · North Star.

### Tier 1 — highest value / unblockers

*(Numbering is stable so later `depends-on` refs stay valid.)*

1. ✅ **DONE — `data_grid` builder refactor.** (See Done.)
2. ✅ **DONE — Column filtering.** (See Done.)
3. ✅ **DONE — Conditional cell formatting.** (See Done.)

### Tier 2 — layout & navigation for wide tables

4. **← NEXT: Horizontal scroll + column resize** — Financial tables are
   wide (price, Δ, %Δ, vol, bid/ask…); users must scroll and resize
   columns to fit their screen. · L · — (resolves fixed-width limit) ·
   Kendo *Columns / Scroll Modes*, Longbridge table.
5. **Column pin / freeze** — Keeping the Symbol/identifier column frozen
   while metric columns scroll is essential for wide quote tables. · M ·
   #4 · Kendo *Columns (locked)*.
6. **Column show/hide + reorder** — Traders curate which metrics they
   watch; show/hide + reorder lets them build their own layout. · M ·
   builder, #4 · Kendo *Columns*.

### Tier 3 — analysis

7. **Aggregates (column summary + selection aggregates)** — Sum/avg of
   volume (or of the selected rows) gives instant totals. · M · — · Kendo
   *Selection Aggregates*.
8. **Multi-column / tiebreak sort** — Sort by sector then %Δ; a natural
   extension now that single-column sort exists. · S–M · sorting · Kendo
   *Sorting*.
9. **Grouping** — Group by sector/exchange to scan categories; valuable
   but heavier and less core than filtering. · L · sorting · Kendo *Grouping*.

### Tier 4 — convenience & persistence

10. **State export for host persistence** — Expose sort/filter/column
    layout as serializable state so the host can save a user's view
    (component stays persistence-free). · S–M · #2,#6 · Kendo *State Persistence*.
11. **Context menu** — Right-click for sort/hide/pin/copy speeds
    power-user workflows. · M · `floating` primitive · Kendo *Context Menu*.
12. **Export visible/all data (CSV)** — Beyond selection-copy, export the
    current view for offline analysis. · S · clipboard work · Kendo *Export*.
13. **Keyboard navigation** (arrows, page up/down, type-ahead) — Keyboard
    -first navigation is an accessibility and power-user win. · M · — ·
    Kendo *Accessibility*.

### Tier 5 — lower priority for a *viewer*

14. **Master-detail / hierarchy** — Expand a symbol to reveal depth/quote
    detail; useful but secondary. · L · · Kendo *Hierarchy*.
15. **Row drag & drop · row/column spanning · row pinning · chart
    integration** — Niche or redundant for a virtualized financial viewer;
    revisit on concrete need.

### Non-goals (for now) — the grid is view-only

- **Data editing / inline editing** — Deliberately **not planned**. The
  grid presents data and never mutates the host's rows; a stock/quote
  *viewer* doesn't edit its data. Inline editing stays a *possible future
  capability* only if a downstream consumer explicitly needs it (e.g. an
  order-entry or IronCalc-style spreadsheet product) — at which point it
  re-enters the backlog. Until then, we don't carry the complexity.

### Explicitly out of scope (for now)

- **Paging** — we virtualize instead; paging is the wrong model for live
  streaming financial data.
- **Init-from-HTML-table** — web-specific; N/A for a xilem/masonry widget.

## How we work

Bite-sized, individually-verifiable checkins to the team branch
(`cargo test` + `cargo clippy --all-targets` + gallery check green before
each commit). One feature may span several micro-commits.

**Strong foundation upon strong foundation.** Build the foundational /
enabling pieces first, and test the hell out of everything: every
ordering, filtering, and aggregation rule gets thorough unit tests; every
checkin verifies no regression in the gallery before it lands. We don't
stack a new feature on an unverified one.
