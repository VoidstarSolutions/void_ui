//! `DropdownMenuLayer` — window-level floating menu for `DropdownButton`.
//!
//! Implements both [`Widget`] and [`Layer`] so masonry renders it above all
//! other content. Item hover is tracked manually (pointer events are handled
//! at the parent level; labels are display-only children). Selection and
//! outside-click dismissal communicate back to the creator widget via
//! [`EventCtx::mutate_later`].

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, Layer, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetId, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::widget::{DropdownButtonAction, ThemedDropdownButton};
use crate::Theme;

/// Vertical padding above and below the item list within the menu container.
const MENU_PAD_V: f64 = 4.0;
/// Corner radius of the menu container.
const MENU_CORNER_RADIUS: f64 = 5.0;
/// Border width of the menu container.
const MENU_BORDER_WIDTH: f64 = 1.0;

/// Window-level floating menu widget.
///
/// Created by [`ThemedDropdownButton`] when the chevron is clicked; removed
/// when an item is selected or when a click outside the menu bounds fires
/// in [`Layer::capture_pointer_event`].
///
/// Child [`Label`] pods provide text rendering; all pointer interaction is
/// handled at this parent level (propagation is disabled so labels are
/// display-only).
pub struct DropdownMenuLayer {
    labels: Vec<WidgetPod<dyn Widget>>,
    /// Rects populated during `layout()` — used for hit-testing in local coords.
    item_rects: Vec<Rect>,
    hover_index: Option<usize>,
    /// ID of the [`ThemedDropdownButton`] that owns this layer.
    creator: WidgetId,
    theme: Theme,
    /// Cached size from last layout — used in `capture_pointer_event`.
    last_size: Size,
}

impl DropdownMenuLayer {
    #[must_use]
    pub fn new(
        items: impl IntoIterator<Item = ArcStr>,
        creator: WidgetId,
        theme: &Theme,
    ) -> Self {
        let labels = items
            .into_iter()
            .map(|text| {
                let mut lbl = Label::new(text)
                    .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
                    .prepare();
                lbl.properties.insert(ContentColor::new(theme.palette.text));
                lbl.erased().to_pod()
            })
            .collect();
        Self {
            labels,
            item_rects: Vec::new(),
            hover_index: None,
            creator,
            theme: *theme,
            last_size: Size::ZERO,
        }
    }

    fn item_height(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
            + 2.0 * f64::from(self.theme.density.button_pad_v)
    }

    fn pad_h(&self) -> f64 {
        f64::from(self.theme.density.button_pad_h)
    }

    fn hit_item(&self, local_pos: Point) -> Option<usize> {
        self.item_rects.iter().position(|r| r.contains(local_pos))
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        let origin = ctx.to_window(Point::ZERO);
        window_pos - origin.to_vec2()
    }

    fn close_via_mutate(&self, ctx: &mut EventCtx<'_>) {
        let self_id = ctx.widget_id();
        let creator = self.creator;
        ctx.mutate_later(creator, move |mut w| {
            let mut w = w.downcast::<ThemedDropdownButton>();
            w.widget.open = false;
            w.widget.menu_layer_id = None;
            w.ctx.remove_layer(self_id);
        });
    }

    fn select_item_via_mutate(&self, ctx: &mut EventCtx<'_>, index: usize) {
        let self_id = ctx.widget_id();
        let creator = self.creator;
        ctx.mutate_later(creator, move |mut w| {
            let mut w = w.downcast::<ThemedDropdownButton>();
            w.widget.open = false;
            w.widget.menu_layer_id = None;
            w.ctx.remove_layer(self_id);
            w.ctx.submit_action::<DropdownButtonAction>(DropdownButtonAction::ItemSelected(index));
        });
    }
}

impl Widget for DropdownMenuLayer {
    type Action = NoAction;

    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let local = Self::to_local(ctx,current.logical_point());
                let new_hover = self.hit_item(local);
                if new_hover != self.hover_index {
                    self.hover_index = new_hover;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.capture_pointer();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) if ctx.is_active() && ctx.is_hovered() => {
                let local = Self::to_local(ctx, state.logical_point());
                if let Some(i) = self.hit_item(local) {
                    self.select_item_via_mutate(ctx, i);
                }
            }
            PointerEvent::Leave(_) if self.hover_index.is_some() => {
                self.hover_index = None;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for label in &mut self.labels {
            ctx.register_child(label);
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
        let item_h = self.item_height();
        let pad_h = self.pad_h();
        let n = self.labels.len();
        match axis {
            Axis::Vertical => {
                // Report the same intrinsic height for all query types: the menu
                // is always exactly as tall as its content. LayerStack sizes layers
                // with SizeDef::MAX (MaxContent on both axes) — returning f64::MAX
                // here gives the widget a near-infinite height which breaks Vello's
                // GPU path renderer for the container background fill.
                let n_f64 = f64::from(u32::try_from(n).unwrap_or(u32::MAX));
                let total = MENU_PAD_V * 2.0 + item_h * n_f64;
                Length::px(total)
            }
            Axis::Horizontal => {
                let inner_cross = cross_length.map(|c| Length::px((c.get() - 2.0 * pad_h).max(0.0)));
                let mut max_w = 80.0f64;
                for label in &mut self.labels {
                    let w = ctx
                        .compute_length(
                            label,
                            len_req.into(),
                            LayoutSize::maybe(Axis::Vertical, inner_cross),
                            Axis::Horizontal,
                            inner_cross,
                        )
                        .get();
                    max_w = max_w.max(w);
                }
                Length::px(max_w + 2.0 * pad_h)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.last_size = size;
        self.item_rects.clear();

        let pad_h = self.pad_h();
        let item_h = self.item_height();
        let label_available = Size::new((size.width - 2.0 * pad_h).max(0.0), item_h);

        let mut y = MENU_PAD_V;
        for label in &mut self.labels {
            let item_rect = Rect::from_origin_size(Point::new(0.0, y), Size::new(size.width, item_h));
            self.item_rects.push(item_rect);

            let label_size =
                ctx.compute_size(label, SizeDef::fit(label_available), label_available.into());
            ctx.run_layout(label, label_size);
            let label_y = y + (item_h - label_size.height) * 0.5;
            ctx.place_child(label, Point::new(pad_h, label_y));

            y += item_h;
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

        // Container background + border
        let rrect = RoundedRect::from_origin_size(Point::ORIGIN, size, MENU_CORNER_RADIUS);
        painter.fill(rrect, p.surface_hi).draw();
        painter
            .stroke(rrect, &Stroke::new(MENU_BORDER_WIDTH), p.border_strong)
            .draw();

        // Per-item hover highlight (painted behind labels)
        if let Some(i) = self.hover_index
            && let Some(&rect) = self.item_rects.get(i)
        {
            painter.fill(rect, p.surface_2).draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Menu
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self.labels.iter().map(WidgetPod::id).collect();
        ChildrenIds::from_slice(&ids)
    }
}

impl Layer for DropdownMenuLayer {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Close the menu when the user clicks outside its bounds.
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            let local = Self::to_local(ctx,state.logical_point());
            let bounds = Rect::from_origin_size(Point::ZERO, self.last_size);
            if !bounds.contains(local) {
                self.close_via_mutate(ctx);
            }
        }
    }
}
