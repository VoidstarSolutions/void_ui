//! `CollectionListWidget` — keyboard-nav/highlight/focus bookkeeping wrapping
//! a `VirtualScroll` built by `overlay_list_body`'s xilem View (not by this
//! widget itself — see the design spec's "Key insight" for why `Fetch`
//! handling moved to the View layer). Structurally parallel to
//! `CollectionBodyWidget` wrapping its own `VirtualScroll` child
//! (`src/collection/body.rs`), except this widget owns no item content at
//! all: `item_count` is the only thing it needs from the item list, kept in
//! sync by `overlay_list`'s wrapping View (`overlay_list.rs`, next task).
//!
//! ## `active_start`
//!
//! Mapping a materialized row's position (`VirtualScroll::children_ids()`'s
//! index) back to its global index into the item list requires knowing the
//! slice-position offset of the first materialized row —
//! `VirtualScroll::len(&self)` only reports the materialized *count*, not
//! this offset, and (checked directly against
//! `masonry::widgets::VirtualScroll`'s source) the widget exposes no other
//! plain `&self` accessor for its active range either: `active_range` is a
//! private field, surfaced only transiently via
//! `VirtualScrollFetchAction::old_active`/`target` (owned by the `Fetch`
//! reaction, which now lives entirely in `overlay_list_body`'s View — see
//! the module doc there) and `VirtualScrollScrollAction::range_in_viewport`
//! (a `Scroll` action, not queryable on demand). So there is no way for this
//! widget to derive `active_start` itself.
//!
//! Instead, `active_start` is tracked as a field here and kept in sync by
//! [`Self::set_active_start`], which `overlay_list`'s wrapping View calls on
//! every rebuild (mirroring how `body_view.rs`'s `CollectionBodyViewState`
//! tracks `active_range` at the View level). A field (rather than an
//! explicit parameter threaded through `move_highlight`/`set_highlight`) is
//! required here specifically because `on_text_event` — where
//! `move_highlight` is actually invoked in response to a keypress — only
//! ever receives `&mut self`, with no channel for a caller to hand it
//! current View-level state at call time; `CollectionBodyWidget::refresh_row_nav`
//! doesn't have this constraint, since it's always called directly by the
//! View, never from inside event handling.

use masonry::accesskit::Role;
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetId, WidgetMut, WidgetPod,
    keyboard::{Key, KeyState, NamedKey},
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;

use super::item_row::OverlayListItem;
use super::scroll::clamp_scroll_index;

pub(crate) struct CollectionListWidget {
    child: WidgetPod<VirtualScrollWidget>,
    item_count: usize,
    /// Slice position of the first currently-materialized row. See the
    /// module doc's "`active_start`" section for why this is a field kept in
    /// sync via [`Self::set_active_start`] rather than a parameter.
    active_start: usize,
    highlighted: Option<usize>,
    /// The materialized [`WidgetId`] of the currently-highlighted row, if
    /// any — kept in sync by [`Self::set_highlight`], which is the only
    /// place with a live, materialized `WidgetMut` for it. This widget owns
    /// no item content and so has no other way to resolve "the id of row
    /// `highlighted`" from `&mut self` alone (no `ctx` access) in
    /// [`Widget::accessibility`], which is exactly where it's needed: to set
    /// `aria-activedescendant` on this widget's own node, mirroring the
    /// pre-virtualization `LabelList::accessibility`'s
    /// `node.set_active_descendant(label.id().into())`. `None` whenever
    /// `highlighted` is `None` *or* its row isn't currently materialized
    /// (matches `set_highlight`'s own materialized-window bounds check —
    /// there's no id to point at in that case).
    highlighted_row_id: Option<WidgetId>,
    container_role: Role,
    /// Estimated per-row height, in px, used *only* to answer content-based
    /// (`MaxContent`/`MinContent`) [`Widget::measure`] requests along the
    /// scroll axis — never for real per-row layout, which
    /// `VirtualScroll`'s own `layout` pass already gets right from each
    /// materialized row's actual measured height. See
    /// [`Widget::measure`]'s doc comment on this type for why real content
    /// measurement is unavailable here.
    item_extent: f64,
    /// Backs [`Widget::accepts_focus`]. Per-instance rather than hardcoded
    /// because the two consumers of this widget want genuinely different
    /// keyboard models: autocomplete's `SuggestionList` wants Tab to move
    /// real keyboard focus into the listbox (`Role::ListBox`'s intended ARIA
    /// pattern there), while `dropdown_button`'s `ThemedDropdownButton` keeps
    /// real focus on its trigger the whole time and drives this widget
    /// imperatively via `set_highlight`/`move_highlight` (a roving-highlight
    /// model, matching how a native `<select>` doesn't expose its options as
    /// separate Tab stops). When this is `false`, masonry's default
    /// unhandled-Tab focus search skips straight past this widget instead of
    /// landing on it — see `src/components/dropdown_button/widget.rs`'s
    /// `tab_does_not_move_focus_into_the_menu_*` tests for the regression
    /// this guards against.
    accepts_focus: bool,
}

impl CollectionListWidget {
    pub(crate) fn new(
        child: NewWidget<VirtualScrollWidget>,
        item_count: usize,
        container_role: Role,
        accepts_focus: bool,
        item_extent: f64,
    ) -> Self {
        Self {
            child: child.to_pod(),
            item_count,
            active_start: 0,
            highlighted: None,
            highlighted_row_id: None,
            container_role,
            item_extent,
            accepts_focus,
        }
    }

    /// Mirrors `CollectionBodyWidget::virtual_scroll_mut` — lets the wrapping
    /// View (Task 5) forward `rebuild`/`teardown`/`message` to the real
    /// `virtual_scroll` child underneath.
    pub(crate) fn virtual_scroll_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, VirtualScrollWidget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// The currently-highlighted global index, if any. Test-only — no
    /// production caller needs the raw index outside `set_highlight`'s own
    /// bookkeeping; production code reads highlight state via `accessibility()`
    /// (through `highlighted_row_id`) instead.
    #[cfg(test)]
    pub(crate) fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    /// Test-only accessor for the cached id `accessibility()` reads to set
    /// `aria-activedescendant`. Exists so tests can assert the *id* stays in
    /// sync with `highlighted`, not just the index — see the
    /// `set_item_count_shrinking_clamps_the_highlighted_row_id_too` and
    /// `set_item_count_to_zero_clears_the_highlighted_row_id` tests below.
    #[cfg(test)]
    pub(crate) fn highlighted_row_id(&self) -> Option<WidgetId> {
        self.highlighted_row_id
    }

    /// Updates the slice-position offset of the first currently-materialized
    /// row. Cheap — like `CollectionBodyWidget::set_row_meta`, it only
    /// affects the next keyboard-nav/highlight call, so no repaint/relayout
    /// is requested. Must be called by the wrapping View on every rebuild
    /// whenever `virtual_scroll`'s materialized range may have changed — see
    /// the module doc.
    pub(crate) fn set_active_start(this: &mut WidgetMut<'_, Self>, active_start: usize) {
        this.widget.active_start = active_start;
    }

    /// Clamps `highlighted` past the new end (or clears it entirely once the
    /// list is empty), reusing `clamp_scroll_index`'s pattern
    /// (`src/collection/scroll.rs`) — it already implements exactly this
    /// "clamp to the last valid index, or `None` if there is none" contract.
    ///
    /// Applies the clamped value through [`Self::set_highlight`] rather than
    /// writing `this.widget.highlighted` directly, so a shrink that changes
    /// the highlighted index (or clears it) keeps `highlighted_row_id` and
    /// the materialized row's own highlight paint flag in sync too —
    /// `set_highlight` is the only place with a live `WidgetMut` onto the
    /// materialized row, and it already does exactly this bookkeeping
    /// (including requesting the accessibility update) for every other
    /// caller.
    pub(crate) fn set_item_count(this: &mut WidgetMut<'_, Self>, count: usize) {
        this.widget.item_count = count;
        if let Some(i) = this.widget.highlighted {
            let clamped = clamp_scroll_index(
                u64::try_from(i).unwrap_or(u64::MAX),
                u64::try_from(count).unwrap_or(u64::MAX),
            );
            Self::set_highlight(this, clamped);
        }
    }

    /// Moves the keyboard highlight by `delta`, wrapping via `rem_euclid`
    /// over `item_count` (not a materialized-window count) — ported from
    /// `LabelList::move_highlight`'s existing, already-correct wrap
    /// semantics (`autocomplete/widget.rs:404-424`, pre-rewrite).
    pub(crate) fn move_highlight(this: &mut WidgetMut<'_, Self>, delta: isize) {
        let n = this.widget.item_count;
        if n == 0 {
            return;
        }
        let next = match this.widget.highlighted {
            None => {
                if delta >= 0 {
                    0
                } else {
                    n - 1
                }
            }
            Some(i) => (i.cast_signed() + delta)
                .rem_euclid(n.cast_signed())
                .cast_unsigned(),
        };
        Self::set_highlight(this, Some(next));
    }

    /// Sets the highlighted index. Scroll-into-view is index-based only
    /// (`VirtualScroll::scroll_to` when the target isn't materialized) — this
    /// widget sits above `VirtualScroll`, so it cannot request a descendant
    /// row's minimal reveal the way `RowClickable` does elsewhere in
    /// `collection`.
    pub(crate) fn set_highlight(this: &mut WidgetMut<'_, Self>, index: Option<usize>) {
        if this.widget.highlighted == index {
            return;
        }
        let prev = this.widget.highlighted;
        this.widget.highlighted = index;
        let active_start = this.widget.active_start;
        let materialized_count = {
            let vs = this.ctx.get_mut(&mut this.widget.child);
            vs.widget.children_ids().len()
        };
        if let Some(i) = index
            && !(active_start..active_start + materialized_count).contains(&i)
        {
            let mut vs = this.ctx.get_mut(&mut this.widget.child);
            VirtualScrollWidget::scroll_to(&mut vs, i);
        }
        let mut new_highlighted_row_id = None;
        {
            let mut vs = this.ctx.get_mut(&mut this.widget.child);
            if let Some(i) = prev
                && let Some(k) = i.checked_sub(active_start)
                && k < vs.widget.children_ids().len()
            {
                let mut row = VirtualScrollWidget::child_mut(&mut vs, i);
                OverlayListItem::set_highlighted(&mut row.downcast(), false);
            }
            if let Some(i) = index
                && (active_start..active_start + materialized_count).contains(&i)
                && let Some(k) = i.checked_sub(active_start)
                && k < vs.widget.children_ids().len()
            {
                let mut row = VirtualScrollWidget::child_mut(&mut vs, i);
                let row_id = row.ctx.widget_id();
                let mut item = row.downcast::<OverlayListItem>();
                OverlayListItem::set_highlighted(&mut item, true);
                new_highlighted_row_id = Some(row_id);
            }
        }
        this.widget.highlighted_row_id = new_highlighted_row_id;
        this.ctx.request_accessibility_update();
    }
}

impl Widget for CollectionListWidget {
    type Action = NoAction;

    fn accepts_focus(&self) -> bool {
        self.accepts_focus
    }

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
        match &key.key {
            Key::Named(NamedKey::ArrowDown) => {
                ctx.mutate_self_later(|mut this| Self::move_highlight(&mut this.downcast(), 1));
                ctx.set_handled();
            }
            Key::Named(NamedKey::ArrowUp) => {
                ctx.mutate_self_later(|mut this| Self::move_highlight(&mut this.downcast(), -1));
                ctx.set_handled();
            }
            Key::Named(NamedKey::Home) if self.item_count > 0 => {
                ctx.mutate_self_later(|mut this| {
                    Self::set_highlight(&mut this.downcast(), Some(0));
                });
                ctx.set_handled();
            }
            Key::Named(NamedKey::End) if self.item_count > 0 => {
                let last = self.item_count - 1;
                ctx.mutate_self_later(move |mut this| {
                    Self::set_highlight(&mut this.downcast(), Some(last));
                });
                ctx.set_handled();
            }
            // Selects the highlighted row (if any) by synthesizing the same
            // action a pointer click would submit — see
            // `OverlayListItem::activate`. Deliberately does *not*
            // `set_handled()`: this widget owns no concept of "close the
            // popup" or "return focus to a trigger" (consumer-specific —
            // autocomplete's `SuggestionList` and the future dropdown menu
            // chrome each want their own version of that), so Enter is left
            // to keep bubbling to whatever chrome wraps this list, exactly
            // like Escape/Tab (neither handled here at all) already do.
            Key::Named(NamedKey::Enter) => {
                ctx.mutate_self_later(|mut this| {
                    let mut this: WidgetMut<'_, Self> = this.downcast();
                    let Some(i) = this.widget.highlighted else {
                        return;
                    };
                    let active_start = this.widget.active_start;
                    let materialized_count = {
                        let vs = this.ctx.get_mut(&mut this.widget.child);
                        vs.widget.children_ids().len()
                    };
                    if (active_start..active_start + materialized_count).contains(&i) {
                        let mut vs = this.ctx.get_mut(&mut this.widget.child);
                        let mut row = VirtualScrollWidget::child_mut(&mut vs, i);
                        OverlayListItem::activate(&mut row.downcast());
                    }
                });
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    /// `VirtualScroll` (masonry) can't compute a real content-based size —
    /// it would have to materialize every item to measure it, defeating
    /// virtualization — so its own `measure` unconditionally returns a fixed
    /// 100px for `MaxContent`/`MinContent` regardless of `item_count` (see
    /// its doc comment in the pinned masonry source). Forwarding straight to
    /// it here made this widget's own natural size just as wrong, which
    /// meant callers that ask "how tall do you want to be" to auto-size a
    /// container to content (`SuggestionList`/`MenuContent`'s `measure`,
    /// clamped by `MAX_LIST_HEIGHT`) got a constant answer no matter how few
    /// items remained — a real, live-reproduced bug (the popover stayed
    /// oversized after typing narrowed the results down to one match).
    /// Fixed by estimating the vertical content-based size ourselves from
    /// `item_count * item_extent` instead of asking the child — `FitContent`
    /// (a real, already-placed size, not a content query) and the cross
    /// axis still forward to the child exactly as before, since neither of
    /// those is affected by this limitation.
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        if axis == Axis::Vertical && matches!(len_req, LenReq::MaxContent | LenReq::MinContent) {
            let item_count = f64::from(u32::try_from(self.item_count).unwrap_or(u32::MAX));
            return Length::px(item_count * self.item_extent);
        }
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(
            &mut self.child,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fixed(size), size.into());
        ctx.run_layout(&mut self.child, child_size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _p: &mut Painter<'_>) {
    }

    fn accessibility_role(&self) -> Role {
        self.container_role
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut masonry::accesskit::Node,
    ) {
        if let Some(id) = self.highlighted_row_id {
            node.set_active_descendant(id.into());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use masonry::accesskit::Role;
    use masonry::core::NewWidget;
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{TextEvent, WidgetId, WidgetMut};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};

    use super::CollectionListWidget;
    use crate::Theme;
    use crate::collection::item_row::OverlayListItem;

    #[test]
    fn constructing_from_a_prebuilt_virtual_scroll_does_not_panic() {
        let vs = NewWidget::new(VirtualScrollWidget::new(0, 100));
        let widget = CollectionListWidget::new(
            vs,
            100,
            Role::ListBox,
            true,
            f64::from(Theme::default().density.row_height),
        );
        let harness = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (200, 400),
        );
        assert!(!harness.root_widget().as_dyn().children_ids().is_empty());
    }

    /// `accepts_focus()` must reflect exactly the constructor parameter, not
    /// a hardcoded value — this is the field the `dropdown_button`/`autocomplete`
    /// keyboard-model split depends on (see the field's doc comment).
    #[test]
    fn accepts_focus_reflects_the_constructor_parameter() {
        let vs = NewWidget::new(VirtualScrollWidget::new(0, 10));
        let widget = CollectionListWidget::new(
            vs,
            10,
            Role::ListBox,
            true,
            f64::from(Theme::default().density.row_height),
        );
        assert!(
            masonry::core::Widget::accepts_focus(&widget),
            "constructed with accepts_focus: true"
        );

        let vs = NewWidget::new(VirtualScrollWidget::new(0, 10));
        let widget = CollectionListWidget::new(
            vs,
            10,
            Role::Menu,
            false,
            f64::from(Theme::default().density.row_height),
        );
        assert!(
            !masonry::core::Widget::accepts_focus(&widget),
            "constructed with accepts_focus: false"
        );
    }

    fn harness_with_count(count: usize) -> TestHarness<CollectionListWidget> {
        let vs = NewWidget::new(VirtualScrollWidget::new(0, count));
        let widget = CollectionListWidget::new(
            vs,
            count,
            Role::ListBox,
            true,
            f64::from(Theme::default().density.row_height),
        );
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (200, 400),
        );
        let root = h.root_widget().id();
        h.focus_on(Some(root));
        h
    }

    fn key_down(named: NamedKey) -> TextEvent {
        TextEvent::key_down(Key::Named(named))
    }

    /// Pumps the harness until `VirtualScroll` stops asking for row changes,
    /// materializing each requested row as a real `OverlayListItem` (not a
    /// bare `Label`) and recording its [`WidgetId`] keyed by row index.
    /// Mirrors `body.rs`'s `drive_to_fixpoint`/`drive_to_fixpoint_clickable`
    /// — `CollectionListWidget` itself never handles `Fetch` (that's
    /// `overlay_list_body`'s job), so a unit test at this level has to drive
    /// `VirtualScroll` directly, the same way `body.rs`'s tests do for
    /// `CollectionBodyWidget`.
    fn drive_to_fixpoint(
        harness: &mut TestHarness<CollectionListWidget>,
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
            harness.edit_root_widget(|mut w| {
                let mut vs = CollectionListWidget::virtual_scroll_mut(&mut w);
                VirtualScrollWidget::will_handle_action(&mut vs, &action);
                for idx in action.old_active().clone() {
                    if !action.target().contains(&idx) {
                        VirtualScrollWidget::remove_child(&mut vs, idx);
                        rows.remove(&idx);
                    }
                }
                for idx in action.target().clone() {
                    if !action.old_active().contains(&idx) {
                        let row = NewWidget::new(OverlayListItem::new(
                            format!("row {idx}").into(),
                            false,
                            &Theme::default(),
                            Role::ListBoxOption,
                            None,
                        ))
                        .erased();
                        let row_id = row.id();
                        VirtualScrollWidget::add_child(&mut vs, idx, row);
                        rows.insert(idx, row_id);
                    }
                }
            });
        }
    }

    /// Builds a list of materialized `OverlayListItem` rows (rather than
    /// bare `Label`s), drives it to a fixpoint, and syncs `active_start` —
    /// needed because `set_highlight` specifically calls
    /// `OverlayListItem::set_highlighted` on a materialized child, so tests
    /// exercising that path need the real row widget type in place, mirroring
    /// why `body.rs`'s `harness_with_clickable_rows` uses `RowClickable`
    /// rather than `Label`.
    fn harness_with_materialized_rows(
        count: usize,
    ) -> (TestHarness<CollectionListWidget>, HashMap<usize, WidgetId>) {
        let vs = NewWidget::new(VirtualScrollWidget::new(0, count));
        let widget = CollectionListWidget::new(
            vs,
            count,
            Role::ListBox,
            true,
            f64::from(Theme::default().density.row_height),
        );
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (200, 400),
        );
        let root = h.root_widget().id();
        h.focus_on(Some(root));
        let mut rows = HashMap::new();
        drive_to_fixpoint(&mut h, &mut rows);
        h.edit_root_widget(|mut w| CollectionListWidget::set_active_start(&mut w, 0));
        (h, rows)
    }

    #[test]
    fn arrow_down_moves_highlight_forward_from_none() {
        let mut h = harness_with_count(3);
        let handled = h.process_text_event(key_down(NamedKey::ArrowDown));
        assert!(handled.is_handled());
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(highlighted, Some(0));
    }

    #[test]
    fn arrow_down_wraps_from_last_to_first() {
        let mut h = harness_with_count(3);
        h.process_text_event(key_down(NamedKey::ArrowDown)); // None -> 0
        h.process_text_event(key_down(NamedKey::ArrowDown)); // 0 -> 1
        h.process_text_event(key_down(NamedKey::ArrowDown)); // 1 -> 2
        let handled = h.process_text_event(key_down(NamedKey::ArrowDown)); // 2 -> wraps to 0
        assert!(handled.is_handled());
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(highlighted, Some(0));
    }

    #[test]
    fn arrow_up_from_none_starts_at_the_last_item() {
        let mut h = harness_with_count(3);
        let handled = h.process_text_event(key_down(NamedKey::ArrowUp));
        assert!(handled.is_handled());
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(highlighted, Some(2));
    }

    #[test]
    fn home_and_end_jump_to_the_first_and_last_item() {
        let mut h = harness_with_count(5);
        h.process_text_event(key_down(NamedKey::End));
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(highlighted, Some(4));

        h.process_text_event(key_down(NamedKey::Home));
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(highlighted, Some(0));
    }

    #[test]
    fn set_item_count_to_zero_clears_the_highlight() {
        let mut h = harness_with_count(3);
        h.edit_root_widget(|mut w| {
            CollectionListWidget::set_highlight(&mut w, Some(1));
            CollectionListWidget::set_item_count(&mut w, 0);
        });
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(highlighted, None);
    }

    #[test]
    fn set_item_count_shrinking_clamps_the_highlight_to_the_new_last_index() {
        let mut h = harness_with_count(5);
        h.edit_root_widget(|mut w| {
            CollectionListWidget::set_highlight(&mut w, Some(4));
            CollectionListWidget::set_item_count(&mut w, 2);
        });
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(
            highlighted,
            Some(1),
            "highlight clamps to the new last valid index (2 items -> index 1)"
        );
    }

    /// The bug this task fixes: `set_item_count`'s shrink-clamp used to write
    /// `this.widget.highlighted` directly, leaving `highlighted_row_id` (what
    /// `accessibility()` reads for `aria-activedescendant`) and the
    /// materialized row's own visual highlight-ring flag both pointed at the
    /// *old* row. Reachable via autocomplete: arrow-highlight a suggestion,
    /// then type more characters so the filtered list shrinks. Asserts the
    /// cached id and the visual ring both follow the clamp, not just the
    /// index (the pre-existing shrink tests above only ever checked the
    /// index).
    #[test]
    fn set_item_count_shrinking_updates_the_highlighted_row_id_and_visual_ring() {
        let (mut h, rows) = harness_with_materialized_rows(5);
        assert_eq!(
            rows.len(),
            5,
            "fixture must materialize every row for this test"
        );

        h.edit_root_widget(|mut w| CollectionListWidget::set_highlight(&mut w, Some(4)));
        h.redraw();
        let highlighted_row_id = h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| {
            w.widget.highlighted_row_id()
        });
        assert_eq!(highlighted_row_id, Some(rows[&4]));

        h.edit_root_widget(|mut w| CollectionListWidget::set_item_count(&mut w, 2));
        h.redraw();

        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(
            highlighted,
            Some(1),
            "index still clamps to the new last valid index (2 items -> index 1)"
        );

        let highlighted_row_id = h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| {
            w.widget.highlighted_row_id()
        });
        assert_eq!(
            highlighted_row_id,
            Some(rows[&1]),
            "highlighted_row_id must follow the clamped index to the row \
             that's actually highlighted now, not stay pointed at stale row 4"
        );

        assert_eq!(
            h.access_node(rows[&4]).expect("row exists").is_selected(),
            Some(false),
            "the old highlighted row's visual highlight ring must clear too"
        );
        assert_eq!(
            h.access_node(rows[&1]).expect("row exists").is_selected(),
            Some(true),
            "the row the highlight clamped to should carry the visual ring"
        );
    }

    /// Same bug, the count-0 branch: shrinking to empty must clear
    /// `highlighted_row_id` (not just `highlighted`) and the previously
    /// highlighted row's visual ring, or `accessibility()` would report a
    /// dangling `aria-activedescendant` for a row that may no longer even be
    /// materialized.
    #[test]
    fn set_item_count_to_zero_clears_the_highlighted_row_id_and_visual_ring() {
        let (mut h, rows) = harness_with_materialized_rows(3);
        h.edit_root_widget(|mut w| CollectionListWidget::set_highlight(&mut w, Some(1)));
        h.redraw();

        h.edit_root_widget(|mut w| CollectionListWidget::set_item_count(&mut w, 0));
        h.redraw();

        let highlighted_row_id = h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| {
            w.widget.highlighted_row_id()
        });
        assert_eq!(highlighted_row_id, None);

        assert_eq!(
            h.access_node(rows[&1]).expect("row exists").is_selected(),
            Some(false),
            "clearing the highlight on count 0 must clear the old row's \
             visual ring too"
        );
    }

    #[test]
    fn set_item_count_growing_leaves_an_in_range_highlight_untouched() {
        let mut h = harness_with_count(5);
        h.edit_root_widget(|mut w| {
            CollectionListWidget::set_highlight(&mut w, Some(1));
            CollectionListWidget::set_item_count(&mut w, 10);
        });
        let highlighted =
            h.edit_root_widget(|w: WidgetMut<'_, CollectionListWidget>| w.widget.highlighted());
        assert_eq!(highlighted, Some(1));
    }

    /// The previously-untested core of `set_highlight`: when the target
    /// index is inside the materialized window, it must reach the real
    /// `OverlayListItem::set_highlighted` call on the corresponding child —
    /// not just flip `self.highlighted`. Every prior test in this file built
    /// its harness via `harness_with_count`, which never drains `Fetch`, so
    /// `children_ids()` was always empty and this branch never actually ran.
    #[test]
    fn set_highlight_pushes_to_a_materialized_row_and_clears_the_previous_one() {
        let (mut h, rows) = harness_with_materialized_rows(20);
        assert!(
            rows.len() >= 2,
            "fixture must materialize at least two rows, got {:?}",
            rows.keys().copied().collect::<Vec<_>>()
        );
        let idx_a = *rows.keys().min().unwrap();
        let idx_b = *rows.keys().filter(|&&i| i != idx_a).min().unwrap();

        h.edit_root_widget(|mut w| CollectionListWidget::set_highlight(&mut w, Some(idx_a)));
        h.redraw();
        assert_eq!(
            h.access_node(rows[&idx_a])
                .expect("row exists")
                .is_selected(),
            Some(true),
            "set_highlight should push highlighted=true onto the materialized \
             row's OverlayListItem (via node.set_selected)"
        );

        h.edit_root_widget(|mut w| CollectionListWidget::set_highlight(&mut w, Some(idx_b)));
        h.redraw();
        assert_eq!(
            h.access_node(rows[&idx_a])
                .expect("row exists")
                .is_selected(),
            Some(false),
            "moving the highlight away should clear the previous row's \
             highlighted state"
        );
        assert_eq!(
            h.access_node(rows[&idx_b])
                .expect("row exists")
                .is_selected(),
            Some(true),
            "moving the highlight to a new materialized row should set its \
             highlighted state"
        );
    }

    /// The other previously-untested branch: when the target index is
    /// outside the materialized window, `set_highlight` must call
    /// `VirtualScroll::scroll_to` to re-anchor toward it. Asserted the same
    /// way `body_view.rs`'s `scroll_to_moves_the_materialized_window_to_the_target`
    /// does — by driving the harness to a fixpoint again and confirming the
    /// materialized window actually moved to include the target, rather than
    /// racing the action queue's timing.
    #[test]
    fn set_highlight_scrolls_to_the_target_when_it_is_not_materialized() {
        const ITEM_COUNT: usize = 1000;
        const TARGET: usize = 900;

        let (mut h, mut rows) = harness_with_materialized_rows(ITEM_COUNT);
        assert!(
            !rows.contains_key(&TARGET),
            "row {TARGET} should be outside the initial materialized window, got {:?}",
            rows.keys().copied().collect::<Vec<_>>()
        );

        h.edit_root_widget(|mut w| CollectionListWidget::set_highlight(&mut w, Some(TARGET)));

        // set_highlight only calls VirtualScroll::scroll_to (a re-anchor) —
        // it doesn't materialize the row itself. Drive back to a fixpoint so
        // the resulting Fetch actually runs and TARGET becomes materialized.
        drive_to_fixpoint(&mut h, &mut rows);

        assert!(
            rows.contains_key(&TARGET),
            "set_highlight on unmaterialized index {TARGET} should call \
             VirtualScroll::scroll_to and re-anchor the materialized window \
             to include it, but row {TARGET} never got materialized"
        );
    }
}
