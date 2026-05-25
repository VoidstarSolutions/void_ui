//! Custom scroll container widget with a clip rect that excludes scrollbar tracks,
//! so scrollbars never render over content.

use std::any::TypeId;
use std::ops::Range;

use masonry::accesskit::{self, Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, AllowRawMut, ChildrenIds, ComposeCtx, EventCtx, FromDynWidget,
    LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PointerEvent, PointerScrollEvent,
    PropertiesMut, PropertiesRef, Property, RegisterCtx, TextEvent, Update, UpdateCtx,
    UsesProperty, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::dpi::{LogicalPosition, PhysicalPosition};
use masonry::imaging::{Composite, GroupRef, Painter};
use masonry::kurbo::{Axis, Point, Rect, Size, Stroke, Vec2};
use masonry::layout::{AsUnit as _, LayoutSize, LenDef, LenReq, Length, SizeDef};
use masonry::peniko::BlendMode;
use masonry::properties::AutoHideScrollBar;
use masonry::{kurbo, theme};
use tracing::{Span, trace_span};

fn compute_pan_range(viewport: Range<f64>, target: Range<f64>) -> Range<f64> {
    let len = viewport.end - viewport.start;
    let start = if target.start < viewport.start {
        target.start
    } else if target.end > viewport.end {
        (target.end - len).max(viewport.start)
    } else {
        viewport.start
    };
    start..start + len
}

// --- MARK: VoidScrollBar

/// Internal scrollbar widget. All stateful fields are `pub(crate)` so
/// [`ScrollView`] can read/write them directly via `get_raw_mut`.
pub(crate) struct VoidScrollBar {
    axis: Axis,
    pub(crate) cursor_progress: f64,
    pub(crate) moved: bool,
    pub(crate) portal_size: f64,
    pub(crate) content_size: f64,
    /// Current rendered opacity (animated toward `target_opacity`).
    opacity: f32,
    /// Target opacity set by the parent.
    pub(crate) target_opacity: f32,
    grab_anchor: Option<f64>,
}

impl VoidScrollBar {
    pub(crate) fn new(axis: Axis) -> Self {
        Self {
            axis,
            cursor_progress: 0.0,
            moved: false,
            portal_size: 0.0,
            content_size: 0.0,
            opacity: 1.0,
            target_opacity: 1.0,
            grab_anchor: None,
        }
    }

    fn track_size(&self, layout_size: Size) -> (f64, f64) {
        let size_ratio = if self.content_size > 0.0 {
            (self.portal_size / self.content_size).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let thumb_len =
            (size_ratio * layout_size.get_coord(self.axis)).max(theme::SCROLLBAR_MIN_SIZE);
        let track_len = layout_size.get_coord(self.axis) - thumb_len;
        (thumb_len, track_len)
    }

    fn thumb_rect(&self, layout_size: Size) -> Rect {
        let (thumb_len, track_len) = self.track_size(layout_size);
        let offset = self.cursor_progress * track_len;
        let pos = self.axis.pack_point(offset, 0.0);
        let cross = layout_size.get_coord(self.axis.cross());
        let thumb_size = self.axis.pack_size(thumb_len, cross);
        Rect::from_origin_size(pos, thumb_size)
    }

    fn progress_from_pos(&self, layout_size: Size, anchor: f64, pos: Point) -> f64 {
        let (thumb_len, track_len) = self.track_size(layout_size);
        let raw = pos.get_coord(self.axis) - anchor * thumb_len;
        (raw / track_len.max(1e-12)).clamp(0.0, 1.0)
    }

    fn scroll_range(&self) -> f64 {
        (self.content_size - self.portal_size).max(0.0)
    }

    fn set_cursor_progress(&mut self, new: f64) -> bool {
        let new = new.clamp(0.0, 1.0);
        if (new - self.cursor_progress).abs() > 1e-12 {
            self.cursor_progress = new;
            self.moved = true;
            true
        } else {
            false
        }
    }
}

impl AllowRawMut for VoidScrollBar {}

impl Widget for VoidScrollBar {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let size = ctx.content_box_size();
        match event {
            PointerEvent::Down(state) => {
                ctx.capture_pointer();
                let local = ctx.local_position(state.state.position);
                let thumb = self.thumb_rect(size);
                if thumb.contains(local) {
                    let major = local.get_coord(self.axis);
                    let thumb_start = if self.axis == Axis::Horizontal {
                        thumb.min_x()
                    } else {
                        thumb.min_y()
                    };
                    let thumb_len = if self.axis == Axis::Horizontal {
                        thumb.width()
                    } else {
                        thumb.height()
                    };
                    self.grab_anchor = Some((major - thumb_start) / thumb_len.max(1e-12));
                } else {
                    // Click outside thumb: center the thumb on the click position.
                    self.grab_anchor = Some(0.5);
                    self.set_cursor_progress(self.progress_from_pos(
                        size,
                        0.5,
                        ctx.local_position(state.state.position),
                    ));
                }
                ctx.request_render();
            }
            PointerEvent::Move(state) if self.grab_anchor.is_some() => {
                let anchor = self.grab_anchor.unwrap();
                let local = ctx.local_position(state.current.position);
                if self.set_cursor_progress(self.progress_from_pos(size, anchor, local)) {
                    ctx.request_render();
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => {
                self.grab_anchor = None;
                ctx.request_render();
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

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        const FADE_MILLIS: f32 = 300.0;
        let delta = (interval as f32 / 1_000_000.0) / FADE_MILLIS;
        let diff = self.target_opacity - self.opacity;
        if diff.abs() > 1e-4 {
            self.opacity = if diff > 0.0 {
                (self.opacity + delta).min(self.target_opacity)
            } else {
                (self.opacity - delta).max(self.target_opacity)
            };
            ctx.request_render();
            if (self.target_opacity - self.opacity).abs() > 1e-4 {
                ctx.request_anim_frame();
            }
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        if axis == self.axis {
            match len_req {
                LenReq::MinContent | LenReq::MaxContent => self.portal_size.px(),
                LenReq::FitContent(space) => space,
            }
        } else {
            (theme::SCROLLBAR_WIDTH + theme::SCROLLBAR_PAD * 2.0).px()
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.content_box_size();
        let needs_opacity = self.opacity < 1.0 - 1e-4;

        if needs_opacity {
            painter.push_fill_clip(ctx.border_box());
            painter.push_group(
                GroupRef::new().with_composite(Composite::new(BlendMode::default(), self.opacity)),
            );
        }

        let pad = theme::SCROLLBAR_PAD;
        let thumb = self.thumb_rect(size).inset((
            -if self.axis == Axis::Vertical {
                pad
            } else {
                0.0
            },
            -if self.axis == Axis::Horizontal {
                pad
            } else {
                0.0
            },
            -if self.axis == Axis::Vertical {
                pad
            } else {
                0.0
            },
            -if self.axis == Axis::Horizontal {
                pad
            } else {
                0.0
            },
        ));
        let thumb = thumb.to_rounded_rect(theme::SCROLLBAR_RADIUS);

        painter.fill(thumb, theme::SCROLLBAR_COLOR).draw();
        painter
            .stroke(
                thumb,
                &Stroke::new(theme::SCROLLBAR_EDGE_WIDTH),
                theme::SCROLLBAR_BORDER_COLOR,
            )
            .draw();

        if needs_opacity {
            painter.pop_group();
            painter.pop_clip();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ScrollBar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_orientation(match self.axis {
            Axis::Horizontal => accesskit::Orientation::Horizontal,
            Axis::Vertical => accesskit::Orientation::Vertical,
        });
        let range = self.scroll_range();
        let value = self.cursor_progress.clamp(0.0, 1.0) * range;
        match self.axis {
            Axis::Horizontal => {
                node.set_scroll_x_min(0.0);
                node.set_scroll_x_max(range);
                node.set_scroll_x(value);
            }
            Axis::Vertical => {
                node.set_scroll_y_min(0.0);
                node.set_scroll_y_max(range);
                node.set_scroll_y(value);
            }
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("VoidScrollBar", id = id.trace())
    }
}

// --- MARK: ContentClip

/// Wraps user content, clips to the effective viewport (excluding scrollbar
/// tracks), and applies the scroll translation. Kept separate from
/// [`ScrollView`] so the scrollbars can be siblings rather than children of
/// the clip — masonry's `set_clip_path` applies to the whole subtree.
pub(crate) struct ContentClip<W: Widget + ?Sized> {
    child: WidgetPod<W>,
    /// Content size set by [`ScrollView`] before each `run_layout` call.
    pub(crate) child_size: Size,
    /// Scroll position set by [`ScrollView`] during compose.
    pub(crate) viewport_pos: Point,
}

impl<W: Widget + ?Sized> AllowRawMut for ContentClip<W> {}

impl<W: Widget + ?Sized> ContentClip<W> {
    fn new(child: NewWidget<W>) -> Self {
        Self {
            child: child.to_pod(),
            child_size: Size::ZERO,
            viewport_pos: Point::ORIGIN,
        }
    }
}

impl<W: Widget + FromDynWidget> ContentClip<W> {
    pub(crate) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + ?Sized> Widget for ContentClip<W> {
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

    fn on_anim_frame(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _interval: u64,
    ) {
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
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
        ctx.compute_length(&mut self.child, len_req.into(), context_size, axis, None)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // size = eff_size (the viewport area excluding scrollbar tracks).
        // child_size = content_size (may be larger than size).
        ctx.run_layout(&mut self.child, self.child_size);
        ctx.set_clip_path(size.to_rect());
        ctx.place_child(&mut self.child, Point::ZERO);
    }

    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        ctx.set_child_scroll_translation(
            &mut self.child,
            Vec2::new(-self.viewport_pos.x, -self.viewport_pos.y),
        );
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
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("ContentClip", id = id.trace())
    }
}

// --- MARK: ScrollView

/// A scrolling viewport with clipping that excludes the scrollbar tracks,
/// so scrollbars are always adjacent to content rather than overlapping it.
pub struct ScrollView<W: Widget + ?Sized> {
    child: WidgetPod<ContentClip<W>>,
    content_size: Size,
    viewport_pos: Point,
    constrain_horizontal: bool,
    constrain_vertical: bool,
    must_fill: bool,
    always_hide_scrollbars: bool,
    scrollbar_h: WidgetPod<VoidScrollBar>,
    scrollbar_h_visible: bool,
    scrollbar_v: WidgetPod<VoidScrollBar>,
    scrollbar_v_visible: bool,
    /// Vertical scrollbar width from last layout (0 when hidden).
    vbar_width: f64,
    /// Horizontal scrollbar height from last layout (0 when hidden).
    hbar_height: f64,
    nanos_since_last_pointer_move: Option<u64>,
}

impl<W: Widget + ?Sized> ScrollView<W> {
    pub fn new(child: NewWidget<W>) -> Self {
        Self {
            child: WidgetPod::new(ContentClip::new(child)),
            content_size: Size::ZERO,
            viewport_pos: Point::ORIGIN,
            constrain_horizontal: false,
            constrain_vertical: false,
            must_fill: false,
            always_hide_scrollbars: false,
            scrollbar_h: WidgetPod::new(VoidScrollBar::new(Axis::Horizontal)),
            scrollbar_h_visible: false,
            scrollbar_v: WidgetPod::new(VoidScrollBar::new(Axis::Vertical)),
            scrollbar_v_visible: false,
            vbar_width: 0.0,
            hbar_height: 0.0,
            nanos_since_last_pointer_move: None,
        }
    }

    pub fn constrain_horizontal(mut self, v: bool) -> Self {
        self.constrain_horizontal = v;
        self
    }

    pub fn constrain_vertical(mut self, v: bool) -> Self {
        self.constrain_vertical = v;
        self
    }

    pub fn content_must_fill(mut self, v: bool) -> Self {
        self.must_fill = v;
        self
    }

    pub fn always_hide_scrollbars(mut self, v: bool) -> Self {
        self.always_hide_scrollbars = v;
        self
    }
}

impl<W: Widget + ?Sized> ScrollView<W> {
    /// Returns a `WidgetMut` for the [`ContentClip`] wrapper.
    /// Call [`ContentClip::child_mut`] on the result to reach the user content.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, ContentClip<W>> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    pub fn set_constrain_horizontal(this: &mut WidgetMut<'_, Self>, v: bool) {
        this.widget.constrain_horizontal = v;
        this.ctx.request_layout();
    }

    pub fn set_constrain_vertical(this: &mut WidgetMut<'_, Self>, v: bool) {
        this.widget.constrain_vertical = v;
        this.ctx.request_layout();
    }

    pub fn set_content_must_fill(this: &mut WidgetMut<'_, Self>, v: bool) {
        this.widget.must_fill = v;
        this.ctx.request_layout();
    }

    pub fn set_always_hide_scrollbars(this: &mut WidgetMut<'_, Self>, v: bool) {
        this.widget.always_hide_scrollbars = v;
        this.ctx.request_layout();
    }
}

// Helpers
impl<W: Widget + ?Sized> ScrollView<W> {
    /// Effective viewport size (content area, excluding scrollbar tracks).
    fn effective_size(&self, size: Size) -> Size {
        Size::new(size.width - self.vbar_width, size.height - self.hbar_height)
    }

    fn scroll_range(eff_size: Size, content_size: Size) -> Size {
        (content_size - eff_size).max(Size::ZERO)
    }

    fn set_viewport_pos_raw(&mut self, eff_size: Size, content_size: Size, pos: Point) -> bool {
        let max = Self::scroll_range(eff_size, content_size);
        let clamped = Point::new(pos.x.clamp(0.0, max.width), pos.y.clamp(0.0, max.height));
        if (clamped - self.viewport_pos).hypot2() > 1e-12 {
            self.viewport_pos = clamped;
            true
        } else {
            false
        }
    }

    fn update_scrollbar_progress(&mut self, ctx: &mut EventCtx<'_>, eff_size: Size) {
        let range = Self::scroll_range(eff_size, self.content_size);

        let px = if range.width > 1e-12 {
            self.viewport_pos.x / range.width
        } else {
            0.0
        };
        let py = if range.height > 1e-12 {
            self.viewport_pos.y / range.height
        } else {
            0.0
        };

        {
            let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_h);
            sb.cursor_progress = px.clamp(0.0, 1.0);
            sb_ctx.request_render();
        }
        {
            let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_v);
            sb.cursor_progress = py.clamp(0.0, 1.0);
            sb_ctx.request_render();
        }
    }

    fn set_viewport_pos_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        eff_size: Size,
        pos: Point,
    ) -> bool {
        let changed = self.set_viewport_pos_raw(eff_size, self.content_size, pos);
        if changed {
            ctx.request_compose();
            self.update_scrollbar_progress(ctx, eff_size);
        }
        changed
    }

    fn pan_by(&mut self, ctx: &mut EventCtx<'_>, eff_size: Size, mut delta: Vec2) -> bool {
        if self.constrain_horizontal {
            delta.x = 0.0;
        }
        if self.constrain_vertical {
            delta.y = 0.0;
        }
        if delta == Vec2::ZERO {
            return false;
        }
        self.set_viewport_pos_event(ctx, eff_size, self.viewport_pos + delta)
    }

    fn sync_from_scrollbars(&mut self, ctx: &mut EventCtx<'_>, eff_size: Size) -> bool {
        let range = Self::scroll_range(eff_size, self.content_size);
        let mut changed = false;

        {
            let (sb, _) = ctx.get_raw_mut(&mut self.scrollbar_h);
            if sb.moved {
                sb.moved = false;
                let x = sb.cursor_progress * range.width;
                changed |= self.set_viewport_pos_raw(
                    eff_size,
                    self.content_size,
                    Point::new(x, self.viewport_pos.y),
                );
            }
        }
        {
            let (sb, _) = ctx.get_raw_mut(&mut self.scrollbar_v);
            if sb.moved {
                sb.moved = false;
                let y = sb.cursor_progress * range.height;
                changed |= self.set_viewport_pos_raw(
                    eff_size,
                    self.content_size,
                    Point::new(self.viewport_pos.x, y),
                );
            }
        }

        if changed {
            ctx.request_compose();
            self.update_scrollbar_progress(ctx, eff_size);
        }
        changed
    }
}

impl<W: Widget> UsesProperty<AutoHideScrollBar> for ScrollView<W> {}

const VISIBILITY_TIMEOUT_NANOS: u64 = 400_000_000;

// --- MARK: IMPL WIDGET
impl<W: Widget + ?Sized> Widget for ScrollView<W> {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let cache = ctx.property_cache();
        let auto_hide = props.get::<AutoHideScrollBar>(cache).0;
        let size = ctx.content_box_size();
        let eff_size = self.effective_size(size);

        match event {
            PointerEvent::Scroll(PointerScrollEvent { delta, .. }) => {
                let scale = ctx.get_scale_factor();
                let line = PhysicalPosition {
                    x: 120.0 * scale,
                    y: 120.0 * scale,
                };
                let page = PhysicalPosition {
                    x: eff_size.width * scale,
                    y: eff_size.height * scale,
                };
                let dp = delta.to_pixel_delta(line, page);
                let LogicalPosition { x, y } = dp.to_logical::<f64>(scale);
                if self.pan_by(ctx, eff_size, -Vec2 { x, y }) {
                    ctx.set_handled();
                }
            }
            PointerEvent::Move(_) if auto_hide => {
                ctx.mutate_child_later(&mut self.scrollbar_h, |mut bar| {
                    bar.widget.target_opacity = 1.0;
                    bar.ctx.request_anim_frame();
                });
                ctx.mutate_child_later(&mut self.scrollbar_v, |mut bar| {
                    bar.widget.target_opacity = 1.0;
                    bar.ctx.request_anim_frame();
                });
                self.nanos_since_last_pointer_move = Some(0);
                ctx.request_anim_frame();
            }
            _ => {}
        }

        if self.sync_from_scrollbars(ctx, eff_size) {
            ctx.set_handled();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let size = ctx.content_box_size();
        let eff_size = self.effective_size(size);
        let target = ctx.target();
        let scrollbar_target = target == self.scrollbar_v.id() || target == self.scrollbar_h.id();

        if let TextEvent::Keyboard(event) = event
            && event.state.is_down()
            && !scrollbar_target
        {
            let line = 120.0;
            let page_y = eff_size.height;

            use masonry::core::keyboard::{Key, NamedKey};
            let mut did_scroll = false;
            match &event.key {
                Key::Named(NamedKey::PageDown) => {
                    did_scroll |= self.pan_by(ctx, eff_size, Vec2::new(0.0, page_y));
                }
                Key::Named(NamedKey::PageUp) => {
                    did_scroll |= self.pan_by(ctx, eff_size, Vec2::new(0.0, -page_y));
                }
                Key::Named(NamedKey::ArrowDown) => {
                    did_scroll |= self.pan_by(ctx, eff_size, Vec2::new(0.0, line));
                }
                Key::Named(NamedKey::ArrowUp) => {
                    did_scroll |= self.pan_by(ctx, eff_size, Vec2::new(0.0, -line));
                }
                Key::Named(NamedKey::ArrowRight) => {
                    did_scroll |= self.pan_by(ctx, eff_size, Vec2::new(line, 0.0));
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    did_scroll |= self.pan_by(ctx, eff_size, Vec2::new(-line, 0.0));
                }
                Key::Named(NamedKey::Home) => {
                    did_scroll |= self.set_viewport_pos_event(ctx, eff_size, Point::ORIGIN);
                }
                Key::Named(NamedKey::End) => {
                    let range = Self::scroll_range(eff_size, self.content_size);
                    did_scroll |= self.set_viewport_pos_event(
                        ctx,
                        eff_size,
                        Point::new(range.width, range.height),
                    );
                }
                _ => {}
            }
            if did_scroll {
                ctx.set_handled();
            }
        }

        if self.sync_from_scrollbars(ctx, eff_size) {
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        let size = ctx.content_box_size();
        let eff_size = self.effective_size(size);
        let target = ctx.target();
        let scrollbar_target = target == self.scrollbar_v.id() || target == self.scrollbar_h.id();

        if !scrollbar_target
            && matches!(
                event.action,
                accesskit::Action::ScrollUp
                    | accesskit::Action::ScrollDown
                    | accesskit::Action::ScrollLeft
                    | accesskit::Action::ScrollRight
            )
        {
            let unit = if let Some(accesskit::ActionData::ScrollUnit(u)) = &event.data {
                *u
            } else {
                accesskit::ScrollUnit::Item
            };
            let line = 120.0;
            let amount = match unit {
                accesskit::ScrollUnit::Item => line,
                accesskit::ScrollUnit::Page => match event.action {
                    accesskit::Action::ScrollLeft | accesskit::Action::ScrollRight => {
                        eff_size.width
                    }
                    _ => eff_size.height,
                },
            };
            let delta = match event.action {
                accesskit::Action::ScrollUp => Vec2::new(0.0, -amount),
                accesskit::Action::ScrollDown => Vec2::new(0.0, amount),
                accesskit::Action::ScrollLeft => Vec2::new(-amount, 0.0),
                accesskit::Action::ScrollRight => Vec2::new(amount, 0.0),
                _ => Vec2::ZERO,
            };
            if self.pan_by(ctx, eff_size, delta) {
                ctx.set_handled();
            }
        }

        if self.sync_from_scrollbars(ctx, eff_size) {
            ctx.set_handled();
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let cache = ctx.property_cache();
        let auto_hide = props.get::<AutoHideScrollBar>(cache).0;

        match self.nanos_since_last_pointer_move.take() {
            None => {
                let target = if auto_hide { 0.0 } else { 1.0 };
                ctx.mutate_child_later(&mut self.scrollbar_h, move |mut bar| {
                    bar.widget.target_opacity = target;
                    bar.ctx.request_anim_frame();
                });
                ctx.mutate_child_later(&mut self.scrollbar_v, move |mut bar| {
                    bar.widget.target_opacity = target;
                    bar.ctx.request_anim_frame();
                });
            }
            Some(mut since) if auto_hide => {
                since += interval;
                if since >= VISIBILITY_TIMEOUT_NANOS {
                    ctx.mutate_child_later(&mut self.scrollbar_h, |mut bar| {
                        bar.widget.target_opacity = 0.0;
                        bar.ctx.request_anim_frame();
                    });
                    ctx.mutate_child_later(&mut self.scrollbar_v, |mut bar| {
                        bar.widget.target_opacity = 0.0;
                        bar.ctx.request_anim_frame();
                    });
                } else {
                    self.nanos_since_last_pointer_move = Some(since);
                    ctx.request_anim_frame();
                }
            }
            Some(since) => self.nanos_since_last_pointer_move = Some(since),
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
        ctx.register_child(&mut self.scrollbar_h);
        ctx.register_child(&mut self.scrollbar_v);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::RequestPanToChild(target) = event {
            let size = ctx.content_box_size();
            let eff_size = self.effective_size(size);
            let viewport = kurbo::Rect::from_origin_size(self.viewport_pos, eff_size);

            let new_x =
                compute_pan_range(viewport.min_x()..viewport.max_x(), target.min_x()..target.max_x())
                    .start;
            let new_y =
                compute_pan_range(viewport.min_y()..viewport.max_y(), target.min_y()..target.max_y())
                    .start;

            self.set_viewport_pos_raw(eff_size, self.content_size, Point::new(new_x, new_y));
            ctx.request_compose();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if AutoHideScrollBar::matches(property_type) {
            ctx.request_anim_frame();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::MinContent => Length::ZERO,
            LenReq::MaxContent => {
                let context_size = LayoutSize::maybe(axis.cross(), cross_length);
                let auto_length = len_req.into();
                let cross = axis.cross();
                let cross_space = cross_length.filter(|_| match cross {
                    Axis::Horizontal => self.constrain_horizontal,
                    Axis::Vertical => self.constrain_vertical,
                });
                ctx.compute_length(
                    &mut self.child,
                    auto_length,
                    context_size,
                    axis,
                    cross_space,
                )
            }
            LenReq::FitContent(space) => space,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let track = theme::SCROLLBAR_WIDTH + theme::SCROLLBAR_PAD * 2.0;

        // First layout pass — content at full/constrained viewport
        let auto_size = SizeDef::new(
            match self.constrain_horizontal {
                true => LenDef::FitContent(size.width.px()),
                false => LenDef::MaxContent,
            },
            match self.constrain_vertical {
                true => LenDef::FitContent(size.height.px()),
                false => LenDef::MaxContent,
            },
        );
        if self.always_hide_scrollbars {
            let content_size = {
                let cs = ctx.compute_size(&mut self.child, auto_size, size.into());
                if self.must_fill { cs.max(size) } else { cs }
            };
            {
                let (clip, _) = ctx.get_raw_mut(&mut self.child);
                clip.child_size = content_size;
            }
            ctx.run_layout(&mut self.child, size);
            self.content_size = content_size;
            self.vbar_width = 0.0;
            self.hbar_height = 0.0;
            self.scrollbar_v_visible = false;
            self.scrollbar_h_visible = false;
            self.set_viewport_pos_raw(size, content_size, self.viewport_pos);
            ctx.set_clip_path(size.to_rect());
            ctx.place_child(&mut self.child, Point::ZERO);
            ctx.set_stashed(&mut self.scrollbar_v, true);
            ctx.set_stashed(&mut self.scrollbar_h, true);
            return;
        }

        let content_size = {
            let cs = ctx.compute_size(&mut self.child, auto_size, size.into());
            if self.must_fill { cs.max(size) } else { cs }
        };

        // Determine scrollbar visibility (cascade: vbar may force hbar and vice versa)
        let vbar = !self.constrain_vertical && content_size.height > size.height;
        let eff_w_if_vbar = if vbar { size.width - track } else { size.width };
        let hbar = !self.constrain_horizontal && content_size.width > eff_w_if_vbar;
        let eff_h_if_hbar = if hbar {
            size.height - track
        } else {
            size.height
        };
        // Re-check vbar if hbar just appeared and reduced height
        let vbar = vbar || (!self.constrain_vertical && content_size.height > eff_h_if_hbar);

        let vbar_w = if vbar { track } else { 0.0 };
        let hbar_h = if hbar { track } else { 0.0 };
        let eff_w = size.width - vbar_w;
        let eff_h = size.height - hbar_h;
        let eff_size = Size::new(eff_w, eff_h);

        // Second layout pass — re-layout content if a constrained axis got narrower
        let content_size =
            if (self.constrain_horizontal && vbar) || (self.constrain_vertical && hbar) {
                let auto2 = SizeDef::new(
                    match self.constrain_horizontal {
                        true => LenDef::FitContent(eff_w.px()),
                        false => LenDef::MaxContent,
                    },
                    match self.constrain_vertical {
                        true => LenDef::FitContent(eff_h.px()),
                        false => LenDef::MaxContent,
                    },
                );
                let cs = ctx.compute_size(&mut self.child, auto2, size.into());
                if self.must_fill { cs.max(eff_size) } else { cs }
            } else {
                content_size
            };

        // Give ContentClip the content size, then lay it out to eff_size.
        // ContentClip.layout clips itself to eff_size.to_rect() and lays out
        // the user content to child_size — so content is clipped at the
        // scrollbar boundary while the scrollbars (siblings) remain unclipped.
        {
            let (clip, _) = ctx.get_raw_mut(&mut self.child);
            clip.child_size = content_size;
        }
        ctx.run_layout(&mut self.child, eff_size);
        self.content_size = content_size;
        self.set_viewport_pos_raw(eff_size, content_size, self.viewport_pos);

        ctx.set_clip_path(size.to_rect());
        ctx.place_child(&mut self.child, Point::ZERO);

        self.scrollbar_v_visible = vbar;
        self.scrollbar_h_visible = hbar;
        self.vbar_width = vbar_w;
        self.hbar_height = hbar_h;

        // Layout vertical scrollbar
        ctx.set_stashed(&mut self.scrollbar_v, !vbar);
        if vbar {
            let range_y = (content_size.height - eff_h).max(0.0);
            {
                let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_v);
                sb.portal_size = eff_h;
                sb.content_size = content_size.height;
                sb.cursor_progress = if range_y > 1e-12 {
                    (self.viewport_pos.y / range_y).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                sb_ctx.request_render();
            }
            let sb_size = ctx.compute_size(&mut self.scrollbar_v, SizeDef::fit(size), size.into());
            ctx.run_layout(&mut self.scrollbar_v, sb_size);
            ctx.place_child(
                &mut self.scrollbar_v,
                Point::new(size.width - sb_size.width, 0.0),
            );
        }

        // Layout horizontal scrollbar
        ctx.set_stashed(&mut self.scrollbar_h, !hbar);
        if hbar {
            let range_x = (content_size.width - eff_w).max(0.0);
            {
                let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_h);
                sb.portal_size = eff_w;
                sb.content_size = content_size.width;
                sb.cursor_progress = if range_x > 1e-12 {
                    (self.viewport_pos.x / range_x).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                sb_ctx.request_render();
            }
            let sb_size = ctx.compute_size(&mut self.scrollbar_h, SizeDef::fit(size), size.into());
            ctx.run_layout(&mut self.scrollbar_h, sb_size);
            ctx.place_child(
                &mut self.scrollbar_h,
                Point::new(0.0, size.height - sb_size.height),
            );
        }
    }

    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        // ContentClip.compose applies the scroll translation to the user content.
        // Update its viewport_pos and mark it for compose so its own compose
        // method runs in the same pass and calls set_child_scroll_translation.
        let (clip, mut clip_ctx) = ctx.get_raw_mut(&mut self.child);
        clip.viewport_pos = self.viewport_pos;
        clip_ctx.request_compose();
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::ScrollView
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_clips_children();
        let size = ctx.content_box_size();
        let eff_size = self.effective_size(size);
        let range = Self::scroll_range(eff_size, self.content_size);

        if !self.constrain_horizontal && range.width > 1e-12 {
            node.set_scroll_x_min(0.0);
            node.set_scroll_x_max(range.width);
            node.set_scroll_x(self.viewport_pos.x);
            if self.viewport_pos.x > 1e-12 {
                node.add_action(accesskit::Action::ScrollLeft);
            }
            if self.viewport_pos.x + 1e-12 < range.width {
                node.add_action(accesskit::Action::ScrollRight);
            }
        }
        if !self.constrain_vertical && range.height > 1e-12 {
            node.set_scroll_y_min(0.0);
            node.set_scroll_y_max(range.height);
            node.set_scroll_y(self.viewport_pos.y);
            if self.viewport_pos.y > 1e-12 {
                node.add_action(accesskit::Action::ScrollUp);
            }
            if self.viewport_pos.y + 1e-12 < range.height {
                node.add_action(accesskit::Action::ScrollDown);
            }
        }
        node.add_child_action(accesskit::Action::ScrollIntoView);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[
            self.child.id(),
            self.scrollbar_v.id(),
            self.scrollbar_h.id(),
        ])
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("ScrollView", id = id.trace())
    }

    fn accepts_focus(&self) -> bool {
        !(self.constrain_horizontal && self.constrain_vertical)
    }
}
