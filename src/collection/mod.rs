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

mod body;
mod body_view;
mod click;
mod ids;
pub(crate) mod row_click;
mod scroll;
mod selection;
pub(crate) mod single_child;

#[expect(
    unused_imports,
    reason = "consumed by the data_grid migration in the next task"
)]
pub(crate) use body::CollectionBodyWidget;
#[expect(
    unused_imports,
    reason = "consumed by the data_grid migration in the next task"
)]
pub(crate) use body_view::{CollectionBodyParams, Lazy, RenderRow, collection_body};
pub(crate) use click::{ItemsFn, SelectionLens, apply_row_click};
pub(crate) use ids::{IdSource, scroll_idx_to_slice, scroll_range_end, visual_range_ids};
pub(crate) use row_click::clickable_row;
pub use scroll::ScrollState;
pub(crate) use scroll::clamp_scroll_index;
pub use selection::SelectionState;
