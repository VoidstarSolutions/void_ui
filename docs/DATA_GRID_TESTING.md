# DataGrid — tester's guide

A click-here / expect-this script for reviewing the `data_grid` component.
Everything is exercised through the live gallery; no code reading required.

```sh
cargo run -p void-ui --example gallery --features gallery
```

Two panels in the left rail drive the grid:

- **Data Grid** — a streaming tick blotter (100 000 synthetic rows).
  Exercises raw capability: virtualization, the wide-table features, live
  appends.
- **Stock Quotes** — a static NASDAQ-style quote board (27 symbols).
  The "value lens": the same generic grid as a real product surface.

Also run the automated suite (unit + doc tests, must be green):

```sh
cargo test
cargo clippy --all-targets    # workspace denies clippy::pedantic
```

---

## Data Grid panel

### Virtualization / scrolling
1. Scroll the body vertically. **Expect:** smooth scroll through 100 000
   rows; only visible rows are built (it stays responsive). In a *debug*
   build mild lag at 100k is expected — re-check with
   `cargo run --release --example gallery --features gallery` if in doubt.
2. **Add 100 ticks** / **Add 10k ticks** buttons append rows live.
   **Expect:** row count (top-right) grows; scroll extent updates.

### Selection (keyed by stable row id)
3. Click a row. **Expect:** it highlights.
4. Shift-click another row. **Expect:** the inclusive on-screen range
   selects.
5. Ctrl/Cmd-click rows. **Expect:** individual rows toggle in/out.
6. **Ctrl/Cmd+C** with a selection. **Expect:** TSV of the selected rows is
   on the clipboard, in display order — paste into a spreadsheet/editor to
   confirm columns line up.
7. **Select 0..50** / **Clear selection** buttons. **Expect:** bulk select
   of the first 50 visible rows / clears.
8. **The key guarantee:** select a row, then **sort** a column (below).
   **Expect:** the *same row* stays selected as it moves — selection
   follows the row, not the slot.

### Sorting — single
9. Click a sortable header (**Time**, **Price**, **Size**, **Side**, **Bid**,
   **Ask**). **Expect:** first click ▲ ascending; second ▼ descending;
   third clears. Sortable headers tint on hover; non-sortable ones
   (Spread, Notional, Exchange, VWAP) don't react — by design (no
   comparator).
10. Sorting is numeric-correct: Price/Size sort by value, not by the
    formatted string (`$9.00` sorts before `$100.00`).

### Sorting — multi-column (tiebreak)
11. Plain-click **Side** (groups buys/sells). Then **Shift+click Price**.
    **Expect:** headers now show `Side ▲ 1` and `Price ▲ 2` (priority
    badges). Within each Side block, rows order by Price.
12. **Shift+click Price** again → its arrow flips (badge `2` stays).
    A third Shift+click drops Price from the sort (Side returns to a lone
    sort, badge clears).
13. Add a third level: Side `1`, Shift+**Time** `2`, Shift+**Price** `3`.
    **Expect:** three numbered levels; within a (Side, Time) tie, rows
    order by Price.
14. Plain-click any header while multi-sorted. **Expect:** collapses back
    to single-sort on that column (badges vanish).
15. **No modifier clash:** Shift+click a *body row* still range-extends the
    selection; Shift+click a *header* sorts. Different regions, no
    interference.

### Filtering (host-side)
16. Type in a column's **Filter** input (e.g. `B` or `S` under Side).
    **Expect:** rows filter live; the filtered column shows a persistent
    accent + ● marker so a filtered view is never mistaken for the full set.
17. Clear the filter input. **Expect:** all rows return.

### Conditional formatting
18. The **Side** column shows `B` green, `S` coral, unknown faint — color
    driven per-row by the value, theme-aware.

### Columns — resize, show/hide, reorder
19. Hover a column boundary in the header → **EwResize cursor**; drag to
    resize. **Expect:** the column resizes live; header/filter/body stay
    aligned.
20. **Hide Notional** / **Show Notional**. **Expect:** the column drops
    out / reappears at the end, cleanly rendered (no blank/zero-width
    column).
21. **Price ←** moves the Price column one slot left each click.
22. **The id-keying payoff:** sort by Price, then **Hide Notional** or
    **Price ←**. **Expect:** the Price sort is preserved across the layout
    change (a positional key would have lost or misattributed it).
23. **Reset cols** restores natural order.

### Theme
24. Open the **⚙ Theme** panel (top-right), switch variant/density.
    **Expect:** the grid re-themes (colors, sizes) with the rest.

---

## Stock Quotes panel — the value lens

A static snapshot, so no live appends; otherwise the same grid.

1. **Renders** 27 NASDAQ symbols × 18 columns (Symbol … Sector). Wide
   enough to **scroll horizontally**.
2. **Conditional color:** Chg / Chg% green for gainers, coral for losers.
3. **The canonical board view:** click **Sector**, then **Shift+click Mkt
   Cap** twice (→ desc). **Expect:** rows group by sector, ranked
   biggest-cap-first within each. `Sector ▲ 1`, `Mkt Cap ▼ 2`.
4. **`—` rendering:** INTC has no P/E (negative earnings); several names
   have no dividend → those cells show `—`. Sorting P/E clusters the `—`s.
5. **Numeric-correct compact values:** sort **Volume** or **Mkt Cap** →
   orders by true value (`3.48T` > `834.0B`), not the `T`/`B`/`M` string.
6. **Filter** by Symbol (`AAPL`) or Sector (`Tech`).
7. **Hide Beta / Sector ← / Reset cols** — same column ops as the blotter.
8. **Selection + copy** work identically (rows keyed by ticker).

---

## Known limitations (by design — not bugs)

- **Read-only.** No cell editing; the grid presents data and emits events.
- **Shift-extend when the anchor is off-screen** (filtered out) degrades to
  a single select — there's no on-screen range to span.
- **Column pin/freeze** (frozen identifier column) is **not** implemented —
  deferred deliberately; see `docs/DATA_GRID_ROADMAP.md`.
- The stock board is a **static snapshot**, shaped like a `yfinance` row;
  swapping in a live source is a data-layer change, not a grid change.

## If something looks wrong

- Confirm you're on a **fresh** gallery build (close stale windows; the
  process can outlive a rebuild).
- Multi-sort tiebreakers are only *visible* on columns with real ties
  (low-cardinality like Side/Sector, or equal values). A unique-valued
  primary leaves nothing for lower levels to reorder — that's correct.
