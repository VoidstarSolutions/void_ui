//! Masonry widget for the two-pane resizable split.
//!
//! [`ResizableWidget`] places two child widgets side-by-side (or top-to-bottom)
//! separated by a thin drag handle. Dragging the handle redistributes space
//! between the panes and emits [`ResizeHandleDragged`] with the updated ratio.
//!
//! The grab zone around the handle is wider than the visual line so small
//! handles remain easy to hit.

use std::any::TypeId;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, FromDynWidget, LayoutCtx,
    MeasureCtx, NewWidget, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
    WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::peniko::Color;

use crate::Theme;

// --- MARK: CONSTANTS

/// Thickness of the visual divider line at rest — matches [`Separator`](crate::Separator)'s
/// default 1px line so a resizable divider reads the same as a static one.
const HANDLE_THICKNESS: f64 = 1.0;
/// Half-width of the invisible grab region on each side of the handle center.
const GRAB_HALF: f64 = 8.0;
/// Default minimum panel size in pixels; callers may override via [`ResizableWidget::set_min_size`].
pub const MIN_PANEL_SIZE: f64 = 40.0;

// --- MARK: ACTION

/// Action emitted on every pointer-move while the resize handle is dragged.
///
/// Carries the updated first-panel fraction (clamped to keep both panels at
/// least `min_size` pixels wide/tall).
#[derive(Debug, Clone)]
pub struct ResizeHandleDragged(pub f32);

// --- MARK: ResizableWidget

/// Two-pane split container with a draggable divider.
///
/// `A` is the first (left/top) child widget type; `B` is the second
/// (right/bottom) child widget type. Both are fully generic so any view
/// composition can be placed inside either pane.
pub struct ResizableWidget<A: Widget + ?Sized, B: Widget + ?Sized> {
    first: WidgetPod<A>,
    second: WidgetPod<B>,
    axis: Axis,
    /// First panel fraction of the usable extent (0.0–1.0).
    ratio: f32,
    /// Minimum size for either panel in pixels.
    min_size: f64,
    theme: Theme,
    handle_hovered: bool,
    dragging: bool,
    /// Split-axis extent (total widget size) from the last layout pass.
    total_extent: f64,
    /// Cross-axis extent from the last layout pass (needed for paint).
    cross_extent: f64,
    /// Center of the handle on the split axis from the last layout pass.
    handle_center: f64,
}

// --- MARK: BUILDERS

impl<A: Widget + ?Sized, B: Widget + ?Sized> ResizableWidget<A, B> {
    #[must_use]
    pub fn new(
        first: NewWidget<A>,
        second: NewWidget<B>,
        axis: Axis,
        ratio: f32,
        min_size: f64,
        theme: &Theme,
    ) -> Self {
        Self {
            first: first.to_pod(),
            second: second.to_pod(),
            axis,
            ratio,
            min_size,
            theme: *theme,
            handle_hovered: false,
            dragging: false,
            total_extent: 0.0,
            cross_extent: 0.0,
            handle_center: 0.0,
        }
    }
}

// --- MARK: WIDGETMUT

impl<A: Widget + FromDynWidget, B: Widget + ?Sized> ResizableWidget<A, B> {
    /// Returns a `WidgetMut` for the first (left/top) child.
    pub fn first_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, A> {
        this.ctx.get_mut(&mut this.widget.first)
    }
}

impl<A: Widget + ?Sized, B: Widget + FromDynWidget> ResizableWidget<A, B> {
    /// Returns a `WidgetMut` for the second (right/bottom) child.
    pub fn second_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, B> {
        this.ctx.get_mut(&mut this.widget.second)
    }
}

impl<A: Widget + ?Sized, B: Widget + ?Sized> ResizableWidget<A, B> {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_ratio(this: &mut WidgetMut<'_, Self>, ratio: f32) {
        if (this.widget.ratio - ratio).abs() > 1e-5 {
            this.widget.ratio = ratio;
            this.ctx.request_layout();
        }
    }

    pub fn set_min_size(this: &mut WidgetMut<'_, Self>, min_size: f64) {
        if (this.widget.min_size - min_size).abs() > 1e-5 {
            this.widget.min_size = min_size;
            this.ctx.request_layout();
        }
    }
}

// --- MARK: HELPERS

impl<A: Widget + ?Sized, B: Widget + ?Sized> ResizableWidget<A, B> {
    fn pos_on_axis(&self, pos: Point) -> f64 {
        match self.axis {
            Axis::Horizontal => pos.x,
            Axis::Vertical => pos.y,
        }
    }

    fn in_handle(&self, pos: Point) -> bool {
        (self.pos_on_axis(pos) - self.handle_center).abs() <= GRAB_HALF
    }

    /// Map a cursor position to a clamped first-panel ratio.
    fn ratio_from_pos(&self, pos: f64) -> f32 {
        let usable = (self.total_extent - HANDLE_THICKNESS).max(1.0);
        let min = self.min_size.min(usable * 0.5);
        let first = (pos - HANDLE_THICKNESS * 0.5).max(min).min(usable - min);
        #[allow(clippy::cast_possible_truncation)]
        {
            (first / usable) as f32
        }
    }

    fn handle_color(&self) -> Color {
        let p = &self.theme.palette;
        if self.dragging {
            p.teal
        } else if self.handle_hovered {
            p.surface_hi
        } else {
            p.border
        }
    }
}

// --- MARK: IMPL WIDGET

impl<A: Widget + ?Sized, B: Widget + ?Sized> Widget for ResizableWidget<A, B> {
    type Action = ResizeHandleDragged;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Move(update) => {
                let pos = ctx.local_position(update.current.position);
                let in_handle = self.in_handle(pos);
                if in_handle != self.handle_hovered {
                    self.handle_hovered = in_handle;
                    ctx.request_paint_only();
                }
                if self.dragging {
                    let new_ratio = self.ratio_from_pos(self.pos_on_axis(pos));
                    if (new_ratio - self.ratio).abs() > 1e-5 {
                        self.ratio = new_ratio;
                        ctx.request_layout();
                        ctx.submit_action::<Self::Action>(ResizeHandleDragged(new_ratio));
                    }
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if self.in_handle(pos) {
                    self.dragging = true;
                    ctx.capture_pointer();
                    ctx.set_handled();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) if self.dragging => {
                self.dragging = false;
                ctx.request_paint_only();
            }
            PointerEvent::Leave(_) if self.handle_hovered => {
                self.handle_hovered = false;
                ctx.request_paint_only();
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
        if let Update::HoveredChanged(false) = event
            && self.handle_hovered
        {
            self.handle_hovered = false;
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.first);
        ctx.register_child(&mut self.second);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        let first_len = ctx.compute_length(
            &mut self.first,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        );
        let second_len = ctx.compute_length(
            &mut self.second,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        );
        if axis == self.axis {
            Length::px(first_len.get() + HANDLE_THICKNESS + second_len.get())
        } else {
            Length::px(first_len.get().max(second_len.get()))
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let (total, cross) = match self.axis {
            Axis::Horizontal => (size.width, size.height),
            Axis::Vertical => (size.height, size.width),
        };
        let usable = (total - HANDLE_THICKNESS).max(0.0);
        let min = self.min_size.min(usable * 0.5);
        let first_extent = (usable * f64::from(self.ratio)).max(min).min(usable - min);
        let second_extent = usable - first_extent;

        self.total_extent = total;
        self.cross_extent = cross;
        self.handle_center = first_extent + HANDLE_THICKNESS * 0.5;

        match self.axis {
            Axis::Horizontal => {
                ctx.run_layout(&mut self.first, Size::new(first_extent, size.height));
                ctx.place_child(&mut self.first, Point::ORIGIN);
                ctx.run_layout(&mut self.second, Size::new(second_extent, size.height));
                ctx.place_child(
                    &mut self.second,
                    Point::new(first_extent + HANDLE_THICKNESS, 0.0),
                );
            }
            Axis::Vertical => {
                ctx.run_layout(&mut self.first, Size::new(size.width, first_extent));
                ctx.place_child(&mut self.first, Point::ORIGIN);
                ctx.run_layout(&mut self.second, Size::new(size.width, second_extent));
                ctx.place_child(
                    &mut self.second,
                    Point::new(0.0, first_extent + HANDLE_THICKNESS),
                );
            }
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let color = self.handle_color();
        // Widen slightly on hover/drag for easier targeting feedback.
        let visual = if self.dragging || self.handle_hovered {
            2.0
        } else {
            HANDLE_THICKNESS
        };
        let offset = self.handle_center - visual * 0.5;
        let rect = match self.axis {
            Axis::Horizontal => Rect::from_origin_size(
                Point::new(offset, 0.0),
                Size::new(visual, self.cross_extent),
            ),
            Axis::Vertical => Rect::from_origin_size(
                Point::new(0.0, offset),
                Size::new(self.cross_extent, visual),
            ),
        };
        painter.fill(rect, color).draw();
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        let local = ctx.to_local(pos);
        if self.in_handle(local) {
            match self.axis {
                Axis::Horizontal => CursorIcon::ColResize,
                Axis::Vertical => CursorIcon::RowResize,
            }
        } else {
            CursorIcon::Default
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
        ChildrenIds::from_slice(&[self.first.id(), self.second.id()])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        true
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("ResizableWidget", id = id.trace())
    }
}
