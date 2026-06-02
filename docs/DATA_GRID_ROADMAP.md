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
- Row selection (anchor + shift-range + ctrl/cmd-toggle), **stable-id keyed**
- TSV clipboard copy of the selection (Ctrl/Cmd+C), **in display order**
- **Single-column sorting**: click-to-cycle asc→desc→off, ▲/▼ arrow,
  hover affordance, numeric-correct ordering, selection stable across sorts
- **Host owns row order (sort + filter unified) + stable-id selection**
  *(resolves architect review #1 + #2)*: the grid is presentation-only and
  renders rows in the order the host serves. Sorting joined filtering on
  the host side — a header click fires the `.sort(state, on_sort)`
  callback (mirror of `.filter`); the host cycles `SortState` and composes
  `filtered_indices` → `sort_indices` to re-derive its view. Selection is
  keyed by a stable row id via `.row_id` (the `getRowId` contract), so it
  follows rows across reordering. Removed the per-frame in-grid sort cache
  (#2: it re-sorted every rebuild) and positional selection keys (#1: they
  pointed at the wrong rows once the filtered slice changed identity).
  Decision rationale + multi-source research recorded in the team note and
  commit; this is the mainstream pattern (AG Grid server-side row model,
  Kendo, `TanStack` `manualSorting`; egui/gpui-component/xilem keep order
  app-side)
- Grid fills its container height
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
- **Horizontal scrolling**: the header+filter+body stack is wrapped in a
  horizontal-only `scroll_container`, so columns wider than the viewport
  are reachable and header/body share the offset automatically; replaced
  the `OverflowWarn` warning *(Tier 2 #4)*
- **Shared column geometry via `ColumnStrip`**: every row (header, body,
  filter) is laid out by a `ColumnStrip` widget that places cells at
  authoritative x-positions from a shared width list (multi-child
  `CollectionWidget` + `ViewSequence`, mirroring `flex_wrap`). Rows align
  *by construction* — fixing the long-standing filter-input drift that
  independent flex-rows caused (they only lined up by coincidence). Cells
  are force-sized via `run_layout`, so even a fill-greedy `text_input`
  can't widen its column. *(Tier 2 #4 foundation)*
- **Drag-to-resize columns**: `ColumnWidths` override model +
  `MIN_COLUMN_WIDTH` clamp. The header `ColumnStrip` *itself* owns the
  resize — it hit-tests a grab zone at each column boundary, draws a
  separator (teal on hover/drag), shows the EwResize cursor, and emits
  the new width via `.on_column_resize` (returning `MessageResult::Action`
  to re-run app_logic). No overlay/handle widgets — same pattern as
  masonry's `Split` owning its bar. Effective widths feed header / filter
  / body / scroll-extent uniformly *(Tier 2 #4)*

## ⚠️ ARCHITECT REVIEW REQUESTED

- **`DataGrid::render` returns `impl WidgetView<State, ()>`, not a named
  view type.** Every *other* component's `render` returns a concrete
  named view (`ButtonView`, `CheckboxView`, …). The grid differs because
  it is a *composition* (CopyOnShortcut → scroll_container → flex_col → …)
  rather than a wrapper around a single widget, so a named `DataGridView`
  would be an awkward, churn-prone alias. **Decision needed:** accept the
  `impl Trait` return for the grid, or introduce a named `DataGridView`
  for strict parity with the other components. *(The entry-point gap —
  missing a `data_grid()` free-fn constructor — has been fixed to match
  the `button()`/`checkbox()` convention.)*

## Deferred polish (tracked, not yet scheduled)

- **Global UI zoom/scaling** — a theme/density-driven scale applied
  across *all* components (supersedes component-local size constants,
  e.g. the filter input). Cross-cutting, not a data_grid-only change.
- ~~**Filter UX**: shift-extend selection + clipboard copy use source
  order, not on-screen order.~~ **RESOLVED** by the host-owns-order +
  stable-id work: both now follow display order. The only residual is
  shift-extend when the *anchor* row isn't in the current (filtered) view
  — it degrades to a single select (see module "Known limitations").
- **Scroll-perf profiling** — mild scroll lag observed at 100K rows in a
  *debug* gallery build. Body still virtualizes (only visible rows
  render), so this is likely (1) the unoptimized build and (2) 12 columns
  × per-row widget rebuilds. Expected use-case max is ~15K rows. **Before
  optimizing:** re-check with `cargo run --release --example gallery`. If
  still laggy in release, profile the row-builder for per-rebuild
  allocations. Not worth optimizing pre-measurement.
- **Clipboard TSV recomputed every rebuild** — `CopyOnShortcutView::
  compute_payload` (`view.rs`) clones the selection and, when non-empty,
  scans all rows in display order to rebuild the TSV string on *every*
  rebuild, though the payload is only consumed on Ctrl/Cmd+C. Cost scales
  with row count × selection size. **Deliberately deferred, not done:**
  consistent with the scroll-perf item above, this is a *measure-first*
  optimization — the empty-selection case (the common one) is cheap, and
  we have no release-build signal that the populated case bites at the
  ~15K expected max. If it shows up, make the payload lazy (recompute on
  the copy event, or gate on a selection-version dirty flag) rather than
  per rebuild. Flagged by the post-merge adversarial review (item "M4").

## Prioritized backlog

Each item: **value justification (≤1 sentence)** · rough size · depends-on · North Star.

### Tier 1 — highest value / unblockers

*(Numbering is stable so later `depends-on` refs stay valid.)*

1. ✅ **DONE — `data_grid` builder refactor.** (See Done.)
2. ✅ **DONE — Column filtering.** (See Done.)
3. ✅ **DONE — Conditional cell formatting.** (See Done.)

### Tier 2 — layout & navigation for wide tables

4. ✅ **DONE — Horizontal scroll + column resize.** (See Done.)
5. ⏸️ **DEFERRED INDEFINITELY — Column pin / freeze.** *(Re-estimated
   L–XL, not M. Deliberately shelved 2026-06-02.)* Investigated to the
   pixel and reverted a spike. **Why deferred:** it's the *only* backlog
   feature that requires a structural change — the current shared
   `Portal` gives header/filter/body their synchronized horizontal scroll
   *for free*; true freeze-during-scroll would have to replace it with a
   custom grid-owned viewport widget that scrolls only non-pinned cells.
   That puts horizontal scroll + scrollbar input + clipping + the
   hard-won cross-row alignment invariant all at regression risk at once,
   and those are visual/interactive (not unit-testable — our 46 tests
   wouldn't catch a scroll-sync or clip break). **Cost/benefit is
   lopsided:** high structural risk to proven features for a *non-
   foundational, leaf* convenience — **nothing else in this backlog
   depends on #5** (verified). The cheap "ColumnStrip reads its own scroll
   offset and counter-translates pinned cells" idea is *impossible*:
   horizontal scroll-translation sits on the ancestor that the `Portal`
   wraps (the whole `flex_col`), not on the strips, and `ComposeCtx`
   exposes no way for a widget to read an ancestor's scroll. **Revisit
   trigger:** if upstream masonry gains a sticky/pinned-child primitive
   (none exists today), this collapses from XL to ~S — reopen then.
   · L–XL · #4 · Kendo *Columns (locked)*.
6. **← NEXT: Column show/hide + reorder** — Traders curate which metrics
   they watch; show/hide + reorder lets them build their own layout.
   Clean fit for the existing builder/widths architecture (no scroll
   coupling, unlike #5). · M · builder, #4 · Kendo *Columns*.

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
