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
//! The list is a thin consumer of the crate-internal
//! [`collection`](crate::collection) substrate: everything vertical —
//! virtualization, scroll-to-index, lazy-load, arrow-key navigation, the
//! selection background, and the modifier-aware click routing — is owned by
//! [`collection_body`]. The list supplies only per-item *content* (wrapped
//! to the fixed [`List::item_height`]) plus its own chrome: a search input
//! above the body and a loading spinner (centered when empty, a footer when
//! loading more).
//!
//! - **Selection** ([`SelectionState`]) is keyed by a stable item id from
//!   [`List::item_id`], exactly like `data_grid`'s row selection.
//! - **Scroll to top / scroll to selected** are plain host call sites on
//!   [`ScrollState::scroll_to_index`] — `0` for "top", the selected item's
//!   display position for "selected" — passed in via [`List::scroll_to`].

use std::sync::Arc;

use xilem::masonry::layout::Length;
use xilem::view::{
    AnyFlexChild, CrossAxisAlignment, MainAxisAlignment, flex_col, flex_item, flex_row, label,
    sized_box,
};
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;
use crate::collection::{
    CollectionBodyParams, IdSource, ItemsFn, Lazy, RenderRow, ScrollState, SelectionLens,
    SelectionState, collection_body,
};
use crate::components::input::input;
use crate::components::spinner::spinner;

/// Boxed stable item-id projector (`Fn(&Item) -> u64`).
type ItemIdFn<Item> = Arc<dyn Fn(&Item) -> u64 + Send + Sync>;
/// Boxed search-query-change callback (`Fn(&mut State, query)`).
type SearchChange<State> = Arc<dyn Fn(&mut State, String) + Send + Sync>;
/// Boxed lazy-load callback (`Fn(&mut State)`), wrapped into the substrate's
/// [`Lazy`] before being handed to `collection_body`.
type LoadMore<State> = Arc<dyn Fn(&mut State) + Send + Sync>;

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
    render_item: Option<RenderRow<State, Item>>,
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
    /// selected, and the theme. The substrate paints the selection
    /// background and handles clicks — the renderer supplies content only.
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
    let render_item: RenderRow<State, Item> = render_item
        .unwrap_or_else(|| Arc::new(|_: &Item, _: bool, _: &Theme| Box::new(label("")) as _));

    // The host owns id keying; default to the slice-position fallback when no
    // projector is supplied (correct for a static list, documented on `item_id`).
    let id_source = match item_id {
        Some(f) => IdSource::Explicit(f),
        None => IdSource::Position,
    };

    // Per-item CONTENT for the substrate: wrap the host's item view in the
    // list's fixed item height. `collection_body` paints the selection
    // background and owns click routing, so this stays content only.
    let render_row: RenderRow<State, Item> = Arc::new(
        move |item: &Item, selected: bool, theme: &Theme| -> Box<AnyWidgetView<State>> {
            Box::new(sized_box((render_item)(item, selected, theme)).fixed_height(Length::px(item_height)))
        },
    );

    let lazy = on_load_more.map(|callback| Lazy {
        threshold: load_threshold,
        callback,
    });

    let body: Box<AnyWidgetView<State>> = if item_count == 0 && loading {
        // Initial load: the spinner replaces the whole body.
        Box::new(centered_spinner(&theme))
    } else {
        let collection = collection_body(CollectionBodyParams {
            item_count,
            items,
            id_source,
            selection_lens,
            scroll,
            lazy,
            render_row,
            theme,
        });
        if loading && item_count > 0 {
            // Loading more: keep the items, append a footer spinner.
            let children: [AnyFlexChild<State, ()>; 2] = [
                flex_item(collection, 1.0).into(),
                flex_item(footer_spinner(&theme), 0.0).into(),
            ];
            Box::new(flex_col(children))
        } else {
            Box::new(collection)
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
    use std::fmt;
    use std::sync::Arc;

    use masonry::core::{NewWidget, WidgetRef};
    use masonry::testing::TestHarness;
    use masonry::widgets::Flex;
    use xilem::ViewCtx;
    use xilem::core::{ProxyError, RawProxy, SendMessage, View, ViewId};

    use super::list;
    use crate::Theme;
    use crate::collection::CollectionBodyWidget;
    use crate::label;

    /// A [`RawProxy`] that drops every message — the test builds the view
    /// directly and never expects a proxied message back.
    #[derive(Debug)]
    struct NoopProxy;

    impl RawProxy for NoopProxy {
        fn send_message(
            &self,
            _path: Arc<[ViewId]>,
            _message: SendMessage,
        ) -> Result<(), ProxyError> {
            Ok(())
        }
        fn dyn_debug(&self) -> &dyn fmt::Debug {
            self
        }
    }

    /// Depth-first search for a [`CollectionBodyWidget`] in the tree.
    fn find_collection_body(
        widget: WidgetRef<'_, dyn masonry::core::Widget>,
    ) -> Option<WidgetRef<'_, CollectionBodyWidget>> {
        if let Some(body) = widget.downcast::<CollectionBodyWidget>() {
            return Some(body);
        }
        widget.children().into_iter().find_map(find_collection_body)
    }

    /// `list`'s body is the collection substrate's [`CollectionBodyWidget`]
    /// — the same virtualization/keyboard-nav substrate `data_grid` builds
    /// on. This materializes a real `list` view and asserts the substrate
    /// body widget is present, the seam that wires `list` onto it.
    #[test]
    fn list_body_is_the_collection_substrate_widget() {
        struct State {
            items: Vec<u64>,
        }

        let mut state = State {
            items: vec![10, 20, 30],
        };

        let view = list::<State, u64>()
            .items(|s: &State| &s.items[..])
            .item_count(3)
            .render_item(|item: &u64, _selected, theme| label(item.to_string()).render(theme))
            .render(&Theme::default());

        let proxy: Arc<dyn RawProxy> = Arc::new(NoopProxy);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
        );
        let mut ctx = ViewCtx::new(proxy, runtime);
        let (pod, _view_state) = view.build(&mut ctx, &mut state);

        let root = Flex::column().with_fixed(pod.new_widget);
        let harness =
            TestHarness::create(masonry::theme::default_property_set(), NewWidget::new(root));

        assert!(
            find_collection_body(harness.root_widget().as_dyn()).is_some(),
            "list's body should be a CollectionBodyWidget (shared virtualization substrate)",
        );
    }
}
