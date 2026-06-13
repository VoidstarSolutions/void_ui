//! Single-child wrapper widget that catches Ctrl/Cmd+C and emits a
//! [`CopyRequested`] action.
//!
//! This is an internal building block for [`super::data_grid`]. The widget
//! holds **no** payload: on Ctrl/Cmd+C it submits a [`CopyRequested`]
//! action, and the xilem view
//! ([`CopyOnShortcutView`](super::view)) computes the TSV projection of the
//! *current* selection on receipt and writes it to the platform clipboard
//! via `set_clipboard` (available on the `WidgetMut`'s `MutateCtx`).
//!
//! Doing the projection on the action — rather than pre-building it on every
//! rebuild — keeps the O(rows) selection scan off the per-frame path: it
//! runs only on an actual copy, not on every scroll/tick/state change while
//! a selection exists. (No `arboard` dependency: masonry's clipboard already
//! speaks to the platform.)
//!
//! `accepts_focus = true`. Focus is taken only on a pointer-down that
//! lands *directly* on this wrapper, never when the click hits a
//! focusable descendant (a filter text input, a clickable row) — those
//! must keep their own focus, and the copy shortcut still works because
//! the key event bubbles up to this wrapper from the focused descendant.
//! Other events pass through to the child; `measure` / `layout` /
//! `paint` delegate verbatim.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};

use crate::collection::single_child;

/// Action emitted by [`CopyOnShortcut`] on Ctrl/Cmd+C. Carries no data: the
/// xilem view computes the clipboard payload from the *current* app state on
/// receipt, so the O(rows) projection runs only on an actual copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyRequested;

/// Wrap an arbitrary child widget; intercept Ctrl/Cmd+C; on press, submit a
/// [`CopyRequested`] action for the view to service.
pub struct CopyOnShortcut {
    child: WidgetPod<dyn Widget>,
}

// --- MARK: BUILDERS
impl CopyOnShortcut {
    /// Wraps `child`. The wrapper holds no payload; the view computes one on
    /// demand when it receives the [`CopyRequested`] action.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }
}

// --- MARK: WIDGETMUT
impl CopyOnShortcut {
    /// Writes `contents` to the platform clipboard. Called by the xilem view
    /// when it services a [`CopyRequested`] action, having just computed the
    /// payload from current app state. `set_clipboard` is available here
    /// because a `WidgetMut`'s context is a `MutateCtx`.
    pub fn write_clipboard(this: &mut WidgetMut<'_, Self>, contents: String) {
        this.ctx.set_clipboard(contents);
    }

    /// Returns a mutable reference to the wrapped child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: KEY HANDLING
impl CopyOnShortcut {
    /// Returns whether the supplied key event is the platform's
    /// "action-modifier + C" combination (Cmd+C on macOS, Ctrl+C
    /// elsewhere). Matches the convention used by masonry's
    /// [`TextArea`](masonry::widgets::TextArea) — see
    /// `text_area.rs:551-558`.
    fn is_copy_shortcut(key_event: &masonry::core::keyboard::KeyboardEvent) -> bool {
        if key_event.state != KeyState::Down {
            return false;
        }
        let is_c =
            matches!(&key_event.key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("c"));
        let action_mod = if cfg!(target_os = "macos") {
            key_event.modifiers.meta()
        } else {
            key_event.modifiers.ctrl()
        };
        is_c && action_mod
    }
}

// --- MARK: IMPL WIDGET
impl Widget for CopyOnShortcut {
    type Action = CopyRequested;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Take focus on a pointer-down that lands *directly* on this
        // wrapper (empty grid chrome) so a later Ctrl/Cmd+C routes here.
        // Crucially, do NOT grab focus when the click lands on a
        // focusable descendant — a filter text input or a clickable row.
        // Those run their own `request_focus` first; since this widget
        // is their ancestor it would otherwise override them (event
        // bubbling reaches ancestors last), stealing focus and breaking
        // text entry. Copy still works in that case because the keyboard
        // event bubbles up to this wrapper from the focused descendant.
        if let PointerEvent::Down(..) = event
            && ctx.target() == ctx.widget_id()
        {
            ctx.request_focus();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        // Emit the request; the view computes the payload from current state
        // and writes the clipboard. We deliberately do not pre-build or cache
        // anything here — that is the whole point of the lazy path.
        if let TextEvent::Keyboard(key_event) = event
            && Self::is_copy_shortcut(key_event)
        {
            ctx.submit_action::<Self::Action>(CopyRequested);
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        single_child::register_children(ctx, &mut self.child);
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
        single_child::measure(ctx, &mut self.child, axis, len_req, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        single_child::layout(ctx, &mut self.child, size);
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
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        single_child::children_ids(&self.child)
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers};
    use masonry::core::{NewWidget, TextEvent};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{CopyOnShortcut, CopyRequested};

    /// Builds the platform copy combo (Cmd+C on macOS, Ctrl+C elsewhere).
    fn copy_event() -> TextEvent {
        let mut modifiers = Modifiers::empty();
        if cfg!(target_os = "macos") {
            modifiers |= Modifiers::META;
        } else {
            modifiers |= Modifiers::CONTROL;
        }
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Character("c".into()),
            modifiers,
            ..Default::default()
        })
    }

    /// A `c` keypress with no action modifier — must NOT trigger copy.
    fn plain_c_event() -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Character("c".into()),
            modifiers: Modifiers::empty(),
            ..Default::default()
        })
    }

    fn harness() -> TestHarness<CopyOnShortcut> {
        let widget = CopyOnShortcut::new(NewWidget::new(Label::new("body")));
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (100, 40))
    }

    #[test]
    fn cmd_or_ctrl_c_emits_copy_requested() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        let handled = harness.process_text_event(copy_event());
        assert!(handled.is_handled(), "copy shortcut should be consumed");
        assert!(
            harness.pop_action::<CopyRequested>().is_some(),
            "Ctrl/Cmd+C should emit a CopyRequested action",
        );
    }

    #[test]
    fn plain_c_does_not_emit_copy_requested() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        harness.process_text_event(plain_c_event());
        assert!(
            harness.pop_action::<CopyRequested>().is_none(),
            "a bare `c` (no action modifier) must not request a copy",
        );
    }

    #[test]
    fn write_clipboard_sets_the_platform_clipboard() {
        let mut harness = harness();
        assert_eq!(harness.clipboard_contents(), "");
        harness.edit_root_widget(|mut w| {
            CopyOnShortcut::write_clipboard(&mut w, "a\tb\nc\td".to_owned());
        });
        assert_eq!(harness.clipboard_contents(), "a\tb\nc\td");
    }
}
