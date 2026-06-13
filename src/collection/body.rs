//! Unified virtualized body: a masonry widget that adds Up/Down row
//! navigation over `VirtualScroll`, plus the xilem `View`
//! (`collection_body`) that drives scroll-to-anchor, lazy-load, and
//! central click routing. (The `View` lands in the next task.)

use masonry::accesskit::Role;
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};
use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;

use super::single_child;

/// Single-child wrapper around masonry's `VirtualScroll` adding Up/Down
/// arrow-key navigation between materialized rows.
pub(crate) struct CollectionBodyWidget {
    child: WidgetPod<VirtualScrollWidget>,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by collection_body in the next task")
)]
impl CollectionBodyWidget {
    pub(crate) fn new(child: NewWidget<VirtualScrollWidget>) -> Self {
        Self {
            child: child.to_pod(),
        }
    }

    pub(crate) fn virtual_scroll_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, VirtualScrollWidget> {
        this.ctx.get_mut(&mut this.widget.child)
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
        let Some(&target) = pos.checked_add_signed(delta).and_then(|i| row_ids.get(i)) else {
            return;
        };
        ctx.set_focus(target);
        ctx.set_handled();
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

    use super::CollectionBodyWidget;

    /// Builds a [`TextEvent`] for a `Down`-state press of the given named key.
    fn arrow_key(named: NamedKey) -> TextEvent {
        TextEvent::key_down(Key::Named(named))
    }

    /// Pumps the harness until `VirtualScroll` stops asking for row
    /// changes, materializing each requested row as a plain `Label` and
    /// recording its [`WidgetId`] keyed by row index.
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

    /// Builds a body of materialized `Label` rows and returns the harness,
    /// the inner `VirtualScroll`'s id, and a map from row index to row id.
    fn harness_with_rows() -> (
        TestHarness<CollectionBodyWidget>,
        WidgetId,
        HashMap<i64, WidgetId>,
    ) {
        let scroll = NewWidget::new(VirtualScroll::new(0).with_valid_range(0..100));
        let body = NewWidget::new(CollectionBodyWidget::new(scroll));
        let mut harness = TestHarness::create_with_size(default_property_set(), body, (200, 400));
        let scroll_id = harness
            .edit_root_widget(|mut body| CollectionBodyWidget::virtual_scroll_mut(&mut body).id());
        let mut rows = HashMap::new();
        drive_to_fixpoint(&mut harness, &mut rows);
        (harness, scroll_id, rows)
    }

    #[test]
    fn arrow_down_moves_focus_to_next_row() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        let first = ids[&0];
        let second = ids[&1];
        harness.focus_on(Some(first));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(second));
    }

    #[test]
    fn arrow_up_moves_focus_to_previous_row() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        let first = ids[&0];
        let second = ids[&1];
        harness.focus_on(Some(second));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowUp));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(first));
    }

    #[test]
    fn arrow_up_at_first_row_is_a_no_op() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        let first = ids[&0];
        harness.focus_on(Some(first));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowUp));
        assert!(!handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(first));
    }

    #[test]
    fn arrow_down_at_last_materialized_row_is_a_no_op() {
        let (mut harness, _scroll, ids) = harness_with_rows();
        let last_idx = ids.keys().copied().max().expect("at least one row");
        let last = ids[&last_idx];
        harness.focus_on(Some(last));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));
        assert!(!handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(last));
    }

    #[test]
    fn arrow_keys_on_non_row_focus_are_unhandled() {
        let (mut harness, scroll, _rows) = harness_with_rows();
        // Focus the VirtualScroll itself (not one of its row children).
        harness.focus_on(Some(scroll));
        let handled = harness.process_text_event(arrow_key(NamedKey::ArrowDown));
        assert!(!handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(scroll));
    }
}
