//! Masonry widget for the multi-pane resizable split.
//!
//! [`ResizableWidget`] places `N` child widgets side-by-side (or top-to-bottom)
//! separated by `N - 1` thin drag handles. Dragging a handle redistributes
//! space between the two panels adjacent to it — every other panel keeps its
//! size — and emits [`ResizeHandleDragged`] with the dragged handle's index
//! and the full updated per-panel ratio vector.
//!
//! The grab zone around each handle is wider than the visual line so small
//! handles remain easy to hit. Exactly one handle is keyboard-focusable at a
//! time (the *active* handle, highlighted with a focus ring): arrow keys along
//! the split axis (Left/Right for a horizontal split, Up/Down for a vertical
//! one) nudge it in pixel-sized steps, same as a mouse drag, and hold Shift
//! for finer-grained nudging. The **orthogonal** arrow keys — otherwise unused
//! by this widget — cycle which handle is active, so every divider remains
//! keyboard-reachable without intercepting Tab or colliding with the nudge
//! keys. The accessibility node mirrors the active handle (masonry widgets
//! expose a single accessibility node, so `N - 1` independent `Splitter` nodes
//! aren't representable from one widget).

use std::any::TypeId;

use masonry::accesskit::{Action, ActionData, Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef,
    QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::peniko::Color;
use masonry::widgets::Passthrough;

use crate::Theme;

// --- MARK: CONSTANTS

/// Thickness of the visual divider line at rest — matches [`Separator`](crate::Separator)'s
/// default 1px line so a resizable divider reads the same as a static one.
const HANDLE_THICKNESS: f64 = 1.0;
/// Half-width of the invisible grab region on each side of a handle's center.
const GRAB_HALF: f64 = 8.0;
/// Structural minimum panel size in pixels, used as a collapse-prevention floor;
/// callers may set per-panel minimums via [`ResizableWidget::set_min_sizes`]/
/// [`ResizableWidget::set_max_sizes`], which are layered on top of this floor.
pub const MIN_PANEL_SIZE: f64 = 40.0;
/// Pixel adjustment to a panel pair's split point per arrow-key press, for
/// keyboard nudging once a handle is focused.
const ARROW_NUDGE_PX: f64 = 16.0;
/// Pixel adjustment per arrow-key press while Shift is held, for finer-grained
/// keyboard nudging.
const ARROW_NUDGE_FINE_PX: f64 = 1.0;
/// Width of the focus ring stroke drawn around the active handle's grab zone.
const FOCUS_RING_WIDTH: f64 = 1.5;

// --- MARK: ACTION

/// Action emitted on every pointer-move while a resize handle is dragged (and
/// on every keyboard nudge).
///
/// Carries the dragged handle's index and the full updated per-panel ratio
/// vector — each panel's fraction of the usable extent, clamped so its pixel
/// size stays within its own optional min/max range and no panel shrinks below
/// the structural [`MIN_PANEL_SIZE`] floor. Only the dragged handle's adjacent
/// pair of ratios changes; every other entry is unchanged from the input.
#[derive(Debug, Clone)]
pub struct ResizeHandleDragged(pub usize, pub Vec<f32>);

// --- MARK: ResizableWidget

/// Multi-pane split container with `N - 1` draggable dividers.
///
/// Every panel is type-erased to [`Passthrough`] (the [`Pod`](xilem::Pod)
/// element produced by boxing a view as `Box<AnyWidgetView<State, Action>>`),
/// so the widget itself needs no generic parameters regardless of how many
/// panels it holds or what they contain.
pub struct ResizableWidget {
    panels: Vec<WidgetPod<Passthrough>>,
    axis: Axis,
    /// Each panel's fraction of the usable extent (0.0–1.0); `len() == panels.len()`.
    /// Only entries `0..len() - 1` are authoritative for layout — the last
    /// panel always receives the remaining space so the total is exact.
    ratios: Vec<f32>,
    /// Optional lower bound on each panel's pixel size, indexed like `panels`.
    min_sizes: Vec<Option<f64>>,
    /// Optional upper bound on each panel's pixel size, indexed like `panels`.
    max_sizes: Vec<Option<f64>>,
    theme: Theme,
    hovered_handle: Option<usize>,
    dragging_handle: Option<usize>,
    /// Index of the handle that keyboard nudging and the accessibility node
    /// target. Always in `0..panels.len() - 1`.
    active_handle: usize,
    /// Split-axis extent (total widget size) from the last layout pass.
    total_extent: f64,
    /// Cross-axis extent from the last layout pass (needed for paint).
    cross_extent: f64,
    /// Centers of each handle on the split axis from the last layout pass;
    /// `len() == panels.len() - 1`.
    handle_centers: Vec<f64>,
}

// --- MARK: BUILDERS

impl ResizableWidget {
    /// # Panics
    ///
    /// Panics if `panels`, `ratios`, `min_sizes`, and `max_sizes` don't all
    /// have the same length, or if there are fewer than two panels.
    #[must_use]
    pub fn new(
        panels: Vec<NewWidget<Passthrough>>,
        axis: Axis,
        ratios: Vec<f32>,
        min_sizes: Vec<Option<f64>>,
        max_sizes: Vec<Option<f64>>,
        theme: &Theme,
    ) -> Self {
        assert!(
            panels.len() >= 2,
            "ResizableWidget needs at least two panels"
        );
        assert_eq!(panels.len(), ratios.len());
        assert_eq!(panels.len(), min_sizes.len());
        assert_eq!(panels.len(), max_sizes.len());
        Self {
            panels: panels.into_iter().map(NewWidget::to_pod).collect(),
            axis,
            ratios,
            min_sizes,
            max_sizes,
            theme: *theme,
            hovered_handle: None,
            dragging_handle: None,
            active_handle: 0,
            total_extent: 0.0,
            cross_extent: 0.0,
            handle_centers: Vec::new(),
        }
    }
}

// --- MARK: WIDGETMUT

impl ResizableWidget {
    /// Returns a `WidgetMut` for the panel at `index`.
    pub fn panel_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        index: usize,
    ) -> WidgetMut<'t, Passthrough> {
        this.ctx.get_mut(&mut this.widget.panels[index])
    }

    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_ratios(this: &mut WidgetMut<'_, Self>, ratios: Vec<f32>) {
        if this.widget.ratios != ratios {
            this.widget.ratios = ratios;
            this.ctx.request_layout();
        }
    }

    pub fn set_min_sizes(this: &mut WidgetMut<'_, Self>, min_sizes: Vec<Option<f64>>) {
        if this.widget.min_sizes != min_sizes {
            this.widget.min_sizes = min_sizes;
            this.ctx.request_layout();
        }
    }

    pub fn set_max_sizes(this: &mut WidgetMut<'_, Self>, max_sizes: Vec<Option<f64>>) {
        if this.widget.max_sizes != max_sizes {
            this.widget.max_sizes = max_sizes;
            this.ctx.request_layout();
        }
    }

    /// Replaces the entire panel set, e.g. when the panel *count* changes.
    ///
    /// `ratios`/`min_sizes`/`max_sizes` must already match `panels.len()`;
    /// pair this with [`set_ratios`](Self::set_ratios)/[`set_min_sizes`](Self::set_min_sizes)/
    /// [`set_max_sizes`](Self::set_max_sizes) (or pass the new values directly
    /// to the next `rebuild`, which diffs against the post-replacement state).
    ///
    /// # Panics
    ///
    /// Panics if `panels` has fewer than two entries.
    pub fn set_panels(this: &mut WidgetMut<'_, Self>, panels: Vec<NewWidget<Passthrough>>) {
        assert!(
            panels.len() >= 2,
            "ResizableWidget needs at least two panels"
        );
        for old in std::mem::take(&mut this.widget.panels) {
            this.ctx.remove_child(old);
        }
        this.widget.panels = panels.into_iter().map(NewWidget::to_pod).collect();
        this.widget.active_handle = this.widget.active_handle.min(this.widget.panels.len() - 2);
        this.ctx.children_changed();
        this.ctx.request_layout();
    }
}

// --- MARK: HELPERS

impl ResizableWidget {
    fn handle_count(&self) -> usize {
        self.panels.len() - 1
    }

    /// Total pixel extent consumed by dividers between panels.
    ///
    /// Panel counts are always small (never near 2^52), so the `usize -> f64`
    /// cast cannot lose precision in practice.
    #[allow(clippy::cast_precision_loss)]
    fn handles_extent(&self) -> f64 {
        self.handle_count() as f64 * HANDLE_THICKNESS
    }

    fn pos_on_axis(&self, pos: Point) -> f64 {
        match self.axis {
            Axis::Horizontal => pos.x,
            Axis::Vertical => pos.y,
        }
    }

    /// Returns the index of the handle whose grab zone contains `pos`, if any.
    fn handle_at(&self, pos: Point) -> Option<usize> {
        let p = self.pos_on_axis(pos);
        self.handle_centers
            .iter()
            .position(|&center| (p - center).abs() <= GRAB_HALF)
    }

    /// Per-panel pixel extents derived from `ratios`, with the last panel
    /// receiving the remainder so the total is exact regardless of float drift.
    fn panel_extents(&self, usable: f64) -> Vec<f64> {
        let mut extents: Vec<f64> = self.ratios[..self.ratios.len() - 1]
            .iter()
            .map(|&r| usable * f64::from(r))
            .collect();
        let consumed: f64 = extents.iter().sum();
        extents.push((usable - consumed).max(0.0));
        extents
    }

    /// Allowed `[lower, upper]` pixel range for panel `index`'s extent within
    /// `pair_extent` — the combined pixel extent of `index` and `index + 1`.
    ///
    /// Layers the optional, asymmetric per-panel `min_sizes`/`max_sizes`
    /// constraints on top of the structural [`MIN_PANEL_SIZE`] floor, which
    /// always applies to both panels of the pair so neither can be squeezed
    /// out of existence. Constraints on the neighbor's pixel size are
    /// translated into bounds on `index`'s extent via
    /// `neighbor_extent = pair_extent - extent`.
    fn pair_extent_bounds(&self, pair_extent: f64, index: usize) -> (f64, f64) {
        let floor = MIN_PANEL_SIZE.min(pair_extent * 0.5);
        let mut lower = floor;
        let mut upper = pair_extent - floor;

        if let Some(min) = self.min_sizes[index] {
            lower = lower.max(min);
        }
        if let Some(max) = self.max_sizes[index] {
            upper = upper.min(max);
        }
        if let Some(neighbor_max) = self.max_sizes[index + 1] {
            lower = lower.max(pair_extent - neighbor_max);
        }
        if let Some(neighbor_min) = self.min_sizes[index + 1] {
            upper = upper.min(pair_extent - neighbor_min);
        }

        let upper = upper.max(lower).min(pair_extent);
        let lower = lower.min(upper);
        (lower, upper)
    }

    /// The pixel offset (on the split axis) where the pair adjacent to handle
    /// `i` begins, and that pair's combined pixel extent.
    fn pair_region(&self, usable: f64, i: usize) -> (f64, f64) {
        let extents = self.panel_extents(usable);
        #[allow(clippy::cast_precision_loss)]
        let preceding_handles = i as f64 * HANDLE_THICKNESS;
        let start: f64 = extents[..i].iter().sum::<f64>() + preceding_handles;
        let pair_extent = extents[i] + extents[i + 1];
        (start, pair_extent)
    }

    /// Recomputes the full ratio vector that results from setting handle `i`'s
    /// adjacent pair so panel `i` occupies `extent` pixels of their combined
    /// `pair_extent`, clamped to the bounds that constrain dragging/nudging.
    fn ratios_from_pair_extent(
        &self,
        usable: f64,
        i: usize,
        pair_extent: f64,
        extent: f64,
    ) -> Vec<f32> {
        let (lower, upper) = self.pair_extent_bounds(pair_extent, i);
        let extent = extent.clamp(lower, upper);
        let mut ratios = self.ratios.clone();
        #[allow(clippy::cast_possible_truncation)]
        {
            ratios[i] = (extent / usable) as f32;
            ratios[i + 1] = ((pair_extent - extent) / usable) as f32;
        }
        ratios
    }

    /// Maps a cursor position to the full ratio vector resulting from dragging
    /// handle `i` so its divider sits at `pos`.
    fn ratios_from_pos(&self, i: usize, pos: f64) -> Vec<f32> {
        let usable = (self.total_extent - self.handles_extent()).max(1.0);
        let (start, pair_extent) = self.pair_region(usable, i);
        let extent = pos - start - HANDLE_THICKNESS * 0.5;
        self.ratios_from_pair_extent(usable, i, pair_extent, extent)
    }

    /// Computes the ratio vector that results from nudging handle `i`'s
    /// divider by `delta` pixels (negative shrinks panel `i`, positive grows
    /// it), clamped to the same bounds that constrain dragging.
    fn nudge_ratios(&self, i: usize, delta: f64) -> Vec<f32> {
        let usable = (self.total_extent - self.handles_extent()).max(1.0);
        let (_, pair_extent) = self.pair_region(usable, i);
        let (lower, upper) = self.pair_extent_bounds(pair_extent, i);
        let current = (usable * f64::from(self.ratios[i])).clamp(lower, upper);
        let nudged = (current + delta).clamp(lower, upper);
        self.ratios_from_pair_extent(usable, i, pair_extent, nudged)
    }

    fn handle_color(&self, index: usize) -> Color {
        let p = &self.theme.palette;
        if self.dragging_handle == Some(index) {
            p.teal
        } else if self.hovered_handle == Some(index) {
            p.surface_hi
        } else {
            p.border
        }
    }
}

// --- MARK: IMPL WIDGET

impl Widget for ResizableWidget {
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
                let hovered = self.handle_at(pos);
                if hovered != self.hovered_handle {
                    self.hovered_handle = hovered;
                    ctx.request_paint_only();
                }
                if let Some(i) = self.dragging_handle {
                    let new_ratios = self.ratios_from_pos(i, self.pos_on_axis(pos));
                    if new_ratios != self.ratios {
                        self.ratios.clone_from(&new_ratios);
                        ctx.request_layout();
                        ctx.submit_action::<Self::Action>(ResizeHandleDragged(i, new_ratios));
                    }
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if let Some(i) = self.handle_at(pos) {
                    self.dragging_handle = Some(i);
                    self.active_handle = i;
                    ctx.request_focus();
                    ctx.capture_pointer();
                    ctx.set_handled();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) if self.dragging_handle.is_some() => {
                self.dragging_handle = None;
                ctx.request_paint_only();
            }
            PointerEvent::Leave(_) if self.hovered_handle.is_some() => {
                self.hovered_handle = None;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key_event) = event else {
            return;
        };
        if !key_event.state.is_down() {
            return;
        }
        let step = if key_event.modifiers.shift() {
            ARROW_NUDGE_FINE_PX
        } else {
            ARROW_NUDGE_PX
        };

        // Along-axis arrows nudge the active handle; orthogonal arrows cycle
        // which handle is active (every divider stays keyboard-reachable
        // without intercepting Tab).
        let nudge = match (self.axis, &key_event.key) {
            (Axis::Horizontal, Key::Named(NamedKey::ArrowLeft))
            | (Axis::Vertical, Key::Named(NamedKey::ArrowUp)) => Some(-step),
            (Axis::Horizontal, Key::Named(NamedKey::ArrowRight))
            | (Axis::Vertical, Key::Named(NamedKey::ArrowDown)) => Some(step),
            _ => None,
        };
        if let Some(delta) = nudge {
            let new_ratios = self.nudge_ratios(self.active_handle, delta);
            if new_ratios != self.ratios {
                self.ratios.clone_from(&new_ratios);
                ctx.request_layout();
                ctx.submit_action::<Self::Action>(ResizeHandleDragged(
                    self.active_handle,
                    new_ratios,
                ));
                ctx.set_handled();
            }
            return;
        }

        let cycle = match (self.axis, &key_event.key) {
            (Axis::Horizontal, Key::Named(NamedKey::ArrowUp))
            | (Axis::Vertical, Key::Named(NamedKey::ArrowLeft)) => Some(false), // backward
            (Axis::Horizontal, Key::Named(NamedKey::ArrowDown))
            | (Axis::Vertical, Key::Named(NamedKey::ArrowRight)) => Some(true), // forward
            _ => None,
        };
        if let Some(forward) = cycle {
            let count = self.handle_count();
            self.active_handle = if forward {
                (self.active_handle + 1) % count
            } else {
                (self.active_handle + count - 1) % count
            };
            ctx.request_paint_only();
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        let usable = (self.total_extent - self.handles_extent()).max(1.0);
        let i = self.active_handle;
        let (_, pair_extent) = self.pair_region(usable, i);
        let (lower, upper) = self.pair_extent_bounds(pair_extent, i);

        let new_ratios = match event.action {
            Action::Increment => Some(self.nudge_ratios(i, ARROW_NUDGE_PX)),
            Action::Decrement => Some(self.nudge_ratios(i, -ARROW_NUDGE_PX)),
            Action::SetValue => match event.data {
                Some(ActionData::NumericValue(percent)) => {
                    let target = (percent / 100.0 * pair_extent).clamp(lower, upper);
                    Some(self.ratios_from_pair_extent(usable, i, pair_extent, target))
                }
                _ => None,
            },
            _ => None,
        };

        if let Some(new_ratios) = new_ratios
            && new_ratios != self.ratios
        {
            self.ratios.clone_from(&new_ratios);
            ctx.request_layout();
            ctx.submit_action::<Self::Action>(ResizeHandleDragged(i, new_ratios));
            ctx.set_handled();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(false) if self.hovered_handle.is_some() => {
                self.hovered_handle = None;
                ctx.request_paint_only();
            }
            Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for panel in &mut self.panels {
            ctx.register_child(panel);
        }
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
        let mut along = 0.0;
        let mut cross = 0.0;
        for panel in &mut self.panels {
            let len = ctx.compute_length(panel, len_req.into(), context_size, axis, cross_length);
            if axis == self.axis {
                along += len.get();
            } else {
                cross = f64::max(cross, len.get());
            }
        }
        if axis == self.axis {
            Length::px(along + self.handles_extent())
        } else {
            Length::px(cross)
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let (total, cross) = match self.axis {
            Axis::Horizontal => (size.width, size.height),
            Axis::Vertical => (size.height, size.width),
        };
        let usable = (total - self.handles_extent()).max(0.0);
        let extents = self.panel_extents(usable);

        self.total_extent = total;
        self.cross_extent = cross;
        self.handle_centers.clear();

        let panel_count = self.panels.len();
        let mut offset = 0.0;
        for (index, (panel, &extent)) in self.panels.iter_mut().zip(&extents).enumerate() {
            let panel_size = match self.axis {
                Axis::Horizontal => Size::new(extent, size.height),
                Axis::Vertical => Size::new(size.width, extent),
            };
            let origin = match self.axis {
                Axis::Horizontal => Point::new(offset, 0.0),
                Axis::Vertical => Point::new(0.0, offset),
            };
            ctx.run_layout(panel, panel_size);
            ctx.place_child(panel, origin);
            offset += extent;
            if index + 1 < panel_count {
                self.handle_centers.push(offset + HANDLE_THICKNESS * 0.5);
                offset += HANDLE_THICKNESS;
            }
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        for (index, &center) in self.handle_centers.iter().enumerate() {
            let color = self.handle_color(index);
            // Widen slightly on hover/drag for easier targeting feedback.
            let visual =
                if self.dragging_handle == Some(index) || self.hovered_handle == Some(index) {
                    2.0
                } else {
                    HANDLE_THICKNESS
                };
            let offset = center - visual * 0.5;
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

            if ctx.is_focus_target() && index == self.active_handle {
                let grab = GRAB_HALF * 2.0;
                let grab_offset = center - GRAB_HALF;
                let focus_rect = match self.axis {
                    Axis::Horizontal => Rect::from_origin_size(
                        Point::new(grab_offset, 0.0),
                        Size::new(grab, self.cross_extent),
                    ),
                    Axis::Vertical => Rect::from_origin_size(
                        Point::new(0.0, grab_offset),
                        Size::new(self.cross_extent, grab),
                    ),
                };
                painter
                    .stroke(
                        focus_rect,
                        &Stroke::new(FOCUS_RING_WIDTH),
                        self.theme.palette.teal,
                    )
                    .draw();
            }
        }
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        let local = ctx.to_local(pos);
        if self.handle_at(local).is_some() {
            match self.axis {
                Axis::Horizontal => CursorIcon::ColResize,
                Axis::Vertical => CursorIcon::RowResize,
            }
        } else {
            CursorIcon::Default
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Splitter
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let usable = (self.total_extent - self.handles_extent()).max(1.0);
        let i = self.active_handle;
        let (_, pair_extent) = self.pair_region(usable, i);
        let (lower, upper) = self.pair_extent_bounds(pair_extent, i);
        node.set_numeric_value(f64::from(self.ratios[i]) * 100.0);
        node.set_min_numeric_value(lower / pair_extent.max(1.0) * 100.0);
        node.set_max_numeric_value(upper / pair_extent.max(1.0) * 100.0);
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
        node.add_action(Action::SetValue);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&self.panels.iter().map(WidgetPod::id).collect::<Vec<_>>())
    }

    fn propagates_pointer_interaction(&self) -> bool {
        true
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("ResizableWidget", id = id.trace())
    }
}
