//! Internal wrapper around the list's `virtual_scroll` body view.
//!
//! [`ListBodyView`] combines two pieces of behavior in a single pass over
//! the `virtual_scroll` child, mirroring
//! [`data_grid::scroll::ScrollToView`](super::super::data_grid::scroll::ScrollToView)
//! plus a lazy-loading hook:
//!
//! - **Scroll-to-index**: when the host's [`ScrollState`] snapshot's
//!   generation changes, clamp the requested index to `0..item_count` and
//!   re-anchor masonry's `VirtualScroll` so that row's top aligns with the
//!   viewport top.
//! - **Lazy loading**: peeks each `VirtualScrollAction` (via
//!   `MessageCtx::maybe_take_message`, which restores the message if `f`
//!   returns `false`) to see the newly active range. When the active range's
//!   end comes within [`List::load_threshold`](super::view::List::load_threshold)
//!   of `item_count`, the host's `on_load_more` callback is invoked. The
//!   message is left untouched so `virtual_scroll`'s own handling proceeds
//!   normally.
//!
//! [`ScrollState`]: crate::components::data_grid::ScrollState

use std::sync::Arc;

use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::accesskit::Role;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};
use xilem::{Pod, ViewCtx};

use crate::components::data_grid::ScrollState;

/// Boxed lazy-load callback (`Fn(&mut State)`), invoked when the
/// virtualized range nears the end of the data.
pub(super) type LoadMore<State> = Arc<dyn Fn(&mut State) + Send + Sync>;

/// `virtual_scroll` range bound: item count (`u64`) → `i64`. Saturates to
/// `i64::MAX` (matches `data_grid::view::scroll_range_end`).
pub(super) fn scroll_range_end(item_count: u64) -> i64 {
    i64::try_from(item_count).unwrap_or(i64::MAX)
}

/// `virtual_scroll` callback index (`i64`) → slice index (`usize`).
/// Saturates so a stray negative/oversized index reads as past-the-end.
pub(super) fn scroll_idx_to_slice(idx: i64) -> usize {
    usize::try_from(idx).unwrap_or(usize::MAX)
}

/// Slice position (`usize`) → item id (`u64`), used by the
/// position-fallback item id ([`ItemIdSource::Position`]). Saturates to
/// `u64::MAX`.
pub(super) fn position_fallback_id(idx: usize) -> u64 {
    u64::try_from(idx).unwrap_or(u64::MAX)
}

/// Clamps a requested scroll index to `0..item_count`, converted to the
/// `i64` anchor domain masonry's `VirtualScroll` uses. `None` when the list
/// is empty — there is no item to anchor to.
pub(super) fn clamp_scroll_index(index: u64, item_count: u64) -> Option<i64> {
    if item_count == 0 {
        return None;
    }
    let clamped = index.min(item_count - 1);
    Some(i64::try_from(clamped).unwrap_or(i64::MAX))
}

/// How the body derives an item's stable id: either the host's projector,
/// or a fallback to the item's slice position when none was supplied.
pub(super) enum ItemIdSource<Item> {
    /// Host-supplied id projector.
    Explicit(Arc<dyn Fn(&Item) -> u64 + Send + Sync>),
    /// No projector: use the item's current slice position as its id.
    Position,
}

// Hand-written so the bound is on the `Arc` (always `Clone`), not on
// `Item` — a derived `Clone` would wrongly require `Item: Clone`.
impl<Item> Clone for ItemIdSource<Item> {
    fn clone(&self) -> Self {
        match self {
            Self::Explicit(f) => Self::Explicit(Arc::clone(f)),
            Self::Position => Self::Position,
        }
    }
}

impl<Item> ItemIdSource<Item> {
    /// The stable id of `item`, which sits at slice position `pos`.
    pub(super) fn id_of(&self, pos: usize, item: &Item) -> u64 {
        match self {
            Self::Explicit(f) => f(item),
            Self::Position => position_fallback_id(pos),
        }
    }
}

/// Single-child wrapper around masonry's [`VirtualScrollWidget`] that adds
/// Up/Down arrow-key navigation between materialized rows.
///
/// `children_ids()` of the wrapped `VirtualScroll` is a dense,
/// index-sorted list of materialized rows (see [`ListBodyView`]). Pressing
/// Up/Down while a row has focus moves focus to the adjacent entry in that
/// list via [`EventCtx::set_focus`]. If focus is on something else (e.g.
/// the search box) or the focused row is at the edge of the materialized
/// window, the key is left unhandled — Tab navigation and click focus are
/// unaffected.
pub(super) struct ListBodyWidget {
    child: WidgetPod<VirtualScrollWidget>,
}

impl ListBodyWidget {
    pub(super) fn new(child: NewWidget<VirtualScrollWidget>) -> Self {
        Self {
            child: child.to_pod(),
        }
    }
}

impl ListBodyWidget {
    /// Returns a mutable reference to the wrapped `VirtualScroll`.
    pub(super) fn virtual_scroll_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, VirtualScrollWidget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for ListBodyWidget {
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
        let delta: isize = match (key.state, &key.key) {
            (KeyState::Down, Key::Named(NamedKey::ArrowDown)) => 1,
            (KeyState::Down, Key::Named(NamedKey::ArrowUp)) => -1,
            _ => return,
        };
        let Some(focused) = ctx.focus_target_id() else {
            return;
        };
        let (virtual_scroll, _) = ctx.get_raw(&mut self.child);
        let row_ids = virtual_scroll.children_ids();
        let Some(pos) = row_ids.iter().position(|&id| id == focused) else {
            return;
        };
        let Some(&target) = pos
            .checked_add_signed(delta)
            .and_then(|i| row_ids.get(i))
        else {
            return;
        };
        ctx.set_focus(target);
        ctx.set_handled();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let auto_length = len_req.into();
        ctx.compute_length(
            &mut self.child,
            auto_length,
            LayoutSize::maybe(axis.cross(), cross_length),
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fixed(size), size.into());
        ctx.run_layout(&mut self.child, child_size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
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
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Wraps the body's `virtual_scroll` view to apply pending [`ScrollState`]
/// requests and to peek lazy-load triggers. See the module docs.
///
/// The element type is [`ListBodyWidget`] (a thin wrapper around the
/// child's `Pod<VirtualScroll>` that adds arrow-key navigation); `rebuild`
/// additionally re-anchors on a new scroll generation, and `message`
/// additionally peeks `VirtualScrollAction`s for the lazy-load threshold.
pub(super) struct ListBodyView<V, State> {
    pub(super) child: V,
    pub(super) scroll: ScrollState,
    pub(super) item_count: u64,
    pub(super) on_load_more: Option<LoadMore<State>>,
    pub(super) load_threshold: u64,
}

/// View state for [`ListBodyView`]: the child's state plus the last applied
/// scroll-request generation. Tracked in view state (not via
/// `prev`-comparison) so a request made *before* the view first builds is
/// still applied at the first rebuild.
pub(super) struct ListBodyViewState<S> {
    child_state: S,
    applied_generation: u64,
}

impl<V, State> ViewMarker for ListBodyView<V, State> {}

impl<State, V> View<State, (), ViewCtx> for ListBodyView<V, State>
where
    State: 'static,
    V: View<State, (), ViewCtx, Element = Pod<VirtualScrollWidget>>,
{
    type Element = Pod<ListBodyWidget>;
    type ViewState = ListBodyViewState<V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_element, child_state) = self.child.build(ctx, app_state);
        // The underlying widget is built anchored at row 0 (the xilem view
        // hardcodes `VirtualScroll::new(0)`), and re-anchoring needs a
        // `WidgetMut` that only exists once the widget is in the tree.
        // Recording generation 0 here means a request pending at build time
        // (snapshot generation != 0) is applied at the first rebuild.
        (
            Pod::new(ListBodyWidget::new(child_element.new_widget)),
            ListBodyViewState {
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
                let mut virtual_scroll = ListBodyWidget::virtual_scroll_mut(&mut element);
                VirtualScrollWidget::overwrite_anchor(&mut virtual_scroll, idx);
            }
        }
        let virtual_scroll = ListBodyWidget::virtual_scroll_mut(&mut element);
        self.child.rebuild(
            &prev.child,
            &mut view_state.child_state,
            ctx,
            virtual_scroll,
            app_state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let virtual_scroll = ListBodyWidget::virtual_scroll_mut(&mut element);
        self.child
            .teardown(&mut view_state.child_state, ctx, virtual_scroll);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        // `VirtualScrollAction` is an action message addressed to this
        // view's own widget (not routed to a loaded row), so it only
        // arrives with an empty remaining path. Routed messages (e.g. a
        // row's click) must not be probed with `maybe_take_message` — doing
        // so trips its "message has reached its target" debug assertion.
        if message.remaining_path().is_empty()
            && let Some(on_load_more) = self.on_load_more.as_ref()
        {
            let item_count = scroll_range_end(self.item_count);
            let threshold = i64::try_from(self.load_threshold).unwrap_or(i64::MAX);
            message.maybe_take_message::<VirtualScrollAction>(|action| {
                if item_count - action.target.end <= threshold {
                    on_load_more(app_state);
                }
                false
            });
        }
        let virtual_scroll = ListBodyWidget::virtual_scroll_mut(&mut element);
        self.child
            .message(&mut view_state.child_state, message, virtual_scroll, app_state)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use masonry::core::keyboard::{Code, Key, KeyState, NamedKey};
    use masonry::core::{KeyboardEvent, Modifiers, NewWidget, TextEvent, Widget, WidgetId};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{ListBodyWidget, VirtualScrollAction, VirtualScrollWidget};
    use crate::Theme;
    use crate::components::data_grid::row_click::RowClickable;

    const ROW_COUNT: i64 = 50;

    fn arrow_key(named: NamedKey) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(named),
            code: Code::Unidentified,
            modifiers: Modifiers::empty(),
            ..KeyboardEvent::default()
        })
    }

    /// Builds a [`ListBodyWidget`]-wrapped [`VirtualScrollWidget`] and drives
    /// it to a fixpoint, recording each materialized row's `WidgetId` keyed
    /// by its `i64` index. Mirrors masonry's own `drive_to_fixpoint` test
    /// helper for `VirtualScroll`, but operates through the
    /// `ListBodyWidget` wrapper and tracks row ids as they're added.
    fn drive_to_fixpoint(
        harness: &mut TestHarness<ListBodyWidget>,
        virtual_scroll_id: WidgetId,
        row_ids: &mut HashMap<i64, WidgetId>,
    ) {
        let mut iteration = 0;
        loop {
            iteration += 1;
            assert!(iteration <= 1000, "took too long to reach fixpoint");
            let Some((action, id)) = harness.pop_action::<VirtualScrollAction>() else {
                break;
            };
            assert_eq!(id, virtual_scroll_id, "only the VirtualScroll emits this action");
            harness.edit_root_widget(|mut root| {
                let mut scroll = ListBodyWidget::virtual_scroll_mut(&mut root);
                VirtualScrollWidget::will_handle_action(&mut scroll, &action);
                for idx in action.old_active.clone() {
                    if !action.target.contains(&idx) {
                        VirtualScrollWidget::remove_child(&mut scroll, idx);
                        row_ids.remove(&idx);
                    }
                }
                for idx in action.target.clone() {
                    if !action.old_active.contains(&idx) {
                        let row = NewWidget::new(RowClickable::new(
                            NewWidget::new(Label::new(format!("row {idx}"))).erased(),
                            false,
                            &Theme::default(),
                        ))
                        .erased();
                        row_ids.insert(idx, row.id());
                        VirtualScrollWidget::add_child(&mut scroll, idx, row);
                    }
                }
            });
        }
    }

    /// Builds a harness with `ROW_COUNT` rows anchored at index 0, returning
    /// the harness, the wrapped `VirtualScroll`'s id, and a map of
    /// materialized row indices to their `WidgetId`s.
    fn harness_with_rows() -> (TestHarness<ListBodyWidget>, WidgetId, HashMap<i64, WidgetId>) {
        let virtual_scroll = VirtualScrollWidget::new(0)
            .with_valid_range(0..ROW_COUNT)
            .prepare();
        let virtual_scroll_id = virtual_scroll.id();
        let widget = NewWidget::new(ListBodyWidget::new(virtual_scroll));
        let mut harness = TestHarness::create_with_size(default_property_set(), widget, (200, 200));
        let mut row_ids = HashMap::new();
        drive_to_fixpoint(&mut harness, virtual_scroll_id, &mut row_ids);
        (harness, virtual_scroll_id, row_ids)
    }

    #[test]
    fn arrow_down_moves_focus_to_next_row() {
        let (mut harness, _vs_id, row_ids) = harness_with_rows();
        let row0 = row_ids[&0];
        let row1 = row_ids[&1];

        harness.focus_on(Some(row0));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));

        assert_eq!(harness.focused_widget_id(), Some(row1));
        assert!(handled.is_handled());
    }

    #[test]
    fn arrow_up_moves_focus_to_previous_row() {
        let (mut harness, _vs_id, row_ids) = harness_with_rows();
        let row1 = row_ids[&1];
        let row0 = row_ids[&0];

        harness.focus_on(Some(row1));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowUp));

        assert_eq!(harness.focused_widget_id(), Some(row0));
        assert!(handled.is_handled());
    }

    #[test]
    fn arrow_up_at_first_row_is_a_no_op() {
        let (mut harness, _vs_id, row_ids) = harness_with_rows();
        let row0 = row_ids[&0];

        harness.focus_on(Some(row0));
        harness.process_text_event(arrow_key(NamedKey::ArrowUp));

        assert_eq!(harness.focused_widget_id(), Some(row0));
    }

    #[test]
    fn arrow_down_at_last_materialized_row_is_a_no_op() {
        let (mut harness, _vs_id, row_ids) = harness_with_rows();
        let last_idx = *row_ids.keys().max().unwrap();
        let last_row = row_ids[&last_idx];

        harness.focus_on(Some(last_row));
        harness.process_text_event(arrow_key(NamedKey::ArrowDown));

        assert_eq!(harness.focused_widget_id(), Some(last_row));
    }

    #[test]
    fn arrow_keys_on_non_row_focus_are_unhandled() {
        let (mut harness, _vs_id, _row_ids) = harness_with_rows();

        harness.focus_on(None);
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));

        assert!(!handled.is_handled());
        assert_eq!(harness.focused_widget_id(), None);
    }
}
