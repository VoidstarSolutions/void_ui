//! Synthetic tick generator + helpers for the data grid gallery panel.
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
//!
//! The gallery wires these into its app state and dispatches to a
//! locally-defined panel function (see `examples/gallery.rs`).
//!
//! Types here are intentionally self-contained — `void_ui` ships as a
//! generic component library and must not depend on any
//! market-data crate.

use xilem::peniko::Color;

use super::column::{
    colored_text_column, optional_text_column, text_column, CellAlign, ColumnDef, ColumnId,
};
use super::filter::{filtered_indices, FilterState};
use super::selection::SelectionState;
use super::sort::{sort_indices, SortState};
use super::width::ColumnWidths;
use crate::Theme;

const START_PRICE_UNITS: i64 = 100_000_000_000; // $100.00 in 1e-9 units.
const TICK_INTERVAL_NS: i64 = 100_000_000; // 100 ms between synthetic trades.
const PRICE_STEP_UNITS: i64 = 50_000_000; // ±$0.05 per tick.
const PRICE_UNITS_PER_DOLLAR: f64 = 1_000_000_000.0;

/// Aggressor side of a synthetic trade.
#[derive(Debug, Clone, Copy)]
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
        !self.filter.is_empty() || self.sort.column().is_some()
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
    /// re-derives the ordered view. Demonstrates the host-side sorting
    /// path — the mirror of [`Self::set_filter`]: the host owns row order
    /// and composes [`filtered_indices`] then [`sort_indices`].
    pub fn cycle_sort(&mut self, column: usize) {
        self.sort.cycle(column);
        self.refresh_visible();
    }

    /// Sets a column's width override (drag-to-resize), keyed by the
    /// column's stable [`ColumnId`]. `ColumnWidths` clamps to the minimum
    /// width; no data refresh is needed.
    pub fn resize_column(&mut self, column: ColumnId, new_width: f64) {
        self.column_widths.set(column, new_width);
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
        sort_indices(&mut idx, &self.ticks, self.sort, &columns);
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
        // Map raw bits into `±PRICE_STEP_UNITS`.
        #[expect(
            clippy::cast_possible_wrap,
            reason = "Wrapping is the intended behavior"
        )]
        let raw_i = raw as i64;
        let price_delta = (raw_i.rem_euclid(2 * PRICE_STEP_UNITS)) - PRICE_STEP_UNITS;
        self.last_price_units = self.last_price_units.saturating_add(price_delta);
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
        .sortable_by_key(|t: &DemoTick| t.price_units),
        text_column("Ask", 100.0, CellAlign::End, |t: &DemoTick| {
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let ask = (t.price_units + 10_000_000) as f64 / PRICE_UNITS_PER_DOLLAR;
            format!("${ask:.2}")
        })
        .sortable_by_key(|t: &DemoTick| t.price_units),
        text_column("Spread", 90.0, CellAlign::End, |_t: &DemoTick| {
            "$0.02".to_string()
        }),
        text_column("Notional", 130.0, CellAlign::End, |t: &DemoTick| {
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let px = t.price_units as f64 / PRICE_UNITS_PER_DOLLAR;
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let sz = t.size.unwrap_or(0) as f64;
            format!("${:.0}", px * sz)
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

/// Synthetic exchange code for the demo blotter, rotated by event time
/// so the `Exchange` column has a few distinct filterable values.
fn demo_exchange(event_ns: i64) -> &'static str {
    match (event_ns / TICK_INTERVAL_NS) % 3 {
        0 => "NYSE",
        1 => "NSDQ",
        _ => "ARCA",
    }
}

#[cfg(test)]
mod tests {
    use super::{side_color, Demo, DemoSide};
    use crate::Theme;

    /// The `Side` column's stable filter id is its title.
    fn side_id() -> crate::components::data_grid::column::ColumnId {
        crate::components::data_grid::column::ColumnId::from("Side")
    }

    #[test]
    fn side_color_uses_trading_palette() {
        let theme = Theme::default();
        assert_eq!(side_color(Some(DemoSide::Buy), &theme), theme.palette.green);
        assert_eq!(side_color(Some(DemoSide::Sell), &theme), theme.palette.coral);
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

    /// Price is column index 1 in `tick_columns`.
    const PRICE_COL: usize = 1;

    #[test]
    fn sorting_materializes_the_view_in_price_order() {
        let mut demo = Demo::with_initial(200);
        demo.cycle_sort(PRICE_COL); // ascending
        assert_eq!(demo.visible.len(), demo.ticks.len());
        assert!(
            demo.visible.windows(2).all(|w| w[0].price_units <= w[1].price_units),
            "visible rows must be in ascending price order"
        );
        // A second cycle flips to descending.
        demo.cycle_sort(PRICE_COL);
        assert!(
            demo.visible.windows(2).all(|w| w[0].price_units >= w[1].price_units),
            "visible rows must be in descending price order"
        );
        // A third cycle clears the sort; with no filter either, the view
        // de-materializes back to reading `ticks` directly.
        demo.cycle_sort(PRICE_COL);
        assert!(demo.visible.is_empty());
        assert_eq!(demo.sort.column(), None);
    }

    #[test]
    fn select_first_keys_by_stable_id_in_display_order() {
        let mut demo = Demo::with_initial(50);
        // Sort descending by price so display order differs from natural.
        demo.cycle_sort(PRICE_COL);
        demo.cycle_sort(PRICE_COL);
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
        assert!(picked_pos < demo.ticks.len() - 1, "max-price row isn't already last");
        demo.selection.replace_with(picked_id);

        // Sort ascending by price; the host materializes `visible`.
        demo.cycle_sort(PRICE_COL);
        assert!(
            demo.visible.windows(2).all(|w| w[0].price_units <= w[1].price_units),
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
        assert_eq!(new_pos, demo.visible.len() - 1, "max-price row sorts to the end");
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
}
