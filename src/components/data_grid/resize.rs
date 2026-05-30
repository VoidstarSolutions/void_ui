//! Thin draggable strip that resizes a column — the trailing-edge
//! handle on a header cell. Emits a [`ResizeDrag`] carrying the
//! column's *new absolute width* as the pointer drags.
//!
//! Modeled on masonry's `Split` divider and this crate's
//! [`header_click`](super::header_click) widget+view split. Kept
//! separate from `HeaderClickable` because the two have distinct
//! gestures (a click that cycles sort vs. a drag that resizes) and we
//! want each handle to live at a column boundary independent of the
//! header-cell click target.
//!
//! ## Why it emits an absolute width
//!
//! On pointer-down the handle snapshots `(base_width, start_x)` and,
//! for every move, emits `base_width + (cursor_x − start_x)`. Snapshotting
//! on *down* (not reading the live width each move) means mid-drag
//! rebuilds — which change the column's width — can't corrupt the
//! in-progress gesture. Clamping to a sane minimum is the model's job
//! (see [`ColumnWidths::set`](super::width::ColumnWidths::set)); the
//! handle reports the raw target width.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

/// Action emitted by [`ResizeHandle`] while dragging: the column's new
/// absolute target width in pixels (un-clamped — the receiver clamps).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResizeDrag {
    /// Proposed new column width in px (before min-width clamping).
    pub new_width: f64,
}

/// In-flight drag snapshot, captured on pointer-down so rebuilds during
/// the drag can't shift the reference point.
#[derive(Clone, Copy, Debug)]
struct DragSnapshot {
    /// Column width at the moment the drag began.
    base_width: f64,
    /// Pointer x (window coords) at the moment the drag began.
    start_x: f64,
}

/// Computes the dragged target width from the drag snapshot and the
/// current pointer x. Pure so it can be unit-tested without a context.
fn dragged_width(base_width: f64, start_x: f64, current_x: f64) -> f64 {
    base_width + (current_x - start_x)
}

/// A leaf widget: a thin vertical strip at a column's trailing edge.
/// Drag it to resize the column. Paints a subtle grip line, brighter
/// while hovered or dragging.
pub struct ResizeHandle {
    /// The column's current width, kept fresh by the view on rebuild and
    /// snapshotted into [`DragSnapshot`] on pointer-down.
    base_width: f64,
    /// Grip line color when idle.
    color: Color,
    /// Grip line color while hovered or actively dragging.
    active_color: Color,
    drag: Option<DragSnapshot>,
}

/// Strip width in px — the hit target / painted column for the grip.
const HANDLE_WIDTH: f64 = 7.0;
/// Painted grip-line thickness in px (centered in the strip).
const GRIP_THICKNESS: f64 = 1.0;

// --- MARK: BUILDERS
impl ResizeHandle {
    #[must_use]
    pub fn new(base_width: f64, color: Color, active_color: Color) -> Self {
        Self {
            base_width,
            color,
            active_color,
            drag: None,
        }
    }

    /// The strip's fixed hit-target width.
    #[must_use]
    pub const fn width() -> f64 {
        HANDLE_WIDTH
    }
}

// --- MARK: WIDGETMUT
impl ResizeHandle {
    /// Refreshes the column's current width (used as the drag base on the
    /// next pointer-down). Does not affect an in-flight drag.
    pub fn set_base_width(this: &mut WidgetMut<'_, Self>, base_width: f64) {
        // `float_cmp`: only skip the assignment when bit-identical; a
        // redundant set is harmless but this avoids needless churn.
        #[expect(clippy::float_cmp, reason = "skip redundant identical sets")]
        if this.widget.base_width != base_width {
            this.widget.base_width = base_width;
        }
    }

    /// Updates the grip colors (e.g. on theme swap); repaints on change.
    pub fn set_colors(this: &mut WidgetMut<'_, Self>, color: Color, active_color: Color) {
        if this.widget.color != color || this.widget.active_color != active_color {
            this.widget.color = color;
            this.widget.active_color = active_color;
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ResizeHandle {
    type Action = ResizeDrag;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                self.drag = Some(DragSnapshot {
                    base_width: self.base_width,
                    start_x: state.position.x,
                });
                // `capture_pointer` makes `is_active()` true for the
                // duration of the press; masonry releases it on pointer
                // up. Consume the event so an underlying header-click
                // target doesn't also treat this press as a sort cycle.
                ctx.capture_pointer();
                ctx.set_handled();
                ctx.request_paint_only();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                if let Some(snap) = self.drag
                    && ctx.is_active()
                {
                    let new_width = dragged_width(snap.base_width, snap.start_x, current.position.x);
                    ctx.submit_action::<Self::Action>(ResizeDrag { new_width });
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                // End any in-flight drag and repaint to clear the
                // "active" grip color.
                let was_dragging = self.drag.is_some();
                self.drag = None;
                if was_dragging {
                    ctx.request_paint_only();
                }
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

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::HoveredChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match axis {
            // Fixed strip width on the main axis.
            Axis::Horizontal => Length::px(HANDLE_WIDTH),
            // Fill the offered height (the header row's fixed height).
            Axis::Vertical => match len_req {
                LenReq::FitContent(space) => space,
                LenReq::MinContent => Length::ZERO,
                LenReq::MaxContent => Length::px(f64::MAX),
            },
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let lit = ctx.is_hovered() || self.drag.is_some();
        let color = if lit { self.active_color } else { self.color };
        if color.components[3] <= 0.0 {
            return;
        }
        // A vertical grip line centered in the strip.
        let x = (size.width - GRIP_THICKNESS) * 0.5;
        let rect =
            Rect::from_origin_size(Point::new(x, 0.0), Size::new(GRIP_THICKNESS, size.height));
        painter.fill(rect, color).draw();
    }

    fn get_cursor(&self, _ctx: &QueryCtx<'_>, _pos: Point) -> CursorIcon {
        // East-west resize cursor over the strip — the standard
        // column-resize affordance (matches masonry's `Split`).
        CursorIcon::EwResize
    }

    fn accessibility_role(&self) -> Role {
        Role::Splitter
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, _node: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

// --- MARK: XILEM VIEW WRAPPER

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// A resize handle view: a draggable column-boundary strip that calls
/// `on_resize(state, new_width)` (un-clamped px) as the user drags.
/// `base_width` is the column's current width (the drag's reference).
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ResizeHandleView<State, F> {
    base_width: f64,
    color: Color,
    active_color: Color,
    on_resize: F,
    phantom: PhantomData<fn() -> State>,
}

/// Constructor for [`ResizeHandleView`].
pub fn resize_handle<State, F>(
    base_width: f64,
    color: Color,
    active_color: Color,
    on_resize: F,
) -> ResizeHandleView<State, F>
where
    F: Fn(&mut State, f64) + Send + Sync + 'static,
    State: 'static,
{
    ResizeHandleView {
        base_width,
        color,
        active_color,
        on_resize,
        phantom: PhantomData,
    }
}

impl<State, F> ViewMarker for ResizeHandleView<State, F> {}

impl<State, F> View<State, (), ViewCtx> for ResizeHandleView<State, F>
where
    F: Fn(&mut State, f64) + Send + Sync + 'static,
    State: 'static,
{
    type Element = Pod<ResizeHandle>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = ResizeHandle::new(self.base_width, self.color, self.active_color);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        #[expect(clippy::float_cmp, reason = "skip redundant identical sets")]
        if self.base_width != prev.base_width {
            ResizeHandle::set_base_width(&mut element, self.base_width);
        }
        if self.color != prev.color || self.active_color != prev.active_color {
            ResizeHandle::set_colors(&mut element, self.color, self.active_color);
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        match message.take_message::<ResizeDrag>() {
            Some(drag) => {
                (self.on_resize)(app_state, drag.new_width);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ResizeDrag, dragged_width};

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn drag_right_grows_width() {
        // base 100, dragged from x=200 to x=260 → +60.
        assert!(approx(dragged_width(100.0, 200.0, 260.0), 160.0));
    }

    #[test]
    fn drag_left_shrinks_width() {
        // base 100, dragged from x=200 to x=170 → −30. Un-clamped here;
        // ColumnWidths::set applies the min-width floor.
        assert!(approx(dragged_width(100.0, 200.0, 170.0), 70.0));
    }

    #[test]
    fn no_movement_keeps_width() {
        assert!(approx(dragged_width(120.0, 50.0, 50.0), 120.0));
    }

    #[test]
    fn resize_drag_default_is_zero() {
        assert!(approx(ResizeDrag::default().new_width, 0.0));
    }
}
