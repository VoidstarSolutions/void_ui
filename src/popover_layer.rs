//! `PopoverLayer` — reusable window-level floating layer with outside-click dismissal.
//!
//! Any component that needs a layer-based popover (dropdown menu, context menu,
//! combobox list, date-picker) wraps its content in `PopoverLayer` instead of
//! implementing `Layer` itself.  Dismissal is communicated back to the creator
//! widget via a caller-supplied `on_outside_click` closure so `PopoverLayer`
//! stays product-agnostic.
//!
//! # Usage
//!
//! ```ignore
//! let close_cb = Arc::new(|mut w: WidgetMut<dyn Widget>, layer_id: WidgetId| {
//!     let mut w = w.downcast::<MyWidget>();
//!     w.widget.open = false;
//!     w.widget.layer_id = None;
//!     w.ctx.remove_layer(layer_id);
//! });
//! let layer_widget = NewWidget::new(PopoverLayer::new(content_widget, creator_id, close_cb));
//! let layer_id = layer_widget.id();
//! ctx.create_layer(LayerType::Other, layer_widget, window_pos);
//! ```

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, Layer, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

/// Corner radius of the popover container background.
const CORNER_RADIUS: f64 = 5.0;
/// Border width of the popover container background.
const BORDER_WIDTH: f64 = 1.0;

/// Callback type invoked when a click outside the popover's bounds is detected.
///
/// Receives a `WidgetMut` of the *creator* widget and the popover's own
/// `WidgetId` so the callback can downcast, update state, and call
/// `ctx.remove_layer(layer_id)`.
pub type OnOutsideClick = Arc<dyn Fn(WidgetMut<'_, dyn Widget>, WidgetId) + Send + Sync + 'static>;

/// Window-level floating layer that wraps arbitrary content with
/// background/border chrome and outside-click dismissal.
///
/// Construct via [`PopoverLayer::new`], then pass the `NewWidget` to
/// `EventCtx::create_layer`.  The caller retains the `WidgetId` to
/// identify the layer in `remove_layer` calls.
pub struct PopoverLayer {
    child: WidgetPod<dyn Widget>,
    /// ID of the widget that owns (and is responsible for removing) this layer.
    creator: WidgetId,
    /// Called when a primary-button click outside the popover's bounds is
    /// captured by [`Layer::capture_pointer_event`].
    on_outside_click: OnOutsideClick,
    /// Cached last-layout size used for outside-click hit-testing.
    last_size: Size,
    /// Background color, drawn before the child.
    bg: masonry::peniko::Color,
    /// Border color.  Transparent = no border drawn.
    border: masonry::peniko::Color,
}

impl PopoverLayer {
    /// Create a `PopoverLayer` wrapping `child`.
    ///
    /// `on_outside_click` is called (via `mutate_later`) when the user clicks
    /// outside the popover bounds; it receives the creator's `WidgetMut` and
    /// this layer's `WidgetId`.
    #[must_use]
    pub fn new(
        child: NewWidget<impl Widget + ?Sized>,
        creator: WidgetId,
        bg: masonry::peniko::Color,
        border: masonry::peniko::Color,
        on_outside_click: OnOutsideClick,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            creator,
            on_outside_click,
            last_size: Size::ZERO,
            bg,
            border,
        }
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        let origin = ctx.to_window(Point::ZERO);
        window_pos - origin.to_vec2()
    }

    fn dismiss(&self, ctx: &mut EventCtx<'_>) {
        let self_id = ctx.widget_id();
        let creator = self.creator;
        let cb = Arc::clone(&self.on_outside_click);
        ctx.mutate_later(creator, move |w| {
            cb(w, self_id);
        });
    }
}

impl Widget for PopoverLayer {
    type Action = NoAction;

    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn propagates_pointer_interaction(&self) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

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
        ctx.compute_length(
            &mut self.child,
            len_req.into(),
            LayoutSize::maybe(axis.cross(), cross_length),
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.last_size = size;
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(size), size.into());
        ctx.run_layout(&mut self.child, child_size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let rrect = RoundedRect::from_origin_size(Point::ORIGIN, size, CORNER_RADIUS);
        if self.bg.components[3] > 0.0 {
            painter.fill(rrect, self.bg).draw();
        }
        if self.border.components[3] > 0.0 {
            painter
                .stroke(rrect, &Stroke::new(BORDER_WIDTH), self.border)
                .draw();
        }
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

impl Layer for PopoverLayer {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            let local = Self::to_local(ctx, state.logical_point());
            let bounds = Rect::from_origin_size(Point::ZERO, self.last_size);
            if !bounds.contains(local) {
                self.dismiss(ctx);
            }
        }
    }
}
