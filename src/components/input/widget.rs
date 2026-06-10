//! Masonry widget for the input frame.
//!
//! [`InputFrame`] is a transparent single-child container that hosts the
//! masonry `TextInput` and adds keyboard behavior the upstream widget does not
//! provide. Today that is Esc-to-clear (#39): when the focused field receives
//! Escape — which the child `TextArea` leaves unhandled, so it bubbles up to
//! us — the frame emits [`InputCleared`]. The view maps that to an empty-string
//! change so the host clears its own state; the widget never mutates the
//! field's contents itself.
//!
//! Layout, measurement, and chrome are delegated to the child; the frame paints
//! nothing of its own. Affixes (prefix/suffix) and the field chrome compose
//! *around* this frame at the view layer; the frame's job is keyboard behavior
//! the upstream editor lacks.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use tracing::{Span, trace_span};

/// Action emitted by [`InputFrame`] when the user presses Escape in the focused
/// field. The view translates it into an empty-string change; the widget never
/// mutates the field's contents itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputCleared;

/// Transparent single-child container hosting the themed masonry `TextInput`,
/// adding Esc-to-clear. See the module docs.
pub struct InputFrame {
    inner: WidgetPod<dyn Widget>,
}

impl InputFrame {
    /// Wrap the given child (the themed `TextInput`).
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            inner: child.erased().to_pod(),
        }
    }

    /// Mutable access to the hosted child, used by the view's rebuild to reach
    /// the inner `TextInput`.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}

impl Widget for InputFrame {
    type Action = InputCleared;

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key_event) = event else {
            return;
        };
        // Only act on the press, and only for Escape. Because the child
        // TextArea doesn't handle Escape, this fires exactly when the field is
        // focused (text events bubble up from the focused widget).
        if key_event.state.is_down() && key_event.key == Key::Named(NamedKey::Escape) {
            ctx.submit_action::<Self::Action>(InputCleared);
            ctx.set_handled();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.inner, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
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
        ChildrenIds::from_slice(&[self.inner.id()])
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("InputFrame", id = id.trace())
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{NewWidget, TextEvent, WidgetId};
    use masonry::testing::TestHarness;
    use masonry::widgets::{TextArea, TextInput};

    use super::{InputCleared, InputFrame};

    /// Builds a harness wrapping a bare `InputFrame` and returns it along with
    /// the id of the inner `TextArea`, the only focusable widget.
    fn harness() -> (TestHarness<InputFrame>, WidgetId) {
        let text_area = TextArea::new_editable("hello");
        let text_input = TextInput::from_text_area(NewWidget::new(text_area));
        let area_id = text_input.area_pod().id();
        let frame = InputFrame::new(NewWidget::new(text_input));
        (
            TestHarness::create(masonry::theme::default_property_set(), NewWidget::new(frame)),
            area_id,
        )
    }

    #[test]
    fn escape_in_focused_field_emits_input_cleared() {
        let (mut harness, area_id) = harness();

        harness.focus_on(Some(area_id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));

        let (action, _) = harness
            .pop_action::<InputCleared>()
            .expect("Escape on the focused field should emit InputCleared");
        assert_eq!(action, InputCleared);
    }

    #[test]
    fn escape_without_focus_emits_nothing() {
        let (mut harness, _area_id) = harness();

        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));

        assert!(harness.pop_action_erased().is_none());
    }

    #[test]
    fn other_keys_emit_nothing() {
        let (mut harness, area_id) = harness();

        harness.focus_on(Some(area_id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowLeft)));

        assert!(harness.pop_action_erased().is_none());
    }
}
