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
use super::row_click::{RowClickAction, clickable_row};
use super::{
    IdSource, ItemsFn, ScrollState, SelectionLens, apply_row_click, clamp_scroll_index,
    scroll_idx_to_slice, scroll_range_end,
};
use crate::Theme;

/// Per-item content renderer: `(item, selected, theme) -> content view`.
pub(crate) type RenderRow<State, Item> =
    Arc<dyn Fn(&Item, bool, &Theme) -> Box<AnyWidgetView<State>> + Send + Sync>;

/// Lazy-load config: fire `callback` when the active range comes within
/// `threshold` items of the end.
pub(crate) struct Lazy<State> {
    pub(crate) threshold: u64,
    pub(crate) callback: Arc<dyn Fn(&mut State) + Send + Sync>,
}

/// All inputs needed to materialize the virtualized body.
pub(crate) struct CollectionBodyParams<State, Item> {
    pub(crate) item_count: u64,
    pub(crate) items: ItemsFn<State, Item>,
    pub(crate) id_source: IdSource<Item>,
    pub(crate) selection_lens: Option<SelectionLens<State>>,
    pub(crate) scroll: ScrollState,
    pub(crate) lazy: Option<Lazy<State>>,
    pub(crate) render_row: RenderRow<State, Item>,
    pub(crate) theme: Theme,
}

/// Builds the virtualized body view. The substrate owns virtualization,
/// scroll-to-anchor, lazy-load, keyboard nav, selection background, and
/// click routing; the caller supplies only per-row content via `render_row`.
pub(crate) fn collection_body<State, Item>(
    params: CollectionBodyParams<State, Item>,
) -> impl WidgetView<State, ()> + use<State, Item>
where
    State: 'static,
    Item: 'static,
{
    let CollectionBodyParams {
        item_count,
        items,
        id_source,
        selection_lens,
        scroll,
        lazy,
        render_row,
        theme,
    } = params;
    let valid_range_end = scroll_range_end(item_count);

    let child = virtual_scroll(0..valid_range_end, {
        let items = Arc::clone(&items);
        let id_source = id_source.clone();
        let selection_lens = selection_lens.clone();
        let render_row = Arc::clone(&render_row);
        move |state: &mut State, idx: i64| {
            let pos = scroll_idx_to_slice(idx);

            let data = (*items)(state);
            let id_at_pos = data.get(pos).map(|item| id_source.id_of(pos, item));
            let is_selected = match (selection_lens.as_ref(), id_at_pos) {
                (Some(sel), Some(id)) => (**sel)(state).contains(id),
                _ => false,
            };

            // Re-borrow: `is_selected` took `&mut State` via the lens.
            let data = (*items)(state);
            let content: Box<AnyWidgetView<State>> = match data.get(pos) {
                Some(item) => render_row(item, is_selected, &theme),
                None => Box::new(label("")),
            };

            let row_bg = if is_selected {
                theme.palette.surface_2
            } else {
                Color::TRANSPARENT
            };
            let row_view = sized_box(content).background_color(row_bg);

            let items = Arc::clone(&items);
            let id_source = id_source.clone();
            let selection_lens = selection_lens.clone();
            clickable_row(
                row_view,
                move |state: &mut State, action: RowClickAction| {
                    apply_row_click(
                        state,
                        action,
                        pos,
                        &items,
                        selection_lens.as_ref(),
                        &id_source,
                    );
                },
            )
        }
    });

    CollectionBodyView {
        child,
        scroll,
        item_count,
        lazy,
    }
}

struct CollectionBodyView<V, State> {
    child: V,
    scroll: ScrollState,
    item_count: u64,
    lazy: Option<Lazy<State>>,
}

struct CollectionBodyViewState<S> {
    child_state: S,
    applied_generation: u64,
}

impl<V, State> ViewMarker for CollectionBodyView<V, State> {}

impl<State, V> View<State, (), ViewCtx> for CollectionBodyView<V, State>
where
    State: 'static,
    V: View<State, (), ViewCtx, Element = Pod<VirtualScrollWidget>>,
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
                VirtualScrollWidget::overwrite_anchor(&mut vs, idx);
            }
        }
        let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
        self.child
            .rebuild(&prev.child, &mut view_state.child_state, ctx, vs, app_state);
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
    ) -> MessageResult<()> {
        if message.remaining_path().is_empty()
            && let Some(lazy) = self.lazy.as_ref()
        {
            let end = scroll_range_end(self.item_count);
            let threshold = i64::try_from(lazy.threshold).unwrap_or(i64::MAX);
            // Peek without consuming (`false`): the `VirtualScrollAction`
            // still routes onward to the child so virtualization handles it.
            message.maybe_take_message::<VirtualScrollAction>(|action| {
                if end - action.target.end <= threshold {
                    (lazy.callback)(app_state);
                }
                false
            });
        }
        let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
        self.child
            .message(&mut view_state.child_state, message, vs, app_state)
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
        rows: &mut HashMap<i64, WidgetId>,
    ) {
        let mut iteration = 0;
        loop {
            iteration += 1;
            assert!(iteration <= 1000, "Took too long to reach fixpoint");
            let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() else {
                break;
            };
            harness.edit_root_widget(|mut body| {
                let mut scroll = CollectionBodyWidget::virtual_scroll_mut(&mut body);
                VirtualScroll::will_handle_action(&mut scroll, &action);
                for idx in action.old_active.clone() {
                    if !action.target.contains(&idx) {
                        VirtualScroll::remove_child(&mut scroll, idx);
                        rows.remove(&idx);
                    }
                }
                for idx in action.target.clone() {
                    if !action.old_active.contains(&idx) {
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
    /// re-anchors the inner `VirtualScroll` via
    /// [`VirtualScroll::overwrite_anchor`] with the clamped index whenever
    /// the `ScrollState` generation changes. This exercises that exact call
    /// at the widget level (the `rebuild` path drives it identically) and
    /// asserts the materialized window moves to include the requested row.
    #[test]
    fn overwrite_anchor_moves_the_materialized_window_to_the_target() {
        const ITEM_COUNT: u64 = 1000;
        const TARGET: u64 = 900;

        let scroll = NewWidget::new(
            VirtualScroll::new(0).with_valid_range(0..i64::try_from(ITEM_COUNT).unwrap()),
        );
        let body = NewWidget::new(CollectionBodyWidget::new(scroll));
        let mut harness = TestHarness::create_with_size(default_property_set(), body, (200, 400));
        let mut rows = HashMap::new();
        drive_to_fixpoint(&mut harness, &mut rows);

        // Initially anchored at row 0; the target near the end is well
        // outside the first materialized window.
        assert!(
            !rows.contains_key(&i64::try_from(TARGET).unwrap()),
            "row {TARGET} should be outside the initial window, got {:?}",
            rows.keys().copied().collect::<Vec<_>>()
        );

        // Re-anchor to the target with the clamped index, exactly as
        // `collection_body::rebuild` does on a ScrollState generation bump.
        let idx = clamp_scroll_index(TARGET, ITEM_COUNT).expect("non-empty grid");
        harness.edit_root_widget(|mut body| {
            let mut vs = CollectionBodyWidget::virtual_scroll_mut(&mut body);
            VirtualScroll::overwrite_anchor(&mut vs, idx);
        });
        drive_to_fixpoint(&mut harness, &mut rows);

        assert!(
            rows.contains_key(&i64::try_from(TARGET).unwrap()),
            "expected materialized window to include row {TARGET} after re-anchor, got {:?}",
            rows.keys().copied().collect::<Vec<_>>()
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
            theme: Theme::default(),
        });

        assert_widget_view(&view);
    }
}
