//! `MenuContent` — item-list widget hosted inside `ThemedDropdownButton`'s
//! `overlay_host: AnchoredOverlay`.
//!
//! Handles layout of label children, per-item hover tracking, chrome
//! painting, and selection — selection is reported to the trigger via
//! [`MenuItemSelected`], which bubbles through [`Widget::on_action`] to
//! [`ThemedDropdownButton::on_action`](super::widget::ThemedDropdownButton).

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;

/// Vertical padding above and below the item list.
const MENU_PAD_V: f64 = 4.0;
/// Corner radius of the menu's background chrome.
const CORNER_RADIUS: f64 = 5.0;
/// Border width of the menu's background chrome.
const BORDER_WIDTH: f64 = 1.0;

/// Action emitted when the user selects item `0` (the index) from the menu.
///
/// Bubbles up to [`ThemedDropdownButton::on_action`](super::widget::ThemedDropdownButton),
/// which closes the menu and re-emits a [`super::widget::DropdownButtonAction::ItemSelected`].
#[derive(Debug)]
pub struct MenuItemSelected(pub usize);

/// Item-list widget for a dropdown menu.
///
/// Lays out one [`Label`] per item, tracks hover, paints its own
/// background/border chrome, and fires [`MenuItemSelected`] on selection.
pub struct MenuContent {
    labels: Vec<WidgetPod<dyn Widget>>,
    /// Rects populated during `layout()` — used for hit-testing in local coords.
    item_rects: Vec<Rect>,
    hover_index: Option<usize>,
    theme: Theme,
}

impl MenuContent {
    #[must_use]
    pub fn new(items: impl IntoIterator<Item = ArcStr>, theme: &Theme) -> Self {
        let labels = items
            .into_iter()
            .map(|text| Self::make_label(&text, theme))
            .collect();
        Self {
            labels,
            item_rects: Vec::new(),
            hover_index: None,
            theme: *theme,
        }
    }

    fn make_label(text: &ArcStr, theme: &Theme) -> WidgetPod<dyn Widget> {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties.insert(ContentColor::new(theme.palette.text));
        lbl.erased().to_pod()
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
}

// --- MARK: WIDGETMUT SETTERS
impl MenuContent {
    /// Restyle existing labels and store the new theme — needed because, unlike
    /// before, `MenuContent` is now a permanent child rather than rebuilt fresh
    /// each time the menu opens.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        for label in &mut this.widget.labels {
            let mut lbl = this.ctx.get_mut(label);
            lbl.insert_prop(ContentColor::new(theme.palette.text));
            let mut lbl = lbl.downcast::<Label>();
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(theme.density.ui_font_size),
            );
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Replace the item list — needed for the same reason as `set_theme`:
    /// `MenuContent` persists across opens, so a live item-list change must
    /// be reflected even while the menu has never been (re)opened.
    pub fn set_items(this: &mut WidgetMut<'_, Self>, items: impl IntoIterator<Item = ArcStr>) {
        for label in this.widget.labels.drain(..) {
            this.ctx.remove_child(label);
        }
        let theme = this.widget.theme;
        this.widget.labels = items
            .into_iter()
            .map(|text| Self::make_label(&text, &theme))
            .collect();
        this.widget.hover_index = None;
        this.ctx.children_changed();
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }
}

impl Widget for MenuContent {
    type Action = MenuItemSelected;

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
                    ctx.submit_action::<Self::Action>(MenuItemSelected(i));
                    ctx.set_handled();
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
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;

        // Background/border chrome — formerly drawn by the wrapping
        // `PopoverLayer` (`popover_layer.rs::paint`); `MenuContent` now paints
        // it directly since it's hosted in-tree, with no such wrapper.
        let bg_rect =
            RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box_size(), CORNER_RADIUS);
        painter.fill(bg_rect, p.surface_hi).draw();
        painter
            .stroke(bg_rect, &Stroke::new(BORDER_WIDTH), p.border_strong)
            .draw();

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `MenuContent` and seeds `item_rects` directly, as if a
    /// layout pass had already run — `hit_item` only reads that field.
    fn menu_with_rects(rects: Vec<Rect>) -> MenuContent {
        let theme = Theme::default();
        let items: Vec<ArcStr> = rects.iter().map(|_| ArcStr::from("item")).collect();
        let mut menu = MenuContent::new(items, &theme);
        menu.item_rects = rects;
        menu
    }

    #[test]
    fn hit_item_finds_the_containing_rect() {
        let menu = menu_with_rects(vec![
            Rect::new(0.0, 0.0, 100.0, 20.0),
            Rect::new(0.0, 20.0, 100.0, 40.0),
        ]);
        assert_eq!(menu.hit_item(Point::new(50.0, 10.0)), Some(0));
        assert_eq!(menu.hit_item(Point::new(50.0, 30.0)), Some(1));
    }

    #[test]
    fn hit_item_returns_none_outside_all_rects() {
        let menu = menu_with_rects(vec![Rect::new(0.0, 0.0, 100.0, 20.0)]);
        assert_eq!(
            menu.hit_item(Point::new(50.0, 50.0)),
            None,
            "below the list"
        );
        assert_eq!(
            menu.hit_item(Point::new(-10.0, 10.0)),
            None,
            "left of the list"
        );
    }

    #[test]
    fn hit_item_on_an_empty_list_is_always_none() {
        let menu = menu_with_rects(Vec::new());
        assert_eq!(menu.hit_item(Point::new(0.0, 0.0)), None);
    }

    #[test]
    fn hit_item_resolves_a_shared_edge_to_the_rect_that_owns_it() {
        // `Rect::contains` is half-open ([x0,x1) x [y0,y1)), so a point sitting
        // exactly on the boundary between two adjacent rects belongs to
        // whichever one's range includes that edge — never both, never neither.
        let menu = menu_with_rects(vec![
            Rect::new(0.0, 0.0, 100.0, 20.0),
            Rect::new(0.0, 20.0, 100.0, 40.0),
        ]);
        assert_eq!(
            menu.hit_item(Point::new(50.0, 0.0)),
            Some(0),
            "top edge of rect 0"
        );
        assert_eq!(
            menu.hit_item(Point::new(50.0, 20.0)),
            Some(1),
            "shared edge belongs to rect 1, not rect 0"
        );
    }
}
