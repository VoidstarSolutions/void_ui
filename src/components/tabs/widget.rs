//! Masonry widget for [`crate::components::tabs`].
//!
//! Lays out `items` (icon/label content built by the view layer) in a
//! horizontal row, paints variant-specific chrome behind/around them, and
//! hit-tests pointer events against each item's placed rect — emitting
//! [`TabSelected`] directly rather than registering per-item action sources
//! (mirrors [`crate::components::resizable::widget::ResizableWidget`]).
//!
//! Each item's content is wrapped in [`TabItemNode`], which exposes it to
//! accessibility as `Role::Tab` with `selected` state and an accessible
//! name. The row itself is a single focus stop ([`Role::TabList`]); arrow
//! keys, Home, and End move and immediately activate the selection.
//!
//! An overflow "..." menu for [`TabsVariant::Default`] is a known follow-up,
//! not implemented here.

use masonry::accesskit::{self, Node, Orientation, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, RoundedRectRadii, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;

use super::TabsVariant;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::item_list;
use crate::focus_ring::{FOCUS_RING_OUTSET, paint_focus_ring};

/// Border/underline stroke width — hairline chrome, not density-scaled.
const BORDER_WIDTH: f64 = 1.0;
/// Inset of the selected/hovered highlight from its item's placed rect, for
/// [`TabsVariant::Pill`], [`TabsVariant::Segmented`] and
/// [`TabsVariant::SegmentedFill`] — a highlight-chrome inset, not density-scaled.
const HIGHLIGHT_INSET: f64 = 2.0;
/// Thickness of the accent line under the selected item in
/// [`TabsVariant::Underline`] — an accent stroke, not density-scaled.
const UNDERLINE_ACCENT_WIDTH: f64 = 2.0;
/// Alpha applied to the hover fill for variants with per-item chrome — a
/// color value, not a spacing/size value, so it doesn't scale with density.
const HOVER_ALPHA: f32 = 0.5;

/// Fills `rect` inset by [`HIGHLIGHT_INSET`] with `fill`, using the corner
/// radius returned by `radius` for the inset rect.
fn paint_highlight(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: impl Fn(Rect) -> RoundedRectRadii,
    fill: Color,
) {
    let inset = rect.inset(-HIGHLIGHT_INSET);
    let radii = radius(inset);
    painter
        .fill(RoundedRect::from_rect(inset, radii), fill)
        .draw();
}

/// Paints the selected item's highlight, and (if a different item is
/// hovered) the hover highlight, sharing the same `radius` shape.
fn paint_selection_highlights(
    painter: &mut Painter<'_>,
    placed: &[Rect],
    selected: usize,
    hovered: Option<usize>,
    radius: impl Fn(Rect) -> RoundedRectRadii,
    selected_fill: Color,
    hover_fill: Color,
) {
    if let Some(sel) = placed.get(selected) {
        paint_highlight(painter, *sel, &radius, selected_fill);
    }
    if let Some(h) = hovered
        && h != selected
        && let Some(hr) = placed.get(h)
    {
        paint_highlight(painter, *hr, &radius, hover_fill);
    }
}

/// Action emitted when the user clicks an unselected tab item.
#[derive(Debug, Clone, Copy)]
pub struct TabSelected(pub usize);

/// Builds the `(content, name)` pairs handed to [`TabsWidget::new`] /
/// [`TabsWidget::set_items`] — `name` becomes the [`TabItemNode`]'s
/// accessible name.
fn wrap_items(
    items: Vec<NewWidget<dyn Widget>>,
    names: Vec<Option<ArcStr>>,
    disabled: &[bool],
    selected: usize,
) -> Vec<WidgetPod<TabItemNode>> {
    items
        .into_iter()
        .zip(names)
        .enumerate()
        .map(|(i, (content, name))| {
            let item_disabled = disabled.get(i).copied().unwrap_or(false);
            TabItemNode::new(content, name, i == selected, item_disabled).to_pod()
        })
        .collect()
}

/// Wraps one tab's content for accessibility.
///
/// `Role::Tab`, unlike `Role::Button`, doesn't compute its accessible name
/// from descendant text — this wrapper sets it explicitly from the
/// [`super::view::TabItem`]'s label or `aria_label`. Layout is pure
/// pass-through: this widget is always exactly its child's size.
pub struct TabItemNode {
    child: WidgetPod<dyn Widget>,
    name: Option<ArcStr>,
    selected: bool,
    disabled: bool,
}

impl TabItemNode {
    fn new(
        child: NewWidget<dyn Widget>,
        name: Option<ArcStr>,
        selected: bool,
        disabled: bool,
    ) -> NewWidget<Self> {
        NewWidget::new(Self {
            child: child.to_pod(),
            name,
            selected,
            disabled,
        })
    }

    fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets the disabled state. Propagates to masonry's system-level
    /// disabled flag (for event routing + accessibility) and requests a
    /// repaint.
    fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    /// Mutable access to the wrapped content widget for the view layer.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for TabItemNode {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Propagate the host-supplied initial `disabled` to masonry on
        // widget add — `new` only sets the widget-internal field, and
        // without this the first build leaves masonry's `is_disabled()` out
        // of sync, breaking event routing and the accessibility pass that
        // drives `node.set_disabled()`.
        if matches!(event, Update::WidgetAdded) {
            ctx.set_disabled(self.disabled);
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
        let cross_axis = match axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        ctx.compute_length(
            &mut self.child,
            len_req.into(),
            LayoutSize::maybe(cross_axis, cross_length),
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
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
        Role::Tab
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_selected(self.selected);
        if let Some(name) = &self.name {
            node.set_label(name.to_string());
        }
        if !self.disabled {
            node.add_action(accesskit::Action::Focus);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Masonry widget backing [`super::view::TabsView`].
pub struct TabsWidget {
    items: Vec<WidgetPod<TabItemNode>>,
    /// Each item's placed rect in local coordinates, set during `layout`.
    placed: Vec<Rect>,
    /// Per-item disabled flag, parallel to `items`/`placed`.
    disabled: Vec<bool>,
    variant: TabsVariant,
    selected: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    theme: Theme,
    /// Average item width from the last real `layout` pass, cached because
    /// `set_items` clears `placed` — a keypress arriving before the next
    /// layout needs `item_rect_or_estimate`'s fallback to stay reasonably
    /// accurate for variable-width labels rather than assuming every item is
    /// the same guessed width. `None` until the first layout ever runs.
    last_avg_item_width: Option<f64>,
}

// --- MARK: BUILDERS
impl TabsWidget {
    #[must_use]
    pub fn new(
        items: Vec<NewWidget<dyn Widget>>,
        names: Vec<Option<ArcStr>>,
        disabled: Vec<bool>,
        variant: TabsVariant,
        selected: usize,
        theme: &Theme,
    ) -> Self {
        Self {
            items: wrap_items(items, names, &disabled, selected),
            placed: Vec::new(),
            disabled,
            variant,
            selected,
            hovered: None,
            pressed: None,
            theme: *theme,
            last_avg_item_width: None,
        }
    }
}

// --- MARK: WIDGETMUT
impl TabsWidget {
    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: usize) {
        if this.widget.selected != selected {
            let old = this.widget.selected;
            this.widget.selected = selected;
            if let Some(item) = this.widget.items.get_mut(old) {
                let mut node = this.ctx.get_mut(item);
                TabItemNode::set_selected(&mut node, false);
            }
            if let Some(item) = this.widget.items.get_mut(selected) {
                let mut node = this.ctx.get_mut(item);
                TabItemNode::set_selected(&mut node, true);
            }
            this.widget.hovered = None;
            this.widget.pressed = None;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: TabsVariant) {
        if this.widget.variant != variant {
            this.widget.variant = variant;
            this.widget.hovered = None;
            this.widget.pressed = None;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Mutable access to item `index`'s [`TabItemNode`] wrapper.
    ///
    /// Two-step access for the view layer: call [`TabItemNode::child_mut`]
    /// on the result to reach the content widget itself.
    pub fn item_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        index: usize,
    ) -> WidgetMut<'t, TabItemNode> {
        this.ctx.get_mut(&mut this.widget.items[index])
    }

    /// Replaces the entire item set, e.g. when the item *count* changes.
    pub fn set_items(
        this: &mut WidgetMut<'_, Self>,
        items: Vec<NewWidget<dyn Widget>>,
        names: Vec<Option<ArcStr>>,
        disabled: Vec<bool>,
    ) {
        for old in std::mem::take(&mut this.widget.items) {
            this.ctx.remove_child(old);
        }
        let selected = this.widget.selected;
        this.widget.items = wrap_items(items, names, &disabled, selected);
        this.widget.disabled = disabled;
        this.widget.hovered = None;
        this.widget.pressed = None;
        this.widget.placed.clear();
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Updates item `index`'s disabled flag in place, e.g. when an item
    /// becomes available/unavailable without the item *count* changing.
    pub fn set_item_disabled(this: &mut WidgetMut<'_, Self>, index: usize, disabled: bool) {
        if this.widget.disabled.get(index) == Some(&disabled) {
            return;
        }
        if let Some(d) = this.widget.disabled.get_mut(index) {
            *d = disabled;
        }
        {
            let mut node = TabsWidget::item_mut(this, index);
            TabItemNode::set_disabled(&mut node, disabled);
        }
        if disabled {
            if this.widget.hovered == Some(index) {
                this.widget.hovered = None;
            }
            if this.widget.pressed == Some(index) {
                this.widget.pressed = None;
            }
        }
        this.ctx.request_paint_only();
    }
}

// --- MARK: HELPERS
impl TabsWidget {
    /// Padding between the widget's own bounds and the row of items —
    /// nonzero for variants with an outer container shape.
    fn outer_pad(&self) -> f64 {
        match self.variant {
            TabsVariant::Underline | TabsVariant::Outline => 0.0,
            _ => f64::from(self.theme.density.pad) / 2.0,
        }
    }

    /// Gap between adjacent items.
    fn gap(&self) -> f64 {
        f64::from(self.theme.density.pad) / 2.0
    }

    /// Padding inside each item box, around its icon/label content.
    fn item_pad(&self) -> (f64, f64) {
        (
            f64::from(self.theme.density.button_pad_h),
            f64::from(self.theme.density.button_pad_v),
        )
    }

    fn item_at(&self, pos: Point) -> Option<usize> {
        self.placed.iter().position(|r| r.contains(pos))
    }

    /// Item rect for `index`, falling back to an estimate when `placed` is
    /// empty (cleared by `set_items`, not yet rebuilt by layout). Items are
    /// horizontal; width is estimated from `last_avg_item_width` (the real
    /// average from the last layout, which tracks variable-width labels far
    /// better than a fixed per-item guess) when available, falling back to a
    /// theme-metric guess only before the very first layout has ever run.
    /// Shared with sidebar nav via [`item_list::item_rect_or_estimate`].
    fn item_rect_or_estimate(&self, index: usize) -> Rect {
        let outer = self.outer_pad();
        let gap = self.gap();
        let (pad_h, pad_v) = self.item_pad();
        let est_item_w = self
            .last_avg_item_width
            .unwrap_or(f64::from(self.theme.density.ui_font_size) * 3.0 + 2.0 * pad_h);
        let est_item_h = f64::from(self.theme.density.ui_font_size) + 2.0 * pad_v;
        item_list::item_rect_or_estimate(
            &self.placed,
            index,
            item_list::EstimateGeometry {
                axis: Axis::Horizontal,
                outer,
                cross_offset: outer,
                main_extent: est_item_w,
                cross_extent: est_item_h,
                gap,
            },
        )
    }

    fn is_disabled(&self, index: usize) -> bool {
        self.disabled.get(index).copied().unwrap_or(false)
    }

    /// The first item index in `0..items.len()` that isn't disabled, or
    /// `None` if every item is disabled.
    fn first_enabled(&self) -> Option<usize> {
        (0..self.items.len()).find(|&i| !self.is_disabled(i))
    }

    /// The last item index in `0..items.len()` that isn't disabled, or
    /// `None` if every item is disabled.
    fn last_enabled(&self) -> Option<usize> {
        (0..self.items.len()).rev().find(|&i| !self.is_disabled(i))
    }

    /// The next non-disabled item starting from `from` and stepping by
    /// `dir` (`1` or `-1`), wrapping around. Returns `from` itself if it's
    /// the only enabled item, or `None` if every item is disabled.
    fn next_enabled(&self, from: usize, dir: isize) -> Option<usize> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let mut i = from;
        for _ in 0..n {
            i = if dir > 0 {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            };
            if !self.is_disabled(i) {
                return Some(i);
            }
        }
        None
    }
}

// --- MARK: IMPL WIDGET
impl Widget for TabsWidget {
    type Action = TabSelected;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Move(update) => {
                let pos = ctx.local_position(update.current.position);
                let hovered = self.item_at(pos).filter(|&i| !self.is_disabled(i));
                if hovered != self.hovered {
                    self.hovered = hovered;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Leave(_) if self.hovered.is_some() || self.pressed.is_some() => {
                self.hovered = None;
                self.pressed = None;
                ctx.request_paint_only();
            }
            // Shared primary-click recognizer; the capture guard only
            // accepts presses that hit an enabled item, so pressing the
            // row's outer padding neither captures the pointer nor arms
            // a press (same as before the port).
            _ => match click::primary_click_when(ctx, event, |ctx, state| {
                let pos = ctx.local_position(state.position);
                self.item_at(pos).is_some_and(|i| !self.is_disabled(i))
            }) {
                Some(ClickPhase::Down(state)) => {
                    // The capture guard verified the hit; re-derive the
                    // index for pressed tracking.
                    let pos = ctx.local_position(state.position);
                    if let Some(i) = self.item_at(pos).filter(|&i| !self.is_disabled(i)) {
                        self.pressed = Some(i);
                        ctx.request_focus();
                        ctx.request_paint_only();
                    }
                }
                Some(ClickPhase::Up { state, completed }) => {
                    if let Some(i) = self.pressed.take() {
                        let pos = ctx.local_position(state.position);
                        if completed && self.item_at(pos) == Some(i) && i != self.selected {
                            ctx.submit_action::<Self::Action>(TabSelected(i));
                        }
                        ctx.request_paint_only();
                    }
                }
                None => {}
            },
        }
    }

    /// Left/Up move to the previous tab, Right/Down to the next (wrapping),
    /// and Home/End jump to the first/last tab — each immediately submitting
    /// [`TabSelected`] for the new index (single tab stop, automatic
    /// activation, mirroring native browser tab strips).
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down || self.items.is_empty() {
            return;
        }
        let new_selected = match key.key {
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => {
                self.next_enabled(self.selected, -1)
            }
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) => {
                self.next_enabled(self.selected, 1)
            }
            Key::Named(NamedKey::Home) => self.first_enabled(),
            Key::Named(NamedKey::End) => self.last_enabled(),
            _ => None,
        };
        if let Some(i) = new_selected {
            ctx.set_handled();
            if i != self.selected {
                ctx.request_scroll_to(self.item_rect_or_estimate(i));
                ctx.submit_action::<Self::Action>(TabSelected(i));
            }
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(_)) {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for item in &mut self.items {
            ctx.register_child(item);
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: masonry::layout::LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let outer = self.outer_pad();
        let gap = self.gap();
        let (pad_h, pad_v) = self.item_pad();
        let auto_length = len_req.into();
        let n = self.items.len();

        match axis {
            Axis::Horizontal => {
                #[allow(clippy::cast_precision_loss)]
                let mut total = 2.0 * outer + gap * (n.saturating_sub(1)) as f64;
                for item in &mut self.items {
                    let content_w = ctx
                        .compute_length(
                            item,
                            auto_length,
                            LayoutSize::maybe(Axis::Vertical, cross_length),
                            axis,
                            cross_length,
                        )
                        .get();
                    total += content_w + 2.0 * pad_h;
                }
                if let masonry::layout::LenReq::FitContent(space) = len_req {
                    total = total.min(space.get());
                }
                Length::px(total)
            }
            Axis::Vertical => {
                let mut max_h: f64 = 0.0;
                for item in &mut self.items {
                    let content_h = ctx
                        .compute_length(
                            item,
                            auto_length,
                            LayoutSize::maybe(Axis::Horizontal, cross_length),
                            axis,
                            cross_length,
                        )
                        .get();
                    max_h = max_h.max(content_h + 2.0 * pad_v);
                }
                Length::px(max_h + 2.0 * outer)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let outer = self.outer_pad();
        let gap = self.gap();
        let (pad_h, pad_v) = self.item_pad();
        let n = self.items.len();

        let mut content_sizes = Vec::with_capacity(n);
        let mut max_content_h: f64 = 0.0;
        for item in &mut self.items {
            let content_size = ctx.compute_size(item, SizeDef::MIN, size.into());
            max_content_h = max_content_h.max(content_size.height);
            content_sizes.push(content_size);
        }
        let item_height = max_content_h + 2.0 * pad_v;

        #[allow(clippy::cast_precision_loss)]
        let n_f = n as f64;
        let gap_total = if n > 0 { gap * (n_f - 1.0) } else { 0.0 };
        let available_for_items = (size.width - 2.0 * outer - gap_total).max(0.0);

        let mut item_widths: Vec<f64> =
            if matches!(self.variant, TabsVariant::SegmentedFill) && n > 0 {
                vec![available_for_items / n_f; n]
            } else {
                content_sizes
                    .iter()
                    .map(|cs| cs.width + 2.0 * pad_h)
                    .collect()
            };

        // `measure`'s `FitContent` arm can report a row width below the
        // items' natural total (clamped to the space the parent offered).
        // When that happens, shrink every item proportionally so the row
        // stays within `size.width` instead of overflowing it. Each item's
        // content is then laid out at its shrunk width too, so it's
        // truncated/compressed by its own widget rather than spilling past
        // its placed rect.
        let mut shrunk = false;
        if !matches!(self.variant, TabsVariant::SegmentedFill) {
            let natural_total: f64 = item_widths.iter().sum();
            if natural_total > available_for_items {
                let scale = if natural_total > 0.0 {
                    available_for_items / natural_total
                } else {
                    1.0
                };
                for w in &mut item_widths {
                    *w *= scale;
                }
                shrunk = true;
            }
        }

        if n > 0 {
            self.last_avg_item_width = Some(item_widths.iter().sum::<f64>() / n_f);
        }

        self.placed.clear();
        let item_y = outer;
        let mut x = outer;
        for (i, item) in self.items.iter_mut().enumerate() {
            let w = item_widths[i];
            let item_rect =
                Rect::from_origin_size(Point::new(x, item_y), Size::new(w, item_height));
            self.placed.push(item_rect);

            let natural_cs = content_sizes[i];
            let cs = if shrunk {
                let max_content_w = (w - 2.0 * pad_h).max(0.0);
                Size::new(natural_cs.width.min(max_content_w), natural_cs.height)
            } else {
                natural_cs
            };
            ctx.run_layout(item, cs);
            let cx = x + (w - cs.width) * 0.5;
            let cy = item_y + (item_height - cs.height) * 0.5;
            ctx.place_child(item, Point::new(cx, cy));

            x += w + gap;
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let p = &self.theme.palette;
        let radius_small = f64::from(self.theme.radius.small);
        let radius_large = f64::from(self.theme.radius.large);

        match self.variant {
            TabsVariant::Default => {
                let rect = RoundedRect::from_origin_size(Point::ORIGIN, size, radius_small);
                painter.fill(rect, p.surface_2).draw();
            }
            TabsVariant::Underline => {
                let line = Rect::from_origin_size(
                    Point::new(0.0, size.height - BORDER_WIDTH),
                    Size::new(size.width, BORDER_WIDTH),
                );
                painter.fill(line, p.border).draw();
                if let Some(sel) = self.placed.get(self.selected) {
                    let accent = Rect::from_origin_size(
                        Point::new(sel.x0, size.height - UNDERLINE_ACCENT_WIDTH),
                        Size::new(sel.width(), UNDERLINE_ACCENT_WIDTH),
                    );
                    painter.fill(accent, p.teal).draw();
                }
            }
            TabsVariant::Pill => {
                let outer = RoundedRect::from_origin_size(
                    Point::ORIGIN,
                    size,
                    RoundedRectRadii::from_single_radius(size.height / 2.0),
                );
                painter.fill(outer, p.surface_2).draw();
                paint_selection_highlights(
                    painter,
                    &self.placed,
                    self.selected,
                    self.hovered,
                    |r| RoundedRectRadii::from_single_radius(r.height() / 2.0),
                    p.surface_hi,
                    p.surface_hi.with_alpha(HOVER_ALPHA),
                );
            }
            TabsVariant::Outline => {
                for (i, r) in self.placed.iter().enumerate() {
                    let rrect = RoundedRect::from_rect(*r, radius_large);
                    if i == self.selected {
                        painter.fill(rrect, p.surface_hi).draw();
                        painter
                            .stroke(rrect, &Stroke::new(BORDER_WIDTH), p.border_strong)
                            .draw();
                    } else {
                        if Some(i) == self.hovered {
                            painter.fill(rrect, p.surface_2).draw();
                        }
                        painter
                            .stroke(rrect, &Stroke::new(BORDER_WIDTH), p.border)
                            .draw();
                    }
                }
            }
            TabsVariant::Segmented | TabsVariant::SegmentedFill => {
                let outer = RoundedRect::from_origin_size(Point::ORIGIN, size, radius_large);
                painter
                    .stroke(outer, &Stroke::new(BORDER_WIDTH), p.border)
                    .draw();
                let highlight_radius = (radius_small - HIGHLIGHT_INSET).max(0.0);
                paint_selection_highlights(
                    painter,
                    &self.placed,
                    self.selected,
                    self.hovered,
                    |_| RoundedRectRadii::from_single_radius(highlight_radius),
                    p.surface_hi,
                    p.surface_2,
                );
            }
        }

        if ctx.is_focus_target()
            && let Some(sel) = self.placed.get(self.selected)
        {
            // Inset (not outset): items can sit flush against the widget's
            // own bounds (e.g. `Underline`/`Outline`, which have no outer
            // padding), so an outward ring would be clipped.
            let ring_rect = RoundedRect::from_rect(sel.inset(-FOCUS_RING_OUTSET), radius_small);
            paint_focus_ring(painter, ring_rect, &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::TabList
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_orientation(Orientation::Horizontal);
        if !self.items.is_empty() {
            node.add_action(accesskit::Action::Focus);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&self.items.iter().map(WidgetPod::id).collect::<Vec<_>>())
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

// --- MARK: TESTS

#[cfg(test)]
mod tests {
    use masonry::core::PointerButton;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::SizedBox;

    use super::*;

    fn item(width: f64, height: f64) -> NewWidget<dyn Widget> {
        NewWidget::new(SizedBox::empty().size(Length::px(width), Length::px(height))).erased()
    }

    fn widget(
        items: Vec<NewWidget<dyn Widget>>,
        variant: TabsVariant,
        selected: usize,
    ) -> TabsWidget {
        let names = vec![None; items.len()];
        let disabled = vec![false; items.len()];
        TabsWidget::new(items, names, disabled, variant, selected, &Theme::default())
    }

    fn harness(
        items: Vec<NewWidget<dyn Widget>>,
        variant: TabsVariant,
        selected: usize,
    ) -> TestHarness<TabsWidget> {
        TestHarness::create(
            default_property_set(),
            NewWidget::new(widget(items, variant, selected)),
        )
    }

    fn harness_with_disabled(
        items: Vec<NewWidget<dyn Widget>>,
        disabled: Vec<bool>,
        selected: usize,
    ) -> TestHarness<TabsWidget> {
        let names = vec![None; items.len()];
        let widget = TabsWidget::new(
            items,
            names,
            disabled,
            TabsVariant::Default,
            selected,
            &Theme::default(),
        );
        TestHarness::create(default_property_set(), NewWidget::new(widget))
    }

    // --- item_at ---

    #[test]
    fn item_at_returns_index_of_containing_rect() {
        let mut w = widget(vec![], TabsVariant::Default, 0);
        w.placed = vec![
            Rect::from_origin_size(Point::ORIGIN, Size::new(50.0, 20.0)),
            Rect::from_origin_size(Point::new(50.0, 0.0), Size::new(50.0, 20.0)),
        ];
        assert_eq!(w.item_at(Point::new(10.0, 10.0)), Some(0));
        assert_eq!(w.item_at(Point::new(60.0, 10.0)), Some(1));
        assert_eq!(w.item_at(Point::new(200.0, 10.0)), None);
    }

    // --- outer_pad / gap ---

    #[test]
    fn outer_pad_is_zero_only_for_underline_and_outline() {
        for variant in [TabsVariant::Underline, TabsVariant::Outline] {
            let w = widget(vec![], variant, 0);
            assert!(w.outer_pad() == 0.0, "{variant:?} should have no outer pad");
        }
        for variant in [
            TabsVariant::Default,
            TabsVariant::Pill,
            TabsVariant::Segmented,
            TabsVariant::SegmentedFill,
        ] {
            let w = widget(vec![], variant, 0);
            assert!(
                w.outer_pad() > 0.0,
                "{variant:?} should frame the row with an outer pad"
            );
        }
    }

    #[test]
    fn gap_and_outer_pad_scale_with_density() {
        let mut theme = Theme::default();
        theme.density.pad *= 2.0;
        let scaled = TabsWidget::new(vec![], vec![], vec![], TabsVariant::Default, 0, &theme);
        let base = widget(vec![], TabsVariant::Default, 0);
        assert!(scaled.gap() > base.gap());
        assert!(scaled.outer_pad() > base.outer_pad());
    }

    // --- layout ---

    #[test]
    fn layout_sizes_items_to_natural_width_for_default_variant() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(80.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        let placed = h.edit_root_widget(|wm| wm.widget.placed.clone());

        assert_eq!(placed.len(), 2);
        assert!(
            placed[1].width() > placed[0].width(),
            "wider content should produce a wider item box"
        );
        // Items don't overlap and are laid out left-to-right.
        assert!(placed[0].x1 <= placed[1].x0 + 1e-6);
    }

    #[test]
    fn layout_distributes_equal_widths_for_segmented_fill() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(80.0, 20.0), item(20.0, 20.0)],
            TabsVariant::SegmentedFill,
            0,
        );
        let placed = h.edit_root_widget(|wm| wm.widget.placed.clone());

        assert_eq!(placed.len(), 3);
        let w0 = placed[0].width();
        for r in &placed[1..] {
            assert!(
                (r.width() - w0).abs() < 1e-6,
                "all segments should share the available width equally"
            );
        }
    }

    #[test]
    fn item_rect_or_estimate_uses_real_average_width_not_uniform_guess() {
        // Wide spread so the fixed theme-metric guess (~3x font size) can't
        // coincidentally match the real average.
        let mut h = harness(
            vec![item(20.0, 20.0), item(300.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        let (real_avg_width, uniform_guess) = h.edit_root_widget(|wm| {
            let pad_h = wm.widget.item_pad().0;
            let uniform_guess = f64::from(wm.widget.theme.density.ui_font_size) * 3.0 + 2.0 * pad_h;
            let real_avg = wm
                .widget
                .last_avg_item_width
                .expect("layout should have run and cached an average width");
            (real_avg, uniform_guess)
        });
        assert!(
            (real_avg_width - uniform_guess).abs() > 1.0,
            "test fixture should produce an average width that's clearly different \
             from the fixed guess (avg={real_avg_width}, guess={uniform_guess})"
        );

        // Simulate the transitional window set_items creates: placed cleared,
        // layout hasn't run yet.
        let estimated_width = h.edit_root_widget(|wm| {
            wm.widget.placed.clear();
            wm.widget.item_rect_or_estimate(0).width()
        });

        assert!(
            (estimated_width - real_avg_width).abs() < 1e-6,
            "estimate should track the real average item width from the last layout \
             (est={estimated_width}, expected={real_avg_width})"
        );
    }

    // --- pointer interaction ---

    #[test]
    fn clicking_an_unselected_item_emits_tab_selected() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        let target = h.edit_root_widget(|wm| wm.widget.placed[1].center());

        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        let (action, widget_id) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(action.0, 1);
        assert_eq!(widget_id, h.root_id());
    }

    #[test]
    fn clicking_the_selected_item_emits_nothing() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        let target = h.edit_root_widget(|wm| wm.widget.placed[0].center());

        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        assert!(h.pop_action::<TabSelected>().is_none());
    }

    #[test]
    fn clicking_a_disabled_item_emits_nothing() {
        let mut h = harness_with_disabled(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            vec![false, true],
            0,
        );
        let target = h.edit_root_widget(|wm| wm.widget.placed[1].center());

        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        assert!(h.pop_action::<TabSelected>().is_none());
    }

    #[test]
    fn moving_over_a_disabled_item_does_not_set_hovered() {
        let mut h = harness_with_disabled(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            vec![false, true],
            0,
        );
        let target = h.edit_root_widget(|wm| wm.widget.placed[1].center());

        h.mouse_move(target);
        assert_eq!(h.edit_root_widget(|wm| wm.widget.hovered), None);
    }

    #[test]
    fn moving_over_an_item_sets_hovered() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        let target = h.edit_root_widget(|wm| wm.widget.placed[1].center());

        h.mouse_move(target);
        let hovered = h.edit_root_widget(|wm| wm.widget.hovered);
        assert_eq!(hovered, Some(1));

        // Moving over the outer padding (not any item) clears hover.
        h.mouse_move(Point::new(0.0, 0.0));
        let hovered = h.edit_root_widget(|wm| wm.widget.hovered);
        assert_eq!(hovered, None);
    }

    // --- setters ---

    #[test]
    fn set_items_resets_interaction_state() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        let target = h.edit_root_widget(|wm| wm.widget.placed[1].center());
        h.mouse_move(target);
        assert_eq!(h.edit_root_widget(|wm| wm.widget.hovered), Some(1));

        h.edit_root_widget(|mut wm| {
            TabsWidget::set_items(&mut wm, vec![item(40.0, 20.0)], vec![None], vec![false]);
        });

        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.hovered, None);
            assert_eq!(wm.widget.pressed, None);
            assert_eq!(wm.widget.items.len(), 1);
            assert_eq!(wm.widget.placed.len(), 1);
        });
    }

    #[test]
    fn set_selected_and_set_variant_update_state() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );

        h.edit_root_widget(|mut wm| {
            TabsWidget::set_selected(&mut wm, 1);
            assert_eq!(wm.widget.selected, 1);

            TabsWidget::set_variant(&mut wm, TabsVariant::Pill);
            assert_eq!(wm.widget.variant, TabsVariant::Pill);
        });
    }

    // --- accessibility ---

    #[test]
    fn new_sets_item_node_name_and_selected_from_args() {
        let items = vec![item(40.0, 20.0), item(40.0, 20.0)];
        let names = vec![Some(ArcStr::from("Account")), None];
        let mut h = TestHarness::create(
            default_property_set(),
            NewWidget::new(TabsWidget::new(
                items,
                names,
                vec![false, false],
                TabsVariant::Default,
                1,
                &Theme::default(),
            )),
        );

        h.edit_root_widget(|mut wm| {
            let item0 = TabsWidget::item_mut(&mut wm, 0);
            assert_eq!(item0.widget.name.as_deref(), Some("Account"));
            assert!(!item0.widget.selected);
        });
        h.edit_root_widget(|mut wm| {
            let item1 = TabsWidget::item_mut(&mut wm, 1);
            assert_eq!(item1.widget.name, None);
            assert!(item1.widget.selected);
        });
    }

    #[test]
    fn set_selected_updates_item_node_selected_flags() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );

        h.edit_root_widget(|mut wm| {
            TabsWidget::set_selected(&mut wm, 1);

            assert!(!TabsWidget::item_mut(&mut wm, 0).widget.selected);
            assert!(TabsWidget::item_mut(&mut wm, 1).widget.selected);
        });
    }

    // --- keyboard navigation ---

    #[test]
    fn arrow_keys_move_selection_and_wrap_when_focused() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
        let (action, widget_id) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(action.0, 1);
        assert_eq!(widget_id, h.root_id());

        h.edit_root_widget(|mut wm| TabsWidget::set_selected(&mut wm, 2));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(
            action.0, 0,
            "ArrowRight should wrap from the last to the first item"
        );

        h.edit_root_widget(|mut wm| TabsWidget::set_selected(&mut wm, 0));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowLeft)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(
            action.0, 2,
            "ArrowLeft should wrap from the first to the last item"
        );
    }

    #[test]
    fn home_and_end_jump_to_first_and_last_item() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            1,
        );
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::End)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(action.0, 2);

        h.edit_root_widget(|mut wm| TabsWidget::set_selected(&mut wm, 1));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Home)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(action.0, 0);
    }

    #[test]
    fn arrow_key_emits_nothing_with_a_single_item() {
        let mut h = harness(vec![item(40.0, 20.0)], TabsVariant::Default, 0);
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));

        assert!(h.pop_action::<TabSelected>().is_none());
    }

    #[test]
    fn arrow_keys_skip_disabled_items() {
        let mut h = harness_with_disabled(
            vec![item(40.0, 20.0), item(40.0, 20.0), item(40.0, 20.0)],
            vec![false, true, false],
            0,
        );
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(
            action.0, 2,
            "ArrowRight should skip the disabled middle item"
        );

        h.edit_root_widget(|mut wm| TabsWidget::set_selected(&mut wm, 0));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowLeft)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(
            action.0, 2,
            "ArrowLeft should wrap and skip the disabled middle item"
        );
    }

    #[test]
    fn home_and_end_skip_disabled_edges() {
        let mut h = harness_with_disabled(
            vec![
                item(40.0, 20.0),
                item(40.0, 20.0),
                item(40.0, 20.0),
                item(40.0, 20.0),
            ],
            vec![true, false, false, true],
            2,
        );
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Home)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(
            action.0, 1,
            "Home should land on the first enabled item, not index 0"
        );

        h.edit_root_widget(|mut wm| TabsWidget::set_selected(&mut wm, 1));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::End)));
        let (action, _) = h.pop_action::<TabSelected>().expect("expected an action");
        assert_eq!(
            action.0, 2,
            "End should land on the last enabled item, not the last index"
        );
    }

    // --- disabled ---

    #[test]
    fn set_item_disabled_updates_node_and_clears_interaction_state() {
        let mut h = harness(
            vec![item(40.0, 20.0), item(40.0, 20.0)],
            TabsVariant::Default,
            0,
        );
        let target = h.edit_root_widget(|wm| wm.widget.placed[1].center());
        h.mouse_move(target);
        assert_eq!(h.edit_root_widget(|wm| wm.widget.hovered), Some(1));

        h.edit_root_widget(|mut wm| {
            TabsWidget::set_item_disabled(&mut wm, 1, true);
        });

        h.edit_root_widget(|wm| {
            assert!(wm.widget.disabled[1]);
            assert_eq!(wm.widget.hovered, None);
        });

        // Once disabled, clicking the item emits nothing.
        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<TabSelected>().is_none());
    }
}
