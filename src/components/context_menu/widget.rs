//! `MenuPanel` — the rich context-menu item-list widget.
//!
//! Lays out one row per [`MenuRowSpec`], tracks per-row hover, paints its own
//! background/border chrome plus separators, and fires [`MenuItemSelected`] when
//! an enabled action row is clicked. Selection is reported by the row's index
//! into the original spec list, so the [`super::view`] layer can map it straight
//! back to the item's callback.
//!
//! This is the foundation the context menu is built on; later chunks enrich the
//! row content (leading icon/check gutter, trailing shortcut, submenu chevron),
//! but the layout/hit-test/selection spine lives here. It deliberately mirrors
//! `dropdown_button`'s `MenuContent` (chrome look, hover model, hit-testing) so
//! the two menu surfaces stay visually consistent.

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
/// Total height of a separator row (line centered within it).
const SEPARATOR_ROW_HEIGHT: f64 = 9.0;
/// Minimum menu width in logical pixels, keeping a readable popup even when all
/// item labels are very short.
const MIN_MENU_WIDTH: f64 = 80.0;

/// One row of a [`MenuPanel`], as handed in by the view layer.
///
/// Display-only — callbacks stay in the view and are matched back up by the
/// row's index (see [`MenuItemSelected`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuRowSpec {
    /// A selectable command row. `disabled` mutes it and blocks selection.
    Action { label: ArcStr, disabled: bool },
    /// A non-interactive horizontal divider.
    Separator,
}

/// Action emitted when the user selects the enabled action row at index `0`
/// (the row's position in the original [`MenuRowSpec`] list).
#[derive(Debug)]
pub struct MenuItemSelected(pub usize);

/// Per-row state resolved at build time, plus the layout rect filled in by
/// `layout()`.
struct Row {
    /// `None` for separators; otherwise the row's label child.
    label: Option<WidgetPod<dyn Widget>>,
    disabled: bool,
    is_separator: bool,
    /// Local-coordinate bounds, populated during `layout`.
    rect: Rect,
}

impl Row {
    /// Whether a pointer/keyboard selection can land on this row.
    fn selectable(&self) -> bool {
        !self.is_separator && !self.disabled
    }
}

/// Rich item-list widget for a context menu.
pub struct MenuPanel {
    rows: Vec<Row>,
    /// Index (into `rows`) of the row the pointer is currently over — only ever
    /// a selectable row.
    hover_index: Option<usize>,
    theme: Theme,
}

impl MenuPanel {
    #[must_use]
    pub fn new(specs: impl IntoIterator<Item = MenuRowSpec>, theme: &Theme) -> Self {
        let rows = specs
            .into_iter()
            .map(|spec| Self::make_row(spec, theme))
            .collect();
        Self {
            rows,
            hover_index: None,
            theme: *theme,
        }
    }

    fn make_row(spec: MenuRowSpec, theme: &Theme) -> Row {
        match spec {
            MenuRowSpec::Action { label, disabled } => Row {
                label: Some(Self::make_label(&label, disabled, theme)),
                disabled,
                is_separator: false,
                rect: Rect::ZERO,
            },
            MenuRowSpec::Separator => Row {
                label: None,
                disabled: false,
                is_separator: true,
                rect: Rect::ZERO,
            },
        }
    }

    fn make_label(text: &ArcStr, disabled: bool, theme: &Theme) -> WidgetPod<dyn Widget> {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties
            .insert(ContentColor::new(Self::label_color(disabled, theme)));
        lbl.erased().to_pod()
    }

    fn label_color(disabled: bool, theme: &Theme) -> masonry::peniko::Color {
        if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text
        }
    }

    fn action_height(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
            + 2.0 * f64::from(self.theme.density.button_pad_v)
    }

    fn pad_h(&self) -> f64 {
        f64::from(self.theme.density.button_pad_h)
    }

    /// The selectable row containing `local_pos`, if any.
    fn hit_row(&self, local_pos: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.selectable() && r.rect.contains(local_pos))
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        let origin = ctx.to_window(Point::ZERO);
        window_pos - origin.to_vec2()
    }
}

// --- MARK: WIDGETMUT SETTERS
impl MenuPanel {
    /// Restyle existing labels and store the new theme — the panel persists
    /// across rebuilds (e.g. a host theme swap), so a live theme change must be
    /// reflected without rebuilding the children.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        for row in &mut this.widget.rows {
            let color = Self::label_color(row.disabled, theme);
            if let Some(label) = &mut row.label {
                let mut lbl = this.ctx.get_mut(label);
                lbl.insert_prop(ContentColor::new(color));
                let mut lbl = lbl.downcast::<Label>();
                Label::insert_style(
                    &mut lbl,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Replace the row list — the panel persists across rebuilds, so a changed
    /// item set must be applied in place.
    pub fn set_rows(this: &mut WidgetMut<'_, Self>, specs: impl IntoIterator<Item = MenuRowSpec>) {
        let old: Vec<_> = this.widget.rows.drain(..).collect();
        for row in old {
            if let Some(label) = row.label {
                this.ctx.remove_child(label);
            }
        }
        let theme = this.widget.theme;
        this.widget.rows = specs
            .into_iter()
            .map(|spec| Self::make_row(spec, &theme))
            .collect();
        this.widget.hover_index = None;
        this.ctx.children_changed();
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }
}

impl Widget for MenuPanel {
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
                let new_hover = self.hit_row(local);
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
                if let Some(i) = self.hit_row(local) {
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
        for row in &mut self.rows {
            if let Some(label) = &mut row.label {
                ctx.register_child(label);
            }
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
        let action_h = self.action_height();
        let pad_h = self.pad_h();
        match axis {
            Axis::Vertical => {
                let content: f64 = self
                    .rows
                    .iter()
                    .map(|r| {
                        if r.is_separator {
                            SEPARATOR_ROW_HEIGHT
                        } else {
                            action_h
                        }
                    })
                    .sum();
                Length::px(MENU_PAD_V * 2.0 + content)
            }
            Axis::Horizontal => {
                let inner_cross =
                    cross_length.map(|c| Length::px((c.get() - 2.0 * pad_h).max(0.0)));
                let mut max_w = MIN_MENU_WIDTH;
                for row in &mut self.rows {
                    if let Some(label) = &mut row.label {
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
                }
                Length::px(max_w + 2.0 * pad_h)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_h = self.pad_h();
        let action_h = self.action_height();
        let label_available = Size::new((size.width - 2.0 * pad_h).max(0.0), action_h);

        let mut y = MENU_PAD_V;
        for row in &mut self.rows {
            let row_h = if row.is_separator {
                SEPARATOR_ROW_HEIGHT
            } else {
                action_h
            };
            row.rect = Rect::from_origin_size(Point::new(0.0, y), Size::new(size.width, row_h));

            if let Some(label) = &mut row.label {
                let label_size =
                    ctx.compute_size(label, SizeDef::fit(label_available), label_available.into());
                ctx.run_layout(label, label_size);
                let label_y = y + (row_h - label_size.height) * 0.5;
                ctx.place_child(label, Point::new(pad_h, label_y));
            }

            y += row_h;
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;
        let pad_h = self.pad_h();

        let bg_rect =
            RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box_size(), CORNER_RADIUS);
        painter.fill(bg_rect, p.surface_hi).draw();
        painter
            .stroke(bg_rect, &Stroke::new(BORDER_WIDTH), p.border_strong)
            .draw();

        if let Some(i) = self.hover_index
            && let Some(row) = self.rows.get(i)
        {
            painter.fill(row.rect, p.surface_2).draw();
        }

        for row in &self.rows {
            if row.is_separator {
                let y = row.rect.y0 + row.rect.height() * 0.5;
                let line = masonry::kurbo::Line::new(
                    Point::new(row.rect.x0 + pad_h, y),
                    Point::new(row.rect.x1 - pad_h, y),
                );
                painter
                    .stroke(line, &Stroke::new(BORDER_WIDTH), p.border_strong)
                    .draw();
            }
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
        let ids: Vec<_> = self
            .rows
            .iter()
            .filter_map(|r| r.label.as_ref().map(WidgetPod::id))
            .collect();
        ChildrenIds::from_slice(&ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `MenuPanel` and seeds row rects directly, as if a layout pass
    /// had already run — `hit_row` only reads `rect`/`selectable`.
    fn panel_with(specs: Vec<MenuRowSpec>, rects: Vec<Rect>) -> MenuPanel {
        let theme = Theme::default();
        let mut panel = MenuPanel::new(specs, &theme);
        for (row, rect) in panel.rows.iter_mut().zip(rects) {
            row.rect = rect;
        }
        panel
    }

    fn action(label: &str) -> MenuRowSpec {
        MenuRowSpec::Action {
            label: label.into(),
            disabled: false,
        }
    }

    fn disabled(label: &str) -> MenuRowSpec {
        MenuRowSpec::Action {
            label: label.into(),
            disabled: true,
        }
    }

    #[test]
    fn hit_row_finds_an_enabled_action() {
        let panel = panel_with(
            vec![action("a"), action("b")],
            vec![
                Rect::new(0.0, 0.0, 100.0, 20.0),
                Rect::new(0.0, 20.0, 100.0, 40.0),
            ],
        );
        assert_eq!(panel.hit_row(Point::new(50.0, 10.0)), Some(0));
        assert_eq!(panel.hit_row(Point::new(50.0, 30.0)), Some(1));
    }

    #[test]
    fn hit_row_skips_separators_and_disabled_rows() {
        let panel = panel_with(
            vec![MenuRowSpec::Separator, disabled("nope"), action("ok")],
            vec![
                Rect::new(0.0, 0.0, 100.0, 9.0),
                Rect::new(0.0, 9.0, 100.0, 29.0),
                Rect::new(0.0, 29.0, 100.0, 49.0),
            ],
        );
        assert_eq!(panel.hit_row(Point::new(50.0, 4.0)), None, "separator");
        assert_eq!(panel.hit_row(Point::new(50.0, 19.0)), None, "disabled");
        assert_eq!(panel.hit_row(Point::new(50.0, 39.0)), Some(2), "enabled");
    }

    #[test]
    fn hit_row_returns_none_outside_all_rects() {
        let panel = panel_with(vec![action("a")], vec![Rect::new(0.0, 0.0, 100.0, 20.0)]);
        assert_eq!(panel.hit_row(Point::new(50.0, 50.0)), None);
    }

    #[test]
    fn separator_is_never_selectable() {
        let panel = panel_with(vec![MenuRowSpec::Separator], vec![Rect::ZERO]);
        assert!(!panel.rows[0].selectable());
    }
}
