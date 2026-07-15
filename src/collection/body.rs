//! Unified virtualized body: a masonry widget that adds Left/Right
//! tree-focus navigation over `VirtualScroll`. Up/Down row-focus navigation
//! is handled locally by each row instead (see
//! [`row_click`](super::row_click)'s module docs for why), driven by this
//! widget's [`refresh_row_nav`](CollectionBodyWidget::refresh_row_nav). The
//! xilem `View` (`collection_body`) that drives scroll-to-anchor, lazy-load,
//! and central click routing lives in the sibling `body_view` module.

use masonry::accesskit::Role;
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};
use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;

use super::row_click::{RowClickable, TreeRowMeta};
use super::single_child;

/// Single-child wrapper around masonry's `VirtualScroll` adding tree
/// keyboard navigation for expandable collections: Left/Right move focus to
/// a row's parent or first child. (Up/Down row-focus movement is handled
/// locally by the focused row instead — see
/// [`RowClickable`](super::row_click::RowClickable) — because `VirtualScroll`
/// sits between a row and this widget and claims Up/Down via its own
/// built-in arrow-key scrolling before this widget's `on_text_event` would
/// run.) Expand/collapse itself is handled by the focused row too; the
/// cases that reach here are the focus-movement ones a row can't do because
/// it doesn't know row order.
///
/// Navigation operates over the **materialized** rows (`VirtualScroll` buffers
/// ~a page beyond the viewport, so adjacent targets are present). A target past
/// the materialized edge is a no-op. Scrolling the newly-focused row into view
/// is a deferred, substrate-wide improvement; see issue #136.
pub(crate) struct CollectionBodyWidget {
    child: WidgetPod<VirtualScrollWidget>,
    /// Per-visible-row tree metadata in materialized order, kept in sync by
    /// [`super::body_view`] so Left/Right can find a row's parent (by depth) or
    /// first child. Empty for a flat (non-tree) collection.
    row_meta: Vec<(WidgetId, TreeRowMeta)>,
}

impl CollectionBodyWidget {
    pub(crate) fn new(child: NewWidget<VirtualScrollWidget>) -> Self {
        Self {
            child: child.to_pod(),
            row_meta: Vec::new(),
        }
    }

    pub(crate) fn virtual_scroll_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, VirtualScrollWidget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Replaces the per-visible-row tree metadata used by Left/Right nav. Cheap:
    /// it only affects the next key press, so no repaint/relayout is requested.
    pub(crate) fn set_row_meta(this: &mut WidgetMut<'_, Self>, meta: Vec<(WidgetId, TreeRowMeta)>) {
        this.widget.row_meta = meta;
    }

    /// Precomputes each visible row's up/down materialized-neighbor
    /// `WidgetId` and pushes it into the row via [`RowClickable::set_nav`],
    /// so Up/Down navigation can be handled locally by the focused row
    /// itself — see [`row_click`](super::row_click)'s module docs for why
    /// this widget can't handle Up/Down directly anymore. Called on every
    /// rebuild by `body_view::CollectionBodyView::rebuild`, mirroring
    /// `refresh_tree_row_meta`'s "materialization has caught up" trigger
    /// point: a momentarily-stale `active_start` mid-transition at worst
    /// yields a transiently-wrong nav map the next settled rebuild corrects.
    ///
    /// # Panics
    ///
    /// Panics (via [`VirtualScrollWidget::child_mut`]) if `active_start` is
    /// stale enough that `active_start + k` falls outside the live active
    /// range for some materialized row `k` — this is the same invariant
    /// `refresh_tree_row_meta` relies on and should not happen once
    /// materialization has settled.
    pub(crate) fn refresh_row_nav(this: &mut WidgetMut<'_, Self>, active_start: usize) {
        let ids: Vec<WidgetId> = {
            let vs = Self::virtual_scroll_mut(this);
            vs.widget.children_ids().iter().copied().collect()
        };
        for k in 0..ids.len() {
            let up = k.checked_sub(1).and_then(|j| ids.get(j)).copied();
            let down = ids.get(k + 1).copied();
            let idx = active_start + k;
            {
                let mut vs = Self::virtual_scroll_mut(this);
                let mut row = VirtualScrollWidget::child_mut(&mut vs, idx);
                let mut row = row.downcast::<RowClickable>();
                RowClickable::set_nav(&mut row, up, down);
            }
        }
    }

    /// The tree metadata for the materialized row with id `id`, if tracked.
    fn meta_of(&self, id: WidgetId) -> Option<TreeRowMeta> {
        self.row_meta
            .iter()
            .find(|(w, _)| *w == id)
            .map(|(_, m)| *m)
    }

    /// Right target: an expanded parent moves focus to its first child (the next
    /// row, since children are spliced right after their parent). A leaf yields
    /// `None` (a collapsed parent's Right was already handled by the row as an
    /// expand, so it never reaches here).
    fn tree_first_child(
        &self,
        focused: WidgetId,
        row_ids: &ChildrenIds,
        pos: usize,
    ) -> Option<WidgetId> {
        let meta = self.meta_of(focused)?;
        if meta.has_children && meta.is_expanded {
            pos.checked_add(1).and_then(|i| row_ids.get(i)).copied()
        } else {
            None
        }
    }

    /// Left target: the row's parent — the nearest preceding materialized row
    /// with a shallower depth. A depth-0 row has no parent (`None`).
    fn tree_parent(&self, focused: WidgetId) -> Option<WidgetId> {
        let idx = self.row_meta.iter().position(|(id, _)| *id == focused)?;
        let depth = self.row_meta[idx].1.depth;
        self.row_meta[..idx]
            .iter()
            .rev()
            .find(|(_, m)| m.depth < depth)
            .map(|(id, _)| *id)
    }
}

impl Widget for CollectionBodyWidget {
    type Action = NoAction;

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        // Only Left/Right (tree collections) reach here — Up/Down are
        // handled locally by the focused row itself
        // (`RowClickable::on_text_event`), since `VirtualScroll`'s built-in
        // arrow-key scrolling would otherwise intercept them before they got
        // this far. See `row_click`'s module docs.
        if !matches!(
            &key.key,
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight)
        ) {
            return;
        }
        let Some(focused) = ctx.focus_target_id() else {
            return;
        };
        let row_ids = {
            let (virtual_scroll, _) = ctx.get_raw(&mut self.child);
            virtual_scroll.children_ids()
        };
        let Some(pos) = row_ids.iter().position(|&id| id == focused) else {
            return;
        };
        // Left/Right do tree focus moves. A case that doesn't apply here (a
        // leaf's Right, a depth-0 Left, or a toggle already handled by the
        // row) yields `None` and is left unhandled so nothing is swallowed.
        let target: Option<WidgetId> = match &key.key {
            Key::Named(NamedKey::ArrowRight) => self.tree_first_child(focused, &row_ids, pos),
            Key::Named(NamedKey::ArrowLeft) => self.tree_parent(focused),
            _ => None,
        };
        if let Some(target) = target {
            ctx.set_focus(target);
            ctx.set_handled();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        single_child::register_children(ctx, &mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        single_child::measure(ctx, &mut self.child, axis, len_req, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        single_child::layout(ctx, &mut self.child, size);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _p: &mut Painter<'_>) {
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut masonry::accesskit::Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        single_child::children_ids(&self.child)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{NewWidget, TextEvent, WidgetId};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;
    use xilem::masonry::widgets::{VirtualScroll, VirtualScrollAction};

    use super::{CollectionBodyWidget, RowClickable, TreeRowMeta};
    use crate::Theme;

    /// Builds a [`TextEvent`] for a `Down`-state press of the given named key.
    fn arrow_key(named: NamedKey) -> TextEvent {
        TextEvent::key_down(Key::Named(named))
    }

    /// Installs a per-visible-row tree layout on the body widget for the
    /// Left/Right nav tests. `layout` is `(row index, depth, has_children,
    /// is_expanded)` in ascending-index (materialized) order.
    fn set_tree(
        harness: &mut TestHarness<CollectionBodyWidget>,
        rows: &HashMap<usize, WidgetId>,
        layout: &[(usize, u16, bool, bool)],
    ) {
        let meta: Vec<(WidgetId, TreeRowMeta)> = layout
            .iter()
            .filter_map(|&(idx, depth, has_children, is_expanded)| {
                rows.get(&idx).map(|&id| {
                    (
                        id,
                        TreeRowMeta {
                            depth,
                            has_children,
                            is_expanded,
                        },
                    )
                })
            })
            .collect();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::set_row_meta(&mut body, meta));
    }

    // A small tree over the first materialized rows:
    //   0  Parent A     depth 0, expanded
    //   1    Child A1   depth 1, leaf
    //   2    Child A2   depth 1, expanded parent
    //   3      GC A2a   depth 2, leaf
    //   4  Parent B     depth 0, collapsed
    const TREE: &[(usize, u16, bool, bool)] = &[
        (0, 0, true, true),
        (1, 1, false, false),
        (2, 1, true, true),
        (3, 2, false, false),
        (4, 0, true, false),
    ];

    /// Pumps the harness until `VirtualScroll` stops asking for row
    /// changes, materializing each requested row as a plain `Label` and
    /// recording its [`WidgetId`] keyed by row index.
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

    /// Builds a body of materialized `Label` rows and returns the harness,
    /// the inner `VirtualScroll`'s id, and a map from row index to row id.
    fn harness_with_rows() -> (
        TestHarness<CollectionBodyWidget>,
        WidgetId,
        HashMap<usize, WidgetId>,
    ) {
        let scroll = NewWidget::new(VirtualScroll::new(0, 100));
        let body = NewWidget::new(CollectionBodyWidget::new(scroll));
        let mut harness = TestHarness::create_with_size(default_property_set(), body, (200, 400));
        let scroll_id = harness
            .edit_root_widget(|mut body| CollectionBodyWidget::virtual_scroll_mut(&mut body).id());
        let mut rows = HashMap::new();
        drive_to_fixpoint(&mut harness, &mut rows);
        (harness, scroll_id, rows)
    }

    /// Like `drive_to_fixpoint`/`harness_with_rows` above, but materializes each
    /// row as a `RowClickable` (wrapping a `Label`) instead of a bare `Label` —
    /// needed to exercise `refresh_row_nav`, which pushes targets into
    /// `RowClickable` specifically.
    fn drive_to_fixpoint_clickable(
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
                        let row = NewWidget::new(RowClickable::new(
                            NewWidget::new(Label::new(format!("row {idx}"))),
                            false,
                            &Theme::default(),
                            None,
                            false,
                            None,
                        ))
                        .erased();
                        let row_id = row.id();
                        VirtualScroll::add_child(&mut scroll, idx, row);
                        rows.insert(idx, row_id);
                    }
                }
            });
        }
    }

    /// Builds a body of materialized `RowClickable` rows and returns the
    /// harness and a map from row index to row id.
    fn harness_with_clickable_rows() -> (TestHarness<CollectionBodyWidget>, HashMap<usize, WidgetId>)
    {
        let scroll = NewWidget::new(VirtualScroll::new(0, 100));
        let body = NewWidget::new(CollectionBodyWidget::new(scroll));
        let mut harness = TestHarness::create_with_size(default_property_set(), body, (200, 400));
        let mut rows = HashMap::new();
        drive_to_fixpoint_clickable(&mut harness, &mut rows);
        (harness, rows)
    }

    /// The core of the fix: after `refresh_row_nav`, pressing `ArrowDown` on a
    /// materialized row moves focus to the next materialized row.
    #[test]
    fn refresh_row_nav_lets_arrow_down_move_focus_between_materialized_rows() {
        let (mut harness, rows) = harness_with_clickable_rows();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));

        let first = rows[&0];
        let second = rows[&1];
        harness.focus_on(Some(first));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(second));
    }

    /// ...and `ArrowUp` moves it back.
    #[test]
    fn refresh_row_nav_lets_arrow_up_move_focus_between_materialized_rows() {
        let (mut harness, rows) = harness_with_clickable_rows();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));

        let first = rows[&0];
        let second = rows[&1];
        harness.focus_on(Some(second));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowUp));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(first));
    }

    /// Unlike a focused row (which now intercepts Up/Down before
    /// `VirtualScroll` ever sees them — see `row_click::tests`), nothing
    /// pre-empts `VirtualScroll` when it is itself the focus target, so its
    /// own built-in scroll-by-arrow-key handling claims the event. This is
    /// masonry's native behavior for a directly-focused scroll area, not a
    /// bug — rows always take focus in practice (`RowClickable::accepts_focus`
    /// is `true` and it calls `ctx.request_focus()` on press), so this is an
    /// edge case, not the common path.
    #[test]
    fn arrow_down_on_non_row_focus_is_handled_by_virtual_scrolls_native_scrolling() {
        let (mut harness, scroll, _rows) = harness_with_rows();
        // Focus the VirtualScroll itself (not one of its row children).
        harness.focus_on(Some(scroll));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(scroll));
    }

    // --- tree Left/Right focus navigation ---

    #[test]
    fn right_on_expanded_parent_focuses_first_child() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        set_tree(&mut harness, &ids, TREE);
        harness.focus_on(Some(ids[&0])); // Parent A (expanded)
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowRight));
        assert!(handled.is_handled());
        assert_eq!(
            harness.focused_widget_id(),
            Some(ids[&1]),
            "Right on an expanded parent moves to its first child",
        );
    }

    #[test]
    fn right_on_leaf_or_collapsed_parent_does_not_move() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        set_tree(&mut harness, &ids, TREE);
        // Leaf (Child A1): Right does nothing here.
        harness.focus_on(Some(ids[&1]));
        assert!(
            !harness
                .process_text_event(arrow_key(NamedKey::ArrowRight))
                .is_handled()
        );
        assert_eq!(harness.focused_widget_id(), Some(ids[&1]));
        // Collapsed parent (Parent B): Right expands via the row, not the body,
        // so the body leaves focus put.
        harness.focus_on(Some(ids[&4]));
        assert!(
            !harness
                .process_text_event(arrow_key(NamedKey::ArrowRight))
                .is_handled()
        );
        assert_eq!(harness.focused_widget_id(), Some(ids[&4]));
    }

    #[test]
    fn left_on_child_focuses_its_parent_skipping_deeper_siblings() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        set_tree(&mut harness, &ids, TREE);
        // Grandchild A2a (depth 2) → parent Child A2 (depth 1).
        harness.focus_on(Some(ids[&3]));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowLeft));
        assert!(handled.is_handled());
        assert_eq!(
            harness.focused_widget_id(),
            Some(ids[&2]),
            "depth 2 → depth 1"
        );
        // Child A2 (depth 1) → Parent A (depth 0), skipping Child A1.
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowLeft));
        assert!(handled.is_handled());
        assert_eq!(
            harness.focused_widget_id(),
            Some(ids[&0]),
            "depth 1 → depth 0"
        );
    }

    #[test]
    fn left_on_depth_zero_row_does_not_move() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        set_tree(&mut harness, &ids, TREE);
        harness.focus_on(Some(ids[&0])); // Parent A, depth 0 — no parent
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowLeft));
        assert!(!handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(ids[&0]));
    }

    #[test]
    fn left_right_are_no_ops_without_tree_metadata() {
        // A flat collection (no row_meta set) ignores Left/Right entirely.
        let (mut harness, _scroll, ids) = harness_with_rows();
        harness.focus_on(Some(ids[&1]));
        for key in [NamedKey::ArrowLeft, NamedKey::ArrowRight] {
            assert!(!harness.process_text_event(arrow_key(key)).is_handled());
            assert_eq!(harness.focused_widget_id(), Some(ids[&1]));
        }
    }

    /// Right on an expanded parent at the materialized edge is a no-op: its
    /// first child (the next row) isn't materialized, so there's nothing to
    /// focus — the same behavior Down has at the edge. Scrolling an off-screen
    /// target into view is deferred (#136).
    #[test]
    fn tree_nav_to_an_unmaterialized_target_is_a_no_op() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        let last_idx = *ids.keys().max().expect("at least one materialized row");
        // Mark the last materialized row an expanded parent.
        set_tree(&mut harness, &ids, &[(last_idx, 0, true, true)]);
        harness.focus_on(Some(ids[&last_idx]));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowRight));
        assert!(
            !handled.is_handled(),
            "no materialized first child → Right no-ops (like Down at the edge)",
        );
        assert_eq!(harness.focused_widget_id(), Some(ids[&last_idx]));
    }
}
