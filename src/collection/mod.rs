//! Crate-internal substrate shared by the virtualized collection
//! components (`data_grid`, and later `list`).
//!
//! Owns the row-virtualization machinery both components need: the
//! selection model, programmatic scroll requests, stable-id keying, the
//! shift/toggle/replace click application, and the unified virtualized
//! body widget. Components supply per-row *content* through a closure;
//! the substrate owns everything vertical (virtualization, scroll-to,
//! lazy-load, keyboard nav, click routing).
//!
//! Not public: the only consumers are in-crate. `SelectionState` and
//! `ScrollState` are surfaced to consumers by re-export from the
//! components and the crate root.

mod selection;

pub use selection::SelectionState;
