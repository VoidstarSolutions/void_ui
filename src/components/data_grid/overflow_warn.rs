//! Single-child wrapper widget that emits a one-shot
//! `tracing::warn!` if the grid's `sum(column.width)` exceeds the
//! viewport width at layout time.
//!
//! Built to back the doc claim on [`super::ColumnDef`] that
//! horizontal overflow is logged in debug builds. The warning is
//! gated by a [`OnceLock`] so resize storms don't flood the log;
//! once fired, it stays silent for the lifetime of this widget
//! instance.
//!
//! Layout / paint / events all delegate to the child unchanged.

use std::sync::OnceLock;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, SizeDef};

/// Wraps a child and warns once if the laid-out width is less than
/// the column-sum width supplied at construction.
pub struct OverflowWarn {
    child: WidgetPod<dyn Widget>,
    expected_width: f64,
    warned: OnceLock<()>,
}

// --- MARK: BUILDERS
impl OverflowWarn {
    /// Wraps `child` and records the column-sum width to compare
    /// against the laid-out viewport width.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, expected_width: f64) -> Self {
        Self {
            child: child.erased().to_pod(),
            expected_width,
            warned: OnceLock::new(),
        }
    }
}

// --- MARK: WIDGETMUT
impl OverflowWarn {
    /// Returns a mutable reference to the wrapped child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Updates the expected width when the column configuration
    /// changes. Does not re-trigger the warning.
    pub fn set_expected_width(this: &mut WidgetMut<'_, Self>, width: f64) {
        this.widget.expected_width = width;
    }
}

// --- MARK: IMPL WIDGET
impl Widget for OverflowWarn {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
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
        cross_length: Option<f64>,
    ) -> f64 {
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
        if size.width < self.expected_width && self.warned.set(()).is_ok() {
            tracing::warn!(
                viewport_width = size.width,
                expected_width = self.expected_width,
                "data_grid: column widths exceed viewport — rightmost columns will clip"
            );
        }
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
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

// --- MARK: XILEM VIEW WRAPPER

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Wraps a child view in an [`OverflowWarn`] widget. Used by
/// `data_grid` to log a one-shot warning when column widths exceed
/// the viewport.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct OverflowWarnView<V, State> {
    child: V,
    expected_width: f64,
    phantom: PhantomData<fn() -> State>,
}

/// Constructor for [`OverflowWarnView`].
pub fn overflow_warn<V, State>(child: V, expected_width: f64) -> OverflowWarnView<V, State>
where
    V: WidgetView<State, ()>,
    State: 'static,
{
    OverflowWarnView {
        child,
        expected_width,
        phantom: PhantomData,
    }
}

impl<V, State> ViewMarker for OverflowWarnView<V, State> {}

impl<V, State> View<State, (), ViewCtx> for OverflowWarnView<V, State>
where
    V: WidgetView<State, ()>,
    State: 'static,
{
    type Element = Pod<OverflowWarn>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = OverflowWarn::new(child_pod.new_widget, self.expected_width);
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
        #[expect(
            clippy::float_cmp,
            reason = "skip the setter unless the value is bit-identical"
        )]
        if self.expected_width != prev.expected_width {
            OverflowWarn::set_expected_width(&mut element, self.expected_width);
        }
        let mut child = OverflowWarn::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = OverflowWarn::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        let mut child = OverflowWarn::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}
