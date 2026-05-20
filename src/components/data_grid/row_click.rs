//! Single-child wrapper widget that detects primary-button clicks
//! with modifier state and emits a [`RowClickAction`].
//!
//! Modeled on [`super::copy_shortcut::CopyOnShortcut`] but for pointer
//! events instead of keyboard events. The widget itself stays dumb —
//! it just reports "primary click happened at these modifiers" — the
//! grid's xilem view layer translates that into the right
//! [`SelectionState`](super::selection::SelectionState) update for the
//! affected row.
//!
//! `accepts_focus = true` so subsequent Ctrl/Cmd+C on the parent
//! [`CopyOnShortcut`](super::copy_shortcut::CopyOnShortcut) wrapper
//! has a focused descendant inside the grid.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

/// Action emitted by [`RowClickable`] on primary-button release. The
/// receiver inspects the modifiers to decide whether this is a plain
/// click (`replace`), a multi-select toggle (`action_mod`), or a
/// shift-extend (`shift`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowClickAction {
    /// Shift modifier was held.
    pub shift: bool,
    /// Platform "action modifier" was held — Cmd on macOS, Ctrl
    /// elsewhere. Matches masonry's `TextArea` convention.
    pub action_mod: bool,
}

/// Single-child wrapper that emits a [`RowClickAction`] on
/// primary-button release inside its bounds.
pub struct RowClickable {
    child: WidgetPod<dyn Widget>,
}

// --- MARK: BUILDERS
impl RowClickable {
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }
}

// --- MARK: WIDGETMUT
impl RowClickable {
    /// Returns a mutable reference to the wrapped child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: IMPL WIDGET
impl Widget for RowClickable {
    type Action = RowClickAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { button, .. }) => {
                if matches!(button, Some(PointerButton::Primary)) {
                    ctx.request_focus();
                    ctx.capture_pointer();
                }
            }
            PointerEvent::Up(PointerButtonEvent { button, state, .. })
                if matches!(button, Some(PointerButton::Primary))
                    && ctx.is_active()
                    && ctx.is_hovered() =>
            {
                let action_mod = if cfg!(target_os = "macos") {
                    state.modifiers.meta()
                } else {
                    state.modifiers.ctrl()
                };
                ctx.submit_action::<Self::Action>(RowClickAction {
                    shift: state.modifiers.shift(),
                    action_mod,
                });
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
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
        Role::ListItem
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

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
}

// --- MARK: XILEM VIEW WRAPPER

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Wraps a child view in [`RowClickable`] and routes pointer
/// release + modifiers through the supplied `on_click` callback.
///
/// `on_click` runs synchronously against the host's app state during
/// xilem's message-handling pass. Use it to apply the right
/// [`SelectionState`](super::selection::SelectionState) op based on
/// the [`RowClickAction`] modifier flags.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ClickableRow<V, State, F> {
    child: V,
    on_click: F,
    phantom: PhantomData<fn() -> State>,
}

/// Constructor for [`ClickableRow`].
pub fn clickable_row<V, State, F>(child: V, on_click: F) -> ClickableRow<V, State, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State, RowClickAction) + Send + Sync + 'static,
    State: 'static,
{
    ClickableRow {
        child,
        on_click,
        phantom: PhantomData,
    }
}

impl<V, State, F> ViewMarker for ClickableRow<V, State, F> {}

impl<V, State, F> View<State, (), ViewCtx> for ClickableRow<V, State, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State, RowClickAction) + Send + Sync + 'static,
    State: 'static,
{
    type Element = Pod<RowClickable>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = RowClickable::new(child_pod.new_widget);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let mut child = RowClickable::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut child = RowClickable::child_mut(&mut element);
            self.child.teardown(view_state, ctx, child.downcast());
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        if let Some(action) = message.take_message::<RowClickAction>() {
            (self.on_click)(app_state, *action);
            MessageResult::Action(())
        } else {
            let mut child = RowClickable::child_mut(&mut element);
            self.child
                .message(view_state, message, child.downcast(), app_state)
        }
    }
}
