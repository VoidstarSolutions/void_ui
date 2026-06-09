//! Synthetic tick generator + gallery panel for the data-grid demo.
//!
//! The generator is a deterministic random walk seeded at $100 with
//! ~$0.05 step size. Side and size are pseudo-random. The xorshift
//! inline avoids pulling in a dependency for what's essentially a
//! demo fixture; if a real RNG is needed elsewhere, switch this to
//! `rand` and feed it a fixed seed.
//!
//! Public surface:
//!
//! - [`Demo`] — state struct (ticks + selection + RNG state).
//! - [`tick_columns`] — column descriptors for browsing [`DemoTick`]s.
//! - [`panel`] — returns the `DataGridDemoPanel` view for the gallery.
//!
//! Types here are intentionally self-contained — `void_ui` ships as a
//! generic component library and must not depend on any
//! market-data crate.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, FlexExt as _, FlexSpacer, flex_col, flex_row, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::column::{
    CellAlign, ColumnDef, ColumnId, colored_text_column, optional_text_column, text_column,
};
use super::filter::{FilterState, filtered_indices};
use super::selection::SelectionState;
use super::sort::{SortState, sort_indices};
use super::width::ColumnWidths;
use crate::Theme;

const START_PRICE_UNITS: i64 = 100_000_000_000; // $100.00 in 1e-9 units.
const TICK_INTERVAL_NS: i64 = 100_000_000; // 100 ms between synthetic trades.
const CENT_UNITS: i64 = 10_000_000; // $0.01 in 1e-9 units.
const PRICE_STEP_CENTS: i64 = 5; // ±5¢ per tick.
const PRICE_UNITS_PER_DOLLAR: f64 = 1_000_000_000.0;

/// Aggressor side of a synthetic trade. `Ord` (Buy < Sell) so the demo's
/// `Side` column is sortable — a low-cardinality primary that makes the
/// multi-column tiebreak sort obvious at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DemoSide {
    Buy,
    Sell,
}

/// A synthetic trade tick used purely by the gallery demo. Flat
/// fields keep this self-contained — see the module-level doc on why
/// `void_ui` doesn't depend on a market-data crate.
#[derive(Debug, Clone, Copy)]
pub struct DemoTick {
    /// Stable, unique row id assigned at creation (a monotonic sequence).
    /// This is the grid's `getRowId` source — selection is keyed by it,
    /// so a selected row stays selected across sort/filter reordering.
    /// Assigned in creation order, so id order is also natural row order.
    pub id: u64,
    /// Event time in nanoseconds since an arbitrary epoch.
    pub event_ns: i64,
    /// Price in 1e-9 units of the quoted currency.
    pub price_units: i64,
    /// Trade size (None means unknown).
    pub size: Option<u64>,
    /// Aggressor side (None means unknown).
    pub side: Option<DemoSide>,
}

/// Demo state. Lives as a field on the gallery's app state.
#[derive(Debug, Clone)]
pub struct Demo {
    /// The synthetic tick history.
    pub ticks: Vec<DemoTick>,
    /// Currently-selected rows, keyed by stable row id (see
    /// [`DemoTick::id`]) — not by position, so selection follows rows
    /// across sort/filter reordering.
    pub selection: SelectionState,
    /// Active column sort (which column + direction).
    pub sort: SortState,
    /// Active per-column filter queries.
    pub filter: FilterState,
    /// Per-column width overrides (drag-to-resize).
    pub column_widths: ColumnWidths,
    /// Materialized filtered-then-sorted rows. Meaningful while the view
    /// is reordered (a filter is active *or* a sort column is set); the
    /// gallery's `rows` lens reads `ticks` directly in the plain
    /// unfiltered+unsorted case (avoiding a full-dataset clone). See
    /// [`Self::view_is_materialized`].
    pub visible: Vec<DemoTick>,
    /// Visible columns in display order, as a list of [`ColumnId`]s —
    /// the host's column-layout state (show/hide + reorder). The grid is
    /// handed `tick_columns` *arranged* by this (see [`Self::columns`]);
    /// because sort/filter/width state is id-keyed, it follows each column
    /// across any reorder or hide for free. `None` until first read, then
    /// defaults to every column in natural order (see [`Self::layout`]).
    column_order: Option<Vec<ColumnId>>,
    rng_state: u64,
    last_time_ns: i64,
    last_price_units: i64,
    /// Next stable row id to hand out (monotonic; never reused).
    next_id: u64,
}

impl Demo {
    /// Constructs a demo with `initial_count` synthetic ticks
    /// pre-seeded. The walk is deterministic — same seed, same
    /// output.
    #[must_use]
    pub fn with_initial(initial_count: usize) -> Self {
        let mut demo = Self {
            ticks: Vec::with_capacity(initial_count.max(64)),
            selection: SelectionState::new(),
            sort: SortState::new(),
            filter: FilterState::new(),
            column_widths: ColumnWidths::new(),
            visible: Vec::new(),
            column_order: None,
            rng_state: 0x0005_DEEC_E66D_u64.wrapping_mul(0xB16B_00B5),
            last_time_ns: 0,
            last_price_units: START_PRICE_UNITS,
            next_id: 0,
        };
        demo.append_n(initial_count);
        demo
    }

    /// Whether the displayed view is a materialized reorder of `ticks`
    /// (a filter is active or a sort column is set). When `false`, the
    /// grid reads `ticks` directly in natural order.
    #[must_use]
    pub fn view_is_materialized(&self) -> bool {
        !self.filter.is_empty() || !self.sort.is_empty()
    }

    /// Appends `n` newly-generated ticks to the tail. Refreshes the
    /// materialized view when one is active so appended rows that match
    /// the filter / fall into the sort order show up.
    pub fn append_n(&mut self, n: usize) {
        self.ticks.reserve(n);
        for _ in 0..n {
            let tick = self.next_tick();
            self.ticks.push(tick);
        }
        if self.view_is_materialized() {
            self.refresh_visible();
        }
    }

    /// Sets a column's filter query (keyed by stable [`ColumnId`]) and
    /// refreshes the visible view. Demonstrates the host-side filtering
    /// path: the host owns the data and runs [`filtered_indices`] over it.
    pub fn set_filter(&mut self, column: ColumnId, query: impl Into<String>) {
        self.filter.set(column, query);
        self.refresh_visible();
    }

    /// Clears every column filter and refreshes the view.
    pub fn clear_filter(&mut self) {
        self.filter.clear_all();
        self.refresh_visible();
    }

    /// Cycles the sort on `column` (the grid's `on_sort` callback) and
    /// re-derives the ordered view. `multi` (Shift held) adds/cycles the
    /// column as a tiebreaker; a plain click replaces with single-sort.
    /// Demonstrates the host-side sorting path — the mirror of
    /// [`Self::set_filter`]: the host owns row order and composes
    /// [`filtered_indices`] then [`sort_indices`].
    pub fn cycle_sort(&mut self, column: ColumnId, multi: bool) {
        if multi {
            self.sort.cycle_additive(column);
        } else {
            self.sort.cycle(column);
        }
        self.refresh_visible();
    }

    /// Sets a column's width override (drag-to-resize), keyed by the
    /// column's stable [`ColumnId`]. `ColumnWidths` clamps to the minimum
    /// width; no data refresh is needed.
    pub fn resize_column(&mut self, column: ColumnId, new_width: f64) {
        self.column_widths.set(column, new_width);
    }

    /// The full set of column ids in natural (definition) order.
    fn all_column_ids() -> Vec<ColumnId> {
        tick_columns::<()>(0)
            .iter()
            .map(ColumnDef::effective_id)
            .collect()
    }

    /// The current visible column layout (ids in display order),
    /// defaulting to every column in natural order before any edit.
    fn layout(&self) -> Vec<ColumnId> {
        self.column_order
            .clone()
            .unwrap_or_else(Self::all_column_ids)
    }

    /// The current visible column layout, as ids in display order — the
    /// snapshot the gallery threads to its panel to arrange columns. (See
    /// [`arrange_columns`], which the panel calls with this.)
    #[must_use]
    pub fn column_layout(&self) -> Vec<ColumnId> {
        self.layout()
    }

    /// Toggles a column's visibility. Hiding removes it from the layout
    /// **and clears its filter / sort level** — with the column gone there
    /// is no filter input or header arrow left on screen, so surviving
    /// criteria would keep trimming/reordering rows with no visible cause.
    /// Showing appends it at the end (cleared state is not resurrected).
    /// A toggle that would hide the last visible column is rejected (keep
    /// at least one).
    pub fn toggle_column(&mut self, id: &ColumnId) {
        let mut order = self.layout();
        if let Some(pos) = order.iter().position(|c| c == id) {
            if order.len() > 1 {
                order.remove(pos);
                self.filter.clear(id);
                self.sort.remove(id);
                self.refresh_visible();
            }
        } else {
            order.push(id.clone());
        }
        self.column_order = Some(order);
    }

    /// Moves a visible column one slot toward the front (left). No-op if
    /// it's already first or not visible.
    pub fn move_column_left(&mut self, id: &ColumnId) {
        let mut order = self.layout();
        if let Some(pos) = order.iter().position(|c| c == id)
            && pos > 0
        {
            order.swap(pos - 1, pos);
            self.column_order = Some(order);
        }
    }

    /// Resets the column layout to every column in natural order.
    pub fn reset_columns(&mut self) {
        self.column_order = None;
    }

    /// Recomputes [`Self::visible`] as the host-ordered view: filter
    /// first, then sort (the canonical pipeline order). Cleared when no
    /// reorder is active — the lens falls back to `ticks` directly then.
    ///
    /// This is the host-owns-order contract the grid expects: the grid
    /// renders `visible` in the order given and never reorders itself.
    fn refresh_visible(&mut self) {
        if !self.view_is_materialized() {
            self.visible.clear();
            return;
        }
        // `tick_columns` carries the per-column filter predicates and
        // comparators; the `State`/`base_time_ns` args don't affect
        // ordering.
        let columns = tick_columns::<()>(0);
        // 1. Filter to surviving indices, 2. sort those indices.
        let mut idx = filtered_indices(&self.ticks, &self.filter, &columns);
        sort_indices(&mut idx, &self.ticks, &self.sort, &columns);
        self.visible = idx.into_iter().map(|i| self.ticks[i]).collect();
    }

    /// Replaces the current selection with the first `n` rows *in the
    /// current display order*, keyed by their stable ids. Bulk-selection
    /// helper used by the gallery demo's toolbar.
    pub fn select_first(&mut self, n: usize) {
        self.selection.clear();
        if n == 0 {
            return;
        }
        // Read ids from whichever slice the grid is currently showing, so
        // "first n" matches what the user sees under sort/filter.
        let ids: Vec<u64> = if self.view_is_materialized() {
            self.visible.iter().take(n).map(|t| t.id).collect()
        } else {
            self.ticks.iter().take(n).map(|t| t.id).collect()
        };
        if let Some(&first) = ids.first() {
            // Anchor at the first row, then select the whole id set.
            self.selection.replace_with(first);
            self.selection.extend_range(ids);
        }
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    fn next_tick(&mut self) -> DemoTick {
        self.last_time_ns += TICK_INTERVAL_NS;
        let raw = xorshift64(&mut self.rng_state);
        // Map raw bits into a whole number of cents in `±PRICE_STEP_CENTS`,
        // then scale to nano-units. Snapping to whole cents keeps every
        // price an exact multiple of a cent, so the *displayed* price
        // (rounded to ¢) and the sort key agree — without this, sub-cent
        // prices display as one cent but sort into a neighbouring bucket,
        // scrambling multi-column sort ties. (See the Price column.)
        #[expect(
            clippy::cast_possible_wrap,
            reason = "Wrapping is the intended behavior"
        )]
        let raw_i = raw as i64;
        let delta_cents = raw_i.rem_euclid(2 * PRICE_STEP_CENTS + 1) - PRICE_STEP_CENTS;
        self.last_price_units = self
            .last_price_units
            .saturating_add(delta_cents * CENT_UNITS);
        let trade_size = (xorshift64(&mut self.rng_state) % 900) + 100;
        let aggressor = if xorshift64(&mut self.rng_state) & 1 == 0 {
            DemoSide::Buy
        } else {
            DemoSide::Sell
        };
        let id = self.next_id;
        self.next_id += 1;
        DemoTick {
            id,
            event_ns: self.last_time_ns,
            price_units: self.last_price_units,
            size: Some(trade_size),
            side: Some(aggressor),
        }
    }
}

#[inline]
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Conditional color for the `Side` column: buys green, sells coral
/// (the classic trading convention), unknown faint. Theme-aware so it
/// resolves correctly across variants. Exercised by the `Side` column's
/// `colored_text_column` and unit-tested below.
fn side_color(side: Option<DemoSide>, theme: &Theme) -> Color {
    match side {
        Some(DemoSide::Buy) => theme.palette.green,
        Some(DemoSide::Sell) => theme.palette.coral,
        None => theme.palette.text_faint,
    }
}

/// Builds the four column descriptors for browsing a [`DemoTick`]
/// stream: `Time` (relative ms from `base_time_ns`), `Price`
/// (`$X.XX`), `Size`, and `Side` (`B`/`S`/`—`).
///
/// `base_time_ns` is captured by the time column's projector so the
/// displayed values are relative — much easier to read than raw
/// nanosecond timestamps. Pass `ticks.first().map(|t| t.event_ns)
/// .unwrap_or(0)` from the caller.
#[must_use]
pub fn tick_columns<State: 'static>(base_time_ns: i64) -> Vec<ColumnDef<DemoTick, State>> {
    vec![
        // Columns sort by their *underlying* value, not the formatted
        // display string — `event_ns`/`price_units` sort numerically
        // rather than lexicographically (e.g. so "$9.00" < "$100.00").
        text_column("Time (ms)", 100.0, CellAlign::End, move |t: &DemoTick| {
            let delta_ns = t.event_ns.saturating_sub(base_time_ns);
            // ns → ms with one decimal.
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let ms = delta_ns as f64 / 1_000_000.0;
            format!("{ms:.1}")
        })
        .sortable_by_key(|t: &DemoTick| t.event_ns),
        text_column("Price", 90.0, CellAlign::End, |t: &DemoTick| {
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let dollars = t.price_units as f64 / PRICE_UNITS_PER_DOLLAR;
            format!("${dollars:.2}")
        })
        // Prices snap to whole cents (see `next_tick`), so equal-priced
        // rows genuinely tie and a 2nd-level sort visibly orders within
        // each price. Sorting by the raw value is exact here.
        .sortable_by_key(|t: &DemoTick| t.price_units),
        optional_text_column("Size", 80.0, CellAlign::End, |t: &DemoTick| {
            t.size.map(|v| v.to_string())
        })
        // `Option<u64>` is `Ord` (None sorts before Some), so unknown
        // sizes cluster at the ascending end.
        .sortable_by_key(|t: &DemoTick| t.size),
        // Conditional formatting: buys green, sells coral (the classic
        // trading convention), unknown faint — see `side_color`.
        colored_text_column(
            "Side",
            60.0,
            CellAlign::Center,
            |t: &DemoTick| match t.side {
                Some(DemoSide::Buy) => "B".to_string(),
                Some(DemoSide::Sell) => "S".to_string(),
                None => "—".to_string(),
            },
            |t: &DemoTick, theme: &Theme| side_color(t.side, theme),
        )
        // Sortable: groups buys then sells (None first) — a low-cardinality
        // primary that makes the multi-column tiebreak sort easy to see.
        .sortable_by_key(|t: &DemoTick| t.side)
        // Filterable by side glyph: query "B" shows buys, "S" sells.
        .filterable_by_text(|t: &DemoTick| match t.side {
            Some(DemoSide::Buy) => "B".to_string(),
            Some(DemoSide::Sell) => "S".to_string(),
            None => String::new(),
        }),
        // --- Extra derived columns: a realistic wide blotter that
        //     overflows the viewport horizontally (Tier 2: H-scroll).
        //     All are pure functions of existing fields — no new data.
        text_column("Bid", 100.0, CellAlign::End, |t: &DemoTick| {
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let bid = (t.price_units - 10_000_000) as f64 / PRICE_UNITS_PER_DOLLAR;
            format!("${bid:.2}")
        })
        .sortable_by_key(|t: &DemoTick| t.price_units - 10_000_000),
        text_column("Ask", 100.0, CellAlign::End, |t: &DemoTick| {
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let ask = (t.price_units + 10_000_000) as f64 / PRICE_UNITS_PER_DOLLAR;
            format!("${ask:.2}")
        })
        .sortable_by_key(|t: &DemoTick| t.price_units + 10_000_000),
        text_column("Spread", 90.0, CellAlign::End, |_t: &DemoTick| {
            "$0.02".to_string()
        }),
        text_column("Notional", 130.0, CellAlign::End, |t: &DemoTick| {
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let px = t.price_units as f64 / PRICE_UNITS_PER_DOLLAR;
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let sz = t.size.unwrap_or(0) as f64;
            format!("${:.0}", px * sz)
        })
        // Sortable by its own value (price × size), distinct from Price —
        // so "Price then Notional" is a meaningful two-key sort.
        .sortable_by_key(|t: &DemoTick| {
            t.price_units
                .saturating_mul(i64::try_from(t.size.unwrap_or(0)).unwrap_or(0))
        }),
        text_column("Exchange", 120.0, CellAlign::Start, |t: &DemoTick| {
            demo_exchange(t.event_ns).to_string()
        })
        .filterable_by_text(|t: &DemoTick| demo_exchange(t.event_ns).to_string()),
        text_column("VWAP", 100.0, CellAlign::End, |t: &DemoTick| {
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let v = t.price_units as f64 / PRICE_UNITS_PER_DOLLAR;
            format!("${v:.2}")
        }),
    ]
}

/// Arranges [`tick_columns`] by a `layout` of [`ColumnId`]s: returns only
/// the listed columns, in the listed order. Ids not matching a column are
/// skipped; columns absent from `layout` are hidden by omission.
///
/// This is how the demo host expresses **show/hide + reorder** — purely
/// by which `ColumnDef`s it hands the grid and in what order. The grid
/// needs no column-layout feature of its own, and because sort/filter/
/// width state is keyed by [`ColumnId`], it follows each column across
/// the rearrangement automatically. (`void_ui` stays product-agnostic:
/// the layout lives with the host, like every other piece of grid state.)
#[must_use]
pub fn arrange_columns<S: 'static>(
    layout: &[ColumnId],
    base_time_ns: i64,
) -> Vec<ColumnDef<DemoTick, S>> {
    let mut defs = tick_columns::<S>(base_time_ns);
    layout
        .iter()
        .filter_map(|id| {
            defs.iter()
                .position(|d| &d.effective_id() == id)
                .map(|pos| defs.remove(pos))
        })
        .collect()
}

/// Synthetic exchange code for the demo blotter, rotated by event time
/// so the `Exchange` column has a few distinct filterable values.
fn demo_exchange(event_ns: i64) -> &'static str {
    match (event_ns / TICK_INTERVAL_NS) % 3 {
        0 => "NYSE",
        1 => "NSDQ",
        _ => "ARCA",
    }
}

// ===========================================================================
// MARK: Stock-quote demo — the "value lens" product mock
// ===========================================================================
//
// A second, distinct demo fixture: a static snapshot of NASDAQ-style symbol
// quotes, shaped like the fields a `yfinance` row carries. Where `DemoTick`
// exercises raw capability (a streaming blotter), this one shows the grid as
// the product it was designed against — an Excel-style quote board.
//
// Still 100% generic from the *component's* side: `StockQuote` is a plain
// demo fixture (like `DemoTick`), the grid knows nothing about it. The data
// is a hand-authored static snapshot — deterministic, no network — but the
// field shape mirrors a real quote so swapping in a live source is a
// data-layer change, not a grid change.

/// A static market-quote snapshot for one symbol — the demo's stock fixture.
///
/// Fields mirror a typical `yfinance` quote row. Prices are plain dollars
/// (authored to 2 decimals, so display and sort agree without the
/// whole-cent dance the streaming `DemoTick` needs). `None` fields render
/// as `—` (e.g. an unprofitable company has no P/E).
#[derive(Debug, Clone, Copy)]
pub struct StockQuote {
    /// Ticker symbol — unique, stable: the grid's `row_id` and the natural
    /// freeze/pin candidate.
    pub symbol: &'static str,
    /// Company name.
    pub name: &'static str,
    /// Last / close price.
    pub last: f64,
    /// Day change in dollars (sign drives the conditional color).
    pub change: f64,
    /// Day change in percent.
    pub change_pct: f64,
    /// Session open.
    pub open: f64,
    /// Session high.
    pub high: f64,
    /// Session low.
    pub low: f64,
    /// Share volume traded this session.
    pub volume: u64,
    /// Trailing average daily volume.
    pub avg_volume: u64,
    /// Market capitalization, in dollars.
    pub market_cap: u64,
    /// Trailing P/E ratio (`None` when earnings are non-positive).
    pub pe: Option<f64>,
    /// Trailing EPS.
    pub eps: f64,
    /// Forward dividend yield in percent (`None` when no dividend).
    pub div_yield: Option<f64>,
    /// 52-week high.
    pub week52_high: f64,
    /// 52-week low.
    pub week52_low: f64,
    /// Beta vs. the broad market.
    pub beta: f64,
    /// GICS-style sector — a low-cardinality column, good for grouping.
    pub sector: &'static str,
}

/// Stock-quote demo state. Mirrors [`Demo`]'s interaction-state shape
/// (selection / sort / filter / widths / column layout) but over the
/// static [`StockQuote`] snapshot. The grid wiring is identical — only the
/// row type and data source differ — which is the whole point of the
/// value-lens demo.
#[derive(Debug, Clone)]
pub struct StockDemo {
    /// The static quote snapshot (never mutated after construction).
    pub quotes: Vec<StockQuote>,
    /// Selected rows, keyed by stable symbol id.
    pub selection: SelectionState,
    /// Active multi-column sort.
    pub sort: SortState,
    /// Active per-column filters.
    pub filter: FilterState,
    /// Per-column width overrides.
    pub column_widths: ColumnWidths,
    /// Materialized filtered-then-sorted view (see [`Self::refresh`]).
    pub visible: Vec<StockQuote>,
    /// Visible columns in display order (show/hide + reorder).
    column_order: Option<Vec<ColumnId>>,
}

impl StockDemo {
    /// Builds the demo over the bundled static snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            quotes: stock_quotes().to_vec(),
            selection: SelectionState::new(),
            sort: SortState::new(),
            filter: FilterState::new(),
            column_widths: ColumnWidths::new(),
            visible: Vec::new(),
            column_order: None,
        }
    }

    /// Whether the displayed view is a materialized reorder of `quotes`
    /// (a filter is active or a sort is set).
    #[must_use]
    pub fn view_is_materialized(&self) -> bool {
        !self.filter.is_empty() || !self.sort.is_empty()
    }

    /// Cycles the sort on `column` and re-derives the view. `multi` (Shift)
    /// adds a tiebreaker; a plain click replaces with single-sort.
    pub fn cycle_sort(&mut self, column: ColumnId, multi: bool) {
        if multi {
            self.sort.cycle_additive(column);
        } else {
            self.sort.cycle(column);
        }
        self.refresh();
    }

    /// Sets a column's filter query (by stable id) and re-derives the view.
    pub fn set_filter(&mut self, column: ColumnId, query: impl Into<String>) {
        self.filter.set(column, query);
        self.refresh();
    }

    /// Sets a column width override (drag-to-resize).
    pub fn resize_column(&mut self, column: ColumnId, new_width: f64) {
        self.column_widths.set(column, new_width);
    }

    /// The current visible column layout, defaulting to all columns in
    /// natural order.
    #[must_use]
    pub fn column_layout(&self) -> Vec<ColumnId> {
        self.column_order.clone().unwrap_or_else(|| {
            stock_columns::<()>()
                .iter()
                .map(ColumnDef::effective_id)
                .collect()
        })
    }

    /// Toggles a column's visibility (hide ⇒ drop **and clear its filter /
    /// sort level**, so no invisible criteria survive; show ⇒ append).
    /// Keeps at least one column visible. Mirrors [`Demo::toggle_column`].
    pub fn toggle_column(&mut self, id: &ColumnId) {
        let mut order = self.column_layout();
        if let Some(pos) = order.iter().position(|c| c == id) {
            if order.len() > 1 {
                order.remove(pos);
                self.filter.clear(id);
                self.sort.remove(id);
                self.refresh();
            }
        } else {
            order.push(id.clone());
        }
        self.column_order = Some(order);
    }

    /// Moves a visible column one slot toward the front. No-op if first
    /// or not visible.
    pub fn move_column_left(&mut self, id: &ColumnId) {
        let mut order = self.column_layout();
        if let Some(pos) = order.iter().position(|c| c == id)
            && pos > 0
        {
            order.swap(pos - 1, pos);
            self.column_order = Some(order);
        }
    }

    /// Resets the column layout to every column in natural order.
    pub fn reset_columns(&mut self) {
        self.column_order = None;
    }

    /// Recomputes [`Self::visible`] as filter-then-sort over `quotes`
    /// (the host-owns-order contract; same shape as [`Demo::refresh_visible`]).
    fn refresh(&mut self) {
        if !self.view_is_materialized() {
            self.visible.clear();
            return;
        }
        let columns = stock_columns::<()>();
        let mut idx = filtered_indices(&self.quotes, &self.filter, &columns);
        sort_indices(&mut idx, &self.quotes, &self.sort, &columns);
        self.visible = idx.into_iter().map(|i| self.quotes[i]).collect();
    }
}

impl Default for StockDemo {
    fn default() -> Self {
        Self::new()
    }
}

/// The static NASDAQ-style quote snapshot. Hand-authored, deterministic,
/// no network. Values are plausible but illustrative — a fixed snapshot,
/// not live data. `change`/`change_pct` carry mixed signs so the
/// conditional up/down coloring is visible; P/E and dividend are `None`
/// for a few names to exercise the `—` rendering.
#[rustfmt::skip]
fn stock_quotes() -> &'static [StockQuote] {
    // symbol, name, last, change, change_pct, open, high, low, volume,
    // avg_volume, market_cap, pe, eps, div_yield, 52wH, 52wL, beta, sector
    const Q: &[StockQuote] = &[
        StockQuote { symbol: "AAPL", name: "Apple Inc.", last: 229.87, change: 1.32, change_pct: 0.58, open: 228.40, high: 230.72, low: 227.93, volume: 41_284_300, avg_volume: 54_900_000, market_cap: 3_480_000_000_000, pe: Some(35.1), eps: 6.55, div_yield: Some(0.43), week52_high: 237.49, week52_low: 164.08, beta: 1.24, sector: "Technology" },
        StockQuote { symbol: "MSFT", name: "Microsoft Corp.", last: 417.22, change: -2.18, change_pct: -0.52, open: 419.80, high: 421.05, low: 415.61, volume: 18_902_100, avg_volume: 21_300_000, market_cap: 3_100_000_000_000, pe: Some(34.6), eps: 12.06, div_yield: Some(0.72), week52_high: 468.35, week52_low: 366.50, beta: 0.91, sector: "Technology" },
        StockQuote { symbol: "NVDA", name: "NVIDIA Corp.", last: 138.07, change: 3.91, change_pct: 2.91, open: 134.60, high: 139.20, low: 134.02, volume: 245_110_800, avg_volume: 268_000_000, market_cap: 3_380_000_000_000, pe: Some(64.8), eps: 2.13, div_yield: Some(0.03), week52_high: 152.89, week52_low: 47.32, beta: 1.66, sector: "Technology" },
        StockQuote { symbol: "AMZN", name: "Amazon.com Inc.", last: 207.89, change: 0.74, change_pct: 0.36, open: 207.10, high: 209.44, low: 206.20, volume: 33_021_500, avg_volume: 41_700_000, market_cap: 2_180_000_000_000, pe: Some(45.2), eps: 4.60, div_yield: None, week52_high: 215.90, week52_low: 144.05, beta: 1.15, sector: "Consumer Cyclical" },
        StockQuote { symbol: "GOOGL", name: "Alphabet Inc.", last: 169.24, change: -1.05, change_pct: -0.62, open: 170.55, high: 171.02, low: 168.40, volume: 22_415_900, avg_volume: 27_800_000, market_cap: 2_070_000_000_000, pe: Some(23.4), eps: 7.23, div_yield: Some(0.47), week52_high: 191.75, week52_low: 130.67, beta: 1.03, sector: "Communication Services" },
        StockQuote { symbol: "META", name: "Meta Platforms Inc.", last: 563.27, change: 6.83, change_pct: 1.23, open: 557.00, high: 565.90, low: 555.12, volume: 14_002_700, avg_volume: 16_900_000, market_cap: 1_430_000_000_000, pe: Some(28.0), eps: 20.12, div_yield: Some(0.35), week52_high: 602.95, week52_low: 414.50, beta: 1.21, sector: "Communication Services" },
        StockQuote { symbol: "TSLA", name: "Tesla Inc.", last: 342.03, change: -8.47, change_pct: -2.42, open: 350.60, high: 352.10, low: 339.85, volume: 88_540_200, avg_volume: 96_400_000, market_cap: 1_090_000_000_000, pe: Some(96.3), eps: 3.55, div_yield: None, week52_high: 488.54, week52_low: 138.80, beta: 2.31, sector: "Consumer Cyclical" },
        StockQuote { symbol: "AVGO", name: "Broadcom Inc.", last: 178.45, change: 2.10, change_pct: 1.19, open: 176.80, high: 179.33, low: 176.02, volume: 19_330_400, avg_volume: 23_100_000, market_cap: 834_000_000_000, pe: Some(168.2), eps: 1.06, div_yield: Some(1.18), week52_high: 186.42, week52_low: 84.86, beta: 1.19, sector: "Technology" },
        StockQuote { symbol: "COST", name: "Costco Wholesale", last: 962.18, change: 4.55, change_pct: 0.47, open: 958.20, high: 965.40, low: 956.70, volume: 1_840_300, avg_volume: 2_300_000, market_cap: 427_000_000_000, pe: Some(54.7), eps: 17.59, div_yield: Some(0.49), week52_high: 1078.23, week52_low: 660.01, beta: 0.79, sector: "Consumer Defensive" },
        StockQuote { symbol: "PEP", name: "PepsiCo Inc.", last: 151.93, change: -0.62, change_pct: -0.41, open: 152.60, high: 153.11, low: 151.20, volume: 5_210_700, avg_volume: 6_100_000, market_cap: 208_000_000_000, pe: Some(22.1), eps: 6.87, div_yield: Some(3.58), week52_high: 183.41, week52_low: 141.51, beta: 0.55, sector: "Consumer Defensive" },
        StockQuote { symbol: "ADBE", name: "Adobe Inc.", last: 484.12, change: -3.20, change_pct: -0.66, open: 487.90, high: 489.55, low: 482.30, volume: 3_104_200, avg_volume: 3_700_000, market_cap: 213_000_000_000, pe: Some(38.9), eps: 12.45, div_yield: None, week52_high: 638.25, week52_low: 433.97, beta: 1.31, sector: "Technology" },
        StockQuote { symbol: "NFLX", name: "Netflix Inc.", last: 897.56, change: 12.04, change_pct: 1.36, open: 886.50, high: 901.20, low: 884.00, volume: 3_660_900, avg_volume: 4_200_000, market_cap: 384_000_000_000, pe: Some(48.6), eps: 18.47, div_yield: None, week52_high: 941.75, week52_low: 542.01, beta: 1.28, sector: "Communication Services" },
        StockQuote { symbol: "INTC", name: "Intel Corp.", last: 24.05, change: -0.41, change_pct: -1.68, open: 24.50, high: 24.62, low: 23.88, volume: 61_220_500, avg_volume: 58_900_000, market_cap: 103_000_000_000, pe: None, eps: -0.13, div_yield: None, week52_high: 45.30, week52_low: 18.51, beta: 1.07, sector: "Technology" },
        StockQuote { symbol: "AMD", name: "Advanced Micro Devices", last: 138.61, change: 1.77, change_pct: 1.29, open: 137.10, high: 139.84, low: 136.55, volume: 38_950_100, avg_volume: 44_600_000, market_cap: 224_000_000_000, pe: Some(118.5), eps: 1.17, div_yield: None, week52_high: 227.30, week52_low: 116.37, beta: 1.70, sector: "Technology" },
        StockQuote { symbol: "QCOM", name: "Qualcomm Inc.", last: 158.84, change: 0.92, change_pct: 0.58, open: 158.00, high: 159.77, low: 157.41, volume: 7_810_600, avg_volume: 9_200_000, market_cap: 176_000_000_000, pe: Some(17.9), eps: 8.87, div_yield: Some(2.14), week52_high: 230.63, week52_low: 149.43, beta: 1.30, sector: "Technology" },
        StockQuote { symbol: "TXN", name: "Texas Instruments", last: 196.30, change: -1.14, change_pct: -0.58, open: 197.55, high: 198.02, low: 195.60, volume: 4_920_800, avg_volume: 5_600_000, market_cap: 179_000_000_000, pe: Some(38.4), eps: 5.11, div_yield: Some(2.77), week52_high: 220.39, week52_low: 157.10, beta: 1.02, sector: "Technology" },
        StockQuote { symbol: "AMAT", name: "Applied Materials", last: 172.41, change: 2.63, change_pct: 1.55, open: 170.20, high: 173.10, low: 169.85, volume: 6_330_400, avg_volume: 7_400_000, market_cap: 142_000_000_000, pe: Some(21.6), eps: 7.98, div_yield: Some(0.93), week52_high: 255.89, week52_low: 158.06, beta: 1.55, sector: "Technology" },
        StockQuote { symbol: "MU", name: "Micron Technology", last: 102.18, change: 3.05, change_pct: 3.08, open: 99.40, high: 103.22, low: 99.10, volume: 21_440_700, avg_volume: 24_800_000, market_cap: 113_000_000_000, pe: Some(146.0), eps: 0.70, div_yield: Some(0.45), week52_high: 157.54, week52_low: 79.15, beta: 1.20, sector: "Technology" },
        StockQuote { symbol: "INTU", name: "Intuit Inc.", last: 638.45, change: -4.10, change_pct: -0.64, open: 643.00, high: 645.20, low: 635.80, volume: 1_220_500, avg_volume: 1_600_000, market_cap: 178_000_000_000, pe: Some(58.3), eps: 10.95, div_yield: Some(0.66), week52_high: 714.78, week52_low: 557.29, beta: 1.18, sector: "Technology" },
        StockQuote { symbol: "AMGN", name: "Amgen Inc.", last: 263.74, change: 0.58, change_pct: 0.22, open: 263.00, high: 265.10, low: 262.20, volume: 2_640_900, avg_volume: 3_100_000, market_cap: 142_000_000_000, pe: Some(38.7), eps: 6.81, div_yield: Some(3.42), week52_high: 346.85, week52_low: 256.16, beta: 0.62, sector: "Healthcare" },
        StockQuote { symbol: "GILD", name: "Gilead Sciences", last: 92.36, change: 1.21, change_pct: 1.33, open: 91.30, high: 92.80, low: 91.05, volume: 6_010_200, avg_volume: 7_000_000, market_cap: 115_000_000_000, pe: Some(22.8), eps: 4.05, div_yield: Some(3.34), week52_high: 102.07, week52_low: 63.91, beta: 0.27, sector: "Healthcare" },
        StockQuote { symbol: "VRTX", name: "Vertex Pharmaceuticals", last: 462.08, change: -2.95, change_pct: -0.63, open: 465.50, high: 466.90, low: 460.10, volume: 1_120_400, avg_volume: 1_400_000, market_cap: 119_000_000_000, pe: Some(28.5), eps: 16.21, div_yield: None, week52_high: 519.88, week52_low: 376.80, beta: 0.42, sector: "Healthcare" },
        StockQuote { symbol: "SBUX", name: "Starbucks Corp.", last: 98.74, change: 0.43, change_pct: 0.44, open: 98.30, high: 99.50, low: 97.90, volume: 7_540_300, avg_volume: 8_900_000, market_cap: 112_000_000_000, pe: Some(28.9), eps: 3.42, div_yield: Some(2.47), week52_high: 117.46, week52_low: 71.55, beta: 0.94, sector: "Consumer Cyclical" },
        StockQuote { symbol: "BKNG", name: "Booking Holdings", last: 4982.11, change: 38.20, change_pct: 0.77, open: 4945.00, high: 5005.40, low: 4938.60, volume: 280_100, avg_volume: 340_000, market_cap: 165_000_000_000, pe: Some(31.2), eps: 159.68, div_yield: Some(0.70), week52_high: 5337.50, week52_low: 3118.32, beta: 1.36, sector: "Consumer Cyclical" },
        StockQuote { symbol: "ISRG", name: "Intuitive Surgical", last: 533.62, change: 5.18, change_pct: 0.98, open: 528.90, high: 535.70, low: 527.40, volume: 1_410_600, avg_volume: 1_800_000, market_cap: 189_000_000_000, pe: Some(83.4), eps: 6.40, div_yield: None, week52_high: 549.84, week52_low: 345.59, beta: 1.45, sector: "Healthcare" },
        StockQuote { symbol: "PYPL", name: "PayPal Holdings", last: 86.42, change: -1.33, change_pct: -1.52, open: 87.80, high: 88.10, low: 85.90, volume: 9_870_400, avg_volume: 11_500_000, market_cap: 86_000_000_000, pe: Some(20.4), eps: 4.24, div_yield: None, week52_high: 93.66, week52_low: 55.86, beta: 1.42, sector: "Financial Services" },
        StockQuote { symbol: "CMCSA", name: "Comcast Corp.", last: 41.18, change: 0.12, change_pct: 0.29, open: 41.05, high: 41.44, low: 40.88, volume: 18_220_900, avg_volume: 20_100_000, market_cap: 158_000_000_000, pe: Some(10.7), eps: 3.85, div_yield: Some(3.01), week52_high: 47.11, week52_low: 36.43, beta: 0.94, sector: "Communication Services" },
        StockQuote { symbol: "HON", name: "Honeywell Intl.", last: 224.55, change: -0.88, change_pct: -0.39, open: 225.60, high: 226.30, low: 223.70, volume: 2_910_700, avg_volume: 3_400_000, market_cap: 146_000_000_000, pe: Some(24.6), eps: 9.13, div_yield: Some(2.00), week52_high: 242.77, week52_low: 183.62, beta: 1.06, sector: "Industrials" },
    ];
    Q
}

/// Conditional color for a signed change value: positive green, negative
/// coral, flat faint — the universal up/down convention. Theme-aware.
fn change_color(v: f64, theme: &Theme) -> Color {
    if v > 0.0 {
        theme.palette.green
    } else if v < 0.0 {
        theme.palette.coral
    } else {
        theme.palette.text_faint
    }
}

/// A stable integer sort key from a dollar `f64`, scaled to cents — `f64`
/// isn't `Ord`, and the displayed values are 2-decimal, so cents are exact.
#[expect(clippy::cast_possible_truncation, reason = "demo display precision")]
fn cents(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

/// A stable integer sort key from a percent/ratio `f64`, scaled to basis
/// points (2 decimals shown ⇒ scale by 100).
#[expect(clippy::cast_possible_truncation, reason = "demo display precision")]
fn bps(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

/// The column descriptors for the stock-quote board — the "value lens"
/// layout: identity (`Symbol`/`Name`), price action (`Last`/`Chg`/`Chg%`/
/// OHLC), liquidity (`Volume`/`Avg Vol`), and fundamentals (`Mkt Cap`/
/// `P/E`/`EPS`/`Div %`/52-week range/`Beta`/`Sector`).
///
/// Numeric columns sort by their *underlying* value (via integer keys),
/// not the formatted string; `Chg`/`Chg%` are conditionally colored by
/// sign; the `Symbol` column is the natural `row_id`. Like
/// [`tick_columns`], this is a plain demo helper — the grid is generic.
#[must_use]
pub fn stock_columns<S: 'static>() -> Vec<ColumnDef<StockQuote, S>> {
    vec![
        text_column("Symbol", 80.0, CellAlign::Start, |q: &StockQuote| {
            q.symbol.to_string()
        })
        .sortable_by_key(|q: &StockQuote| q.symbol)
        .filterable_by_text(|q: &StockQuote| q.symbol.to_string()),
        text_column("Name", 190.0, CellAlign::Start, |q: &StockQuote| {
            q.name.to_string()
        })
        .sortable_by_key(|q: &StockQuote| q.name)
        .filterable_by_text(|q: &StockQuote| q.name.to_string()),
        text_column("Last", 90.0, CellAlign::End, |q: &StockQuote| {
            format!("${:.2}", q.last)
        })
        .sortable_by_key(|q: &StockQuote| cents(q.last)),
        colored_text_column(
            "Chg",
            80.0,
            CellAlign::End,
            |q: &StockQuote| format!("{:+.2}", q.change),
            |q: &StockQuote, theme: &Theme| change_color(q.change, theme),
        )
        .sortable_by_key(|q: &StockQuote| cents(q.change)),
        colored_text_column(
            "Chg%",
            80.0,
            CellAlign::End,
            |q: &StockQuote| format!("{:+.2}%", q.change_pct),
            |q: &StockQuote, theme: &Theme| change_color(q.change_pct, theme),
        )
        .sortable_by_key(|q: &StockQuote| bps(q.change_pct)),
        text_column("Open", 90.0, CellAlign::End, |q: &StockQuote| {
            format!("${:.2}", q.open)
        })
        .sortable_by_key(|q: &StockQuote| cents(q.open)),
        text_column("High", 90.0, CellAlign::End, |q: &StockQuote| {
            format!("${:.2}", q.high)
        })
        .sortable_by_key(|q: &StockQuote| cents(q.high)),
        text_column("Low", 90.0, CellAlign::End, |q: &StockQuote| {
            format!("${:.2}", q.low)
        })
        .sortable_by_key(|q: &StockQuote| cents(q.low)),
        text_column("Volume", 110.0, CellAlign::End, |q: &StockQuote| {
            fmt_compact(q.volume)
        })
        .sortable_by_key(|q: &StockQuote| q.volume),
        text_column("Avg Vol", 110.0, CellAlign::End, |q: &StockQuote| {
            fmt_compact(q.avg_volume)
        })
        .sortable_by_key(|q: &StockQuote| q.avg_volume),
        text_column("Mkt Cap", 110.0, CellAlign::End, |q: &StockQuote| {
            fmt_compact(q.market_cap)
        })
        .sortable_by_key(|q: &StockQuote| q.market_cap),
        optional_text_column("P/E", 80.0, CellAlign::End, |q: &StockQuote| {
            q.pe.map(|p| format!("{p:.1}"))
        })
        // `Option<i64>` is `Ord` (None sorts first), clustering N/A P/Es.
        .sortable_by_key(|q: &StockQuote| q.pe.map(bps)),
        text_column("EPS", 80.0, CellAlign::End, |q: &StockQuote| {
            format!("${:.2}", q.eps)
        })
        .sortable_by_key(|q: &StockQuote| cents(q.eps)),
        optional_text_column("Div %", 80.0, CellAlign::End, |q: &StockQuote| {
            q.div_yield.map(|d| format!("{d:.2}%"))
        })
        .sortable_by_key(|q: &StockQuote| q.div_yield.map(bps)),
        text_column("52W H", 90.0, CellAlign::End, |q: &StockQuote| {
            format!("${:.2}", q.week52_high)
        })
        .sortable_by_key(|q: &StockQuote| cents(q.week52_high)),
        text_column("52W L", 90.0, CellAlign::End, |q: &StockQuote| {
            format!("${:.2}", q.week52_low)
        })
        .sortable_by_key(|q: &StockQuote| cents(q.week52_low)),
        // Trailing space + a little extra width so the right-aligned Beta
        // value keeps a visible gap from the left-aligned Sector column
        // that follows it (cells have no internal horizontal padding).
        text_column("Beta", 84.0, CellAlign::End, |q: &StockQuote| {
            format!("{:.2}  ", q.beta)
        })
        .sortable_by_key(|q: &StockQuote| bps(q.beta)),
        text_column("Sector", 170.0, CellAlign::Start, |q: &StockQuote| {
            q.sector.to_string()
        })
        // Low-cardinality → a great multi-sort primary ("Sector then
        // Mkt Cap" groups by sector and ranks by size within each).
        .sortable_by_key(|q: &StockQuote| q.sector)
        .filterable_by_text(|q: &StockQuote| q.sector.to_string()),
    ]
}

/// Formats a large count compactly (`41.3M`, `3.48T`) for volume / market
/// cap cells. Display-only; the columns sort by the raw integer.
#[expect(clippy::cast_precision_loss, reason = "Display only")]
fn fmt_compact(n: u64) -> String {
    let v = n as f64;
    if v >= 1e12 {
        format!("{:.2}T", v / 1e12)
    } else if v >= 1e9 {
        format!("{:.1}B", v / 1e9)
    } else if v >= 1e6 {
        format!("{:.1}M", v / 1e6)
    } else if v >= 1e3 {
        format!("{:.1}K", v / 1e3)
    } else {
        n.to_string()
    }
}

/// Arranges [`stock_columns`] by a `layout` of [`ColumnId`]s — the
/// stock-board analogue of [`arrange_columns`] (show/hide + reorder).
#[must_use]
pub fn arrange_stock_columns<S: 'static>(layout: &[ColumnId]) -> Vec<ColumnDef<StockQuote, S>> {
    let mut defs = stock_columns::<S>();
    layout
        .iter()
        .filter_map(|id| {
            defs.iter()
                .position(|d| &d.effective_id() == id)
                .map(|pos| defs.remove(pos))
        })
        .collect()
}

/// Stable u64 row-id for a [`StockQuote`], used as the grid's `row_id`.
///
/// `symbol` is a `&'static str` literal, so each distinct ticker is a
/// distinct static object with a unique address — the pointer is therefore
/// a guaranteed-unique identity (no hash collisions possible).
#[must_use]
pub fn stock_row_id(q: &StockQuote) -> u64 {
    q.symbol.as_ptr() as u64
}

// ===========================================================================
// MARK: Gallery panel
// ===========================================================================

type InnerView = Box<AnyWidgetView<Demo>>;
type InnerViewState = <InnerView as View<Demo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the data-grid demo panel (internal to the
/// [`View`] impl; not part of the gallery's app state).
pub struct DataGridDemoPanelState {
    demo: Demo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// The data-grid gallery panel, returned by [`panel`].
pub struct DataGridDemoPanel {
    theme: Theme,
}

/// Renders the data-grid demo panel.
///
/// The panel owns its own [`Demo`] state (100k initial ticks). The toolbar
/// lets you add rows, bulk-select, and show/hide/reorder columns. Click a
/// header to sort; type in a filter input to filter.
#[must_use]
pub fn panel(theme: &Theme) -> DataGridDemoPanel {
    DataGridDemoPanel { theme: *theme }
}

fn build_inner(theme: &Theme, demo: &Demo) -> impl WidgetView<Demo> + use<> {
    let dg_visible_len = if demo.view_is_materialized() {
        demo.visible.len()
    } else {
        demo.ticks.len()
    };
    let row_count = u64::try_from(dg_visible_len).unwrap_or(u64::MAX);
    let base_time_ns = demo.ticks.first().map_or(0, |t| t.event_ns);
    let sort = demo.sort.clone();
    let filter = demo.filter.clone();
    let widths = demo.column_widths.clone();
    let column_layout = demo.column_layout();

    let columns = arrange_columns::<Demo>(&column_layout, base_time_ns);
    let notional_id = ColumnId::from("Notional");
    let notional_shown = column_layout.contains(&notional_id);
    let theme_copy = *theme;

    let toolbar = flex_row((
        crate::components::button::button(|s: &mut Demo| {
            s.append_n(100);
        })
        .label("Add 100 ticks")
        .render(theme),
        crate::components::button::button(|s: &mut Demo| {
            s.append_n(10_000);
        })
        .label("Add 10k ticks")
        .render(theme),
        crate::components::button::button(|s: &mut Demo| {
            s.select_first(50);
        })
        .label("Select 0..50")
        .render(theme),
        crate::components::button::button(|s: &mut Demo| {
            s.clear_selection();
        })
        .label("Clear selection")
        .render(theme),
        FlexSpacer::Flex(1.0),
        crate::components::button::button(move |s: &mut Demo| {
            s.toggle_column(&ColumnId::from("Notional"));
        })
        .label(if notional_shown {
            "Hide Notional"
        } else {
            "Show Notional"
        })
        .render(theme),
        crate::components::button::button(|s: &mut Demo| {
            s.move_column_left(&ColumnId::from("Price"));
        })
        .label("Price \u{2190}")
        .render(theme),
        crate::components::button::button(|s: &mut Demo| {
            s.reset_columns();
        })
        .label("Reset cols")
        .render(theme),
        FlexSpacer::Flex(1.0),
        crate::label(format!("{row_count} rows"))
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0));

    let grid = super::view::data_grid(columns)
        .rows(|s: &Demo| {
            if s.view_is_materialized() {
                &s.visible[..]
            } else {
                &s.ticks[..]
            }
        })
        .row_count(row_count)
        .row_id(|t: &DemoTick| t.id)
        .selection(|s: &mut Demo| &mut s.selection)
        .sort(sort, |s: &mut Demo, col: ColumnId, multi: bool| {
            s.cycle_sort(col, multi);
        })
        .filter(filter, |s: &mut Demo, col: ColumnId, query: String| {
            s.set_filter(col, query);
        })
        .column_widths(widths)
        .on_column_resize(|s: &mut Demo, col: ColumnId, new_width: f64| {
            s.resize_column(col, new_width);
        })
        .row_height(22.0)
        .render(&theme_copy);

    flex_col((toolbar, sized_box(grid).flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(12.0))
}

// ===========================================================================
// MARK: Stock-quotes gallery panel
// ===========================================================================

type StockInnerView = Box<AnyWidgetView<StockDemo>>;
type StockInnerViewState = <StockInnerView as View<StockDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the stock-quotes demo panel.
pub struct StockQuotesDemoPanelState {
    demo: StockDemo,
    inner_view: StockInnerView,
    inner_state: StockInnerViewState,
}

/// The stock-quotes gallery panel, returned by [`stock_quotes_panel`].
pub struct StockQuotesDemoPanel {
    theme: Theme,
}

/// Renders the stock-quotes ("value lens") demo panel.
///
/// The panel owns its own [`StockDemo`] (static NASDAQ-style snapshot).
/// Same grid wiring as [`panel`], only the row type and data source differ.
#[must_use]
pub fn stock_quotes_panel(theme: &Theme) -> StockQuotesDemoPanel {
    StockQuotesDemoPanel { theme: *theme }
}

fn build_stock_inner(theme: &Theme, demo: &StockDemo) -> impl WidgetView<StockDemo> + use<> {
    let sq_visible_len = if demo.view_is_materialized() {
        demo.visible.len()
    } else {
        demo.quotes.len()
    };
    let row_count = u64::try_from(sq_visible_len).unwrap_or(u64::MAX);
    let sort = demo.sort.clone();
    let filter = demo.filter.clone();
    let widths = demo.column_widths.clone();
    let column_layout = demo.column_layout();

    let columns = arrange_stock_columns::<StockDemo>(&column_layout);
    let beta_id = ColumnId::from("Beta");
    let beta_shown = column_layout.contains(&beta_id);
    let theme_copy = *theme;

    let toolbar = flex_row((
        crate::label("NASDAQ symbols — static snapshot")
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
        FlexSpacer::Flex(1.0),
        crate::components::button::button(move |s: &mut StockDemo| {
            s.toggle_column(&ColumnId::from("Beta"));
        })
        .label(if beta_shown { "Hide Beta" } else { "Show Beta" })
        .render(theme),
        crate::components::button::button(|s: &mut StockDemo| {
            s.move_column_left(&ColumnId::from("Sector"));
        })
        .label("Sector \u{2190}")
        .render(theme),
        crate::components::button::button(|s: &mut StockDemo| {
            s.reset_columns();
        })
        .label("Reset cols")
        .render(theme),
        FlexSpacer::Flex(1.0),
        crate::label(format!("{row_count} symbols"))
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0));

    let grid = super::view::data_grid(columns)
        .rows(|s: &StockDemo| {
            if s.view_is_materialized() {
                &s.visible[..]
            } else {
                &s.quotes[..]
            }
        })
        .row_count(row_count)
        .row_id(stock_row_id)
        .selection(|s: &mut StockDemo| &mut s.selection)
        .sort(sort, |s: &mut StockDemo, col: ColumnId, multi: bool| {
            s.cycle_sort(col, multi);
        })
        .filter(filter, |s: &mut StockDemo, col: ColumnId, query: String| {
            s.set_filter(col, query);
        })
        .column_widths(widths)
        .on_column_resize(|s: &mut StockDemo, col: ColumnId, new_width: f64| {
            s.resize_column(col, new_width);
        })
        .row_height(24.0)
        .render(&theme_copy);

    flex_col((toolbar, sized_box(grid).flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(12.0))
}

impl ViewMarker for StockQuotesDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for StockQuotesDemoPanel {
    type ViewState = StockQuotesDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = StockDemo::new();
        let inner_view: StockInnerView = Box::new(build_stock_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            StockQuotesDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut StockQuotesDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: StockInnerView = Box::new(build_stock_inner(&self.theme, &vs.demo));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.demo,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut StockQuotesDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut StockQuotesDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}

impl ViewMarker for DataGridDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for DataGridDemoPanel {
    type ViewState = DataGridDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = Demo::with_initial(100_000);
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            DataGridDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut DataGridDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.demo));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.demo,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut DataGridDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut DataGridDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ColumnId, Demo, DemoSide, DemoTick, StockDemo, arrange_columns, arrange_stock_columns,
        side_color, stock_columns,
    };
    use crate::Theme;

    /// The `Side` column's stable filter id is its title.
    fn side_id() -> crate::components::data_grid::column::ColumnId {
        crate::components::data_grid::column::ColumnId::from("Side")
    }

    #[test]
    fn side_color_uses_trading_palette() {
        let theme = Theme::default();
        assert_eq!(side_color(Some(DemoSide::Buy), &theme), theme.palette.green);
        assert_eq!(
            side_color(Some(DemoSide::Sell), &theme),
            theme.palette.coral
        );
        assert_eq!(side_color(None, &theme), theme.palette.text_faint);
    }

    #[test]
    fn filtering_side_keeps_only_matching_rows() {
        let mut demo = Demo::with_initial(200);
        demo.set_filter(side_id(), "B");
        assert!(
            !demo.visible.is_empty(),
            "200 deterministic ticks should include some buys"
        );
        assert!(
            demo.visible
                .iter()
                .all(|t| matches!(t.side, Some(DemoSide::Buy))),
            "every visible row must be a buy under the 'B' filter"
        );
    }

    #[test]
    fn clearing_filter_empties_the_materialized_view() {
        let mut demo = Demo::with_initial(64);
        demo.set_filter(side_id(), "S");
        demo.clear_filter();
        // With no active filter *or* sort the lens reads `ticks`
        // directly, so the materialized view is dropped.
        assert!(demo.visible.is_empty());
        assert!(demo.filter.is_empty());
    }

    /// The `Price` column's stable sort id is its title.
    fn price_id() -> crate::components::data_grid::column::ColumnId {
        crate::components::data_grid::column::ColumnId::from("Price")
    }

    /// Price sort key (matches the Price column's `sortable_by_key`).
    /// Prices snap to whole cents in `next_tick`, so the raw value is an
    /// exact cent multiple — display and sort agree, and equal-priced
    /// rows genuinely tie.
    fn price_cents(t: &DemoTick) -> i64 {
        t.price_units
    }

    #[test]
    fn sorting_materializes_the_view_in_price_order() {
        let mut demo = Demo::with_initial(200);
        demo.cycle_sort(price_id(), false); // ascending
        assert_eq!(demo.visible.len(), demo.ticks.len());
        assert!(
            demo.visible
                .windows(2)
                .all(|w| price_cents(&w[0]) <= price_cents(&w[1])),
            "visible rows must be in ascending price order"
        );
        // A second cycle flips to descending.
        demo.cycle_sort(price_id(), false);
        assert!(
            demo.visible
                .windows(2)
                .all(|w| price_cents(&w[0]) >= price_cents(&w[1])),
            "visible rows must be in descending price order"
        );
        // A third cycle clears the sort; with no filter either, the view
        // de-materializes back to reading `ticks` directly.
        demo.cycle_sort(price_id(), false);
        assert!(demo.visible.is_empty());
        assert_eq!(demo.sort.primary(), None);
    }

    #[test]
    fn select_first_keys_by_stable_id_in_display_order() {
        let mut demo = Demo::with_initial(50);
        // Sort descending by price so display order differs from natural.
        demo.cycle_sort(price_id(), false);
        demo.cycle_sort(price_id(), false);
        demo.select_first(3);
        // The selected ids must be exactly the first three visible rows'
        // ids — i.e. selection tracks what the user sees, not positions.
        let want: Vec<u64> = demo.visible.iter().take(3).map(|t| t.id).collect();
        let mut got: Vec<u64> = demo.selection.iter().collect();
        got.sort_unstable();
        let mut want_sorted = want.clone();
        want_sorted.sort_unstable();
        assert_eq!(got, want_sorted);
    }

    /// THE #1 guarantee, end-to-end through the real host path: a
    /// selection keyed by stable id follows its row across a sort, where
    /// a positional key would have pointed at a different row.
    #[test]
    fn selection_follows_the_row_across_a_sort() {
        let mut demo = Demo::with_initial(200);

        // Pick the row with the globally-max price, at its *natural*
        // position. Under an ascending price sort it must land at the
        // very end, so unless it was already last its display position
        // provably changes — no reliance on seed luck. (max_by gives the
        // first max on ties, a single deterministic row.)
        let picked_pos = demo
            .ticks
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.price_units.cmp(&b.price_units))
            .map(|(i, _)| i)
            .expect("non-empty");
        let picked_id = demo.ticks[picked_pos].id;
        assert!(
            picked_pos < demo.ticks.len() - 1,
            "max-price row isn't already last"
        );
        demo.selection.replace_with(picked_id);

        // Sort ascending by price; the host materializes `visible`.
        demo.cycle_sort(price_id(), false);
        assert!(
            demo.visible
                .windows(2)
                .all(|w| price_cents(&w[0]) <= price_cents(&w[1])),
            "precondition: a real reorder happened"
        );

        // The id is still selected — selection followed the row.
        assert!(
            demo.selection.contains(picked_id),
            "the selected id must survive the sort"
        );

        // It tracked the *row*, not the slot: the max-price row is now
        // last, a provably different display position. A positional key
        // (still pointing at the old natural index) would select whatever
        // row now occupies that slot — a different row.
        let new_pos = demo
            .visible
            .iter()
            .position(|t| t.id == picked_id)
            .expect("selected row must still be present in the view");
        assert_eq!(
            new_pos,
            demo.visible.len() - 1,
            "max-price row sorts to the end"
        );
        assert_ne!(
            new_pos, picked_pos,
            "the selected row moved; an index key would now mis-select"
        );
        assert_ne!(
            demo.visible[picked_pos].id, picked_id,
            "a different row occupies the old slot — index keying would break here"
        );
    }

    /// Selection is id-keyed and independent of the current view: a row
    /// filtered out of view keeps its selection and is still selected
    /// when it returns. (Complements the `visual_range_ids` unit test for
    /// the anchor-filtered-out shift-extend degrade.)
    #[test]
    fn selection_persists_across_a_filter_that_hides_the_row() {
        let mut demo = Demo::with_initial(200);

        // Find a buy and a sell so we can filter one out deterministically.
        let buy_id = demo
            .ticks
            .iter()
            .find(|t| matches!(t.side, Some(DemoSide::Buy)))
            .expect("200 ticks include a buy")
            .id;
        demo.selection.replace_with(buy_id);

        // Filter to sells only — the selected buy leaves the view.
        demo.set_filter(side_id(), "S");
        assert!(
            !demo.visible.iter().any(|t| t.id == buy_id),
            "the selected buy must be filtered out of the view"
        );
        // The selection set still holds the id (it's view-independent).
        assert!(
            demo.selection.contains(buy_id),
            "selection is id-keyed, so a filtered-out row stays selected"
        );

        // Clear the filter; the row returns and is still selected.
        demo.clear_filter();
        assert!(demo.selection.contains(buy_id));
        assert!(demo.ticks.iter().any(|t| t.id == buy_id));
    }

    #[test]
    fn hiding_a_column_removes_it_from_the_arranged_set() {
        let mut demo = Demo::with_initial(8);
        let full = demo.column_layout().len();
        demo.toggle_column(&price_id()); // hide Price
        let layout = demo.column_layout();
        assert_eq!(layout.len(), full - 1);
        assert!(!layout.contains(&price_id()), "Price is hidden");
        // The arranged columns the grid would receive omit Price.
        let cols = arrange_columns::<()>(&layout, 0);
        assert!(!cols.iter().any(|c| c.effective_id() == price_id()));
        // Showing it again appends it back.
        demo.toggle_column(&price_id());
        assert!(demo.column_layout().contains(&price_id()));
    }

    /// Hiding a column must clear its filter: with the column gone there
    /// is no filter input (or header indicator) left on screen, so a
    /// surviving query would keep trimming rows with no visible cause.
    #[test]
    fn hiding_a_column_clears_its_filter() {
        let mut demo = Demo::with_initial(64);
        demo.set_filter(side_id(), "B");
        assert!(demo.view_is_materialized());

        demo.toggle_column(&side_id()); // hide the filtered column
        assert!(
            demo.filter.get(&side_id()).is_none(),
            "hidden column's filter is cleared"
        );
        // No criteria remain → the view de-materializes; nothing is
        // invisibly trimmed.
        assert!(!demo.view_is_materialized());
        assert!(demo.visible.is_empty());
    }

    /// Hiding a column must drop its sort level for the same reason: no
    /// header arrow remains to explain (or clear) the ordering.
    #[test]
    fn hiding_a_column_removes_its_sort_level() {
        let mut demo = Demo::with_initial(64);
        demo.cycle_sort(side_id(), false); // primary: Side
        demo.cycle_sort(price_id(), true); // tiebreaker: Price

        demo.toggle_column(&price_id()); // hide the tiebreaker column
        assert_eq!(
            demo.sort.direction_for(&price_id()),
            None,
            "hidden column's sort level is removed"
        );
        // The visible column's sort is untouched.
        assert_eq!(demo.sort.primary(), Some(&side_id()));

        // Re-showing the column doesn't resurrect the cleared state.
        demo.toggle_column(&price_id());
        assert_eq!(demo.sort.direction_for(&price_id()), None);
    }

    /// Same contract through the stock demo's mirror implementation.
    #[test]
    fn stock_demo_hiding_a_column_clears_its_criteria() {
        let mut demo = StockDemo::new();
        let sector = ColumnId::from("Sector");
        demo.set_filter(sector.clone(), "tech");
        demo.cycle_sort(sector.clone(), false);
        assert!(demo.view_is_materialized());

        demo.toggle_column(&sector);
        assert!(demo.filter.get(&sector).is_none());
        assert_eq!(demo.sort.direction_for(&sector), None);
        assert!(!demo.view_is_materialized());
        assert!(demo.visible.is_empty());
    }

    #[test]
    fn sort_survives_hiding_then_showing_a_different_column() {
        // The payoff: id-keyed sort state is untouched by a layout change
        // to *another* column. (A positional key would have shifted.)
        let mut demo = Demo::with_initial(50);
        demo.cycle_sort(price_id(), false); // ascending by Price
        assert_eq!(demo.sort.primary(), Some(&price_id()));

        // Hide, then show, an unrelated column (Notional).
        let notional = ColumnId::from("Notional");
        demo.toggle_column(&notional);
        demo.toggle_column(&notional);

        // Sort is still on Price, ascending — and the view is still
        // correctly ordered by price.
        assert_eq!(demo.sort.primary(), Some(&price_id()));
        assert!(
            demo.visible
                .windows(2)
                .all(|w| price_cents(&w[0]) <= price_cents(&w[1])),
            "Price sort still holds after the column-layout change"
        );
    }

    #[test]
    fn reordering_columns_preserves_an_active_sort() {
        let mut demo = Demo::with_initial(50);
        demo.cycle_sort(price_id(), false); // ascending by Price

        // Move Price one slot left — its display position changes.
        demo.move_column_left(&price_id());

        // Sort still keyed to Price; order still by price.
        assert_eq!(demo.sort.primary(), Some(&price_id()));
        assert!(
            demo.visible
                .windows(2)
                .all(|w| price_cents(&w[0]) <= price_cents(&w[1])),
            "Price sort follows the column across a reorder"
        );
    }

    /// Replays the exact gallery click sequence reported as broken:
    /// plain-click Side, Shift+click Price, Shift+click Time — and asserts
    /// the materialized `visible` view obeys all three levels in priority
    /// order. Proves the multi-sort works through the *real demo* path
    /// (`columns` + `refresh_visible`), not just the isolated
    /// `sort_indices`.
    #[test]
    fn gallery_three_level_sort_orders_visible_correctly() {
        let mut demo = Demo::with_initial(500);
        let side = ColumnId::from("Side");
        let time = ColumnId::from("Time (ms)");

        demo.cycle_sort(side.clone(), false); // 1: Side asc (plain)
        demo.cycle_sort(price_id(), true); // 2: Price asc (shift)
        demo.cycle_sort(time.clone(), true); // 3: Time asc (shift)

        // State has the three levels in order.
        assert_eq!(demo.sort.len(), 3, "three sort levels active");
        assert_eq!(demo.sort.priority_of(&side), Some(0));
        assert_eq!(demo.sort.priority_of(&price_id()), Some(1));
        assert_eq!(demo.sort.priority_of(&time), Some(2));

        // The materialized view must be ordered by (Side, Price, event_ns)
        // lexicographically — the exact multi-key contract. Prices are
        // whole cents, so the raw value is the exact sort key.
        let key = |t: &DemoTick| (t.side, t.price_units, t.event_ns);
        assert!(
            demo.visible.windows(2).all(|w| key(&w[0]) <= key(&w[1])),
            "visible rows must obey Side → Price → Time ordering"
        );
        // And the *secondary* level must actually do work: within a single
        // Side group there must be at least one adjacent pair where Side
        // ties and Price strictly increases (else level 2 is moot).
        assert!(
            demo.visible
                .windows(2)
                .any(|w| w[0].side == w[1].side && w[0].price_units < w[1].price_units),
            "the Price tiebreaker must visibly order within a Side group"
        );
        // The *tertiary* level must also do work: a pair where Side AND
        // Price tie but Time strictly increases — proving level 3 fires.
        assert!(
            demo.visible.windows(2).any(|w| {
                w[0].side == w[1].side
                    && w[0].price_units == w[1].price_units
                    && w[0].event_ns < w[1].event_ns
            }),
            "the Time tiebreaker must visibly order within a (Side,Price) group"
        );
    }

    // --- Stock-quote ("value lens") demo ---

    fn sym_id() -> ColumnId {
        ColumnId::from("Symbol")
    }
    fn sector_id() -> ColumnId {
        ColumnId::from("Sector")
    }
    fn mktcap_id() -> ColumnId {
        ColumnId::from("Mkt Cap")
    }

    #[test]
    fn stock_demo_seeds_a_nonempty_snapshot_with_unique_symbols() {
        let demo = StockDemo::new();
        assert!(demo.quotes.len() >= 20, "a real board of symbols");
        // Symbol is the row_id — must be unique across the snapshot.
        let mut syms: Vec<&str> = demo.quotes.iter().map(|q| q.symbol).collect();
        let n = syms.len();
        syms.sort_unstable();
        syms.dedup();
        assert_eq!(syms.len(), n, "every symbol is unique (it's the row_id)");
    }

    #[test]
    fn stock_row_ids_are_unique() {
        let quotes = super::stock_quotes();
        let mut ids: Vec<u64> = quotes.iter().map(super::stock_row_id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "every stock_row_id is unique");
    }

    #[test]
    fn stock_columns_are_all_uniquely_identified() {
        // ColumnId uniqueness (titles are the default ids) — else column
        // state would collide.
        let cols = stock_columns::<()>();
        let mut ids: Vec<ColumnId> = cols.iter().map(super::ColumnDef::effective_id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "every column id is unique");
    }

    #[test]
    fn stock_demo_single_sort_orders_by_symbol() {
        let mut demo = StockDemo::new();
        demo.cycle_sort(sym_id(), false); // Symbol asc
        assert!(
            demo.visible.windows(2).all(|w| w[0].symbol <= w[1].symbol),
            "visible rows ascend by symbol"
        );
        assert_eq!(demo.visible.len(), demo.quotes.len());
    }

    #[test]
    fn stock_demo_sector_then_market_cap_multi_sort() {
        // The canonical board view: group by Sector, rank by Mkt Cap
        // descending within each sector.
        let mut demo = StockDemo::new();
        demo.cycle_sort(sector_id(), false); // 1: Sector asc
        demo.cycle_sort(mktcap_id(), true); // 2: Mkt Cap asc...
        demo.cycle_sort(mktcap_id(), true); // ...then desc
        assert_eq!(demo.sort.len(), 2);

        let key = |q: &super::StockQuote| (q.sector, std::cmp::Reverse(q.market_cap));
        assert!(
            demo.visible.windows(2).all(|w| key(&w[0]) <= key(&w[1])),
            "grouped by sector, market-cap-desc within each"
        );
        // The 2nd level must actually order within a sector group.
        assert!(
            demo.visible
                .windows(2)
                .any(|w| { w[0].sector == w[1].sector && w[0].market_cap > w[1].market_cap }),
            "Mkt Cap tiebreaker orders within a Sector group"
        );
    }

    #[test]
    fn stock_demo_filter_by_sector_keeps_only_matches() {
        let mut demo = StockDemo::new();
        demo.set_filter(sector_id(), "Technology");
        assert!(!demo.visible.is_empty());
        assert!(
            demo.visible.iter().all(|q| q.sector == "Technology"),
            "only Technology rows survive the sector filter"
        );
    }

    #[test]
    fn arrange_stock_columns_hides_by_omission() {
        let all: Vec<ColumnId> = stock_columns::<()>()
            .iter()
            .map(super::ColumnDef::effective_id)
            .collect();
        // Drop "Beta" from the layout → it's absent from the arranged set.
        let layout: Vec<ColumnId> = all
            .into_iter()
            .filter(|id| id != &ColumnId::from("Beta"))
            .collect();
        let cols = arrange_stock_columns::<()>(&layout);
        assert!(
            !cols
                .iter()
                .any(|c| c.effective_id() == ColumnId::from("Beta"))
        );
    }
}
