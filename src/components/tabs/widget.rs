//! Masonry widget for [`crate::components::tabs`].
//!
//! Lays out `items` (icon/label content built by the view layer) in a
//! horizontal row, paints variant-specific chrome behind/around them, and
//! hit-tests pointer events against each item's placed rect — emitting
//! [`TabSelected`] directly rather than registering per-item action sources
//! (mirrors [`crate::components::resizable::widget::ResizableWidget`]).
//!
//! Keyboard navigation and an overflow "..." menu for [`TabsVariant::Default`]
//! are known follow-ups, not implemented here.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, RoundedRectRadii, Size, Stroke};
use masonry::layout::{LayoutSize, Length, SizeDef};

use super::TabsVariant;
use crate::Theme;

/// Border/underline stroke width.
const BORDER_WIDTH: f64 = 1.0;
/// Inset of the selected/hovered highlight from its item's placed rect, for
/// [`TabsVariant::Pill`], [`TabsVariant::Segmented`] and
/// [`TabsVariant::SegmentedFill`].
const HIGHLIGHT_INSET: f64 = 2.0;
/// Thickness of the accent line under the selected item in
/// [`TabsVariant::Underline`].
const UNDERLINE_ACCENT_WIDTH: f64 = 2.0;
/// Alpha applied to the hover fill for variants with per-item chrome.
const HOVER_ALPHA: f32 = 0.5;

/// Action emitted when the user clicks an unselected tab item.
#[derive(Debug, Clone, Copy)]
pub struct TabSelected(pub usize);

/// Masonry widget backing [`super::view::TabsView`].
pub struct TabsWidget {
    items: Vec<WidgetPod<dyn Widget>>,
    /// Each item's placed rect in local coordinates, set during `layout`.
    placed: Vec<Rect>,
    variant: TabsVariant,
    selected: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    theme: Theme,
}

// --- MARK: BUILDERS
impl TabsWidget {
    #[must_use]
    pub fn new(
        items: Vec<NewWidget<dyn Widget>>,
        variant: TabsVariant,
        selected: usize,
        theme: &Theme,
    ) -> Self {
        Self {
            items: items.into_iter().map(NewWidget::to_pod).collect(),
            placed: Vec::new(),
            variant,
            selected,
            hovered: None,
            pressed: None,
            theme: *theme,
        }
    }
}

// --- MARK: WIDGETMUT
impl TabsWidget {
    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: usize) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: TabsVariant) {
        if this.widget.variant != variant {
            this.widget.variant = variant;
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

    /// Mutable access to item `index`'s content for the view layer.
    pub fn item_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        index: usize,
    ) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.items[index])
    }

    /// Replaces the entire item set, e.g. when the item *count* changes.
    pub fn set_items(this: &mut WidgetMut<'_, Self>, items: Vec<NewWidget<dyn Widget>>) {
        for old in std::mem::take(&mut this.widget.items) {
            this.ctx.remove_child(old);
        }
        this.widget.items = items.into_iter().map(NewWidget::to_pod).collect();
        this.widget.hovered = None;
        this.widget.pressed = None;
        this.widget.placed.clear();
        this.ctx.children_changed();
        this.ctx.request_layout();
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
                let hovered = self.item_at(pos);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if let Some(i) = self.item_at(pos) {
                    self.pressed = Some(i);
                    ctx.capture_pointer();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if let Some(i) = self.pressed.take() {
                    let pos = ctx.local_position(state.position);
                    if self.item_at(pos) == Some(i) && i != self.selected {
                        ctx.submit_action::<Self::Action>(TabSelected(i));
                    }
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Leave(_) if self.hovered.is_some() || self.pressed.is_some() => {
                self.hovered = None;
                self.pressed = None;
                ctx.request_paint_only();
            }
            _ => {}
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

        let item_widths: Vec<f64> = if matches!(self.variant, TabsVariant::SegmentedFill) && n > 0 {
            #[allow(clippy::cast_precision_loss)]
            let n_f = n as f64;
            let avail = (size.width - 2.0 * outer - gap * (n_f - 1.0)).max(0.0);
            let w = avail / n_f;
            vec![w; n]
        } else {
            content_sizes
                .iter()
                .map(|cs| cs.width + 2.0 * pad_h)
                .collect()
        };

        self.placed.clear();
        let item_y = outer;
        let mut x = outer;
        for (i, item) in self.items.iter_mut().enumerate() {
            let w = item_widths[i];
            let item_rect =
                Rect::from_origin_size(Point::new(x, item_y), Size::new(w, item_height));
            self.placed.push(item_rect);

            let cs = content_sizes[i];
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
                if let Some(sel) = self.placed.get(self.selected) {
                    let pill = sel.inset(-HIGHLIGHT_INSET);
                    let radii = RoundedRectRadii::from_single_radius(pill.height() / 2.0);
                    painter
                        .fill(RoundedRect::from_rect(pill, radii), p.surface_hi)
                        .draw();
                }
                if let Some(h) = self.hovered
                    && h != self.selected
                    && let Some(hr) = self.placed.get(h)
                {
                    let pill = hr.inset(-HIGHLIGHT_INSET);
                    let radii = RoundedRectRadii::from_single_radius(pill.height() / 2.0);
                    painter
                        .fill(
                            RoundedRect::from_rect(pill, radii),
                            p.surface_hi.with_alpha(HOVER_ALPHA),
                        )
                        .draw();
                }
            }
            TabsVariant::Outline => {
                for (i, r) in self.placed.iter().enumerate() {
                    let rrect = RoundedRect::from_rect(*r, radius_small);
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
                let outer = RoundedRect::from_origin_size(Point::ORIGIN, size, radius_small);
                painter
                    .stroke(outer, &Stroke::new(BORDER_WIDTH), p.border)
                    .draw();
                let highlight_radius = (radius_small - HIGHLIGHT_INSET).max(0.0);
                if let Some(sel) = self.placed.get(self.selected) {
                    let seg = sel.inset(-HIGHLIGHT_INSET);
                    let radii = RoundedRectRadii::from_single_radius(highlight_radius);
                    painter
                        .fill(RoundedRect::from_rect(seg, radii), p.surface_hi)
                        .draw();
                }
                if let Some(h) = self.hovered
                    && h != self.selected
                    && let Some(hr) = self.placed.get(h)
                {
                    let seg = hr.inset(-HIGHLIGHT_INSET);
                    let radii = RoundedRectRadii::from_single_radius(highlight_radius);
                    painter
                        .fill(RoundedRect::from_rect(seg, radii), p.surface_2)
                        .draw();
                }
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::TabList
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&self.items.iter().map(WidgetPod::id).collect::<Vec<_>>())
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
}
