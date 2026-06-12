//! `MenuItemNode` — the per-row accessibility + layout wrapper inside a
//! [`MenuPanel`](super::widget::MenuPanel).
//!
//! Each menu row is wrapped in one of these so a screen reader sees a real
//! `MenuItem` / `MenuItemCheckBox` / `Splitter` / section label — with name,
//! checked state, disabled state, and position-in-set — rather than anonymous
//! text runs. This mirrors `tabs`' `TabItemNode` and `sidebar`'s
//! `SidebarNavItemNode`. The node also owns the row's three-column layout (gutter
//! glyph · label + optional sub-title · trailing shortcut); `MenuPanel` stacks
//! the nodes vertically and paints the chrome/hover/focus-ring/separators around
//! their placed rects.

use masonry::accesskit::{Action, Node, Role, Toggled};
use masonry::core::{
    AccessCtx, AccessEvent, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::widget::MenuRowSpec;
use crate::Theme;
use crate::components::icon::{IconName, icon};

/// Total height of a separator row (line centered within it).
pub(crate) const SEPARATOR_ROW_HEIGHT: f64 = 9.0;
/// Gap between the leading-glyph gutter and the label.
const ICON_GAP: f64 = 8.0;
/// Minimum gap between the label column and the trailing shortcut column.
const SHORTCUT_GAP: f64 = 24.0;
/// Vertical gap between an item's label and its sub-title line.
const SUBTITLE_GAP: f64 = 2.0;

/// What kind of row a [`MenuItemNode`] represents — drives selectability (in
/// `MenuPanel`), height, role, and paint.
#[derive(Clone, Copy)]
pub(crate) enum RowKind {
    Action,
    Separator,
    Section,
}

/// Action a [`MenuItemNode`] submits when activated via the accessibility tree
/// (an AT "invoke" / `Action::Click`). Carries the row's index so
/// `MenuPanel::on_action` can re-emit it as a `MenuAction::Selected`.
#[derive(Debug)]
pub(crate) struct NodeActivated(pub(crate) usize);

/// The leading-gutter glyph width (square, font-sized) reserved when any row in
/// a menu has an icon or is checkable.
#[must_use]
pub(crate) fn gutter_glyph_width(theme: &Theme) -> f64 {
    f64::from(theme.density.ui_font_size) + ICON_GAP
}

/// Whether a spec reserves the gutter (checkable or icon-bearing).
pub(crate) fn reserves_gutter(spec: &MenuRowSpec) -> bool {
    matches!(
        spec,
        MenuRowSpec::Action {
            checked: Some(_),
            ..
        } | MenuRowSpec::Action {
            icon: Some(_),
            ..
        }
    )
}

fn label_color(disabled: bool, theme: &Theme) -> Color {
    if disabled {
        theme.palette.text_faint
    } else {
        theme.palette.text
    }
}

fn muted_color(disabled: bool, theme: &Theme) -> Color {
    if disabled {
        theme.palette.text_faint
    } else {
        theme.palette.text_muted
    }
}

fn make_text(text: &ArcStr, size: f32, color: Color) -> WidgetPod<dyn Widget> {
    let mut lbl = Label::new(text.clone())
        .with_style(StyleProperty::FontSize(size))
        .prepare();
    lbl.properties.insert(ContentColor::new(color));
    lbl.erased().to_pod()
}

fn make_icon(name: IconName, disabled: bool, theme: &Theme) -> WidgetPod<dyn Widget> {
    icon(name)
        .color(label_color(disabled, theme))
        .build_widget(theme)
        .erased()
        .to_pod()
}

/// Per-row accessibility + layout wrapper. See module docs.
pub(crate) struct MenuItemNode {
    gutter: Option<WidgetPod<dyn Widget>>,
    label: Option<WidgetPod<dyn Widget>>,
    subtitle: Option<WidgetPod<dyn Widget>>,
    shortcut: Option<WidgetPod<dyn Widget>>,
    kind: RowKind,
    /// Accessible name for action/section rows.
    name: ArcStr,
    checked: Option<bool>,
    disabled: bool,
    /// Row index within the panel, carried into [`NodeActivated`].
    index: usize,
    /// 1-based position and total over the action items, for a11y.
    set_pos: Option<(usize, usize)>,
    /// Shared gutter column width (0 when no row reserves it).
    gutter_width: f64,
    theme: Theme,
}

impl MenuItemNode {
    pub(crate) fn new(
        spec: MenuRowSpec,
        gutter_width: f64,
        index: usize,
        set_pos: Option<(usize, usize)>,
        theme: &Theme,
    ) -> NewWidget<Self> {
        let size = theme.density.ui_font_size;
        let caption = theme.typography.size_caption;
        let node = match spec {
            MenuRowSpec::Action {
                label,
                subtitle,
                icon,
                shortcut,
                checked,
                disabled,
            } => {
                // The gutter holds a check for a checkable row, otherwise the
                // icon; a checkable-but-unchecked row reserves it but is empty.
                let gutter = match checked {
                    Some(true) => Some(make_icon(IconName::Check, disabled, theme)),
                    Some(false) => None,
                    None => icon.map(|name| make_icon(name, disabled, theme)),
                };
                MenuItemNode {
                    gutter,
                    label: Some(make_text(&label, size, label_color(disabled, theme))),
                    subtitle: subtitle.map(|s| make_text(&s, caption, muted_color(disabled, theme))),
                    shortcut: shortcut.map(|s| make_text(&s, size, muted_color(disabled, theme))),
                    kind: RowKind::Action,
                    name: label,
                    checked,
                    disabled,
                    index,
                    set_pos,
                    gutter_width,
                    theme: *theme,
                }
            }
            MenuRowSpec::Separator => MenuItemNode {
                gutter: None,
                label: None,
                subtitle: None,
                shortcut: None,
                kind: RowKind::Separator,
                name: ArcStr::from(""),
                checked: None,
                disabled: false,
                index,
                set_pos: None,
                gutter_width,
                theme: *theme,
            },
            MenuRowSpec::Section { text } => MenuItemNode {
                gutter: None,
                label: Some(make_text(&text, size, theme.palette.text_faint)),
                subtitle: None,
                shortcut: None,
                kind: RowKind::Section,
                name: text,
                checked: None,
                disabled: false,
                index,
                set_pos: None,
                gutter_width,
                theme: *theme,
            },
        };
        NewWidget::new(node)
    }

    /// Restyle this row's children for a new theme.
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        this.widget.theme = *theme;
        let disabled = this.widget.disabled;
        let is_section = matches!(this.widget.kind, RowKind::Section);
        let label_fg = if is_section {
            theme.palette.text_faint
        } else {
            label_color(disabled, theme)
        };
        let muted_fg = muted_color(disabled, theme);
        let size = theme.density.ui_font_size;
        let caption = theme.typography.size_caption;

        for (child, color, fsize) in [
            (&mut this.widget.gutter, label_color(disabled, theme), size),
            (&mut this.widget.label, label_fg, size),
            (&mut this.widget.shortcut, muted_fg, size),
            (&mut this.widget.subtitle, muted_fg, caption),
        ] {
            if let Some(child) = child {
                let mut lbl = this.ctx.get_mut(child);
                lbl.insert_prop(ContentColor::new(color));
                let mut lbl = lbl.downcast::<Label>();
                Label::insert_style(&mut lbl, StyleProperty::FontSize(fsize));
            }
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    fn action_height(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
            + 2.0 * f64::from(self.theme.density.button_pad_v)
    }

    /// This row's full height, including a sub-title's extra line.
    pub(crate) fn row_height(&self) -> f64 {
        if matches!(self.kind, RowKind::Separator) {
            return SEPARATOR_ROW_HEIGHT;
        }
        let base = self.action_height();
        if self.subtitle.is_some() {
            base + SUBTITLE_GAP + f64::from(self.theme.typography.size_caption)
        } else {
            base
        }
    }
}

impl Widget for MenuItemNode {
    type Action = NodeActivated;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for child in [
            &mut self.gutter,
            &mut self.label,
            &mut self.subtitle,
            &mut self.shortcut,
        ]
        .into_iter()
        .flatten()
        {
            ctx.register_child(child);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Sync masonry's system-level disabled flag on attach so event routing
        // and the accessibility pass (`node.set_disabled`) stay correct —
        // mirrors `TabItemNode`/checkbox.
        if matches!(event, Update::WidgetAdded) {
            ctx.set_disabled(self.disabled);
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        // An AT "invoke" on an enabled action behaves like a click — re-emit as
        // a `NodeActivated` so the panel turns it into a `MenuAction::Selected`.
        if matches!(self.kind, RowKind::Action)
            && !self.disabled
            && event.action == Action::Click
        {
            ctx.submit_action::<Self::Action>(NodeActivated(self.index));
            ctx.set_handled();
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Vertical => Length::px(self.row_height()),
            Axis::Horizontal => {
                let mut measure = |pod: &mut WidgetPod<dyn Widget>| {
                    ctx.compute_length(
                        pod,
                        len_req.into(),
                        LayoutSize::maybe(Axis::Vertical, cross_length),
                        Axis::Horizontal,
                        cross_length,
                    )
                    .get()
                };
                let mut max_label = 0.0_f64;
                if let Some(label) = &mut self.label {
                    max_label = max_label.max(measure(label));
                }
                if let Some(subtitle) = &mut self.subtitle {
                    max_label = max_label.max(measure(subtitle));
                }
                let shortcut_col = if let Some(shortcut) = &mut self.shortcut {
                    SHORTCUT_GAP + measure(shortcut)
                } else {
                    0.0
                };
                Length::px(self.gutter_width + max_label + shortcut_col)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let gutter = self.gutter_width;
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let has_subtitle = self.subtitle.is_some();
        let row_h = size.height;
        let full = Size::new(size.width.max(0.0), row_h);
        let label_avail = Size::new((size.width - gutter).max(0.0), row_h);

        // Label first — its vertical centerline is what the gutter glyph and
        // shortcut align to. With a sub-title the label is top-aligned and the
        // sub-title sits beneath it; otherwise the label is centred.
        let mut line_center = row_h * 0.5;
        if let Some(label) = &mut self.label {
            let label_size = ctx.compute_size(label, SizeDef::fit(label_avail), label_avail.into());
            ctx.run_layout(label, label_size);
            let label_y = if has_subtitle {
                pad_v
            } else {
                (row_h - label_size.height) * 0.5
            };
            ctx.place_child(label, Point::new(gutter, label_y));
            line_center = label_y + label_size.height * 0.5;

            if let Some(subtitle) = &mut self.subtitle {
                let sub_size =
                    ctx.compute_size(subtitle, SizeDef::fit(label_avail), label_avail.into());
                ctx.run_layout(subtitle, sub_size);
                ctx.place_child(subtitle, Point::new(gutter, label_y + label_size.height + SUBTITLE_GAP));
            }
        }

        // Leading gutter glyph (check or icon), aligned to the label line.
        if let Some(g) = &mut self.gutter {
            let g_size = ctx.compute_size(g, SizeDef::MIN, full.into());
            ctx.run_layout(g, g_size);
            ctx.place_child(g, Point::new(0.0, line_center - g_size.height * 0.5));
        }

        // Trailing shortcut, right-aligned to the label line.
        if let Some(shortcut) = &mut self.shortcut {
            let sc_size = ctx.compute_size(shortcut, SizeDef::MIN, full.into());
            ctx.run_layout(shortcut, sc_size);
            let sx = size.width - sc_size.width;
            ctx.place_child(shortcut, Point::new(sx, line_center - sc_size.height * 0.5));
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Children paint themselves; `MenuPanel` paints chrome/hover/ring/
        // separator lines around our placed rect.
    }

    fn accessibility_role(&self) -> Role {
        match self.kind {
            RowKind::Action if self.checked.is_some() => Role::MenuItemCheckBox,
            RowKind::Action => Role::MenuItem,
            RowKind::Separator => Role::Splitter,
            RowKind::Section => Role::Label,
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        match self.kind {
            RowKind::Action => {
                node.set_label(self.name.to_string());
                if let Some(checked) = self.checked {
                    node.set_toggled(if checked { Toggled::True } else { Toggled::False });
                }
                if let Some((pos, size)) = self.set_pos {
                    node.set_position_in_set(pos);
                    node.set_size_of_set(size);
                }
                if !self.disabled {
                    node.add_action(Action::Click);
                    node.add_action(Action::Focus);
                }
            }
            RowKind::Section => node.set_label(self.name.to_string()),
            RowKind::Separator => {}
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = [
            self.gutter.as_ref(),
            self.label.as_ref(),
            self.subtitle.as_ref(),
            self.shortcut.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(WidgetPod::id)
        .collect();
        ChildrenIds::from_slice(&ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(checked: Option<bool>, icon: Option<IconName>) -> MenuRowSpec {
        MenuRowSpec::Action {
            label: "x".into(),
            subtitle: None,
            icon,
            shortcut: None,
            checked,
            disabled: false,
        }
    }

    #[test]
    fn checkable_or_icon_rows_reserve_the_gutter() {
        assert!(reserves_gutter(&action(Some(true), None)), "checked");
        assert!(reserves_gutter(&action(Some(false), None)), "unchecked still reserves");
        assert!(reserves_gutter(&action(None, Some(IconName::Copy))), "icon");
        assert!(!reserves_gutter(&action(None, None)), "plain action");
        assert!(!reserves_gutter(&MenuRowSpec::Separator));
        assert!(!reserves_gutter(&MenuRowSpec::Section { text: "S".into() }));
    }
}
