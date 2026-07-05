//! Custom scroll container widget with a clip rect that excludes scrollbar tracks,
//! so scrollbars never render over content.

use std::any::TypeId;
use std::ops::Range;

use masonry::accesskit::{self, Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
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
        // Fade-out duration for the scrollbar thumb — an animation timing
        // value, not spacing, so it doesn't scale with density.
        const FADE_MILLIS: f32 = 300.0;
        // Quantize the frame interval to whole microseconds: a u16 µs count
        // converts to f32 losslessly, and a >65 ms frame just clamps the step.
        let interval_us = u16::try_from(interval / 1_000).unwrap_or(u16::MAX);
        let delta = (f32::from(interval_us) / 1_000.0) / FADE_MILLIS;
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
pub struct ContentClip<W: Widget + ?Sized> {
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
    /// Returns a `WidgetMut` for the wrapped content widget.
    ///
    /// This is the second hop of the documented path from a scroll view to
    /// its content: [`ScrollView::child_mut`] then `ContentClip::child_mut`.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + ?Sized> Widget for ContentClip<W> {
    type Action = NoAction;

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
        ctx.compute_length(
            &mut self.child,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        )
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

/// Per-axis constraint flags: a constrained axis sizes content to the
/// viewport instead of scrolling.
#[derive(Clone, Copy, Default)]
struct ConstrainAxes {
    horizontal: bool,
    vertical: bool,
}

/// A scrolling viewport with clipping that excludes the scrollbar tracks,
/// so scrollbars are always adjacent to content rather than overlapping it.
pub struct ScrollView<W: Widget + ?Sized> {
    child: WidgetPod<ContentClip<W>>,
    content_size: Size,
    viewport_pos: Point,
    constrain: ConstrainAxes,
    must_fill: bool,
    always_hide_scrollbars: bool,
    scrollbar_h: WidgetPod<VoidScrollBar>,
    scrollbar_v: WidgetPod<VoidScrollBar>,
    /// Vertical scrollbar width from last layout (0 when hidden).
    vbar_width: f64,
    /// Horizontal scrollbar height from last layout (0 when hidden).
    hbar_height: f64,
    nanos_since_last_pointer_move: Option<u64>,
}

impl<W: Widget + ?Sized> ScrollView<W> {
    #[must_use]
    pub fn new(child: NewWidget<W>) -> Self {
        Self {
            child: WidgetPod::new(ContentClip::new(child)),
            content_size: Size::ZERO,
            viewport_pos: Point::ORIGIN,
            constrain: ConstrainAxes::default(),
            must_fill: false,
            always_hide_scrollbars: false,
            scrollbar_h: WidgetPod::new(VoidScrollBar::new(Axis::Horizontal)),
            scrollbar_v: WidgetPod::new(VoidScrollBar::new(Axis::Vertical)),
            vbar_width: 0.0,
            hbar_height: 0.0,
            nanos_since_last_pointer_move: None,
        }
    }

    #[must_use]
    pub fn constrain_horizontal(mut self, v: bool) -> Self {
        self.constrain.horizontal = v;
        self
    }

    #[must_use]
    pub fn constrain_vertical(mut self, v: bool) -> Self {
        self.constrain.vertical = v;
        self
    }

    #[must_use]
    pub fn content_must_fill(mut self, v: bool) -> Self {
        self.must_fill = v;
        self
    }

    #[must_use]
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
        this.widget.constrain.horizontal = v;
        this.ctx.request_layout();
    }

    pub fn set_constrain_vertical(this: &mut WidgetMut<'_, Self>, v: bool) {
        this.widget.constrain.vertical = v;
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

    /// Resets the scroll position to the top-left corner. Useful when content
    /// is replaced wholesale (e.g. a filtered list) and any prior scroll
    /// offset no longer makes sense.
    pub fn scroll_to_origin(this: &mut WidgetMut<'_, Self>) {
        if this.widget.viewport_pos != Point::ORIGIN {
            this.widget.viewport_pos = Point::ORIGIN;
            this.ctx.request_compose();
        }
        // Always resync the scrollbar thumbs, even if the viewport was
        // already at the origin: `layout`'s own re-clamp
        // (`set_viewport_pos_raw`, run every pass to keep `viewport_pos` in
        // bounds as `content_size` changes) can silently reset
        // `viewport_pos` back to the origin on a content swap without ever
        // touching `cursor_progress` — leaving a thumb that was previously
        // scrolled down visually stuck there even though there's nothing
        // left to scroll. Bailing out here whenever `viewport_pos` already
        // happened to be at the origin would skip fixing exactly that case.
        let eff_size = this.widget.effective_size(this.ctx.border_box_size());
        let (px, py) = this.widget.scrollbar_progress(eff_size);
        {
            let (sb, mut sb_ctx) = this.ctx.get_raw_mut(&mut this.widget.scrollbar_h);
            sb.cursor_progress = px;
            sb_ctx.request_render();
        }
        {
            let (sb, mut sb_ctx) = this.ctx.get_raw_mut(&mut this.widget.scrollbar_v);
            sb.cursor_progress = py;
            sb_ctx.request_render();
        }
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

    /// Returns `(px, py)` — each axis's scroll progress in `[0, 1]`.
    fn scrollbar_progress(&self, eff_size: Size) -> (f64, f64) {
        let range = Self::scroll_range(eff_size, self.content_size);
        let px = if range.width > 1e-12 {
            (self.viewport_pos.x / range.width).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let py = if range.height > 1e-12 {
            (self.viewport_pos.y / range.height).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (px, py)
    }

    fn update_scrollbar_progress(&mut self, ctx: &mut EventCtx<'_>, eff_size: Size) {
        let (px, py) = self.scrollbar_progress(eff_size);
        {
            let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_h);
            sb.cursor_progress = px;
            sb_ctx.request_render();
        }
        {
            let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_v);
            sb.cursor_progress = py;
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
        if self.constrain.horizontal {
            delta.x = 0.0;
        }
        if self.constrain.vertical {
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

// --- MARK: Layout helpers
impl<W: Widget + ?Sized> ScrollView<W> {
    /// `SizeDef` for measuring content: a constrained axis fits the given
    /// viewport length, an unconstrained axis takes its max-content size.
    fn content_size_def(&self, viewport: Size) -> SizeDef {
        let horizontal = if self.constrain.horizontal {
            LenDef::FitContent(viewport.width.px())
        } else {
            LenDef::MaxContent
        };
        let vertical = if self.constrain.vertical {
            LenDef::FitContent(viewport.height.px())
        } else {
            LenDef::MaxContent
        };
        SizeDef::new(horizontal, vertical)
    }

    /// Layout with scrollbars disabled (always-hidden mode): content gets the
    /// full viewport and both bars are stashed.
    fn layout_hidden_bars(&mut self, ctx: &mut LayoutCtx<'_>, size: Size) {
        let auto_size = self.content_size_def(size);
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
        self.set_viewport_pos_raw(size, content_size, self.viewport_pos);
        ctx.set_clip_path(size.to_rect());
        ctx.place_child(&mut self.child, Point::ZERO);
        ctx.set_stashed(&mut self.scrollbar_v, true);
        ctx.set_stashed(&mut self.scrollbar_h, true);
    }

    /// Stashes or lays out one scrollbar along `axis` at the viewport edge.
    ///
    /// Reads `self.content_size` and `self.viewport_pos`, so the caller must
    /// finish content layout first.
    fn layout_scrollbar(
        &mut self,
        ctx: &mut LayoutCtx<'_>,
        axis: Axis,
        size: Size,
        eff_size: Size,
        visible: bool,
    ) {
        let (pod, portal_len, content_len, viewport_offset) = match axis {
            Axis::Vertical => (
                &mut self.scrollbar_v,
                eff_size.height,
                self.content_size.height,
                self.viewport_pos.y,
            ),
            Axis::Horizontal => (
                &mut self.scrollbar_h,
                eff_size.width,
                self.content_size.width,
                self.viewport_pos.x,
            ),
        };
        ctx.set_stashed(pod, !visible);
        if !visible {
            return;
        }
        let range = (content_len - portal_len).max(0.0);
        {
            let (sb, mut sb_ctx) = ctx.get_raw_mut(pod);
            sb.portal_size = portal_len;
            sb.content_size = content_len;
            sb.cursor_progress = if range > 1e-12 {
                (viewport_offset / range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            sb_ctx.request_render();
        }
        let sb_size = ctx.compute_size(pod, SizeDef::fit(eff_size), eff_size.into());
        ctx.run_layout(pod, sb_size);
        let origin = match axis {
            Axis::Vertical => Point::new(size.width - sb_size.width, 0.0),
            Axis::Horizontal => Point::new(0.0, size.height - sb_size.height),
        };
        ctx.place_child(pod, origin);
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
                let scale = ctx.scale_factor();
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

            let new_x = compute_pan_range(
                viewport.min_x()..viewport.max_x(),
                target.min_x()..target.max_x(),
            )
            .start;
            let new_y = compute_pan_range(
                viewport.min_y()..viewport.max_y(),
                target.min_y()..target.max_y(),
            )
            .start;

            if self.set_viewport_pos_raw(eff_size, self.content_size, Point::new(new_x, new_y)) {
                ctx.request_compose();
                let (px, py) = self.scrollbar_progress(eff_size);
                {
                    let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_h);
                    sb.cursor_progress = px;
                    sb_ctx.request_render();
                }
                {
                    let (sb, mut sb_ctx) = ctx.get_raw_mut(&mut self.scrollbar_v);
                    sb.cursor_progress = py;
                    sb_ctx.request_render();
                }
            }
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
                    Axis::Horizontal => self.constrain.horizontal,
                    Axis::Vertical => self.constrain.vertical,
                });
                let child_length = ctx.compute_length(
                    &mut self.child,
                    auto_length,
                    context_size,
                    axis,
                    cross_space,
                );
                if axis == Axis::Horizontal && !self.constrain.vertical {
                    let track = theme::SCROLLBAR_WIDTH + theme::SCROLLBAR_PAD * 2.0;
                    Length::px(child_length.get() + track)
                } else {
                    child_length
                }
            }
            LenReq::FitContent(space) => space,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let track = theme::SCROLLBAR_WIDTH + theme::SCROLLBAR_PAD * 2.0;

        if self.always_hide_scrollbars {
            self.layout_hidden_bars(ctx, size);
            return;
        }

        // First layout pass — content at full/constrained viewport
        let auto_size = self.content_size_def(size);
        let content_size = {
            let cs = ctx.compute_size(&mut self.child, auto_size, size.into());
            if self.must_fill { cs.max(size) } else { cs }
        };

        // Determine scrollbar visibility (cascade: vbar may force hbar and vice versa)
        let vbar = !self.constrain.vertical && content_size.height > size.height;
        let eff_w_if_vbar = if vbar { size.width - track } else { size.width };
        let hbar = !self.constrain.horizontal && content_size.width > eff_w_if_vbar;
        let eff_h_if_hbar = if hbar {
            size.height - track
        } else {
            size.height
        };
        // Re-check vbar if hbar just appeared and reduced height
        let vbar = vbar || (!self.constrain.vertical && content_size.height > eff_h_if_hbar);

        let vbar_w = if vbar { track } else { 0.0 };
        let hbar_h = if hbar { track } else { 0.0 };
        let eff_size = Size::new(size.width - vbar_w, size.height - hbar_h);

        // Second layout pass — re-layout content if a constrained axis got narrower
        let content_size =
            if (self.constrain.horizontal && vbar) || (self.constrain.vertical && hbar) {
                let auto2 = self.content_size_def(eff_size);
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

        self.vbar_width = vbar_w;
        self.hbar_height = hbar_h;

        self.layout_scrollbar(ctx, Axis::Vertical, size, eff_size, vbar);
        self.layout_scrollbar(ctx, Axis::Horizontal, size, eff_size, hbar);
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

        if !self.constrain.horizontal && range.width > 1e-12 {
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
        if !self.constrain.vertical && range.height > 1e-12 {
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
        !(self.constrain.horizontal && self.constrain.vertical)
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::kurbo::{Axis, Point, Size};
    use masonry::layout::Length;
    use masonry::testing::TestHarness;
    use masonry::widgets::SizedBox;

    use super::{ScrollView, VoidScrollBar, compute_pan_range};

    /// Builds a `ScrollView` whose content is much taller than the 100x100
    /// viewport, so it starts out vertically scrollable.
    fn tall_content_harness() -> TestHarness<ScrollView<SizedBox>> {
        let content = SizedBox::empty().size(Length::px(50.0), Length::px(1000.0));
        let view = ScrollView::new(NewWidget::new(content));
        TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(view),
            (100, 100),
        )
    }

    // --- scroll_to_origin ---

    /// `scroll_to_origin` must resync the scrollbar thumb even when
    /// `viewport_pos` is already at the origin: `layout`'s own re-clamp
    /// (`set_viewport_pos_raw`, run every pass to keep `viewport_pos` in
    /// bounds as `content_size` changes) can silently reset `viewport_pos`
    /// back to the origin without ever touching `cursor_progress` — e.g. a
    /// content swap that makes everything fit reclamps the position but
    /// leaves a thumb that was previously scrolled down still reporting its
    /// old progress. Reproduced here by setting up exactly that
    /// inconsistent state directly, then checking `scroll_to_origin` still
    /// corrects it.
    #[test]
    fn scroll_to_origin_resyncs_thumb_even_when_viewport_already_at_origin() {
        let mut harness = tall_content_harness();

        harness.edit_root_widget(|mut root| {
            // Simulate the aftermath of a silent layout-time reclamp: the
            // viewport is already back at the origin, but the thumb is
            // stale, still reporting a scrolled-down position.
            root.widget.viewport_pos = Point::ORIGIN;
            let sb = root.ctx.get_mut(&mut root.widget.scrollbar_v);
            sb.widget.cursor_progress = 0.8;
        });

        harness.edit_root_widget(|mut root| {
            ScrollView::scroll_to_origin(&mut root);
        });

        let progress = harness.edit_root_widget(|mut root| {
            let sb = root.ctx.get_mut(&mut root.widget.scrollbar_v);
            sb.widget.cursor_progress
        });
        assert!(
            progress.abs() < 1e-9,
            "scroll_to_origin should resync the stale thumb to match the \
             (already-origin) viewport, not skip the sync because the \
             viewport didn't need to move (progress={progress})"
        );
    }

    // --- compute_pan_range ---

    #[test]
    fn pan_range_no_movement_when_target_inside_viewport() {
        let result = compute_pan_range(10.0..50.0, 20.0..30.0);
        assert_eq!(result, 10.0..50.0);
    }

    #[test]
    fn pan_range_scrolls_forward_when_target_is_past_viewport_end() {
        // viewport 0..50, target 40..60 → start = (60-50).max(0) = 10
        let result = compute_pan_range(0.0..50.0, 40.0..60.0);
        assert_eq!(result, 10.0..60.0);
    }

    #[test]
    fn pan_range_scrolls_back_when_target_is_before_viewport_start() {
        // viewport 30..80, target 10..20 → start = target.start = 10
        let result = compute_pan_range(30.0..80.0, 10.0..20.0);
        assert_eq!(result, 10.0..60.0);
    }

    #[test]
    fn pan_range_aligns_to_target_start_when_target_exceeds_viewport_length() {
        // viewport 0..50, target 10..80 → target.end (80) > viewport.end, so
        // start = (80-50).max(0) = 30; the full target cannot fit, so we show
        // as much as possible from the start.
        let result = compute_pan_range(0.0..50.0, 10.0..80.0);
        assert_eq!(result, 30.0..80.0);
    }

    #[test]
    fn pan_range_preserves_viewport_length() {
        let vp = 10.0..90.0;
        let len = vp.end - vp.start;
        for target in [5.0..15.0, 40.0..60.0, 85.0..120.0] {
            let result = compute_pan_range(vp.clone(), target);
            assert!(
                (result.end - result.start - len).abs() < 1e-10,
                "viewport length changed: {result:?}"
            );
        }
    }

    // --- ScrollView::scroll_range ---

    #[test]
    fn scroll_range_is_zero_when_content_fits_viewport() {
        let range = ScrollView::<VoidScrollBar>::scroll_range(
            Size::new(200.0, 300.0),
            Size::new(100.0, 150.0),
        );
        assert_eq!(range, Size::ZERO);
    }

    #[test]
    fn scroll_range_is_zero_when_content_exactly_fills_viewport() {
        let range = ScrollView::<VoidScrollBar>::scroll_range(
            Size::new(200.0, 300.0),
            Size::new(200.0, 300.0),
        );
        assert_eq!(range, Size::ZERO);
    }

    #[test]
    fn scroll_range_returns_overflow_when_content_exceeds_viewport() {
        let range = ScrollView::<VoidScrollBar>::scroll_range(
            Size::new(100.0, 200.0),
            Size::new(350.0, 500.0),
        );
        assert_eq!(range, Size::new(250.0, 300.0));
    }

    #[test]
    fn scroll_range_clamps_independent_axes() {
        // width fits but height overflows
        let range = ScrollView::<VoidScrollBar>::scroll_range(
            Size::new(400.0, 200.0),
            Size::new(300.0, 500.0),
        );
        assert_eq!(range, Size::new(0.0, 300.0));
    }

    // --- VoidScrollBar ---

    #[test]
    fn scrollbar_scroll_range_is_content_minus_portal() {
        let mut bar = VoidScrollBar::new(Axis::Vertical);
        bar.content_size = 500.0;
        bar.portal_size = 200.0;
        assert!((bar.scroll_range() - 300.0).abs() < 1e-12);
    }

    #[test]
    fn scrollbar_scroll_range_is_zero_when_content_fits() {
        let mut bar = VoidScrollBar::new(Axis::Vertical);
        bar.content_size = 100.0;
        bar.portal_size = 200.0;
        assert!(bar.scroll_range() < 1e-12);
    }

    #[test]
    fn scrollbar_set_cursor_progress_clamps_to_unit_interval() {
        let mut bar = VoidScrollBar::new(Axis::Horizontal);
        bar.set_cursor_progress(-0.5);
        assert!(bar.cursor_progress.abs() < 1e-12);
        bar.set_cursor_progress(1.5);
        assert!((bar.cursor_progress - 1.0).abs() < 1e-12);
    }

    #[test]
    fn scrollbar_set_cursor_progress_returns_true_only_on_change() {
        let mut bar = VoidScrollBar::new(Axis::Horizontal);
        assert!(bar.set_cursor_progress(0.5));
        assert!(!bar.set_cursor_progress(0.5));
        assert!(bar.set_cursor_progress(0.6));
    }
}
