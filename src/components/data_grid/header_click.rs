//! Single-child wrapper widget that detects a primary-button click and
//! emits a [`HeaderClicked`] action — the header-row counterpart to
//! [`super::row_click::RowClickable`].
//!
//! Kept deliberately separate from `RowClickable`. Header clicks are
//! modifier-agnostic (a plain click cycles the column's sort) and must
//! not participate in row selection or take keyboard focus the way a
//! row does. Splitting the two keeps each one's intent obvious and
//! leaves room for header-specific behavior later (resize handles,
//! column menus) without entangling it with row selection.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

/// Action emitted by [`HeaderClickable`] on primary-button release
/// inside its bounds. Carries no modifiers — a header click means
/// "cycle this column's sort," nothing more.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeaderClicked;

/// Single-child wrapper that emits [`HeaderClicked`] on primary-button
/// release inside its bounds.
pub struct HeaderClickable {
    child: WidgetPod<dyn Widget>,
}

// --- MARK: BUILDERS
impl HeaderClickable {
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }
}

// --- MARK: WIDGETMUT
impl HeaderClickable {
    /// Returns a mutable reference to the wrapped child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: IMPL WIDGET
impl Widget for HeaderClickable {
    type Action = HeaderClicked;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { button, .. }) => {
                if matches!(button, Some(PointerButton::Primary)) {
                    // Capture so the release can be matched against
                    // `is_active`; no focus request — headers don't own
                    // the keyboard (the grid's copy shortcut does).
                    ctx.capture_pointer();
                }
            }
            PointerEvent::Up(PointerButtonEvent { button, .. })
                if matches!(button, Some(PointerButton::Primary))
                    && ctx.is_active()
                    && ctx.is_hovered() =>
            {
                ctx.submit_action::<Self::Action>(HeaderClicked);
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
        Role::ColumnHeader
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
        false
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

/// Wraps a child view in [`HeaderClickable`] and runs `on_click`
/// against the host's app state on each primary-button release.
///
/// Use it to cycle the grid's
/// [`SortState`](super::sort::SortState) for the clicked column.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ClickableHeader<V, State, F> {
    child: V,
    on_click: F,
    phantom: PhantomData<fn() -> State>,
}

/// Constructor for [`ClickableHeader`].
pub fn clickable_header<V, State, F>(child: V, on_click: F) -> ClickableHeader<V, State, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State) + Send + Sync + 'static,
    State: 'static,
{
    ClickableHeader {
        child,
        on_click,
        phantom: PhantomData,
    }
}

impl<V, State, F> ViewMarker for ClickableHeader<V, State, F> {}

impl<V, State, F> View<State, (), ViewCtx> for ClickableHeader<V, State, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State) + Send + Sync + 'static,
    State: 'static,
{
    type Element = Pod<HeaderClickable>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = HeaderClickable::new(child_pod.new_widget);
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
        let mut child = HeaderClickable::child_mut(&mut element);
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
            let mut child = HeaderClickable::child_mut(&mut element);
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
        if message.take_message::<HeaderClicked>().is_some() {
            (self.on_click)(app_state);
            MessageResult::Action(())
        } else {
            let mut child = HeaderClickable::child_mut(&mut element);
            self.child
                .message(view_state, message, child.downcast(), app_state)
        }
    }
}
