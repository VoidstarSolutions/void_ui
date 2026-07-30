//! Collapse-based reveal wrapper for a sidebar row's trailing action.
//!
//! [`RevealBox`] wraps a single child and hides it by **collapsing** it along
//! the row's main (horizontal) axis — reporting zero *width* in
//! measure/layout and marking it [stashed] — rather than clipping it to an
//! empty rect. The parent [`super::widget::ThemedSidebarItem`] owns the
//! hover/focus decision and drives this box via [`RevealBox::set_revealed`];
//! this widget holds no opinion of its own, it only obeys.
//!
//! Collapsing (rather than clipping) is deliberate: the design's "reclaim
//! space" decision means a hidden action must free its slot for the label to
//! reflow into, not just become invisible while still reserving room. It
//! also means a hidden action is automatically excluded from pointer
//! hit-testing and Tab focus order for free — masonry's own hit-test and
//! focus-traversal passes both skip stashed subtrees.
//!
//! Collapsing is deliberately **width-only**: the box always reports its
//! child's real height, revealed or not. The row this wraps
//! (`ThemedSidebarItem`'s `Content::Row`) is a horizontal `Flex::row()`, so
//! the row's own measured height is the max of its children's cross-axis
//! (vertical) extents. Reporting zero height while hidden would make that
//! max — and so the whole row's height, as seen by its parent list — swing
//! between the label's height and `max(label, action)` every time the action
//! reveals or hides, under a pointer that hasn't moved. That vertical jump
//! was reproduced live (not just in headless tests, which force a fixed
//! window size regardless of measured content and so never exposed it): it
//! shifts content under a stationary cursor, which trips hover on and off
//! and reads as "the gear won't stay revealed."
//!
//! [stashed]: masonry::doc::masonry_concepts#stashed

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

/// A wrapper that reveals or hides its single child by collapsing it to
/// zero width and stashing it. See the module docs.
pub(super) struct RevealBox {
    child: WidgetPod<dyn Widget>,
    /// Whether the child is currently shown. Driven by the parent row;
    /// hidden by default so a freshly-attached action starts out of view.
    revealed: bool,
}

impl RevealBox {
    /// Wrap `child`, hidden until revealed.
    pub(super) fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
            revealed: false,
        }
    }

    /// Show or hide the child. Requests layout on change; a no-op when
    /// unchanged, so callers may drive this unconditionally on every
    /// relevant hover/focus update without worrying about spurious
    /// layout passes.
    pub(super) fn set_revealed(this: &mut WidgetMut<'_, Self>, revealed: bool) {
        if this.widget.revealed != revealed {
            this.widget.revealed = revealed;
            this.ctx.request_layout();
        }
    }

    /// Mutable handle to the wrapped child — the hook the view uses to
    /// rebuild the action subtree through the reveal wrapper.
    pub(super) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for RevealBox {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        // Collapse only the main (horizontal) axis when hidden. The cross
        // (vertical) axis always reports the child's real extent so the
        // containing row's own height — determined by the max of its
        // children's cross-axis extents — stays constant whether the action
        // is shown or not. See the module docs for why: reporting zero on
        // both axes made the row's height swing on every reveal/hide.
        if !self.revealed && axis == Axis::Horizontal {
            return Length::ZERO;
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
        if !self.revealed {
            // Stash and stop: masonry debug-panics if `run_layout` is
            // called on a stashed child ("trying to compute layout of a
            // stashed widget"), so we must not touch it further this pass.
            ctx.set_stashed(&mut self.child, true);
            return;
        }
        ctx.set_stashed(&mut self.child, false);
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(size), size.into());
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
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::RevealBox;

    fn harness() -> TestHarness<RevealBox> {
        let widget = RevealBox::new(NewWidget::new(Label::new("action")));
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (120, 24))
    }

    #[test]
    fn hidden_by_default() {
        // Freshly constructed harness: child should be stashed since revealed = false.
        let mut h = harness();
        h.edit_root_widget(|wm| {
            assert!(
                wm.ctx.child_is_stashed(&wm.widget.child),
                "child should be stashed by default (revealed = false)"
            );
        });
    }

    #[test]
    fn revealing_unstashes_the_child() {
        // When set_revealed(true), the child should NOT be stashed.
        let mut h = harness();
        h.edit_root_widget(|mut wm| RevealBox::set_revealed(&mut wm, true));
        h.redraw();
        h.edit_root_widget(|wm| {
            assert!(
                !wm.ctx.child_is_stashed(&wm.widget.child),
                "child should NOT be stashed when revealed"
            );
        });
    }

    #[test]
    fn hiding_after_revealing_restashes_the_child() {
        // Reveal, then hide: child should be stashed again after hiding.
        let mut h = harness();
        h.edit_root_widget(|mut wm| RevealBox::set_revealed(&mut wm, true));
        h.redraw();
        h.edit_root_widget(|mut wm| RevealBox::set_revealed(&mut wm, false));
        h.redraw();
        h.edit_root_widget(|wm| {
            assert!(
                wm.ctx.child_is_stashed(&wm.widget.child),
                "child should be stashed after hiding"
            );
        });
    }

    #[test]
    fn child_is_reachable_for_rebuild() {
        // The view rebuilds the action subtree through this handle; make
        // sure it resolves to the wrapped child rather than panicking,
        // both hidden and revealed.
        let mut h = harness();
        h.edit_root_widget(|mut wm| {
            let expected = wm.widget.child.id();
            let child = RevealBox::child_mut(&mut wm);
            assert_eq!(child.ctx.widget_id(), expected);
        });
        h.edit_root_widget(|mut wm| RevealBox::set_revealed(&mut wm, true));
        h.edit_root_widget(|mut wm| {
            let expected = wm.widget.child.id();
            let child = RevealBox::child_mut(&mut wm);
            assert_eq!(child.ctx.widget_id(), expected);
        });
    }
}
