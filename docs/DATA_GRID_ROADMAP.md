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

## Done (baseline — Chunks 1–4)

- Row virtualization over `&[R]` (very large/append-only streams)
- Row selection (anchor + shift-range + ctrl/cmd-toggle), source-indexed
- TSV clipboard copy of the selection (Ctrl/Cmd+C)
- **Single-column sorting**: click-to-cycle asc→desc→off, ▲/▼ arrow,
  hover affordance, numeric-correct ordering, selection stable across sorts
- Grid fills its container height; one-shot overflow warning
- Rich (widget-returning) cell renderers — partial "templates"

## Prioritized backlog

Each item: **value justification (≤1 sentence)** · rough size · depends-on · North Star.

### Tier 1 — next up (highest value / unblockers)

1. **`data_grid` builder refactor** — Collapse the 8-arg constructor into
   a builder so every later feature is an additive, readable call instead
   of another positional parameter. · S–M · — · Kendo *Configuration*.
   *(Clears the current `#[expect(too_many_arguments)]`.)*
2. **Column filtering** — Narrowing thousands of symbols to the ones a
   user cares about is the single highest-value "find my data" feature.
   · L · builder · Kendo *Filtering* (reuse `checkbox` / a text field).
3. **Conditional cell formatting** (e.g. gain=green/loss=red, number
   formats) — Color and number formatting are how a finance user reads
   sign and magnitude at a glance, and the cell renderer already supports
   it, so it's high value at low cost. · S · — · IronCalc, Kendo *Appearance/Templates*.

### Tier 2 — layout & navigation for wide tables

4. **Horizontal scroll + column resize** — Financial tables are wide
   (price, Δ, %Δ, vol, bid/ask…); users must scroll and resize columns to
   fit their screen. · L · — (resolves fixed-width limit) · Kendo
   *Columns / Scroll Modes*, Longbridge table.
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
