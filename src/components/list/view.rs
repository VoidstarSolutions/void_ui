//! Xilem [`View`](xilem::core::View) composition for the list.
//!
//! `List<State, Item>` is a single-column, row-virtualized list — a
//! stripped-down [`data_grid`](super::super::data_grid) without table
//! chrome (no header/columns/sort/per-column-filter/horizontal scroll). It
//! follows the same host-owns-data model: the list never filters, sorts, or
//! pages its own data. It emits intents — a search-query edit, "near the end
//! of the data" (lazy loading), a scroll-to-index request — and the host
//! supplies the resulting `items`/`item_count`.
//!
//! - **Selection** ([`SelectionState`]) is keyed by a stable item id from
//!   [`List::item_id`], exactly like `data_grid`'s row selection; clicking
//!   an item (with optional Shift / Ctrl-Cmd modifiers) updates it via
//!   [`clickable_row`].
//! - **Scroll to top / scroll to selected** are plain host call sites on
//!   [`ScrollState::scroll_to_index`] — `0` for "top", the selected item's
//!   display position for "selected" — passed in via [`List::scroll_to`].
//! - **Lazy loading** and scroll-anchoring are both handled by
//!   [`body::ListBodyView`].

use std::sync::Arc;

use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{
    AnyFlexChild, CrossAxisAlignment, MainAxisAlignment, flex_col, flex_item, flex_row, label,
    sized_box, virtual_scroll,
};
use xilem::{AnyWidgetView, WidgetView};

use super::body::{ItemIdSource, ListBodyView, LoadMore, scroll_idx_to_slice, scroll_range_end};
use crate::Theme;
use crate::components::data_grid::row_click::{RowClickAction, clickable_row};
use crate::components::data_grid::{ScrollState, SelectionState};
use crate::components::input::input;
use crate::components::spinner::spinner;

/// Boxed item-data accessor (`Fn(&State) -> &[Item]`).
type ItemsFn<State, Item> = Arc<dyn for<'a> Fn(&'a State) -> &'a [Item] + Send + Sync>;
/// Boxed selection lens (`Fn(&mut State) -> &mut SelectionState`).
type SelectionLens<State> =
    Arc<dyn for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync>;
/// Boxed item renderer (`Fn(&Item, selected, &Theme) -> Box<AnyWidgetView<State>>`).
type ItemRenderer<State, Item> =
    Arc<dyn Fn(&Item, bool, &Theme) -> Box<AnyWidgetView<State>> + Send + Sync>;
/// Boxed stable item-id projector (`Fn(&Item) -> u64`).
type ItemIdFn<Item> = Arc<dyn Fn(&Item) -> u64 + Send + Sync>;
/// Boxed search-query-change callback (`Fn(&mut State, query)`).
type SearchChange<State> = Arc<dyn Fn(&mut State, String) + Send + Sync>;

/// Default fixed item height when [`List::item_height`] is unset.
const DEFAULT_ITEM_HEIGHT: f64 = 32.0;
/// Default lazy-load threshold when [`List::load_threshold`] is unset: the
/// host's `on_load_more` fires once the active range comes within this many
/// items of `item_count`.
const DEFAULT_LOAD_THRESHOLD: u64 = 20;

/// Builder for a virtualized, theme-driven list view.
///
/// Construct with [`list`], attach data and behavior through the chained
/// setters, then materialize the xilem view with [`List::render`].
///
/// ```ignore
/// list()
///     .items(|s: &State| &s.items[..])
///     .item_count(n)
///     .item_height(28.0)
///     .render_item(|item, selected, theme| label(item.name.clone()).render(theme))
///     .selection(|s| &mut s.selection)
///     .render(&theme)
/// ```
///
/// All setters are optional — an empty `List` renders an empty body.
#[must_use = "List does nothing until rendered with .render(&theme)"]
pub struct List<State, Item> {
    items: Option<ItemsFn<State, Item>>,
    item_count: u64,
    item_height: f64,
    render_item: Option<ItemRenderer<State, Item>>,
    item_id: Option<ItemIdFn<Item>>,
    selection_lens: Option<SelectionLens<State>>,
    scroll: ScrollState,
    loading: bool,
    search: Option<(String, SearchChange<State>)>,
    search_placeholder: String,
    on_load_more: Option<LoadMore<State>>,
    load_threshold: u64,
}

impl<State, Item> List<State, Item>
where
    State: 'static,
    Item: 'static,
{
    /// Starts an empty list. Attach data with [`Self::items`] +
    /// [`Self::item_count`] and a renderer with [`Self::render_item`] before
    /// rendering.
    pub fn new() -> Self {
        Self {
            items: None,
            item_count: 0,
            item_height: DEFAULT_ITEM_HEIGHT,
            render_item: None,
            item_id: None,
            selection_lens: None,
            scroll: ScrollState::new(),
            loading: false,
            search: None,
            search_placeholder: String::new(),
            on_load_more: None,
            load_threshold: DEFAULT_LOAD_THRESHOLD,
        }
    }

    /// Sets the item-data accessor. Required for the list to show data;
    /// without it the body renders empty.
    pub fn items<F>(mut self, items: F) -> Self
    where
        F: for<'a> Fn(&'a State) -> &'a [Item] + Send + Sync + 'static,
    {
        self.items = Some(Arc::new(items));
        self
    }

    /// Sets the current item count — the body virtualizes over
    /// `0..item_count`. Snapshot it from host state at frame time.
    pub fn item_count(mut self, item_count: u64) -> Self {
        self.item_count = item_count;
        self
    }

    /// Fixed pixel item height (defaults to [`DEFAULT_ITEM_HEIGHT`]).
    pub fn item_height(mut self, item_height: f64) -> Self {
        self.item_height = item_height;
        self
    }

    /// Sets the per-item renderer, given the item, whether it's currently
    /// selected, and the theme.
    pub fn render_item<F, V>(mut self, render: F) -> Self
    where
        F: Fn(&Item, bool, &Theme) -> V + Send + Sync + 'static,
        V: WidgetView<State, ()> + 'static,
    {
        self.render_item = Some(Arc::new(move |item, selected, theme| {
            Box::new(render(item, selected, theme))
        }));
        self
    }

    /// Supplies a **stable, unique item id** for each item — the same
    /// `getRowId` contract as
    /// [`DataGrid::row_id`](super::super::data_grid::DataGrid::row_id).
    /// Selection is keyed by this id, so it stays attached to the right
    /// items across host-side reordering/filtering.
    ///
    /// If omitted, the list uses each item's current slice position as its
    /// id — fine for a static list, but a positional key follows the
    /// documented index-keying failure mode under reordering/filtering.
    pub fn item_id<F>(mut self, id: F) -> Self
    where
        F: Fn(&Item) -> u64 + Send + Sync + 'static,
    {
        self.item_id = Some(Arc::new(id));
        self
    }

    /// Enables item selection via a lens into the host's [`SelectionState`].
    /// Pair with [`Self::item_id`] for a stable key. Omit for a
    /// non-selectable list.
    pub fn selection<F>(mut self, lens: F) -> Self
    where
        F: for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync + 'static,
    {
        self.selection_lens = Some(Arc::new(lens));
        self
    }

    /// Supplies the current [`ScrollState`] snapshot for programmatic
    /// scrolling — "scroll to top" is `scroll_to_index(0)`, "scroll to
    /// selected" is `scroll_to_index(<selected item's display position>)`.
    /// See [`DataGrid::scroll_to`](super::super::data_grid::DataGrid::scroll_to)
    /// for the full request/generation contract (identical here).
    pub fn scroll_to(mut self, scroll: ScrollState) -> Self {
        self.scroll = scroll;
        self
    }

    /// Shows a loading spinner. When `item_count == 0` the spinner replaces
    /// the whole body (initial load); when `item_count > 0` it appears as a
    /// footer row beneath the items (loading more).
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Enables a search input above the list, seeded with `query`.
    /// `on_change` is invoked with the full updated query on every edit; the
    /// list never filters its own data — the host re-derives
    /// `items`/`item_count` from the query and passes the result back.
    pub fn search<F>(mut self, query: impl Into<String>, on_change: F) -> Self
    where
        F: Fn(&mut State, String) + Send + Sync + 'static,
    {
        self.search = Some((query.into(), Arc::new(on_change)));
        self
    }

    /// Placeholder text for the search input (requires [`Self::search`]).
    pub fn search_placeholder(mut self, text: impl Into<String>) -> Self {
        self.search_placeholder = text.into();
        self
    }

    /// Lazy loading: invoked when the virtualized active range comes within
    /// [`Self::load_threshold`] items of `item_count`. The host typically
    /// grows `item_count` (and/or `items`) in response.
    pub fn on_load_more<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut State) + Send + Sync + 'static,
    {
        self.on_load_more = Some(Arc::new(f));
        self
    }

    /// Distance (in items) from the end of `item_count` at which
    /// [`Self::on_load_more`] fires (defaults to [`DEFAULT_LOAD_THRESHOLD`]).
    pub fn load_threshold(mut self, load_threshold: u64) -> Self {
        self.load_threshold = load_threshold;
        self
    }

    /// Materializes the xilem view at the supplied theme.
    #[must_use]
    pub fn render(self, theme: &Theme) -> impl WidgetView<State, ()> + use<State, Item> {
        build_list_view(self, theme)
    }
}

impl<State, Item> Default for List<State, Item>
where
    State: 'static,
    Item: 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Starts a [`List`] — the free-function entry point mirroring the other
/// components' constructors (`data_grid(columns)`, `button(..)`, …).
/// Equivalent to [`List::new`]; attach data/behavior with the chained
/// setters, then [`List::render`].
pub fn list<State, Item>() -> List<State, Item>
where
    State: 'static,
    Item: 'static,
{
    List::new()
}

/// Resolves the stable ids of the items spanning the **visual** range
/// between `anchor_id` and `target_id` (inclusive), in the slice's current
/// display order. Mirrors
/// `data_grid::view::visual_range_ids`.
fn visual_range_ids<Item>(
    data: &[Item],
    item_id: &ItemIdSource<Item>,
    anchor_id: u64,
    target_id: u64,
) -> Option<Vec<u64>> {
    let mut anchor_pos = None;
    let mut target_pos = None;
    for (pos, item) in data.iter().enumerate() {
        let id = item_id.id_of(pos, item);
        if id == anchor_id {
            anchor_pos = Some(pos);
        }
        if id == target_id {
            target_pos = Some(pos);
        }
    }
    let (a, t) = (anchor_pos?, target_pos?);
    let (lo, hi) = if a <= t { (a, t) } else { (t, a) };
    Some(
        (lo..=hi)
            .map(|pos| item_id.id_of(pos, &data[pos]))
            .collect(),
    )
}

/// Bundles the per-render captures needed to build one row, keeping
/// [`build_row`] under clippy's too-many-arguments limit.
struct RowContext<'a, State, Item> {
    items: &'a ItemsFn<State, Item>,
    render_item: &'a ItemRenderer<State, Item>,
    selection_lens: Option<&'a SelectionLens<State>>,
    item_id_source: &'a ItemIdSource<Item>,
    item_height: f64,
    theme: &'a Theme,
}

/// Applies a row click to the host's [`SelectionState`], per
/// [`RowClickAction`]: shift extends the visual range from the current
/// anchor, the action modifier toggles membership, and a plain click
/// replaces the selection with `pos`'s item id.
fn apply_row_click<State, Item>(
    state: &mut State,
    action: RowClickAction,
    pos: usize,
    items: &ItemsFn<State, Item>,
    selection_lens: Option<&SelectionLens<State>>,
    item_id_source: &ItemIdSource<Item>,
) where
    State: 'static,
    Item: 'static,
{
    let Some(sel_lens) = selection_lens else {
        return;
    };
    let Some(target_id) = ({
        let data = (*items)(state);
        data.get(pos).map(|item| item_id_source.id_of(pos, item))
    }) else {
        return;
    };

    if action.shift {
        let anchor = (**sel_lens)(state).anchor();
        let range = anchor.and_then(|anchor_id| {
            let data = (*items)(state);
            visual_range_ids(data, item_id_source, anchor_id, target_id)
        });
        match range {
            Some(ids) => (**sel_lens)(state).extend_range(ids),
            None => (**sel_lens)(state).replace_with(target_id),
        }
    } else if action.action_mod {
        (**sel_lens)(state).toggle(target_id);
    } else {
        (**sel_lens)(state).replace_with(target_id);
    }
}

/// Builds one virtualized row: resolves the item at `idx`, applies the
/// selected-row background, and wraps it in [`clickable_row`] so clicks
/// update [`SelectionState`] via [`apply_row_click`].
fn build_row<State, Item>(
    state: &mut State,
    idx: i64,
    ctx: &RowContext<State, Item>,
) -> impl WidgetView<State, ()> + use<State, Item>
where
    State: 'static,
    Item: 'static,
{
    let pos = scroll_idx_to_slice(idx);

    let data = (*ctx.items)(state);
    let item_id_at_pos = data
        .get(pos)
        .map(|item| ctx.item_id_source.id_of(pos, item));

    let is_selected = match (ctx.selection_lens, item_id_at_pos) {
        (Some(sel), Some(id)) => (**sel)(state).contains(id),
        _ => false,
    };

    // Re-borrow: the `is_selected` arm above took a `&mut State` through the
    // selection lens, ending the earlier `&[Item]` borrow.
    let data = (*ctx.items)(state);
    let item_view: Box<AnyWidgetView<State>> = match data.get(pos) {
        Some(item) => (ctx.render_item)(item, is_selected, ctx.theme),
        None => Box::new(label("")),
    };

    let row_bg = if is_selected {
        ctx.theme.palette.surface_2
    } else {
        Color::TRANSPARENT
    };
    let row_view = sized_box(item_view)
        .fixed_height(Length::px(ctx.item_height))
        .background_color(row_bg);

    let selection_lens = ctx.selection_lens.cloned();
    let item_id_source = ctx.item_id_source.clone();
    let items = Arc::clone(ctx.items);
    clickable_row(
        row_view,
        is_selected,
        ctx.theme,
        move |state: &mut State, action: RowClickAction| {
            apply_row_click(
                state,
                action,
                pos,
                &items,
                selection_lens.as_ref(),
                &item_id_source,
            );
        },
    )
}

fn build_list_view<State, Item>(
    list: List<State, Item>,
    theme: &Theme,
) -> impl WidgetView<State, ()> + use<State, Item>
where
    State: 'static,
    Item: 'static,
{
    let theme = *theme;
    let List {
        items,
        item_count,
        item_height,
        render_item,
        item_id,
        selection_lens,
        scroll,
        loading,
        search,
        search_placeholder,
        on_load_more,
        load_threshold,
    } = list;

    // Default the data accessor to an empty slice and the renderer to an
    // empty cell when unset, mirroring `data_grid::build_grid_view`'s
    // defaulting of `rows`.
    let items: ItemsFn<State, Item> =
        items.unwrap_or_else(|| Arc::new(|_: &State| -> &[Item] { &[] }));
    let render_item: ItemRenderer<State, Item> = render_item
        .unwrap_or_else(|| Arc::new(|_: &Item, _: bool, _: &Theme| Box::new(label("")) as _));

    let item_id_source = match item_id {
        Some(f) => ItemIdSource::Explicit(f),
        None => ItemIdSource::Position,
    };

    let valid_range_end = scroll_range_end(item_count);

    let body: Box<AnyWidgetView<State>> = if item_count == 0 && loading {
        Box::new(centered_spinner(&theme))
    } else {
        let virtualized = virtual_scroll(0..valid_range_end, {
            let items = Arc::clone(&items);
            let render_item = Arc::clone(&render_item);
            let selection_lens = selection_lens.clone();
            let item_id_source = item_id_source.clone();
            move |state: &mut State, idx: i64| {
                let ctx = RowContext {
                    items: &items,
                    render_item: &render_item,
                    selection_lens: selection_lens.as_ref(),
                    item_id_source: &item_id_source,
                    item_height,
                    theme: &theme,
                };
                build_row(state, idx, &ctx)
            }
        });

        let body_view = ListBodyView {
            child: virtualized,
            scroll,
            item_count,
            on_load_more,
            load_threshold,
        };

        if loading && item_count > 0 {
            let children: [AnyFlexChild<State, ()>; 2] = [
                flex_item(body_view, 1.0).into(),
                flex_item(footer_spinner(&theme), 0.0).into(),
            ];
            Box::new(flex_col(children))
        } else {
            Box::new(body_view)
        }
    };

    let mut top: Vec<AnyFlexChild<State, ()>> = Vec::with_capacity(2);
    if let Some((query, on_change)) = search {
        let input_view = input(query, move |state: &mut State, text: String| {
            on_change(state, text);
        })
        .placeholder(search_placeholder)
        .render(&theme);
        top.push(flex_item(input_view, 0.0).into());
    }
    top.push(flex_item(body, 1.0).into());

    flex_col(top).cross_axis_alignment(CrossAxisAlignment::Stretch)
}

/// A spinner centered in the full space given to it — used when the list is
/// empty and [`List::loading`] is set.
fn centered_spinner<State: 'static>(theme: &Theme) -> impl WidgetView<State, ()> + use<State> {
    flex_row((spinner().render::<State, ()>(theme),))
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center)
}

/// A fixed-height "loading more" footer row, shown beneath the items when
/// [`List::loading`] is set and `item_count > 0`. Height derives from the
/// theme's density (row pitch + surface padding).
fn footer_spinner<State: 'static>(theme: &Theme) -> impl WidgetView<State, ()> + use<State> {
    let height = f64::from(theme.density.row) + f64::from(theme.density.pad);
    sized_box(
        flex_row((spinner().render::<State, ()>(theme),))
            .main_axis_alignment(MainAxisAlignment::Center)
            .cross_axis_alignment(CrossAxisAlignment::Center),
    )
    .fixed_height(Length::px(height))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ItemIdSource;
    use super::visual_range_ids;

    /// Item id == the item value itself, so test slices read naturally.
    fn id_is_value() -> ItemIdSource<u64> {
        ItemIdSource::Explicit(Arc::new(|item: &u64| *item))
    }

    #[test]
    fn visual_range_spans_anchor_to_target_in_display_order() {
        let data = [10_u64, 20, 30, 40, 50];
        let ids = visual_range_ids(&data, &id_is_value(), 20, 40);
        assert_eq!(ids, Some(vec![20, 30, 40]));
    }

    #[test]
    fn visual_range_is_orientation_independent() {
        let data = [10_u64, 20, 30, 40, 50];
        let ids = visual_range_ids(&data, &id_is_value(), 40, 20);
        assert_eq!(ids, Some(vec![20, 30, 40]));
    }

    #[test]
    fn visual_range_none_when_anchor_missing() {
        let data = [10_u64, 30, 40, 50];
        assert_eq!(visual_range_ids(&data, &id_is_value(), 20, 40), None);
    }

    #[test]
    fn visual_range_position_fallback_ids_are_slice_positions() {
        let data = ["a", "b", "c", "d", "e"];
        let ids = visual_range_ids(&data, &ItemIdSource::Position, 1, 3);
        assert_eq!(ids, Some(vec![1, 2, 3]));
    }
}
