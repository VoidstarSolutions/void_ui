//! Single-child wrapper widget that catches Ctrl/Cmd+C and dumps a
//! pre-built `String` payload to the platform clipboard.
//!
//! This is an internal building block for [`super::data_grid`]. The
//! xilem view rebuilds the payload (a TSV projection of the current
//! selection) on every rebuild and pushes it via [`set_payload`]; the
//! widget then handles the keyboard shortcut end-to-end without
//! routing actions through xilem — there is no `CopyRequested` event
//! to plumb, and no `arboard` dependency, because masonry's
//! [`EventCtx::set_clipboard`] already speaks to the platform.
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
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};

use crate::collection::single_child;

/// Wrap an arbitrary child widget; intercept Ctrl/Cmd+C; on press,
/// write the cached payload to the clipboard.
pub struct CopyOnShortcut {
    child: WidgetPod<dyn Widget>,
    payload: Option<String>,
}

// --- MARK: BUILDERS
impl CopyOnShortcut {
    /// Creates a wrapper with no payload set. Calls to Ctrl/Cmd+C
    /// before a payload is pushed via [`set_payload`] are no-ops.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
            payload: None,
        }
    }

    /// Builder-style payload setter. Mostly useful at construction
    /// time; afterward, use the [`set_payload`] `WidgetMut` method.
    #[must_use]
    pub fn with_payload(mut self, payload: Option<String>) -> Self {
        self.payload = payload;
        self
    }
}

// --- MARK: WIDGETMUT
impl CopyOnShortcut {
    /// Pushes a new clipboard payload. The string is copied verbatim
    /// to the platform clipboard when the user presses Ctrl/Cmd+C
    /// with the wrapper focused. `None` disables the shortcut.
    ///
    /// No layout/paint side effects — the payload only matters at
    /// keyboard-event time.
    pub fn set_payload(this: &mut WidgetMut<'_, Self>, payload: Option<String>) {
        this.widget.payload = payload;
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
    type Action = NoAction;

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
        if let TextEvent::Keyboard(key_event) = event
            && Self::is_copy_shortcut(key_event)
            && let Some(payload) = &self.payload
        {
            ctx.set_clipboard(payload.clone());
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
