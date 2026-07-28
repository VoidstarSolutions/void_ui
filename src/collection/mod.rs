//! Crate-internal substrate shared by the virtualized collection
//! components (`data_grid` and `list`).
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
mod item_row;
mod item_row_view;
mod overlay_list_body;
pub(crate) mod row_click;
mod scroll;
mod selection;
pub(crate) mod single_child;

// Re-exported for the data_grid widget-tree integration test; the body
// widget itself is constructed internally by `collection_body`.
#[cfg(test)]
pub(crate) use body::CollectionBodyWidget;
pub(crate) use body_view::{
    CollectionBodyParams, Lazy, LeadingHitZoneFn, OnToggle, RenderRow, TreeMetaFn, collection_body,
};
pub(crate) use click::{ItemsFn, OnActivate, SelectionLens, apply_row_activate, apply_row_click};
pub(crate) use ids::{IdSource, nearing_end, scroll_range_end, visual_range_ids};
pub(crate) use item_row::render_overlay_list_item;
pub(crate) use item_row_view::{OnSelect, overlay_list_item};
pub(crate) use overlay_list_body::overlay_list_body;
pub(crate) use row_click::{LeadingHitZone, TreeRowMeta};
pub use scroll::ScrollState;
pub(crate) use scroll::clamp_scroll_index;
pub use selection::SelectionState;
