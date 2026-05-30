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

use super::column::{colored_text_column, optional_text_column, text_column, CellAlign, ColumnDef};
use super::filter::{filtered_indices, FilterState};
use super::selection::SelectionState;
use super::sort::SortState;
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
    /// Currently-selected row indices.
    pub selection: SelectionState,
    /// Active column sort (which column + direction).
    pub sort: SortState,
    /// Active per-column filter queries.
    pub filter: FilterState,
    /// Materialized filtered rows. Only meaningful while `filter` is
    /// non-empty; the gallery's `rows` lens reads `ticks` directly when
    /// unfiltered (avoiding a full-dataset clone in the common case).
    pub visible: Vec<DemoTick>,
    rng_state: u64,
    last_time_ns: i64,
    last_price_units: i64,
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
            visible: Vec::new(),
            rng_state: 0x0005_DEEC_E66D_u64.wrapping_mul(0xB16B_00B5),
            last_time_ns: 0,
            last_price_units: START_PRICE_UNITS,
        };
        demo.append_n(initial_count);
        demo
    }

    /// Appends `n` newly-generated ticks to the tail. Refreshes the
    /// filtered view if a filter is active so appended rows that match
    /// become visible.
    pub fn append_n(&mut self, n: usize) {
        self.ticks.reserve(n);
        for _ in 0..n {
            let tick = self.next_tick();
            self.ticks.push(tick);
        }
        if !self.filter.is_empty() {
            self.refresh_visible();
        }
    }

    /// Sets a column's filter query and refreshes the visible view.
    /// Demonstrates the host-side filtering path: the host owns the
    /// data and runs [`filtered_indices`] over it.
    pub fn set_filter(&mut self, column: usize, query: impl Into<String>) {
        self.filter.set(column, query);
        self.refresh_visible();
    }

    /// Clears every column filter and the materialized view.
    pub fn clear_filter(&mut self) {
        self.filter.clear_all();
        self.refresh_visible();
    }

    /// Recomputes [`Self::visible`] from `ticks` + `filter`. A no-op'd
    /// (cleared) view when no filter is active — the lens falls back to
    /// `ticks` directly in that case.
    fn refresh_visible(&mut self) {
        if self.filter.is_empty() {
            self.visible.clear();
            return;
        }
        // `tick_columns` carries the per-column filter predicates; the
        // `State`/`base_time_ns` args don't affect filtering.
        let columns = tick_columns::<()>(0);
        let idx = filtered_indices(&self.ticks, &self.filter, &columns);
        self.visible = idx.into_iter().map(|i| self.ticks[i]).collect();
    }

    /// Replaces the current selection with rows `0..n`. Bulk-
    /// selection helper used by the gallery demo's toolbar as a
    /// quick alternate to clicking individual rows.
    pub fn select_first(&mut self, n: u64) {
        self.selection.clear();
        if n == 0 {
            return;
        }
        self.selection.replace_with(0);
        if n > 1 {
            self.selection.extend_to(n - 1);
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
        DemoTick {
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
    ]
}

#[cfg(test)]
mod tests {
    use super::{side_color, Demo, DemoSide};
    use crate::Theme;

    /// Side is column index 3 in `tick_columns`.
    const SIDE_COL: usize = 3;

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
        demo.set_filter(SIDE_COL, "B");
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
        demo.set_filter(SIDE_COL, "S");
        demo.clear_filter();
        // With no active filter the lens reads `ticks` directly, so the
        // materialized view is dropped.
        assert!(demo.visible.is_empty());
        assert!(demo.filter.is_empty());
    }
}
