//! `MenuPanel` — the rich context-menu item-list widget.
//!
//! Stacks one [`MenuItemNode`] per [`MenuRowSpec`] vertically, tracks per-row
//! hover and a keyboard highlight, paints its own background/border chrome plus
//! separator lines and the focus ring, and fires [`MenuAction`] when an enabled
//! action row is selected (by pointer, keyboard, or an accessibility invoke).
//! Selection is reported by the row's index into the original spec list, so the
//! [`super::view`] layer can map it straight back to the item's callback.
//!
//! The per-row column layout (leading gutter glyph · label + optional sub-title
//! · trailing shortcut) and per-row accessibility (role/name/checked/disabled/
//! position-in-set) live in [`MenuItemNode`]; `MenuPanel` owns only the vertical
//! stacking, chrome, hover/keyboard/hit-testing, and selection routing. It
//! mirrors `dropdown_button`'s `MenuContent` look so the two menu surfaces stay
//! visually consistent, and matches `tabs`/`sidebar`'s per-item a11y node model.

use masonry::accesskit::{Action, Node, Orientation, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Line, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

use super::item_node::{MenuItemNode, NodeActivated, gutter_glyph_width, reserves_gutter};
use crate::Theme;
use crate::components::icon::IconName;
use crate::focus_ring::paint_focus_ring;

/// Vertical padding above and below the item list.
const MENU_PAD_V: f64 = 4.0;
/// Corner radius of the menu's background chrome.
const CORNER_RADIUS: f64 = 5.0;
/// Border width of the menu's background chrome.
const BORDER_WIDTH: f64 = 1.0;
/// Minimum menu width in logical pixels, keeping a readable popup even when all
/// item labels are very short.
const MIN_MENU_WIDTH: f64 = 80.0;
/// Inset of the keyboard-highlight focus ring from its row's bounds.
const HIGHLIGHT_RING_INSET: f64 = 2.0;

/// One row of a [`MenuPanel`], as handed in by the view layer.
///
/// Display-only — callbacks stay in the view and are matched back up by the
/// row's index (see [`MenuAction::Selected`]).
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

impl MenuRowSpec {
    fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }

    fn is_action(&self) -> bool {
        matches!(self, Self::Action { .. })
    }

    /// Whether a pointer/keyboard selection can land on this row.
    fn selectable(&self) -> bool {
        matches!(
            self,
            Self::Action {
                disabled: false,
                ..
            }
        )
    }
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

/// The action a [`MenuPanel`] emits. A masonry widget can only submit its single
/// declared `Action` type, so selection and dismissal share one enum.
#[derive(Debug)]
pub enum MenuAction {
    /// The enabled action row at this index (its position in the original
    /// [`MenuRowSpec`] list) was selected.
    Selected(usize),
    /// The menu was dismissed by keyboard (Escape) without a selection. A
    /// hosting [`ContextMenuArea`](super::area::ContextMenuArea) closes on this;
    /// a standalone inline menu ignores it.
    Dismissed,
}

/// A row's node plus the lightweight metadata `MenuPanel` needs for
/// hit-testing, hover/highlight, and separator painting without reaching into
/// the node widget.
struct RowEntry {
    node: WidgetPod<MenuItemNode>,
    is_separator: bool,
    selectable: bool,
    /// Local-coordinate bounds (full panel width), populated during `layout`.
    rect: Rect,
}

/// Rich item-list widget for a context menu.
pub struct MenuPanel {
    rows: Vec<RowEntry>,
    /// Index (into `rows`) of the row the pointer is currently over — only ever
    /// a selectable row.
    hover_index: Option<usize>,
    /// Keyboard-highlighted row (roving focus), painted as a focus ring. Only
    /// ever a selectable row; cleared on focus loss / close.
    highlighted: Option<usize>,
    theme: Theme,
}

impl MenuPanel {
    #[must_use]
    pub fn new(specs: impl IntoIterator<Item = MenuRowSpec>, theme: &Theme) -> Self {
        Self {
            rows: build_rows(specs, theme),
            hover_index: None,
            highlighted: None,
            theme: *theme,
        }
    }

    fn pad_h(&self) -> f64 {
        f64::from(self.theme.density.button_pad_h)
    }

    /// The selectable row containing `local_pos`, if any.
    fn hit_row(&self, local_pos: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.selectable && r.rect.contains(local_pos))
    }

    /// Row indices that can receive the keyboard highlight (selectable actions),
    /// in order — separators, section headers, and disabled rows are skipped.
    fn selectable_indices(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| self.rows[i].selectable)
            .collect()
    }

    /// The next highlight position when moving by `dir` (+1 down, -1 up),
    /// wrapping at the ends and skipping non-selectable rows. `None` when there
    /// is nothing selectable.
    fn step_highlight(&self, dir: isize) -> Option<usize> {
        let sel = self.selectable_indices();
        if sel.is_empty() {
            return None;
        }
        let pos = self
            .highlighted
            .and_then(|cur| sel.iter().position(|&i| i == cur));
        let len = sel.len().cast_signed();
        let next = match pos {
            // Already on a selectable row: step with wraparound.
            Some(p) => (p.cast_signed() + dir).rem_euclid(len),
            // No current highlight: down enters at the top, up at the bottom.
            None if dir >= 0 => 0,
            None => len - 1,
        };
        Some(sel[next.cast_unsigned()])
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        let origin = ctx.to_window(Point::ZERO);
        window_pos - origin.to_vec2()
    }
}

/// Build the per-row nodes + metadata from specs, computing the shared gutter
/// width and the action position-in-set for accessibility.
fn build_rows(specs: impl IntoIterator<Item = MenuRowSpec>, theme: &Theme) -> Vec<RowEntry> {
    let specs: Vec<MenuRowSpec> = specs.into_iter().collect();
    let gutter_width = if specs.iter().any(reserves_gutter) {
        gutter_glyph_width(theme)
    } else {
        0.0
    };
    let action_count = specs.iter().filter(|s| s.is_action()).count();

    let mut action_pos = 0usize;
    specs
        .into_iter()
        .enumerate()
        .map(|(i, spec)| {
            let is_separator = spec.is_separator();
            let selectable = spec.selectable();
            let set_pos = if spec.is_action() {
                action_pos += 1;
                Some((action_pos, action_count))
            } else {
                None
            };
            let node = MenuItemNode::new(spec, gutter_width, i, set_pos, theme).to_pod();
            RowEntry {
                node,
                is_separator,
                selectable,
                rect: Rect::ZERO,
            }
        })
        .collect()
}

// --- MARK: WIDGETMUT SETTERS
impl MenuPanel {
    /// Restyle the per-row nodes and store the new theme — the panel persists
    /// across rebuilds (e.g. a host theme swap).
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        for i in 0..this.widget.rows.len() {
            let mut node = this.ctx.get_mut(&mut this.widget.rows[i].node);
            MenuItemNode::set_theme(&mut node, theme);
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    /// Set the keyboard-highlighted row, driven externally by a host that owns
    /// focus (e.g. [`ContextMenuArea`](super::area::ContextMenuArea)). Pass
    /// `None` to clear. Standalone/inline menus drive their own highlight via
    /// the keyboard handler instead.
    pub fn set_highlighted(this: &mut WidgetMut<'_, Self>, index: Option<usize>) {
        if this.widget.highlighted != index {
            this.widget.highlighted = index;
            this.ctx.request_paint_only();
            // The highlight is the menu's active item — let AT follow it.
            this.ctx.request_accessibility_update();
        }
    }

    /// Replace the row list — the panel persists across rebuilds, so a changed
    /// item set must be applied in place.
    pub fn set_rows(this: &mut WidgetMut<'_, Self>, specs: impl IntoIterator<Item = MenuRowSpec>) {
        for row in std::mem::take(&mut this.widget.rows) {
            this.ctx.remove_child(row.node);
        }
        let theme = this.widget.theme;
        this.widget.rows = build_rows(specs, &theme);
        this.widget.hover_index = None;
        this.widget.highlighted = None;
        this.ctx.children_changed();
        this.ctx.request_layout();
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }
}

impl Widget for MenuPanel {
    type Action = MenuAction;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn propagates_pointer_interaction(&self) -> bool {
        // We hit-test rows ourselves against their placed rects; the per-row
        // nodes (and their label children) take no pointer events.
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
                    ctx.submit_action::<Self::Action>(MenuAction::Selected(i));
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

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        let is_space = matches!(&key.key, Key::Character(c) if c.as_str() == " ");
        match &key.key {
            Key::Named(NamedKey::ArrowDown) => {
                self.highlighted = self.step_highlight(1);
                ctx.request_paint_only();
                ctx.set_handled();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.highlighted = self.step_highlight(-1);
                ctx.request_paint_only();
                ctx.set_handled();
            }
            Key::Named(NamedKey::Home) => {
                self.highlighted = self.selectable_indices().first().copied();
                ctx.request_paint_only();
                ctx.set_handled();
            }
            Key::Named(NamedKey::End) => {
                self.highlighted = self.selectable_indices().last().copied();
                ctx.request_paint_only();
                ctx.set_handled();
            }
            // Activate the highlighted row (no-op if nothing is highlighted).
            Key::Named(NamedKey::Enter) => {
                if let Some(i) = self.highlighted.take() {
                    ctx.submit_action::<Self::Action>(MenuAction::Selected(i));
                    ctx.request_paint_only();
                }
                ctx.set_handled();
            }
            Key::Character(_) if is_space => {
                if let Some(i) = self.highlighted.take() {
                    ctx.submit_action::<Self::Action>(MenuAction::Selected(i));
                    ctx.request_paint_only();
                }
                ctx.set_handled();
            }
            Key::Named(NamedKey::Escape) => {
                self.highlighted = None;
                ctx.submit_action::<Self::Action>(MenuAction::Dismissed);
                ctx.request_paint_only();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    /// An accessibility invoke on a row bubbles up as [`NodeActivated`]; re-emit
    /// it as our own [`MenuAction::Selected`] (same path as a pointer/keyboard
    /// selection).
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        if let Some(&NodeActivated(index)) = action.downcast_ref::<NodeActivated>() {
            ctx.submit_action::<Self::Action>(MenuAction::Selected(index));
            ctx.set_handled();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            // Stash state flipping (a right-click menu opening/closing) resets
            // the highlight so it never lingers across opens. The host area
            // transfers focus to us on open (`ContextMenuArea` → `set_focus`).
            Update::StashedChanged(_) => {
                self.highlighted = None;
            }
            // Focus left us (e.g. an outside click): clear the keyboard
            // highlight so the focus ring doesn't paint while unfocused.
            Update::FocusChanged(false) if self.highlighted.is_some() => {
                self.highlighted = None;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for row in &mut self.rows {
            ctx.register_child(&mut row.node);
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
        let pad_h = self.pad_h();
        match axis {
            Axis::Vertical => {
                let content: f64 = self
                    .rows
                    .iter_mut()
                    .map(|r| {
                        ctx.compute_length(
                            &mut r.node,
                            len_req.into(),
                            LayoutSize::maybe(Axis::Horizontal, cross_length),
                            Axis::Vertical,
                            cross_length,
                        )
                        .get()
                    })
                    .sum();
                Length::px(MENU_PAD_V * 2.0 + content)
            }
            Axis::Horizontal => {
                let inner_cross =
                    cross_length.map(|c| Length::px((c.get() - 2.0 * pad_h).max(0.0)));
                let mut max_w = 0.0_f64;
                for row in &mut self.rows {
                    let w = ctx
                        .compute_length(
                            &mut row.node,
                            len_req.into(),
                            LayoutSize::maybe(Axis::Vertical, inner_cross),
                            Axis::Horizontal,
                            inner_cross,
                        )
                        .get();
                    max_w = max_w.max(w);
                }
                Length::px(max_w.max(MIN_MENU_WIDTH) + 2.0 * pad_h)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_h = self.pad_h();
        let node_w = (size.width - 2.0 * pad_h).max(0.0);

        let mut y = MENU_PAD_V;
        for row in &mut self.rows {
            // The node's height is intrinsic (its row height); force its width
            // to the panel's content width so trailing shortcuts right-align to
            // a consistent edge.
            let node_h = ctx
                .compute_size(&mut row.node, SizeDef::MIN, Size::new(node_w, 0.0).into())
                .height;
            ctx.run_layout(&mut row.node, Size::new(node_w, node_h));
            ctx.place_child(&mut row.node, Point::new(pad_h, y));
            row.rect = Rect::from_origin_size(Point::new(0.0, y), Size::new(size.width, node_h));
            y += node_h;
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

        // Keyboard focus ring on the highlighted row, inset from its bounds.
        if let Some(i) = self.highlighted
            && let Some(row) = self.rows.get(i)
        {
            let inset = HIGHLIGHT_RING_INSET;
            let ring = Rect::new(
                row.rect.x0 + inset,
                row.rect.y0 + inset,
                row.rect.x1 - inset,
                row.rect.y1 - inset,
            );
            paint_focus_ring(painter, ring, &self.theme);
        }

        for row in &self.rows {
            if row.is_separator {
                let y = row.rect.y0 + row.rect.height() * 0.5;
                let line = Line::new(
                    Point::new(row.rect.x0 + pad_h, y),
                    Point::new(row.rect.x1 - pad_h, y),
                );
                painter
                    .stroke(line, &Stroke::new(BORDER_WIDTH), p.border_strong)
                    .draw();
            }
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn accessibility_role(&self) -> Role {
        Role::Menu
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        // Container-level a11y: a vertical, focusable menu. Each row's
        // `MenuItem`/`MenuItemCheckBox`/`Splitter`/label semantics live on its
        // `MenuItemNode` child.
        node.set_orientation(Orientation::Vertical);
        if self.rows.iter().any(|r| r.selectable) {
            node.add_action(Action::Focus);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self.rows.iter().map(|r| r.node.id()).collect();
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
    fn separators_and_sections_are_never_selectable() {
        let panel = panel_with(
            vec![
                MenuRowSpec::Separator,
                MenuRowSpec::Section {
                    text: "View".into(),
                },
            ],
            vec![Rect::ZERO; 2],
        );
        assert!(!panel.rows[0].selectable);
        assert!(!panel.rows[1].selectable);
        assert_eq!(panel.selectable_indices(), Vec::<usize>::new());
    }

    // --- keyboard navigation (TestHarness) ---

    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{Handled, NewWidget, TextEvent};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;

    fn harness(specs: Vec<MenuRowSpec>) -> TestHarness<MenuPanel> {
        let theme = Theme::default();
        let widget = MenuPanel::new(specs, &theme);
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        h.focus_on(Some(h.root_id()));
        h
    }

    fn press(h: &mut TestHarness<MenuPanel>, key: NamedKey) -> Handled {
        h.process_text_event(TextEvent::key_down(Key::Named(key)))
    }

    fn highlight(h: &mut TestHarness<MenuPanel>) -> Option<usize> {
        h.edit_root_widget(|wm| wm.widget.highlighted)
    }

    #[test]
    fn arrow_navigation_skips_nonselectable_rows_and_wraps() {
        // selectable rows are 0 and 3; 1 (separator) and 2 (disabled) are not.
        let mut h = harness(vec![
            action("a"),
            MenuRowSpec::Separator,
            disabled("b"),
            action("c"),
        ]);
        press(&mut h, NamedKey::ArrowDown);
        assert_eq!(highlight(&mut h), Some(0), "down enters at the first selectable");
        press(&mut h, NamedKey::ArrowDown);
        assert_eq!(highlight(&mut h), Some(3), "skips the separator and disabled row");
        press(&mut h, NamedKey::ArrowDown);
        assert_eq!(highlight(&mut h), Some(0), "wraps past the end to the top");
        press(&mut h, NamedKey::ArrowUp);
        assert_eq!(highlight(&mut h), Some(3), "up from the top wraps to the bottom");
    }

    #[test]
    fn home_and_end_jump_to_first_and_last_selectable() {
        let mut h = harness(vec![action("a"), MenuRowSpec::Separator, action("c")]);
        assert_eq!(press(&mut h, NamedKey::End), Handled::Yes);
        assert_eq!(highlight(&mut h), Some(2));
        press(&mut h, NamedKey::Home);
        assert_eq!(highlight(&mut h), Some(0));
    }

    #[test]
    fn enter_activates_the_highlighted_row() {
        let mut h = harness(vec![action("a"), action("b")]);
        press(&mut h, NamedKey::ArrowDown);
        press(&mut h, NamedKey::ArrowDown); // highlight row 1
        press(&mut h, NamedKey::Enter);
        let (action, _) = h
            .pop_action::<MenuAction>()
            .expect("Enter on a highlighted row selects it");
        assert!(matches!(action, MenuAction::Selected(1)));
    }

    #[test]
    fn enter_without_a_highlight_selects_nothing() {
        let mut h = harness(vec![action("a")]);
        assert_eq!(press(&mut h, NamedKey::Enter), Handled::Yes);
        assert!(h.pop_action::<MenuAction>().is_none());
    }

    #[test]
    fn escape_emits_dismissed_and_is_handled() {
        let mut h = harness(vec![action("a")]);
        assert_eq!(press(&mut h, NamedKey::Escape), Handled::Yes);
        assert!(matches!(
            h.pop_action::<MenuAction>().map(|(a, _)| a),
            Some(MenuAction::Dismissed)
        ));
    }

    // --- per-row accessibility (TestHarness) ---

    #[test]
    fn rows_expose_per_item_accessibility_semantics() {
        use masonry::accesskit::{Role, Toggled};

        let checkable = MenuRowSpec::Action {
            label: "Wrap".into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: Some(true),
            disabled: false,
        };
        let specs = vec![
            action("Copy"),
            checkable,
            MenuRowSpec::Separator,
            MenuRowSpec::Section {
                text: "View".into(),
            },
        ];
        let theme = Theme::default();
        let widget = MenuPanel::new(specs, &theme);
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        let ids: Vec<_> =
            h.edit_root_widget(|wm| wm.widget.rows.iter().map(|r| r.node.id()).collect());
        h.redraw();

        // Action → MenuItem, labeled, position 1 of the 2 action rows.
        let copy = h.access_node(ids[0]).expect("action node exists");
        assert_eq!(copy.role(), Role::MenuItem);
        assert_eq!(copy.label(), Some("Copy".to_string()));
        assert_eq!(copy.position_in_set(), Some(1));
        assert_eq!(copy.size_of_set(), Some(2));

        // Checkable action → MenuItemCheckBox, toggled on.
        let wrap = h.access_node(ids[1]).expect("checkable node exists");
        assert_eq!(wrap.role(), Role::MenuItemCheckBox);
        assert_eq!(wrap.toggled(), Some(Toggled::True));
        assert_eq!(wrap.position_in_set(), Some(2));

        // Separator → Splitter.
        let sep = h.access_node(ids[2]).expect("separator node exists");
        assert_eq!(sep.role(), Role::Splitter);

        // Section header → a labeled, non-interactive node.
        let section = h.access_node(ids[3]).expect("section node exists");
        assert_eq!(section.role(), Role::Label);
        assert_eq!(section.label(), Some("View".to_string()));
    }
}
