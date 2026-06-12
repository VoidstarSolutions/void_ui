//! `MenuPanel` — the rich context-menu item-list widget.
//!
//! Lays out one row per [`MenuRowSpec`], tracks per-row hover, paints its own
//! background/border chrome plus separators, and fires [`MenuItemSelected`] when
//! an enabled action row is clicked. Selection is reported by the row's index
//! into the original spec list, so the [`super::view`] layer can map it straight
//! back to the item's callback.
//!
//! Rows are laid out in three columns: a leading gutter (check or icon glyph),
//! the label, and optional right-aligned keyboard-shortcut text. An item may
//! also carry a sub-title — a smaller muted second line under the label, which
//! makes that row taller; the gutter glyph and shortcut then align to the label
//! line, not the row center. The gutter is reserved for every row as soon as
//! any row is checkable or has an icon, so labels stay aligned. Rows come in
//! three kinds — selectable actions, separators, and muted non-interactive
//! section headers. A later chunk adds the submenu chevron. It deliberately
//! mirrors `dropdown_button`'s `MenuContent` (chrome look, hover model,
//! hit-testing) so the two menu surfaces stay visually consistent.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;
use crate::components::icon::{IconName, icon};

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
/// Gap between the leading-icon gutter and the label.
const ICON_GAP: f64 = 8.0;
/// Minimum gap between the label column and the trailing shortcut column, so a
/// long label and a long shortcut never collide.
const SHORTCUT_GAP: f64 = 24.0;
/// Vertical gap between an item's label and its sub-title line.
const SUBTITLE_GAP: f64 = 2.0;

/// One row of a [`MenuPanel`], as handed in by the view layer.
///
/// Display-only — callbacks stay in the view and are matched back up by the
/// row's index (see [`MenuItemSelected`]).
#[derive(Clone)]
pub enum MenuRowSpec {
    /// A selectable command row: optional leading icon, label, optional trailing
    /// keyboard-shortcut text. `checked` makes it a checkable row (the gutter
    /// shows a check when `Some(true)`); `disabled` mutes it and blocks
    /// selection.
    Action {
        label: ArcStr,
        subtitle: Option<ArcStr>,
        icon: Option<IconName>,
        shortcut: Option<ArcStr>,
        checked: Option<bool>,
        disabled: bool,
    },
    /// A non-interactive horizontal divider.
    Separator,
    /// A non-interactive, muted section header.
    Section { text: ArcStr },
}

impl PartialEq for MenuRowSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Action {
                    label: l1,
                    subtitle: sub1,
                    icon: i1,
                    shortcut: s1,
                    checked: c1,
                    disabled: d1,
                },
                Self::Action {
                    label: l2,
                    subtitle: sub2,
                    icon: i2,
                    shortcut: s2,
                    checked: c2,
                    disabled: d2,
                },
            ) => {
                // `IconName` (lucide's `Icon`) is compared by glyph since it
                // doesn't itself implement `PartialEq` — matches how
                // `dropdown_button` diffs icons.
                l1 == l2
                    && sub1 == sub2
                    && d1 == d2
                    && c1 == c2
                    && s1 == s2
                    && i1.map(char::from) == i2.map(char::from)
            }
            (Self::Separator, Self::Separator) => true,
            (Self::Section { text: t1 }, Self::Section { text: t2 }) => t1 == t2,
            _ => false,
        }
    }
}

/// Action emitted when the user selects the enabled action row at index `0`
/// (the row's position in the original [`MenuRowSpec`] list).
#[derive(Debug)]
pub struct MenuItemSelected(pub usize);

/// What kind of row this is — drives selectability, height, and paint.
enum RowKind {
    Action,
    Separator,
    Section,
}

/// Per-row state resolved at build time, plus the layout rect filled in by
/// `layout()`.
struct Row {
    /// Leading-gutter glyph (a check or an icon), if any. `None` for a
    /// checkable-but-unchecked row, which still reserves the gutter.
    gutter: Option<WidgetPod<dyn Widget>>,
    /// Whether this row reserves the leading gutter (checkable or has an icon).
    reserves_gutter: bool,
    /// `None` for separators; otherwise the row's label child.
    label: Option<WidgetPod<dyn Widget>>,
    /// Secondary line under the label, if any.
    subtitle: Option<WidgetPod<dyn Widget>>,
    /// Trailing keyboard-shortcut text, if any.
    shortcut: Option<WidgetPod<dyn Widget>>,
    disabled: bool,
    kind: RowKind,
    /// Local-coordinate bounds, populated during `layout`.
    rect: Rect,
}

impl Row {
    /// Whether a pointer/keyboard selection can land on this row.
    fn selectable(&self) -> bool {
        matches!(self.kind, RowKind::Action) && !self.disabled
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
            MenuRowSpec::Action {
                label,
                subtitle,
                icon,
                shortcut,
                checked,
                disabled,
            } => {
                // The gutter holds a check for a checkable row, otherwise the
                // icon. A checkable row reserves the gutter even when unchecked
                // (empty slot) so its label aligns with checked siblings.
                let gutter = match checked {
                    Some(true) => Some(Self::make_icon(IconName::Check, disabled, theme)),
                    Some(false) => None,
                    None => icon.map(|name| Self::make_icon(name, disabled, theme)),
                };
                Row {
                    gutter,
                    reserves_gutter: checked.is_some() || icon.is_some(),
                    label: Some(Self::make_label(&label, Self::label_color(disabled, theme), theme)),
                    subtitle: subtitle.map(|s| Self::make_subtitle(&s, disabled, theme)),
                    shortcut: shortcut.map(|s| Self::make_shortcut(&s, disabled, theme)),
                    disabled,
                    kind: RowKind::Action,
                    rect: Rect::ZERO,
                }
            }
            MenuRowSpec::Separator => Row {
                gutter: None,
                reserves_gutter: false,
                label: None,
                subtitle: None,
                shortcut: None,
                disabled: false,
                kind: RowKind::Separator,
                rect: Rect::ZERO,
            },
            MenuRowSpec::Section { text } => Row {
                gutter: None,
                reserves_gutter: false,
                label: Some(Self::make_label(&text, theme.palette.text_faint, theme)),
                subtitle: None,
                shortcut: None,
                disabled: false,
                kind: RowKind::Section,
                rect: Rect::ZERO,
            },
        }
    }

    fn make_label(text: &ArcStr, color: Color, theme: &Theme) -> WidgetPod<dyn Widget> {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties.insert(ContentColor::new(color));
        lbl.erased().to_pod()
    }

    /// Leading icon, dogfooding the `icon` component's widget builder.
    fn make_icon(name: IconName, disabled: bool, theme: &Theme) -> WidgetPod<dyn Widget> {
        icon(name)
            .color(Self::label_color(disabled, theme))
            .build_widget(theme)
            .erased()
            .to_pod()
    }

    /// Trailing keyboard-shortcut text — muted relative to the label.
    fn make_shortcut(text: &ArcStr, disabled: bool, theme: &Theme) -> WidgetPod<dyn Widget> {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties
            .insert(ContentColor::new(Self::shortcut_color(disabled, theme)));
        lbl.erased().to_pod()
    }

    /// Secondary line under the label — smaller (caption size) and muted.
    fn make_subtitle(text: &ArcStr, disabled: bool, theme: &Theme) -> WidgetPod<dyn Widget> {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.typography.size_caption))
            .prepare();
        lbl.properties
            .insert(ContentColor::new(Self::shortcut_color(disabled, theme)));
        lbl.erased().to_pod()
    }

    fn label_color(disabled: bool, theme: &Theme) -> Color {
        if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text
        }
    }

    fn shortcut_color(disabled: bool, theme: &Theme) -> Color {
        if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text_muted
        }
    }

    /// The label color for a row, given its kind — section headers are muted.
    fn row_label_color(kind: &RowKind, disabled: bool, theme: &Theme) -> Color {
        match kind {
            RowKind::Section => theme.palette.text_faint,
            _ => Self::label_color(disabled, theme),
        }
    }

    /// Whether any row reserves the leading gutter (checkable or icon-bearing) —
    /// if so all rows reserve it so labels align.
    fn has_gutter(&self) -> bool {
        self.rows.iter().any(|r| r.reserves_gutter)
    }

    /// Width reserved on the left for the gutter (glyph column + gap), or 0 when
    /// no row reserves it.
    fn gutter(&self) -> f64 {
        if self.has_gutter() {
            f64::from(self.theme.density.ui_font_size) + ICON_GAP
        } else {
            0.0
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
        let font_size = theme.density.ui_font_size;
        for row in &mut this.widget.rows {
            let gutter_fg = Self::label_color(row.disabled, theme);
            let label_fg = Self::row_label_color(&row.kind, row.disabled, theme);
            let shortcut_fg = Self::shortcut_color(row.disabled, theme);
            for (child, color) in [
                (&mut row.gutter, gutter_fg),
                (&mut row.label, label_fg),
                (&mut row.shortcut, shortcut_fg),
            ] {
                if let Some(child) = child {
                    let mut lbl = this.ctx.get_mut(child);
                    lbl.insert_prop(ContentColor::new(color));
                    let mut lbl = lbl.downcast::<Label>();
                    Label::insert_style(&mut lbl, StyleProperty::FontSize(font_size));
                }
            }
            // The sub-title uses the smaller caption size, so it's restyled
            // separately from the ui-font-size trio above.
            if let Some(subtitle) = &mut row.subtitle {
                let mut lbl = this.ctx.get_mut(subtitle);
                lbl.insert_prop(ContentColor::new(shortcut_fg));
                let mut lbl = lbl.downcast::<Label>();
                Label::insert_style(
                    &mut lbl,
                    StyleProperty::FontSize(theme.typography.size_caption),
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
            for child in [row.gutter, row.label, row.subtitle, row.shortcut]
                .into_iter()
                .flatten()
            {
                this.ctx.remove_child(child);
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
            for child in [
                &mut row.gutter,
                &mut row.label,
                &mut row.subtitle,
                &mut row.shortcut,
            ]
            .into_iter()
            .flatten()
            {
                ctx.register_child(child);
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
                let subtitle_extra =
                    SUBTITLE_GAP + f64::from(self.theme.typography.size_caption);
                let content: f64 = self
                    .rows
                    .iter()
                    .map(|r| match r.kind {
                        RowKind::Separator => SEPARATOR_ROW_HEIGHT,
                        _ if r.subtitle.is_some() => action_h + subtitle_extra,
                        _ => action_h,
                    })
                    .sum();
                Length::px(MENU_PAD_V * 2.0 + content)
            }
            Axis::Horizontal => {
                let inner_cross =
                    cross_length.map(|c| Length::px((c.get() - 2.0 * pad_h).max(0.0)));
                let gutter = self.gutter();
                let mut max_label = 0.0_f64;
                let mut max_shortcut = 0.0_f64;
                let mut measure = |pod: &mut WidgetPod<dyn Widget>| {
                    ctx.compute_length(
                        pod,
                        len_req.into(),
                        LayoutSize::maybe(Axis::Vertical, inner_cross),
                        Axis::Horizontal,
                        inner_cross,
                    )
                    .get()
                };
                for row in &mut self.rows {
                    if let Some(label) = &mut row.label {
                        max_label = max_label.max(measure(label));
                    }
                    // The sub-title shares the label column, so it widens it.
                    if let Some(subtitle) = &mut row.subtitle {
                        max_label = max_label.max(measure(subtitle));
                    }
                    if let Some(shortcut) = &mut row.shortcut {
                        max_shortcut = max_shortcut.max(measure(shortcut));
                    }
                }
                let shortcut_col = if max_shortcut > 0.0 {
                    SHORTCUT_GAP + max_shortcut
                } else {
                    0.0
                };
                let content = gutter + max_label + shortcut_col;
                Length::px(content.max(MIN_MENU_WIDTH) + 2.0 * pad_h)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_h = self.pad_h();
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let action_h = self.action_height();
        let subtitle_extra = SUBTITLE_GAP + f64::from(self.theme.typography.size_caption);
        let gutter = self.gutter();

        let mut y = MENU_PAD_V;
        for row in &mut self.rows {
            let has_subtitle = row.subtitle.is_some();
            let row_h = match row.kind {
                RowKind::Separator => SEPARATOR_ROW_HEIGHT,
                _ if has_subtitle => action_h + subtitle_extra,
                _ => action_h,
            };
            row.rect = Rect::from_origin_size(Point::new(0.0, y), Size::new(size.width, row_h));
            let full = Size::new((size.width - 2.0 * pad_h).max(0.0), row_h);
            let label_avail = Size::new((size.width - 2.0 * pad_h - gutter).max(0.0), row_h);

            // Label first — its vertical centerline is what the gutter glyph and
            // shortcut align to. With a sub-title the label is top-aligned and
            // the sub-title sits beneath it; otherwise the label is centred.
            let mut line_center = y + row_h * 0.5;
            if let Some(label) = &mut row.label {
                let label_size = ctx.compute_size(label, SizeDef::fit(label_avail), label_avail.into());
                ctx.run_layout(label, label_size);
                let label_y = if has_subtitle {
                    y + pad_v
                } else {
                    y + (row_h - label_size.height) * 0.5
                };
                ctx.place_child(label, Point::new(pad_h + gutter, label_y));
                line_center = label_y + label_size.height * 0.5;

                if let Some(subtitle) = &mut row.subtitle {
                    let sub_size =
                        ctx.compute_size(subtitle, SizeDef::fit(label_avail), label_avail.into());
                    ctx.run_layout(subtitle, sub_size);
                    let sub_y = label_y + label_size.height + SUBTITLE_GAP;
                    ctx.place_child(subtitle, Point::new(pad_h + gutter, sub_y));
                }
            }

            // Leading gutter glyph (check or icon), aligned to the label line.
            if let Some(g) = &mut row.gutter {
                let g_size = ctx.compute_size(g, SizeDef::MIN, full.into());
                ctx.run_layout(g, g_size);
                ctx.place_child(g, Point::new(pad_h, line_center - g_size.height * 0.5));
            }

            // Trailing shortcut, right-aligned to the label line.
            if let Some(shortcut) = &mut row.shortcut {
                let sc_size = ctx.compute_size(shortcut, SizeDef::MIN, full.into());
                ctx.run_layout(shortcut, sc_size);
                let sx = size.width - pad_h - sc_size.width;
                ctx.place_child(shortcut, Point::new(sx, line_center - sc_size.height * 0.5));
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
            if matches!(row.kind, RowKind::Separator) {
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
            .flat_map(|r| {
                [
                    r.gutter.as_ref(),
                    r.label.as_ref(),
                    r.subtitle.as_ref(),
                    r.shortcut.as_ref(),
                ]
            })
            .flatten()
            .map(WidgetPod::id)
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
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: None,
            disabled: false,
        }
    }

    fn disabled(label: &str) -> MenuRowSpec {
        MenuRowSpec::Action {
            label: label.into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: None,
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

    #[test]
    fn section_header_is_never_selectable() {
        let panel = panel_with(
            vec![MenuRowSpec::Section {
                text: "View".into(),
            }],
            vec![Rect::new(0.0, 0.0, 100.0, 20.0)],
        );
        assert!(!panel.rows[0].selectable());
        assert_eq!(panel.hit_row(Point::new(50.0, 10.0)), None);
    }

    #[test]
    fn checkable_rows_reserve_the_gutter() {
        let checkable = |checked: bool| MenuRowSpec::Action {
            label: "x".into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: Some(checked),
            disabled: false,
        };
        let panel = panel_with(vec![checkable(true), checkable(false)], vec![Rect::ZERO; 2]);
        // Checked row has a check glyph; unchecked has none — but both reserve
        // the gutter so their labels align.
        assert!(panel.rows[0].gutter.is_some());
        assert!(panel.rows[1].gutter.is_none());
        assert!(panel.rows[0].reserves_gutter);
        assert!(panel.rows[1].reserves_gutter);
        assert!(panel.has_gutter());
    }
}
