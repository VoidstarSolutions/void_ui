//! `collection_body` — the virtualized body view shared by the collection
//! components. Owns virtualization, scroll-to-anchor (generation-tracked),
//! lazy-load, central row-click routing, and the selection background.
//! The caller supplies only per-row content via `render_row`.
//!
//! The `#[cfg(test)]` tests below exercise the builder and the scroll-to
//! mechanism directly at the widget level.

use std::sync::Arc;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{label, sized_box, virtual_scroll};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::body::CollectionBodyWidget;
use super::row_click::{LeadingHitZone, RowClickAction, TreeRowMeta, clickable_row};
use super::{
    IdSource, ItemsFn, OnActivate, ScrollState, SelectionLens, apply_row_activate, apply_row_click,
    clamp_scroll_index, nearing_end, scroll_range_end,
};
use crate::Theme;

/// Per-item content renderer: `(item, selected, theme) -> content view`.
pub(crate) type RenderRow<State, Item> =
    Arc<dyn Fn(&Item, bool, &Theme) -> Box<AnyWidgetView<State>> + Send + Sync>;

/// Per-item leading defer-to-child hit zone (`item -> zone`). Returns the
/// [`LeadingHitZone`] the row hands off to an interactive child sitting there
/// (a disclosure chevron on an expandable row) instead of selecting; `None`
/// reserves nothing. See
/// [`ClickableRow::leading_hit`](super::row_click::ClickableRow::leading_hit).
pub(crate) type LeadingHitZoneFn<Item> = Arc<dyn Fn(&Item) -> Option<LeadingHitZone> + Send + Sync>;

/// Per-item tree metadata (`item -> meta`) for keyboard tree navigation.
/// `data_grid`'s expandable rows supply one so Right/Left can toggle a row's
/// expansion (and, later, move focus by depth). See
/// [`ClickableRow::tree_meta`](super::row_click::ClickableRow::tree_meta).
pub(crate) type TreeMetaFn<Item> = Arc<dyn Fn(&Item) -> Option<TreeRowMeta> + Send + Sync>;

/// Host row-expansion toggle callback: `(state, row_id)`, run when Right/Left
/// toggles an expandable row. Same shape as [`OnActivate`]; the substrate
/// resolves the row's stable id and hands it in.
pub(crate) type OnToggle<State> = Arc<dyn Fn(&mut State, u64) + Send + Sync>;

/// A row's [`TreeRowMeta`] by slice position, read from host state — closes over
/// the `items` accessor + the `tree_meta` projector with `Item` erased so the
/// body view (generic only over `State`) can hold it. Used to refresh the
/// depth map for Left/Right nav *without* touching live `VirtualScroll` children
/// (which can be mid-transition and panic on read).
type DepthAtFn<State> = Arc<dyn Fn(&State, usize) -> Option<TreeRowMeta> + Send + Sync>;

/// Builds a [`DepthAtFn`] from the tree-meta projector + items accessor,
/// erasing `Item` so the body view (generic only over `State`) can hold it.
/// `None` when the collection isn't an expandable tree.
fn build_depth_at<State: 'static, Item: 'static>(
    tree_meta: Option<&TreeMetaFn<Item>>,
    items: &ItemsFn<State, Item>,
) -> Option<DepthAtFn<State>> {
    let tm = tree_meta?.clone();
    let items = Arc::clone(items);
    Some(Arc::new(move |state: &State, pos: usize| {
        (*items)(state).get(pos).and_then(|i| tm(i))
    }))
}

/// Lazy-load config: fire `callback` when the active range comes within
/// `threshold` items of the end.
pub(crate) struct Lazy<State, Action> {
    pub(crate) threshold: u64,
    pub(crate) callback: Arc<dyn Fn(&mut State) -> Action + Send + Sync>,
}

/// All inputs needed to materialize the virtualized body.
pub(crate) struct CollectionBodyParams<State, Item, Action> {
    pub(crate) item_count: u64,
    pub(crate) items: ItemsFn<State, Item>,
    pub(crate) id_source: IdSource<Item>,
    pub(crate) selection_lens: Option<SelectionLens<State>>,
    pub(crate) scroll: ScrollState,
    pub(crate) lazy: Option<Lazy<State, Action>>,
    pub(crate) render_row: RenderRow<State, Item>,
    /// Optional per-row leading defer-to-child hit zone. `None` (the common
    /// case) reserves nothing — every row selects across its whole width.
    /// `data_grid`'s expandable rows supply one so the chevron's box defers
    /// to the disclosure toggle.
    pub(crate) leading_hit_zone: Option<LeadingHitZoneFn<Item>>,
    /// Optional row-activation handler (Enter on the focused row). `None`
    /// leaves Enter falling back to selection.
    pub(crate) on_activate: Option<OnActivate<State>>,
    /// Optional per-row tree metadata for keyboard tree nav (Right/Left).
    /// `None` (the common case) leaves rows non-tree.
    pub(crate) tree_meta: Option<TreeMetaFn<Item>>,
    /// Optional expansion-toggle handler, paired with `tree_meta`. Run when
    /// Right/Left toggles an expandable row.
    pub(crate) on_toggle: Option<OnToggle<State>>,
    pub(crate) theme: Theme,
}

/// Builds the virtualized body view. The substrate owns virtualization,
/// scroll-to-anchor, lazy-load, keyboard nav, selection background, and
/// click routing; the caller supplies only per-row content via `render_row`.
pub(crate) fn collection_body<State, Item, Action>(
    params: CollectionBodyParams<State, Item, Action>,
) -> impl WidgetView<State, Action> + use<State, Item, Action>
where
    State: 'static,
    Item: 'static,
    Action: Default + 'static,
{
    let CollectionBodyParams {
        item_count,
        items,
        id_source,
        selection_lens,
        scroll,
        lazy,
        render_row,
        leading_hit_zone,
        on_activate,
        tree_meta,
        on_toggle,
        theme,
    } = params;
    let valid_range_end = scroll_range_end(item_count);
    // For an expandable tree, a closure yielding a row's `TreeRowMeta` by slice
    // position, read from host state (never from live row widgets — see
    // `refresh_tree_row_meta`). `None` for a flat list.
    let depth_at = build_depth_at(tree_meta.as_ref(), &items);

    // Collection-level, and unconditionally true: `RowClickable`'s capture
    // guard (see `row_click.rs`) is what actually decides whether a press
    // reaches a child — a non-capturing cell still bubbles the press up and
    // the row still selects, so propagating is safe even for a plain,
    // non-expandable grid/list. Must be decided here, not per row —
    // `RowClickable` caches it at creation and virtualization recycles a row
    // widget across positions.
    let defers_to_children = true;

    let child = virtual_scroll(valid_range_end, {
        let items = Arc::clone(&items);
        let id_source = id_source.clone();
        let selection_lens = selection_lens.clone();
        let render_row = Arc::clone(&render_row);
        let leading_hit_zone = leading_hit_zone.clone();
        let on_activate = on_activate.clone();
        let tree_meta = tree_meta.clone();
        let on_toggle = on_toggle.clone();
        move |state: &mut State, pos: usize| {
            let data = (*items)(state);
            let id_at_pos = data.get(pos).map(|item| id_source.id_of(pos, item));
            let is_selected = match (selection_lens.as_ref(), id_at_pos) {
                (Some(sel), Some(id)) => (**sel)(state).contains(id),
                _ => false,
            };

            // Re-borrow: `is_selected` took `&mut State` via the lens.
            let data = (*items)(state);
            // Compute the per-row leading defer-to-child hit zone alongside
            // the content, from the same item borrow. An empty (past-end)
            // row reserves nothing.
            let (content, leading, row_tree): (
                Box<AnyWidgetView<State>>,
                Option<LeadingHitZone>,
                Option<TreeRowMeta>,
            ) = match data.get(pos) {
                Some(item) => (
                    render_row(item, is_selected, &theme),
                    leading_hit_zone.as_ref().and_then(|f| f(item)),
                    tree_meta.as_ref().and_then(|f| f(item)),
                ),
                // pos past the end (a row scrolled past a shrinking
                // dataset) — render an inert empty row.
                None => (Box::new(label("")), None, None),
            };

            let row_bg = if is_selected {
                theme.palette.surface_2
            } else {
                Color::TRANSPARENT
            };
            let row_view = sized_box(content).background_color(row_bg);

            let row = clickable_row(row_view, is_selected, &theme, {
                let items = Arc::clone(&items);
                let id_source = id_source.clone();
                let selection_lens = selection_lens.clone();
                move |state: &mut State, action: RowClickAction| {
                    apply_row_click(
                        state,
                        action,
                        pos,
                        &items,
                        selection_lens.as_ref(),
                        &id_source,
                    );
                }
            })
            .leading_hit(leading)
            .propagate_pointer_to_children(defers_to_children)
            .tree_meta(row_tree);
            // Enter-activation, only when the host wired a handler: resolve the
            // row's stable id at activate time (mirroring `apply_row_click`) and
            // hand it to the host callback.
            let row = match on_activate.clone() {
                Some(host_on_activate) => {
                    let items = Arc::clone(&items);
                    let id_source = id_source.clone();
                    row.on_activate(move |state: &mut State| {
                        apply_row_activate(state, pos, &items, &id_source, &host_on_activate);
                    })
                }
                None => row,
            };
            // Right/Left expansion toggle, only when the host wired one: resolve
            // the row's stable id at toggle time and hand it to the host.
            match on_toggle.clone() {
                Some(host_on_toggle) => {
                    let items = Arc::clone(&items);
                    let id_source = id_source.clone();
                    row.on_toggle(move |state: &mut State| {
                        apply_row_activate(state, pos, &items, &id_source, &host_on_toggle);
                    })
                }
                None => row,
            }
        }
    });

    CollectionBodyView {
        child,
        scroll,
        item_count,
        lazy,
        depth_at,
    }
}

struct CollectionBodyView<V, State, Action> {
    child: V,
    scroll: ScrollState,
    item_count: u64,
    lazy: Option<Lazy<State, Action>>,
    /// For an expandable tree, reads a row's [`TreeRowMeta`] by slice position
    /// from host state; the body refreshes its per-visible-row depth map from
    /// this on rebuild (for Left/Right nav). `None` for a flat collection.
    depth_at: Option<DepthAtFn<State>>,
}

/// Rebuilds [`CollectionBodyWidget`]'s per-visible-row depth map for Left/Right
/// nav. The materialized rows are `children_ids()` (authoritative + live); each
/// row's slice position is `active_start + k`, and its
/// [`TreeRowMeta`](super::row_click::TreeRowMeta) is read from **host state**
/// via `depth_at` — never from the live `VirtualScroll` child (which can be
/// mid-transition and panic on read). A stale `active_start` at worst yields a
/// transiently-wrong map that the next settled rebuild corrects; it can't panic.
fn refresh_tree_row_meta<State>(
    element: &mut Mut<'_, Pod<CollectionBodyWidget>>,
    depth_at: &DepthAtFn<State>,
    active_start: usize,
    app_state: &State,
) {
    use masonry::core::Widget as _;
    let ids: Vec<masonry::core::WidgetId> = {
        let vs = CollectionBodyWidget::virtual_scroll_mut(element);
        vs.widget.children_ids().iter().copied().collect()
    };
    let meta: Vec<(masonry::core::WidgetId, TreeRowMeta)> = ids
        .iter()
        .enumerate()
        .filter_map(|(k, &id)| {
            let pos = active_start.checked_add(k)?;
            depth_at(app_state, pos).map(|m| (id, m))
        })
        .collect();
    CollectionBodyWidget::set_row_meta(element, meta);
}

struct CollectionBodyViewState<S> {
    child_state: S,
    applied_generation: u64,
    /// Latest active (materialized) index range, captured from the child's
    /// `VirtualScrollAction` in `message` and consumed by the *next* `rebuild`
    /// (once materialization has caught up) to refresh the tree depth map.
    active_range: std::ops::Range<usize>,
}

impl<V, State, Action> ViewMarker for CollectionBodyView<V, State, Action> {}

impl<State, Action, V> View<State, Action, ViewCtx> for CollectionBodyView<V, State, Action>
where
    State: 'static,
    Action: Default + 'static,
    V: View<State, Action, ViewCtx, Element = Pod<VirtualScrollWidget>>,
{
    type Element = Pod<CollectionBodyWidget>;
    type ViewState = CollectionBodyViewState<V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_element, child_state) = self.child.build(ctx, app_state);
        (
            Pod::new(CollectionBodyWidget::new(child_element.new_widget)),
            CollectionBodyViewState {
                child_state,
                applied_generation: 0,
                active_range: 0..0,
            },
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.scroll.generation() != view_state.applied_generation {
            view_state.applied_generation = self.scroll.generation();
            if let Some(idx) = clamp_scroll_index(self.scroll.index(), self.item_count) {
                let mut vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
                VirtualScrollWidget::scroll_to(&mut vs, idx);
            }
        }
        {
            let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
            self.child
                .rebuild(&prev.child, &mut view_state.child_state, ctx, vs, app_state);
        }
        // Materialization has now caught up, so refresh each row's Up/Down
        // nav targets — every collection needs this, not just tree ones
        // (see `refresh_tree_row_meta` just below for the tree-only case).
        CollectionBodyWidget::refresh_row_nav(&mut element, view_state.active_range.start);
        // Materialization has now caught up, so refresh the tree depth map for
        // Left/Right nav — read from `app_state`, never from live row widgets.
        if let Some(depth_at) = self.depth_at.as_ref() {
            refresh_tree_row_meta(
                &mut element,
                depth_at,
                view_state.active_range.start,
                app_state,
            );
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
        self.child.teardown(&mut view_state.child_state, ctx, vs);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // Only peek once the message has stopped routing to a child:
        // maybe_take_message debug-asserts the path is empty, so probing
        // mid-route would panic.
        let mut lazy_action: Option<Action> = None;
        let mut active_range_update: Option<std::ops::Range<usize>> = None;
        if message.remaining_path().is_empty() {
            // Peek without consuming (`false`): the `VirtualScrollAction`
            // still routes onward to the child so virtualization handles it.
            message.maybe_take_message::<VirtualScrollAction>(|action| {
                // Only the `Fetch` variant carries a materialized-range change;
                // `Scroll` (viewport movement within the already-materialized
                // window) doesn't affect lazy-load, the tree depth map, or
                // row nav targets.
                if let VirtualScrollAction::Fetch(fetch) = action {
                    if let Some(lazy) = self.lazy.as_ref()
                        && nearing_end(self.item_count, fetch.target().end, lazy.threshold)
                    {
                        lazy_action = Some((lazy.callback)(app_state));
                    }
                    // Capture the new active range so we can refresh the tree
                    // depth map and row nav targets once the child has
                    // materialized it (below) — every collection needs the
                    // latter, not just tree ones.
                    active_range_update = Some(fetch.target().clone());
                }
                false
            });
        }
        let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
        let child_result = self
            .child
            .message(&mut view_state.child_state, message, vs, app_state);
        // Materialization for `active_range_update` is deferred to the next
        // rebuild, so just remember the range; that rebuild refreshes the
        // tree depth map and row nav once the rows are actually present
        // (reading here would panic).
        if let Some(active) = active_range_update {
            view_state.active_range = active;
        }
        match lazy_action {
            // The lazy-load callback's action wins: it must reach the root
            // so the host's app logic re-runs with the grown item_count.
            Some(action) => MessageResult::Action(action),
            None => child_result,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use masonry::core::{NewWidget, WidgetId};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;
    use xilem::masonry::widgets::{VirtualScroll, VirtualScrollAction};

    use crate::collection::body::CollectionBodyWidget;
    use crate::collection::clamp_scroll_index;

    /// Pumps the harness until `VirtualScroll` stops asking for row
    /// changes, materializing each requested row as a plain `Label` keyed
    /// by row index. Mirrors `collection::body`'s test driver.
    fn drive_to_fixpoint(
        harness: &mut TestHarness<CollectionBodyWidget>,
        rows: &mut HashMap<usize, WidgetId>,
    ) {
        let mut iteration = 0;
        loop {
            iteration += 1;
            assert!(iteration <= 1000, "Took too long to reach fixpoint");
            let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() else {
                break;
            };
            let VirtualScrollAction::Fetch(action) = action else {
                continue;
            };
            harness.edit_root_widget(|mut body| {
                let mut scroll = CollectionBodyWidget::virtual_scroll_mut(&mut body);
                VirtualScroll::will_handle_action(&mut scroll, &action);
                for idx in action.old_active().clone() {
                    if !action.target().contains(&idx) {
                        VirtualScroll::remove_child(&mut scroll, idx);
                        rows.remove(&idx);
                    }
                }
                for idx in action.target().clone() {
                    if !action.old_active().contains(&idx) {
                        let row = NewWidget::new(Label::new(format!("row {idx}"))).erased();
                        let row_id = row.id();
                        VirtualScroll::add_child(&mut scroll, idx, row);
                        rows.insert(idx, row_id);
                    }
                }
            });
        }
    }

    /// The substrate's scroll-to mechanism: `collection_body`'s `rebuild`
    /// re-anchors the inner `VirtualScroll` via [`VirtualScroll::scroll_to`]
    /// with the clamped index whenever the `ScrollState` generation changes.
    /// This exercises that exact call at the widget level (the `rebuild`
    /// path drives it identically) and asserts the materialized window moves
    /// to include the requested row.
    #[test]
    fn scroll_to_moves_the_materialized_window_to_the_target() {
        const ITEM_COUNT: u64 = 1000;
        const TARGET: u64 = 900;

        let scroll = NewWidget::new(VirtualScroll::new(0, usize::try_from(ITEM_COUNT).unwrap()));
        let body = NewWidget::new(CollectionBodyWidget::new(scroll));
        let mut harness = TestHarness::create_with_size(default_property_set(), body, (200, 400));
        let mut rows = HashMap::new();
        drive_to_fixpoint(&mut harness, &mut rows);

        // Initially anchored at row 0; the target near the end is well
        // outside the first materialized window.
        assert!(
            !rows.contains_key(&usize::try_from(TARGET).unwrap()),
            "row {TARGET} should be outside the initial window, got {:?}",
            rows.keys().copied().collect::<Vec<_>>()
        );

        // Re-anchor to the target with the clamped index, exactly as
        // `collection_body::rebuild` does on a ScrollState generation bump.
        let idx = clamp_scroll_index(TARGET, ITEM_COUNT).expect("non-empty grid");
        harness.edit_root_widget(|mut body| {
            let mut vs = CollectionBodyWidget::virtual_scroll_mut(&mut body);
            VirtualScroll::scroll_to(&mut vs, idx);
        });
        drive_to_fixpoint(&mut harness, &mut rows);

        assert!(
            rows.contains_key(&usize::try_from(TARGET).unwrap()),
            "expected materialized window to include row {TARGET} after re-anchor, got {:?}",
            rows.keys().copied().collect::<Vec<_>>()
        );
    }

    /// BASELINE (reference only): times the per-row build work the
    /// virtualized body's `virtual_scroll` closure performs for one
    /// rebuild of a window of visible rows — resolve the item, compute
    /// `is_selected` through the selection lens, build the per-row content
    /// view, wrap it in the selection-background `sized_box`, and set up
    /// the `clickable_row` click closure (which clones the items accessor,
    /// id source, and selection lens `Arc`s). The shared `apply_row_click`
    /// centralizes the click *logic* but not this per-row wiring, so the
    /// clones remain; this is the cost a future opt-in memoization (deferred,
    /// see the design spec) would reduce.
    ///
    /// Honest scope: this replicates the *body* of `collection_body`'s
    /// per-row closure rather than driving the closure through the View
    /// machinery (no app/view-level rebuild harness exists on this xilem
    /// rev — only masonry's widget-level `TestHarness`). View values are
    /// lazy, so this measures the construction of the per-row view tree and
    /// the `Arc` clones, *not* xilem's diff/`rebuild` traversal or any
    /// widget mutation. Numbers are relative and hardware-dependent — for
    /// before/after comparison only.
    ///
    /// Ignored by default; run with:
    /// `cargo test --all-features -- --ignored --nocapture row_build_baseline`
    #[test]
    #[ignore = "baseline timing, not a correctness check; run with --ignored --nocapture"]
    #[expect(
        clippy::too_many_lines,
        reason = "self-contained measurement harness: synthetic state + per-row replica + timing loop"
    )]
    fn row_build_baseline() {
        use std::hint::black_box;
        use std::sync::Arc;
        use std::time::Instant;

        use xilem::AnyWidgetView;
        use xilem::peniko::Color;
        use xilem::style::Style as _;
        use xilem::view::{flex_row, label, sized_box};

        use super::{RenderRow, collection_body};
        use crate::Theme;
        use crate::collection::row_click::{RowClickAction, clickable_row};
        use crate::collection::{
            IdSource, ItemsFn, SelectionLens, SelectionState, apply_row_click,
        };

        // How many visible rows one rebuild materializes. A realistic
        // virtualized window is a few dozen rows; 40 is representative.
        const ROWS: usize = 40;
        // Repeat the windowed build many times for a stable median.
        const ITERS: usize = 2_000;

        struct S {
            items: Vec<Row>,
            sel: SelectionState,
        }
        // Small realistic row: a handful of cell-sized fields.
        struct Row {
            id: u64,
            name: String,
            qty: u64,
            note: String,
        }

        // One windowed build = the exact per-row work `collection_body`'s
        // `virtual_scroll` closure runs, for ROWS consecutive positions.
        fn build_window(
            state: &mut S,
            items_fn: &ItemsFn<S, Row>,
            lens: &SelectionLens<S>,
            id_source: &IdSource<Row>,
            render_row: &RenderRow<S, Row>,
            theme: &Theme,
        ) {
            for pos in 0..ROWS {
                let data = (*items_fn)(state);
                let id_at_pos = data.get(pos).map(|item| id_source.id_of(pos, item));
                let is_selected = id_at_pos.is_some_and(|id| (*lens)(state).contains(id));

                let data = (*items_fn)(state);
                let content: Box<AnyWidgetView<S>> = match data.get(pos) {
                    Some(item) => render_row(item, is_selected, theme),
                    None => Box::new(label("")),
                };

                let row_bg = if is_selected {
                    theme.palette.surface_2
                } else {
                    Color::TRANSPARENT
                };
                let row_view = sized_box(content).background_color(row_bg);

                let items_fn = Arc::clone(items_fn);
                let id_source = id_source.clone();
                let selection_lens = Some(Arc::clone(lens));
                let row: crate::collection::row_click::ClickableRow<_, S, (), _> = clickable_row(
                    row_view,
                    is_selected,
                    theme,
                    move |state: &mut S, action: RowClickAction| {
                        apply_row_click(
                            state,
                            action,
                            pos,
                            &items_fn,
                            selection_lens.as_ref(),
                            &id_source,
                        );
                    },
                );
                black_box(&row);
            }
        }

        // Keep this baseline wired to the code path it measures: if
        // `collection_body`'s signature changes, this test won't compile.
        let _ = collection_body::<S, u64, ()>;

        let items: Vec<Row> = (0..ROWS as u64)
            .map(|i| Row {
                id: i,
                name: format!("item {i}"),
                qty: i * 3,
                note: format!("note for row {i}"),
            })
            .collect();
        let mut sel = SelectionState::new();
        // A non-empty selection so the lens path does real work.
        sel.replace_with(7);
        let mut state = S { items, sel };

        let items_fn: ItemsFn<S, Row> = Arc::new(|s: &S| &s.items[..]);
        let lens: SelectionLens<S> = Arc::new(|s: &mut S| &mut s.sel);
        let id_source: IdSource<Row> = IdSource::Explicit(Arc::new(|r: &Row| r.id));
        let theme = Theme::default();
        // Mirror data_grid: each row is a strip of a few labeled cells.
        let render_row: RenderRow<S, Row> = Arc::new(
            |row: &Row, _selected: bool, _theme: &Theme| -> Box<AnyWidgetView<S>> {
                Box::new(flex_row((
                    label(row.name.clone()),
                    label(row.qty.to_string()),
                    label(row.note.clone()),
                )))
            },
        );

        // Warm up (allocator, branch prediction) before timing.
        for _ in 0..50 {
            build_window(
                &mut state,
                &items_fn,
                &lens,
                &id_source,
                &render_row,
                &theme,
            );
        }

        let mut samples: Vec<u128> = Vec::with_capacity(ITERS);
        for _ in 0..ITERS {
            let start = Instant::now();
            build_window(
                &mut state,
                &items_fn,
                &lens,
                &id_source,
                &render_row,
                &theme,
            );
            samples.push(start.elapsed().as_nanos());
        }
        samples.sort_unstable();
        let median_window_ns = samples[samples.len() / 2];
        #[expect(
            clippy::cast_precision_loss,
            reason = "informational baseline print; ns values are far below f64's exact-integer range"
        )]
        let median_row_ns = median_window_ns as f64 / ROWS as f64;

        println!(
            "row_build_baseline: ROWS={ROWS} ITERS={ITERS} \
             median window={median_window_ns}ns, per-row={median_row_ns:.1}ns"
        );
    }

    /// Smoke-test the public builder: constructing `collection_body` from a
    /// fully-populated `CollectionBodyParams` (selection lens, lazy-load,
    /// explicit id source, and a content renderer) yields a `WidgetView`
    /// value without panicking. Exercises the builder + per-row closure
    /// wiring that the scroll-to test (which drives the widget directly)
    /// does not.
    #[test]
    fn collection_body_builds_a_view_from_full_params() {
        use std::sync::Arc;

        use xilem::WidgetView;
        use xilem::view::label;

        use super::{CollectionBodyParams, Lazy, RenderRow, collection_body};
        use crate::Theme;
        use crate::collection::{IdSource, ItemsFn, ScrollState, SelectionLens, SelectionState};

        struct S {
            items: Vec<u64>,
            sel: SelectionState,
        }

        // Assert the result satisfies the `WidgetView` bound consumers rely on.
        fn assert_widget_view<V: WidgetView<S, ()>>(_: &V) {}

        let items: ItemsFn<S, u64> = Arc::new(|s: &S| &s.items[..]);
        let lens: SelectionLens<S> = Arc::new(|s: &mut S| &mut s.sel);
        let render_row: RenderRow<S, u64> =
            Arc::new(|item: &u64, _selected, _theme| Box::new(label(format!("row {item}"))));

        let view = collection_body(CollectionBodyParams {
            item_count: 3,
            items,
            id_source: IdSource::Explicit(Arc::new(|item: &u64| *item)),
            selection_lens: Some(lens),
            scroll: ScrollState::new(),
            lazy: Some(Lazy {
                threshold: 8,
                callback: Arc::new(|_state: &mut S| {}),
            }),
            render_row,
            leading_hit_zone: None,
            on_activate: Some(Arc::new(|_state: &mut S, _id: u64| {})),
            tree_meta: None,
            on_toggle: None,
            theme: Theme::default(),
        });

        assert_widget_view(&view);
    }

    #[derive(Default, Debug, PartialEq)]
    struct Marker;

    #[test]
    fn collection_body_is_generic_over_the_host_action() {
        use std::sync::Arc;

        use xilem::WidgetView;
        use xilem::view::label;

        use super::{CollectionBodyParams, Lazy, RenderRow, collection_body};
        use crate::Theme;
        use crate::collection::{IdSource, ItemsFn, ScrollState};

        struct S {
            items: Vec<u64>,
        }
        fn assert_widget_view<V: WidgetView<S, Marker>>(_: &V) {}

        let items: ItemsFn<S, u64> = Arc::new(|s: &S| &s.items[..]);
        let render_row: RenderRow<S, u64> =
            Arc::new(|item: &u64, _selected, _theme| Box::new(label(format!("row {item}"))));

        let view = collection_body(CollectionBodyParams {
            item_count: 3,
            items,
            id_source: IdSource::Position,
            selection_lens: None,
            scroll: ScrollState::new(),
            lazy: Some(Lazy {
                threshold: 8,
                callback: Arc::new(|_state: &mut S| Marker),
            }),
            render_row,
            leading_hit_zone: None,
            on_activate: None,
            tree_meta: None,
            on_toggle: None,
            theme: Theme::default(),
        });
        assert_widget_view(&view);
    }
}
