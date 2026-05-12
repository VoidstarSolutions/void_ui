//! `PointerInert` widget + `pointer_inert` xilem view: pointer-event-
//! transparent wrappers used to overlay non-interactive chrome
//! (chart-meta legends, status overlays) on top of interactive
//! widgets without stealing clicks.
//!
//! Both `accepts_pointer_interaction` and `propagates_pointer_interaction`
//! return `false`, so hit-testing skips this widget and any descendants
//! entirely — the parent's search moves on to siblings (in a
//! [`ZStack`](xilem_masonry::view::zstack), the widget drawn beneath
//! receives the click instead).

use std::marker::PhantomData;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::LenReq;
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

/// Masonry widget that hosts a child but is invisible to pointer
/// hit-testing — the equivalent of CSS `pointer-events: none`.
///
/// Both `accepts_pointer_interaction` and `propagates_pointer_interaction`
/// return `false`, so a `find_widget_under_pointer` walk that enters this
/// widget's bounding box returns `None` rather than the widget or any of
/// its descendants. The parent's hit-test moves on to siblings (in a
/// `ZStack`, that means a widget drawn below this one in z-order receives
/// the click instead).
///
/// Use it to overlay informational chrome — a chart-meta legend, a status
/// bar — on top of an interactive widget without stealing clicks. Layout,
/// painting, and accessibility forward through to the child unchanged.
pub struct PointerInert {
    inner: WidgetPod<dyn Widget>,
}

impl PointerInert {
    /// Create a `PointerInert` wrapping the given child widget.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            inner: child.erased().to_pod(),
        }
    }

    /// Return the wrapped child's [`WidgetId`].
    #[must_use]
    pub fn inner_id(&self) -> WidgetId {
        self.inner.id()
    }

    /// Mutable access to the wrapped child, used by the [`View`] layer's
    /// `rebuild`/`teardown`/`message` pass-through.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}

impl Widget for PointerInert {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
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
        cross_length: Option<f64>,
    ) -> f64 {
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
}

/// Construct a [`PointerInertView`] that renders `child` but ignores
/// pointer events on it and its descendants.
pub fn pointer_inert<State, Action, V>(child: V) -> PointerInertView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    PointerInertView {
        child,
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct PointerInertView<V, State, Action> {
    child: V,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for PointerInertView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for PointerInertView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<PointerInert>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.child.build(ctx, app_state);
        let widget = PointerInert::new(child.new_widget.erased());
        (ctx.create_pod(widget), child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let mut child = PointerInert::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = PointerInert::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut child = PointerInert::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}
