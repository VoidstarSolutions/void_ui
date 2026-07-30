//! Unified virtualized body: a masonry widget wrapping `VirtualScroll`.
//! Left/Right tree-focus navigation, like Up/Down, is handled locally by
//! each focused row instead (see [`row_click`](super::row_click)'s module
//! docs for why) — this widget's job is precomputing each visible row's
//! nav targets (up/down neighbor, and for a tree collection, its
//! materialized parent) via [`refresh_row_nav`](CollectionBodyWidget::refresh_row_nav)
//! and pushing them down, since a row doesn't know its siblings or
//! ancestors on its own. The xilem `View` (`collection_body`) that drives
//! scroll-to-anchor, lazy-load, and central click routing lives in the
//! sibling `body_view` module.

use std::collections::HashSet;

use masonry::accesskit::Role;
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};
use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;

use super::row_click::{RowClickable, TreeRowMeta};
use super::single_child;
use super::window::MaterializedWindow;

/// Single-child wrapper around masonry's `VirtualScroll`. All keyboard focus
/// navigation (Up/Down/Left/Right) is handled locally by the focused row
/// itself — see [`RowClickable`](super::row_click::RowClickable) — because
/// this widget can't issue a scroll-into-view request for a descendant row:
/// `VirtualScroll` sits between a row and this widget, so a scroll request
/// from *this* widget's own `ctx` would walk up through this widget's
/// ancestors, never reaching `VirtualScroll`. This widget's job is instead
/// precomputing each visible row's nav targets (up/down neighbor via
/// `refresh_row_nav`; for a tree collection, its materialized parent too)
/// and pushing them down, since a row doesn't know its siblings or
/// ancestors on its own.
///
/// Navigation operates over the **materialized** rows (`VirtualScroll` buffers
/// ~a page beyond the viewport, so adjacent targets are present). A target past
/// the materialized edge is a no-op. Moving focus also requests a
/// scroll-into-view for the new target — see
/// `RowClickable::request_scroll_by_rows` in `row_click.rs`.
pub(crate) struct CollectionBodyWidget {
    child: WidgetPod<VirtualScrollWidget>,
    /// Per-visible-row tree metadata in materialized order, kept in sync by
    /// [`super::body_view`] so Left nav can find a row's materialized parent
    /// (by depth) — see [`tree_parent_with_distance`](Self::tree_parent_with_distance).
    /// Right's first-child target doesn't need this: it's always the next
    /// materialized row, the same `nav_down` target Down already uses. Empty
    /// for a flat (non-tree) collection. Carries one entry per materialized
    /// row — the same set `refresh_row_nav` walks — with `None` for a row
    /// that isn't tree-tracked (host `tree_meta` returned `None` for it, or
    /// it's past the data edge): such a row still occupies a materialized
    /// position and must still count toward another row's parent distance,
    /// it just can't itself be a parent-match candidate.
    row_meta: Vec<(WidgetId, Option<TreeRowMeta>)>,
    /// Row ids seen in `VirtualScroll`'s materialized set as of the end of the
    /// previous [`refresh_row_nav`](Self::refresh_row_nav) call. See that
    /// method's doc for why this exists: a row `VirtualScroll::add_child`-ed
    /// during the *current* rebuild isn't safe to touch yet, but by the next
    /// call it will be.
    registered_row_ids: Vec<WidgetId>,
}

impl CollectionBodyWidget {
    pub(crate) fn new(child: NewWidget<VirtualScrollWidget>) -> Self {
        Self {
            child: child.to_pod(),
            row_meta: Vec::new(),
            registered_row_ids: Vec::new(),
        }
    }

    pub(crate) fn virtual_scroll_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, VirtualScrollWidget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Replaces the per-visible-row tree metadata used by Left/Right nav. Cheap:
    /// it only affects the next key press, so no repaint/relayout is requested.
    pub(crate) fn set_row_meta(
        this: &mut WidgetMut<'_, Self>,
        meta: Vec<(WidgetId, Option<TreeRowMeta>)>,
    ) {
        this.widget.row_meta = meta;
    }

    /// Precomputes each visible row's up/down materialized-neighbor
    /// `WidgetId`, and (for a tree collection) its materialized-parent target
    /// and distance, pushing both into the row via
    /// [`RowClickable::set_nav`]/[`RowClickable::set_tree_parent_nav`] so
    /// Up/Down/Left navigation can all be handled locally by the focused row
    /// itself — see [`row_click`](super::row_click)'s module docs for why
    /// this widget can't handle them directly. Called on every rebuild by
    /// `body_view::CollectionBodyView::rebuild`, mirroring
    /// `refresh_tree_row_meta`'s "materialization has caught up" trigger
    /// point (which must run first each rebuild, so this method sees a fresh
    /// `row_meta` — see that call site's ordering).
    ///
    /// A row `VirtualScroll::add_child`-ed by the *same* rebuild pass (via
    /// the child's own `rebuild`, called just before this) is skipped rather
    /// than touched: masonry only registers freshly added children with its
    /// mutate arena in the update pass that runs *after* this rebuild
    /// returns, so [`VirtualScrollWidget::child_mut`] on one would panic
    /// (`"get_mut: child not found"`) — this is not a stale-index problem
    /// `active_start` accuracy can fix, it's a pass-ordering one. We detect
    /// "added this pass" by comparing against the row ids seen at the *end*
    /// of the previous call: at least one full settle cycle always runs
    /// between any two `rebuild`s, so anything already in that set from the
    /// prior call is guaranteed registered by now. Skipped rows keep whatever nav
    /// target they had before (`None` if brand new) until the next call,
    /// once they've had a chance to register — the same
    /// "transiently-wrong-then-self-heals" contract `refresh_tree_row_meta`
    /// already relies on, just triggered by a different condition.
    pub(crate) fn refresh_row_nav(this: &mut WidgetMut<'_, Self>, active_start: usize) {
        let window = MaterializedWindow::new(active_start);
        let ids: Vec<WidgetId> = {
            let vs = Self::virtual_scroll_mut(this);
            vs.widget.children_ids().iter().copied().collect()
        };
        let previously_registered: HashSet<WidgetId> =
            std::mem::replace(&mut this.widget.registered_row_ids, ids.clone())
                .into_iter()
                .collect();
        // Computed from `row_meta` before `virtual_scroll_mut` reborrows
        // `this` below — `row_meta` is empty for a flat (non-tree)
        // collection, so every entry is `None` there, same as today.
        let nav_parents: Vec<Option<(WidgetId, u32)>> = ids
            .iter()
            .map(|&id| this.widget.tree_parent_with_distance(id))
            .collect();
        let mut vs = Self::virtual_scroll_mut(this);
        for k in 0..ids.len() {
            let id = ids[k];
            if !previously_registered.contains(&id) {
                continue;
            }
            let up = k.checked_sub(1).and_then(|j| ids.get(j)).copied();
            let down = ids.get(k + 1).copied();
            let idx = window.index_for_slot(k);
            let mut row = VirtualScrollWidget::child_mut(&mut vs, idx);
            let mut row = row.downcast::<RowClickable>();
            RowClickable::set_nav(&mut row, up, down);
            RowClickable::set_tree_parent_nav(&mut row, nav_parents[k]);
        }
    }

    /// The materialized-parent target for `focused` — the nearest preceding
    /// row in `row_meta` with a shallower depth — and its distance from
    /// `focused`, counted in materialized rows (not tree-depth difference:
    /// skipping over a deeper sibling's whole subtree still counts each of
    /// those intervening rows, since `RowClickable::request_scroll_by_rows`
    /// needs a row-count, not a depth delta). The distance walks `row_meta`'s
    /// full index range — including rows with no tracked metadata — so a row
    /// the host didn't tag as tree-tracked still counts as one materialized
    /// row of distance; only rows *with* metadata are candidates for the
    /// shallower-parent match itself. A depth-0 row, or one not tracked in
    /// `row_meta` at all (a flat collection, or the materialized/data edge),
    /// yields `None`.
    fn tree_parent_with_distance(&self, focused: WidgetId) -> Option<(WidgetId, u32)> {
        let idx = self.row_meta.iter().position(|(id, _)| *id == focused)?;
        let depth = self.row_meta[idx].1?.depth;
        self.row_meta[..idx]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(parent_idx, (id, m))| {
                let m = (*m)?;
                (m.depth < depth)
                    .then(|| (*id, u32::try_from(idx - parent_idx).unwrap_or(u32::MAX)))
            })
    }
}

impl Widget for CollectionBodyWidget {
    type Action = NoAction;

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
    /// is_expanded)`, need not cover every materialized row — one that's
    /// omitted mirrors a row the host's `tree_meta` returned `None` for, and
    /// still gets a `row_meta` entry (`None`) so it counts toward another
    /// row's parent distance without being a parent-match candidate itself
    /// (see `tree_parent_with_distance`).
    fn set_tree(
        harness: &mut TestHarness<CollectionBodyWidget>,
        rows: &HashMap<usize, WidgetId>,
        layout: &[(usize, u16, bool, bool)],
    ) {
        let tracked: HashMap<usize, TreeRowMeta> = layout
            .iter()
            .map(|&(idx, depth, has_children, is_expanded)| {
                (
                    idx,
                    TreeRowMeta {
                        depth,
                        has_children,
                        is_expanded,
                    },
                )
            })
            .collect();
        let mut indices: Vec<usize> = rows.keys().copied().collect();
        indices.sort_unstable();
        let meta: Vec<(WidgetId, Option<TreeRowMeta>)> = indices
            .into_iter()
            .map(|idx| (rows[&idx], tracked.get(&idx).copied()))
            .collect();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::set_row_meta(&mut body, meta));

        // Also push each row's own TreeRowMeta directly onto its
        // RowClickable — production wires this per-row at view-build time
        // (see body_view.rs's TreeMetaFn), independent of set_row_meta
        // above. RowClickable::on_text_event now needs it locally to decide
        // whether Right moves focus to a materialized first child.
        for &(idx, depth, has_children, is_expanded) in layout {
            if !rows.contains_key(&idx) {
                continue;
            }
            harness.edit_root_widget(|mut body| {
                let mut vs = CollectionBodyWidget::virtual_scroll_mut(&mut body);
                let mut row = VirtualScroll::child_mut(&mut vs, idx);
                let mut row = row.downcast::<RowClickable>();
                RowClickable::set_tree_meta(
                    &mut row,
                    Some(TreeRowMeta {
                        depth,
                        has_children,
                        is_expanded,
                    }),
                );
            });
        }
    }

    // A small tree over the first materialized rows:
    //   0  Parent A     depth 0, expanded
    //   1    Child A1   depth 1, leaf
    //   2    Child A2   depth 1, collapsed parent (has a materialized child
    //                   below it anyway — this is injected test metadata, not
    //                   an enforced expand/collapse invariant — kept
    //                   *collapsed* rather than expanded so pressing
    //                   `ArrowLeft` while focused here exercises
    //                   `nav_parent`'s skip logic instead of being
    //                   intercepted as a collapse-toggle: a genuinely
    //                   *expanded* parent's own `ArrowLeft` always collapses
    //                   first, per `RowClickable::on_text_event`'s
    //                   `toggles_on` guard — see `left_toggles_an_expanded_parent_right_does_not`
    //                   in `row_click.rs`)
    //   3      GC A2a   depth 2, leaf
    //   4  Parent B     depth 0, collapsed
    const TREE: &[(usize, u16, bool, bool)] = &[
        (0, 0, true, true),
        (1, 1, false, false),
        (2, 1, true, false),
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
    ///
    /// `refresh_row_nav` is called twice: the first call only primes the
    /// registered-rows cache (these rows were `add_child`-ed by
    /// `harness_with_clickable_rows`, so they're not yet known-registered
    /// from *this* function's perspective — see its doc), the second
    /// actually sets nav targets now that they're confirmed registered.
    #[test]
    fn refresh_row_nav_lets_arrow_down_move_focus_between_materialized_rows() {
        let (mut harness, rows) = harness_with_clickable_rows();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
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
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));

        let first = rows[&0];
        let second = rows[&1];
        harness.focus_on(Some(second));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowUp));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(first));
    }

    /// #136: pressing `ArrowDown` to move focus onto a row that's
    /// materialized but outside the current viewport (the far edge of
    /// `VirtualScroll`'s buffered page) must scroll it into view — not just
    /// move focus there and leave the viewport where it was.
    #[test]
    fn arrow_down_past_the_viewport_edge_scrolls_the_new_focus_into_view() {
        let (mut harness, rows) = harness_with_clickable_rows();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));

        // Drain any Scroll/Fetch actions emitted while the harness settled,
        // so the only action left in the queue after the key press below is
        // the one it (should) trigger.
        while harness.pop_action::<VirtualScrollAction>().is_some() {}

        // The very last materialized row sits at the outer edge of
        // VirtualScroll's buffered page — reliably outside the current
        // viewport (see CollectionBodyWidget's module docs).
        let last_idx = *rows.keys().max().expect("at least one materialized row");
        let prev_idx = last_idx - 1;
        assert!(
            rows.contains_key(&prev_idx),
            "fixture must materialize at least two rows for this test"
        );

        harness.focus_on(Some(rows[&prev_idx]));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(rows[&last_idx]));

        let mut scrolled_to_last = false;
        while let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() {
            if let VirtualScrollAction::Scroll(scroll) = action
                && scroll.range_in_viewport().contains(&last_idx)
            {
                scrolled_to_last = true;
            }
        }
        assert!(
            scrolled_to_last,
            "ArrowDown onto row {last_idx} (the far edge of the materialized \
             buffer, well past the viewport) should emit a \
             VirtualScrollAction::Scroll whose viewport range includes it, \
             but none did"
        );
    }

    /// ...and `ArrowUp` scrolls a newly-focused row back into view when
    /// moving focus onto a materialized-but-offscreen row above the
    /// viewport (mirrors the `ArrowDown` test above, using `scroll_to` to
    /// move the viewport away from row 0 first so there's a materialized
    /// row above it for `ArrowUp` to reveal).
    #[test]
    fn arrow_up_before_the_viewport_edge_scrolls_the_new_focus_into_view() {
        let (mut harness, rows) = harness_with_clickable_rows();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));

        // Scroll down first so row 0 (still materialized, per VirtualScroll's
        // buffered page) ends up above the current viewport.
        harness.edit_root_widget(|mut body| {
            let mut vs = CollectionBodyWidget::virtual_scroll_mut(&mut body);
            VirtualScroll::scroll_to(&mut vs, 10);
        });
        assert!(
            rows.contains_key(&0) && rows.contains_key(&1),
            "fixture must still have rows 0 and 1 materialized after scroll_to(10)"
        );

        while harness.pop_action::<VirtualScrollAction>().is_some() {}

        harness.focus_on(Some(rows[&1]));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowUp));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(rows[&0]));

        let mut scrolled_to_first = false;
        while let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() {
            if let VirtualScrollAction::Scroll(scroll) = action
                && scroll.range_in_viewport().contains(&0)
            {
                scrolled_to_first = true;
            }
        }
        assert!(
            scrolled_to_first,
            "ArrowUp onto row 0 (scrolled out of view above) should emit a \
             VirtualScrollAction::Scroll whose viewport range includes it, \
             but none did"
        );
    }

    /// Reproduces #175's regression crash: `refresh_row_nav` must not touch a
    /// row `VirtualScroll::add_child`-ed in the *same* pass. Masonry only
    /// registers a freshly added child with its mutate arena in the update
    /// pass that runs *after* the current rebuild returns, so calling
    /// `VirtualScrollWidget::child_mut` on one immediately (as
    /// `body_view::CollectionBodyView::rebuild` used to, by calling
    /// `refresh_row_nav` right after materializing new rows) panicked with
    /// `"get_mut: child not found"`. This drives `add_child` and
    /// `refresh_row_nav` inside a single `edit_root_widget` call, exactly
    /// mirroring `rebuild`'s ordering, and asserts it doesn't panic.
    #[test]
    fn refresh_row_nav_does_not_touch_a_row_added_in_the_same_pass() {
        let scroll = NewWidget::new(VirtualScroll::new(0, 100));
        let body = NewWidget::new(CollectionBodyWidget::new(scroll));
        let mut harness = TestHarness::create_with_size(default_property_set(), body, (200, 400));

        let (action, _id) = harness
            .pop_action::<VirtualScrollAction>()
            .expect("initial layout requests the first materialized range");
        let VirtualScrollAction::Fetch(action) = action else {
            panic!("expected a Fetch action");
        };

        harness.edit_root_widget(|mut body| {
            {
                let mut scroll = CollectionBodyWidget::virtual_scroll_mut(&mut body);
                VirtualScroll::will_handle_action(&mut scroll, &action);
                for idx in action.target().clone() {
                    let row = NewWidget::new(RowClickable::new(
                        NewWidget::new(Label::new(format!("row {idx}"))),
                        false,
                        &Theme::default(),
                        None,
                        false,
                        None,
                    ))
                    .erased();
                    VirtualScroll::add_child(&mut scroll, idx, row);
                }
            }
            // Must not panic: these rows were just add_child-ed above, in
            // this same pass, so they aren't registered with masonry's
            // mutate arena yet.
            CollectionBodyWidget::refresh_row_nav(&mut body, action.target().start);
        });
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
        let (mut harness, ids) = harness_with_clickable_rows();
        set_tree(&mut harness, &ids, TREE);
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
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
        let (mut harness, ids) = harness_with_clickable_rows();
        set_tree(&mut harness, &ids, TREE);
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        // Leaf (Child A1): Right does nothing here.
        harness.focus_on(Some(ids[&1]));
        assert!(
            !harness
                .process_text_event(arrow_key(NamedKey::ArrowRight))
                .is_handled()
        );
        assert_eq!(harness.focused_widget_id(), Some(ids[&1]));
        // Collapsed parent (Parent B): a collapsed parent's own `ArrowRight`
        // is intercepted as an expand-toggle before nav (see `TREE`'s doc
        // comment above and `RowClickable::on_text_event`'s `toggles_on`
        // guard) — so this *is* handled, just not as a focus move: the row
        // toggles, not `CollectionBodyWidget`'s nav_down, and focus stays put.
        harness.focus_on(Some(ids[&4]));
        assert!(
            harness
                .process_text_event(arrow_key(NamedKey::ArrowRight))
                .is_handled(),
            "Right on a collapsed parent is handled as an expand-toggle by the row itself",
        );
        assert_eq!(harness.focused_widget_id(), Some(ids[&4]));
    }

    #[test]
    fn left_on_child_focuses_its_parent_skipping_deeper_siblings() {
        let (mut harness, ids) = harness_with_clickable_rows();
        set_tree(&mut harness, &ids, TREE);
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
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

    /// #203 follow-up: a row between `focused` and its materialized parent
    /// that carries no tracked `TreeRowMeta` (the host's `tree_meta` returned
    /// `None` for it) must still count toward the parent distance — it
    /// occupies a real materialized row, and `RowClickable::request_scroll_by_rows`
    /// needs an accurate row count to land the scroll on the actual parent,
    /// not just a count of tree-tracked rows. See `tree_parent_with_distance`.
    #[test]
    fn tree_parent_distance_counts_rows_without_tracked_metadata() {
        let (mut harness, ids) = harness_with_clickable_rows();
        assert!(
            ids.contains_key(&4),
            "fixture must materialize at least 5 rows"
        );
        let meta: Vec<(WidgetId, Option<TreeRowMeta>)> = vec![
            (
                ids[&0],
                Some(TreeRowMeta {
                    depth: 0,
                    has_children: true,
                    is_expanded: true,
                }),
            ),
            (ids[&1], None),
            (ids[&2], None),
            (ids[&3], None),
            (
                ids[&4],
                Some(TreeRowMeta {
                    depth: 1,
                    has_children: false,
                    is_expanded: false,
                }),
            ),
        ];
        let (parent, distance) = harness
            .edit_root_widget(|mut body| {
                CollectionBodyWidget::set_row_meta(&mut body, meta);
                body.widget.tree_parent_with_distance(ids[&4])
            })
            .expect("row 0 is a shallower-depth ancestor of row 4");
        assert_eq!(parent, ids[&0]);
        assert_eq!(
            distance, 4,
            "distance must count every materialized row between focused and \
             parent (rows 1-3), not just the ones carrying tracked metadata"
        );
    }

    #[test]
    fn left_on_depth_zero_row_does_not_move() {
        let (mut harness, ids) = harness_with_clickable_rows();
        set_tree(&mut harness, &ids, TREE);
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        // Parent B (depth 0, collapsed — not Parent A) so `ArrowLeft` reaches
        // the `nav_parent` no-op path here instead of being intercepted as a
        // collapse-toggle: Parent A is an *expanded* parent, and an expanded
        // parent's own `ArrowLeft` always collapses first (see `TREE`'s doc
        // comment above).
        harness.focus_on(Some(ids[&4])); // Parent B, depth 0 — no parent
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowLeft));
        assert!(!handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(ids[&4]));
    }

    #[test]
    fn left_right_are_no_ops_without_tree_metadata() {
        // A flat collection (no row_meta / row tree_meta set) ignores
        // Left/Right entirely.
        let (mut harness, ids) = harness_with_clickable_rows();
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.focus_on(Some(ids[&1]));
        for key in [NamedKey::ArrowLeft, NamedKey::ArrowRight] {
            assert!(!harness.process_text_event(arrow_key(key)).is_handled());
            assert_eq!(harness.focused_widget_id(), Some(ids[&1]));
        }
    }

    /// Right on an expanded parent at the materialized edge is a no-op: its
    /// first child (the next row) isn't materialized, so `nav_down` is
    /// `None` — the same behavior Down has at the edge.
    #[test]
    fn tree_nav_to_an_unmaterialized_target_is_a_no_op() {
        let (mut harness, ids) = harness_with_clickable_rows();
        let last_idx = *ids.keys().max().expect("at least one materialized row");
        // Mark the last materialized row an expanded parent.
        set_tree(&mut harness, &ids, &[(last_idx, 0, true, true)]);
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.focus_on(Some(ids[&last_idx]));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowRight));
        assert!(
            !handled.is_handled(),
            "no materialized first child → Right no-ops (like Down at the edge)",
        );
        assert_eq!(harness.focused_widget_id(), Some(ids[&last_idx]));
    }

    /// #203: pressing `ArrowLeft` to move focus onto a materialized ancestor
    /// several rows back — outside the current viewport but still inside
    /// `VirtualScroll`'s buffered page — must scroll it into view. This is
    /// the actual bug: the old code moved focus correctly but never
    /// requested a scroll, and unlike Up/Down (a fixed 1-row offset), a
    /// parent can be an arbitrary number of materialized rows away, which is
    /// exactly what this test exercises (not just the 1-row case Up/Down
    /// already covers).
    #[test]
    fn left_past_the_viewport_edge_scrolls_the_new_focus_into_view() {
        let (mut harness, ids) = harness_with_clickable_rows();
        // A depth-0 parent at row 0, with 30 depth-1 leaf children directly
        // after it (rows 1..=30) — the parent is a materialized ancestor
        // many rows back from any of its later children, well past a single
        // viewport's worth of rows.
        let mut layout: Vec<(usize, u16, bool, bool)> = vec![(0, 0, true, true)];
        for idx in 1..=30 {
            layout.push((idx, 1, false, false));
        }
        set_tree(&mut harness, &ids, &layout);
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));
        harness.edit_root_widget(|mut body| CollectionBodyWidget::refresh_row_nav(&mut body, 0));

        // Scroll down first so row 0 ends up above the current viewport —
        // otherwise it would already be visible from the initial unscrolled
        // position, making the scroll assertion below vacuous. scroll_to(15)
        // rather than, say, scroll_to(30): row 0 must stay inside
        // VirtualScroll's buffered (overscan) page or it gets stashed —
        // scrolling all the way to row 30 pushes row 0 out of that buffer
        // entirely (confirmed empirically: it then panics laying out a
        // stashed widget). 15 keeps row 0 comfortably buffered while still
        // scrolling it out of the *visible* viewport, which is all this test
        // needs.
        harness.edit_root_widget(|mut body| {
            let mut vs = CollectionBodyWidget::virtual_scroll_mut(&mut body);
            VirtualScroll::scroll_to(&mut vs, 15);
        });
        let row_0_still_materialized = harness.edit_root_widget(|mut body| {
            use masonry::core::Widget as _;
            let vs = CollectionBodyWidget::virtual_scroll_mut(&mut body);
            vs.widget.children_ids().iter().any(|&id| id == ids[&0])
        });
        assert!(
            row_0_still_materialized,
            "row 0 must still be materialized (inside VirtualScroll's buffer) \
             after scroll_to(15) — otherwise the Left-nav target below \
             wouldn't exist to focus at all"
        );

        while harness.pop_action::<VirtualScrollAction>().is_some() {}

        harness.focus_on(Some(ids[&30]));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowLeft));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(ids[&0]));

        let mut scrolled_to_parent = false;
        while let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() {
            if let VirtualScrollAction::Scroll(scroll) = action
                && scroll.range_in_viewport().contains(&0)
            {
                scrolled_to_parent = true;
            }
        }
        assert!(
            scrolled_to_parent,
            "ArrowLeft onto row 0 (30 materialized rows back, well past the \
             viewport) should emit a VirtualScrollAction::Scroll whose \
             viewport range includes it, but none did"
        );
    }
}
