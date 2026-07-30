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
mod imperative_list;
mod item_row;
mod item_row_view;
mod overlay_list;
mod overlay_list_body;
pub(crate) mod row_click;
mod scroll;
mod selection;
pub(crate) mod single_child;
mod window;

// Re-exported for the data_grid widget-tree integration test; the body
// widget itself is constructed internally by `collection_body`.
#[cfg(test)]
pub(crate) use body::CollectionBodyWidget;
pub(crate) use body_view::{
    CollectionBodyParams, Lazy, LeadingHitZoneFn, OnToggle, RenderRow, TreeMetaFn, collection_body,
};
pub(crate) use click::{ItemsFn, OnActivate, SelectionLens, apply_row_activate, apply_row_click};
pub(crate) use ids::{IdSource, nearing_end, scroll_range_end, visual_range_ids};
// Unlike autocomplete's `SuggestionList`, `dropdown_button`'s `MenuContent`
// needs to reach into the wrapped `CollectionListWidget` from outside this
// module: `ThemedDropdownButton` keeps real keyboard focus on its trigger
// button (not the listbox, unlike autocomplete's Tab-into-listbox model), so
// its own `on_text_event` still drives arrow-key highlight movement and must
// push the result down into `CollectionListWidget::set_highlight` externally
// via `mutate_child_later`/`mutate_later` — the same shape it already used
// for the pre-rewrite `MenuContent::set_highlighted`. That requires naming
// `CollectionListWidget` concretely outside `#[cfg(test)]`, unlike Task 6,
// which never needed to (autocomplete's own highlight/nav lives entirely
// inside `CollectionListWidget` via real focus, so `AutocompleteWidget` never
// downcasts to it). See `src/components/dropdown_button/widget.rs`'s
// `ThemedDropdownButton::set_highlight`.
pub(crate) use imperative_list::CollectionListWidget;
// `render_overlay_list_item` has no non-test production callers (production
// code reaches rows only through `overlay_list`/`overlay_list_body`'s own
// View-driven `virtual_scroll` closure) — `#[cfg(test)]`-gated here, which
// is safe even though `autocomplete::widget`'s and `dropdown_button::widget`'s
// own `#[cfg(test)]` modules import it too: `cfg(test)` is one crate-wide
// compilation flag, not scoped per module, so it stays reachable from any
// other module's own `#[cfg(test)]` code during a test build.
pub(crate) use item_row::OnActivated;
#[cfg(test)]
pub(crate) use item_row::render_overlay_list_item;
pub(crate) use item_row_view::OnSelect;
pub(crate) use overlay_list::overlay_list;
pub(crate) use row_click::{LeadingHitZone, TreeRowMeta};
pub use scroll::ScrollState;
pub(crate) use scroll::clamp_scroll_index;
pub use selection::SelectionState;
