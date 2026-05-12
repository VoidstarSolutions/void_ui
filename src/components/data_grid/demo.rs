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
//! - [`tick_columns`] — column descriptors for browsing `Tick`s.
//!
//! The gallery wires these into its app state and dispatches to a
//! locally-defined panel function (see `examples/gallery.rs`).

use citadel_core::{Price, Side, Tick, Timestamp, Timestamps, Volume};

use super::column::{CellAlign, ColumnDef, optional_text_column, text_column};
use super::selection::SelectionState;

const START_PRICE_UNITS: i64 = 100_000_000_000; // $100.00 in 1e-9 units.
const TICK_INTERVAL_NS: i64 = 100_000_000; // 100 ms between synthetic trades.
const PRICE_STEP_UNITS: i64 = 50_000_000; // ±$0.05 per tick.

/// Demo state. Lives as a field on the gallery's app state.
#[derive(Debug, Clone)]
pub struct Demo {
    /// The synthetic tick history.
    pub ticks: Vec<Tick>,
    /// Currently-selected row indices.
    pub selection: SelectionState,
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
            rng_state: 0x0005_DEEC_E66D_u64.wrapping_mul(0xB16B_00B5),
            last_time_ns: 0,
            last_price_units: START_PRICE_UNITS,
        };
        demo.append_n(initial_count);
        demo
    }

    /// Appends `n` newly-generated ticks to the tail.
    pub fn append_n(&mut self, n: usize) {
        self.ticks.reserve(n);
        for _ in 0..n {
            let tick = self.next_tick();
            self.ticks.push(tick);
        }
    }

    /// Replaces the current selection with rows `0..n`. Useful for
    /// exercising the clipboard path before a click widget exists.
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

    fn next_tick(&mut self) -> Tick {
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
            Side::Buy
        } else {
            Side::Sell
        };
        Tick {
            timestamps: Timestamps {
                event: Timestamp(self.last_time_ns),
                receive: None,
                ingest: None,
            },
            price: Price::from_units(self.last_price_units),
            size: Some(Volume(trade_size)),
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

/// Builds the four column descriptors for browsing a `Tick` stream:
/// `Time` (relative ms from `base_time_ns`), `Price` (`$X.XX`),
/// `Size`, and `Side` (`B`/`S`/`—`).
///
/// `base_time_ns` is captured by the time column's projector so the
/// displayed values are relative — much easier to read than raw
/// nanosecond timestamps. Pass `ticks.first().map(|t| t.timestamps.event.0)
/// .unwrap_or(0)` from the caller.
#[must_use]
pub fn tick_columns<State: 'static>(base_time_ns: i64) -> Vec<ColumnDef<Tick, State>> {
    vec![
        text_column("Time (ms)", 100.0, CellAlign::End, move |t: &Tick| {
            let delta_ns = t.timestamps.event.0.saturating_sub(base_time_ns);
            // ns → ms with one decimal.
            #[expect(clippy::cast_precision_loss, reason = "Display only")]
            let ms = delta_ns as f64 / 1_000_000.0;
            format!("{ms:.1}")
        }),
        text_column("Price", 90.0, CellAlign::End, |t: &Tick| {
            format!("${:.2}", t.price.to_f64())
        }),
        optional_text_column("Size", 80.0, CellAlign::End, |t: &Tick| {
            t.size.map(|v| v.0.to_string())
        }),
        optional_text_column("Side", 60.0, CellAlign::Center, |t: &Tick| {
            t.side.map(|s| match s {
                Side::Buy => "B".to_string(),
                Side::Sell => "S".to_string(),
            })
        }),
    ]
}
