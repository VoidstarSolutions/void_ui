//! `MenuContent` — item-list widget rendered inside a `PopoverLayer`.
//!
//! Handles layout of label children, per-item hover tracking, and selection.
//! No `Layer` impl — that's provided by the wrapping `PopoverLayer`.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetId, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::widget::{DropdownButtonAction, ThemedDropdownButton};
use crate::Theme;

/// Vertical padding above and below the item list.
const MENU_PAD_V: f64 = 4.0;

/// Item-list widget for a dropdown menu.
///
/// Lays out one [`Label`] per item, tracks hover, and fires selection back to
/// the creator [`ThemedDropdownButton`] via `mutate_later`.  Pointer
/// interaction is handled at this widget level; labels are display-only.
pub struct MenuContent {
    labels: Vec<WidgetPod<dyn Widget>>,
    /// Rects populated during `layout()` — used for hit-testing in local coords.
    item_rects: Vec<Rect>,
    hover_index: Option<usize>,
    /// ID of the [`ThemedDropdownButton`] that owns the containing layer.
    creator: WidgetId,
    theme: Theme,
}

impl MenuContent {
    #[must_use]
    pub fn new(items: impl IntoIterator<Item = ArcStr>, creator: WidgetId, theme: &Theme) -> Self {
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

    fn select_item_via_mutate(&self, ctx: &mut EventCtx<'_>, index: usize) {
        let creator = self.creator;
        ctx.mutate_later(creator, move |mut w| {
            let mut w = w.downcast::<ThemedDropdownButton>();
            // Remove the PopoverLayer using the creator's stored layer id.
            if let Some(layer_id) = w.widget.menu_layer_id.take() {
                w.ctx.remove_layer(layer_id);
            }
            w.widget.open = false;
            w.ctx
                .submit_action::<DropdownButtonAction>(DropdownButtonAction::ItemSelected(index));
        });
    }
}

impl Widget for MenuContent {
    type Action = NoAction;

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
                let local = Self::to_local(ctx, current.logical_point());
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

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

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
                let n_f64 = f64::from(u32::try_from(n).unwrap_or(u32::MAX));
                Length::px(MENU_PAD_V * 2.0 + item_h * n_f64)
            }
            Axis::Horizontal => {
                let inner_cross =
                    cross_length.map(|c| Length::px((c.get() - 2.0 * pad_h).max(0.0)));
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
        self.item_rects.clear();

        let pad_h = self.pad_h();
        let item_h = self.item_height();
        let label_available = Size::new((size.width - 2.0 * pad_h).max(0.0), item_h);

        let mut y = MENU_PAD_V;
        for label in &mut self.labels {
            let item_rect =
                Rect::from_origin_size(Point::new(0.0, y), Size::new(size.width, item_h));
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
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;
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
