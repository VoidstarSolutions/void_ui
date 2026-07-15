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
use crate::components::item_list;
use crate::focus_ring::paint_focus_ring;

/// Border width of the menu's background chrome — hairline chrome, not density-scaled.
const BORDER_WIDTH: f64 = 1.0;
/// Minimum menu width in logical pixels, keeping a readable popup even when all
/// item labels are very short — a clamp, not a density-scaled dimension.
const MIN_MENU_WIDTH: f64 = 80.0;
/// Inset of the keyboard-highlight focus ring from its row's bounds — focus chrome, not density-scaled.
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
    /// selection. `id` is the global leaf id the view assigns for callback
    /// routing — stable across nesting, unlike the row's position.
    Action {
        id: usize,
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
    /// A row that opens a nested fly-out menu of `children` on hover. Shows a
    /// trailing chevron; itself carries no callback.
    Submenu {
        label: ArcStr,
        icon: Option<IconName>,
        children: Vec<MenuRowSpec>,
    },
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

    /// The leaf id emitted when this row is selected (action rows only).
    fn leaf_id(&self) -> Option<usize> {
        match self {
            Self::Action { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// The visible label used for type-ahead / first-letter navigation — an
    /// action's or submenu's text; `None` for separators and sections (which
    /// are never highlightable anyway).
    fn type_ahead_label(&self) -> Option<ArcStr> {
        match self {
            Self::Action { label, .. } | Self::Submenu { label, .. } => Some(label.clone()),
            _ => None,
        }
    }
}

impl PartialEq for MenuRowSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Action {
                    id: id1,
                    label: l1,
                    subtitle: sub1,
                    icon: i1,
                    shortcut: s1,
                    checked: c1,
                    disabled: d1,
                },
                Self::Action {
                    id: id2,
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
                id1 == id2
                    && l1 == l2
                    && sub1 == sub2
                    && d1 == d2
                    && c1 == c2
                    && s1 == s2
                    && i1.map(char::from) == i2.map(char::from)
            }
            (Self::Separator, Self::Separator) => true,
            (Self::Section { text: t1 }, Self::Section { text: t2 }) => t1 == t2,
            (
                Self::Submenu {
                    label: l1,
                    icon: i1,
                    children: ch1,
                },
                Self::Submenu {
                    label: l2,
                    icon: i2,
                    children: ch2,
                },
            ) => l1 == l2 && i1.map(char::from) == i2.map(char::from) && ch1 == ch2,
            _ => false,
        }
    }
}

/// A normalized menu navigation key, so the keyboard owner (the inline panel or
/// the [`ContextMenuArea`](super::area::ContextMenuArea)) forwards intent rather
/// than raw `winit` keys into [`MenuPanel::handle_menu_key`].
#[derive(Clone, Copy)]
pub(crate) enum MenuKey {
    Up,
    Down,
    Home,
    End,
    /// Enter a submenu / open the highlighted submenu.
    Right,
    /// Leave the current submenu, back to its parent.
    Left,
    /// Enter / Space — select the highlighted row (or open a submenu).
    Activate,
    Escape,
    /// Type-ahead: a printable character typed to jump to the next row whose
    /// label starts with it (WAI-ARIA first-letter navigation).
    Search(char),
}

/// What the recursive key walk produced at one menu level, propagated up so the
/// *outermost* panel can emit. Selection/dismissal are returned (not submitted
/// deep down) because actions submitted from a nested `MutateCtx` don't bubble
/// through ancestors' `on_action` during a mutate pass — only the outermost
/// panel's submit routes correctly.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyOutcome {
    Handled,
    Ignored,
    /// Left at this (fly-out) level — the parent should collapse us.
    Close,
    /// A leaf was activated this deep; the outermost panel emits `Selected`.
    Select(usize),
    /// Escape — the outermost panel emits `Dismissed`.
    Dismiss,
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
    /// Whether this row opens a submenu fly-out on hover.
    is_submenu: bool,
    /// Global leaf id emitted when this row is selected (action rows only).
    leaf_id: Option<usize>,
    /// Visible label, kept for type-ahead / first-letter navigation. `None`
    /// for separators and sections.
    label: Option<ArcStr>,
    /// Local-coordinate bounds (full panel width), populated during `layout`.
    rect: Rect,
}

impl RowEntry {
    /// Whether the pointer can hover this row (selectable actions + submenus).
    fn hoverable(&self) -> bool {
        self.selectable || self.is_submenu
    }
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
    /// The submenu row whose fly-out is currently open, if any.
    open_submenu: Option<usize>,
    /// The submenu row whose fly-out currently has *keyboard* focus (so keys are
    /// delegated into it), if any.
    kbd_submenu: Option<usize>,
    /// Whether this panel takes focus on click. `true` for a standalone/inline
    /// menu (so clicking re-arms its keyboard); `false` when a host owns focus
    /// and forwards keys (the right-click area and every fly-out — see
    /// [`Self::hosted`]).
    self_focus: bool,
    theme: Theme,
}

impl MenuPanel {
    #[must_use]
    pub fn new(specs: impl IntoIterator<Item = MenuRowSpec>, theme: &Theme) -> Self {
        Self {
            rows: build_rows(specs, theme),
            hover_index: None,
            highlighted: None,
            open_submenu: None,
            kbd_submenu: None,
            self_focus: true,
            theme: *theme,
        }
    }

    /// Mark this panel as host-driven: it won't grab focus on click (its host —
    /// the right-click area, or a parent menu for a fly-out — owns focus and
    /// forwards navigation keys into it).
    #[must_use]
    pub fn hosted(mut self) -> Self {
        self.self_focus = false;
        self
    }

    fn pad_h(&self) -> f64 {
        item_list::pad_h(&self.theme.density)
    }

    fn menu_pad_v(&self) -> f64 {
        item_list::menu_pad_v(&self.theme.density)
    }

    /// The selectable row containing `local_pos`, if any.
    fn hit_row(&self, local_pos: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.selectable && r.rect.contains(local_pos))
    }

    /// The hoverable row containing `local_pos` (selectable actions + submenus).
    fn hit_hoverable(&self, local_pos: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.hoverable() && r.rect.contains(local_pos))
    }

    /// Push open/closed state into row `row`'s submenu node.
    fn set_node_submenu_open(&mut self, ctx: &mut EventCtx<'_>, row: usize, open: bool) {
        ctx.mutate_child_later(&mut self.rows[row].node, move |mut w| {
            MenuItemNode::set_submenu_open(&mut w, open);
        });
    }

    /// Close whichever submenu is currently open (if any).
    fn close_open_submenu(&mut self, ctx: &mut EventCtx<'_>) {
        if let Some(row) = self.open_submenu.take() {
            self.set_node_submenu_open(ctx, row, false);
        }
    }

    /// Open row `row`'s submenu, closing any previously-open one first.
    fn open_submenu_at(&mut self, ctx: &mut EventCtx<'_>, row: usize) {
        self.close_open_submenu(ctx);
        self.open_submenu = Some(row);
        self.set_node_submenu_open(ctx, row, true);
    }

    /// Row indices the keyboard highlight can land on (selectable actions *and*
    /// submenu rows, which open on Right/Enter).
    fn hoverable_indices(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| self.rows[i].hoverable())
            .collect()
    }

    /// The next highlight position when moving by `dir` (+1 down, -1 up),
    /// wrapping at the ends and skipping non-hoverable rows. `None` when there
    /// is nothing to land on.
    fn step_highlight(&self, dir: isize) -> Option<usize> {
        let sel = self.hoverable_indices();
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

    /// Type-ahead target: the next hoverable row whose label starts with `ch`
    /// (case-insensitive), scanning forward from the current highlight and
    /// wrapping. Because the scan starts *after* the current highlight,
    /// repeated presses of the same letter cycle through the matches. `None`
    /// when nothing matches.
    fn search_highlight(&self, ch: char) -> Option<usize> {
        let needle = ch.to_ascii_lowercase();
        let sel = self.hoverable_indices();
        if sel.is_empty() {
            return None;
        }
        // Start just after the current highlight (or at the top) so a repeated
        // letter advances to the next match rather than sticking.
        let start = self
            .highlighted
            .and_then(|cur| sel.iter().position(|&i| i == cur))
            .map_or(0, |p| p + 1);
        let len = sel.len();
        (0..len)
            .map(|off| sel[(start + off) % len])
            .find(|&row| self.row_label_starts_with(row, needle))
    }

    /// Whether row `row`'s label begins with `needle` (already lowercased),
    /// compared case-insensitively on the first character.
    fn row_label_starts_with(&self, row: usize, needle: char) -> bool {
        self.rows[row]
            .label
            .as_ref()
            .and_then(|l| l.chars().next())
            .is_some_and(|first| first.to_ascii_lowercase() == needle)
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        item_list::to_local(ctx, window_pos)
    }

    #[cfg(test)]
    pub(crate) fn dbg_kbd_submenu(&self) -> Option<usize> {
        self.kbd_submenu
    }
}

/// Map a raw key to a normalized [`MenuKey`], or `None` for keys the menu
/// ignores. Shared by the inline panel and the right-click area.
pub(crate) fn menu_key_from(key: &Key) -> Option<MenuKey> {
    match key {
        Key::Named(NamedKey::ArrowDown) => Some(MenuKey::Down),
        Key::Named(NamedKey::ArrowUp) => Some(MenuKey::Up),
        Key::Named(NamedKey::Home) => Some(MenuKey::Home),
        Key::Named(NamedKey::End) => Some(MenuKey::End),
        Key::Named(NamedKey::ArrowRight) => Some(MenuKey::Right),
        Key::Named(NamedKey::ArrowLeft) => Some(MenuKey::Left),
        Key::Named(NamedKey::Enter) => Some(MenuKey::Activate),
        Key::Character(c) if c.as_str() == " " => Some(MenuKey::Activate),
        Key::Named(NamedKey::Escape) => Some(MenuKey::Escape),
        // Any other printable character begins type-ahead (first-letter nav);
        // its first char is the search key. Control chars are ignored.
        Key::Character(c) => c
            .chars()
            .next()
            .filter(|ch| !ch.is_control())
            .map(MenuKey::Search),
        Key::Named(_) => None,
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
        .map(|spec| {
            let is_separator = spec.is_separator();
            let is_submenu = matches!(spec, MenuRowSpec::Submenu { .. });
            let selectable = spec.selectable();
            let leaf_id = spec.leaf_id();
            let label = spec.type_ahead_label();
            let set_pos = if spec.is_action() {
                action_pos += 1;
                Some((action_pos, action_count))
            } else {
                None
            };
            // The node carries the leaf id so its accessibility `Click` routes
            // through the same `MenuAction::Selected(id)` path.
            let node = MenuItemNode::new(spec, gutter_width, leaf_id.unwrap_or(0), set_pos, theme)
                .to_pod();
            RowEntry {
                node,
                is_separator,
                selectable,
                is_submenu,
                leaf_id,
                label,
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

    /// Handle one navigation key for this menu and its open fly-out chain. Owns
    /// highlight movement, submenu open/close, and selection/dismissal emission
    /// (`MutateCtx::submit_action`). Recurses into the active fly-out so the
    /// deepest open submenu receives the key; returns [`KeyOutcome::Close`] when
    /// Left is pressed at this level so a parent collapses us.
    ///
    /// The keyboard owner (the focused inline panel via `mutate_self_later`, or
    /// the focused [`ContextMenuArea`](super::area::ContextMenuArea) via
    /// `mutate_child_later`) forwards keys here.
    pub(crate) fn handle_menu_key(this: &mut WidgetMut<'_, Self>, key: MenuKey) {
        // Only the outermost panel (this one) submits — emitting from a nested
        // fly-out's `MutateCtx` wouldn't bubble through `on_action`.
        match Self::nav_key(this, key) {
            KeyOutcome::Select(id) => {
                this.ctx
                    .submit_action::<MenuAction>(MenuAction::Selected(id));
                this.ctx.request_paint_only();
            }
            KeyOutcome::Dismiss => {
                this.ctx.submit_action::<MenuAction>(MenuAction::Dismissed);
                this.ctx.request_paint_only();
            }
            _ => {}
        }
    }

    /// Dismiss the whole menu on a focus-out (Tab): collapse any open fly-out,
    /// clear the highlight, and emit [`MenuAction::Dismissed`]. Unlike Escape,
    /// the Tab key itself is left **unhandled** by the caller so masonry's
    /// focus traversal moves focus out of the menu, per the WAI-ARIA menu
    /// pattern. Dismisses the entire chain regardless of submenu depth.
    pub(crate) fn dismiss(this: &mut WidgetMut<'_, Self>) {
        if let Some(row) = this.widget.open_submenu.take() {
            let mut node = this.ctx.get_mut(&mut this.widget.rows[row].node);
            MenuItemNode::set_submenu_open(&mut node, false);
        }
        this.widget.kbd_submenu = None;
        this.widget.highlighted = None;
        this.ctx.submit_action::<MenuAction>(MenuAction::Dismissed);
        this.ctx.request_paint_only();
    }

    /// Recursive key walk: moves highlight / opens-closes submenus at the active
    /// level (delegating into an open fly-out), and *returns* selection or
    /// dismissal up to [`Self::handle_menu_key`] rather than submitting.
    fn nav_key(this: &mut WidgetMut<'_, Self>, key: MenuKey) -> KeyOutcome {
        // Delegate to the active fly-out first, if any.
        if let Some(srow) = this.widget.kbd_submenu {
            let outcome = {
                let mut node = this.ctx.get_mut(&mut this.widget.rows[srow].node);
                match MenuItemNode::flyout_mut(&mut node) {
                    Some(mut flyout) => Self::nav_key(&mut flyout, key),
                    None => KeyOutcome::Ignored,
                }
            };
            if outcome == KeyOutcome::Close {
                // The fly-out asked to be closed (Left at its top level):
                // collapse it and return the highlight to its parent row.
                {
                    let mut node = this.ctx.get_mut(&mut this.widget.rows[srow].node);
                    MenuItemNode::set_submenu_open(&mut node, false);
                }
                this.widget.kbd_submenu = None;
                this.widget.open_submenu = None;
                this.widget.highlighted = Some(srow);
                this.ctx.request_paint_only();
                return KeyOutcome::Handled;
            }
            // Select / Dismiss / Handled / Ignored propagate to the outermost.
            return outcome;
        }

        match key {
            MenuKey::Down => {
                this.widget.highlighted = this.widget.step_highlight(1);
                this.ctx.request_paint_only();
                KeyOutcome::Handled
            }
            MenuKey::Up => {
                this.widget.highlighted = this.widget.step_highlight(-1);
                this.ctx.request_paint_only();
                KeyOutcome::Handled
            }
            MenuKey::Home => {
                this.widget.highlighted = this.widget.hoverable_indices().first().copied();
                this.ctx.request_paint_only();
                KeyOutcome::Handled
            }
            MenuKey::End => {
                this.widget.highlighted = this.widget.hoverable_indices().last().copied();
                this.ctx.request_paint_only();
                KeyOutcome::Handled
            }
            MenuKey::Right => match this.widget.highlighted {
                Some(row) if this.widget.rows[row].is_submenu => {
                    Self::enter_submenu(this, row);
                    KeyOutcome::Handled
                }
                _ => KeyOutcome::Ignored,
            },
            // Left collapses *this* panel when it's a fly-out (the parent acts
            // on `Close`); at the top level there's no parent, so it's a no-op.
            MenuKey::Left => KeyOutcome::Close,
            MenuKey::Activate => match this.widget.highlighted {
                Some(row) if this.widget.rows[row].is_submenu => {
                    Self::enter_submenu(this, row);
                    KeyOutcome::Handled
                }
                Some(row) => match this.widget.rows[row].leaf_id {
                    Some(id) => {
                        this.widget.highlighted = None;
                        this.ctx.request_paint_only();
                        KeyOutcome::Select(id)
                    }
                    None => KeyOutcome::Handled,
                },
                None => KeyOutcome::Handled,
            },
            MenuKey::Escape => {
                this.widget.highlighted = None;
                this.ctx.request_paint_only();
                KeyOutcome::Dismiss
            }
            MenuKey::Search(ch) => {
                if let Some(target) = this.widget.search_highlight(ch) {
                    this.widget.highlighted = Some(target);
                    this.ctx.request_paint_only();
                }
                KeyOutcome::Handled
            }
        }
    }

    /// Open row `srow`'s submenu, give it keyboard focus, and highlight its
    /// first item.
    fn enter_submenu(this: &mut WidgetMut<'_, Self>, srow: usize) {
        {
            let mut node = this.ctx.get_mut(&mut this.widget.rows[srow].node);
            MenuItemNode::set_submenu_open(&mut node, true);
            if let Some(mut flyout) = MenuItemNode::flyout_mut(&mut node) {
                let first = flyout.widget.hoverable_indices().first().copied();
                flyout.widget.highlighted = first;
                flyout.ctx.request_paint_only();
            }
        }
        this.widget.kbd_submenu = Some(srow);
        this.widget.open_submenu = Some(srow);
        this.widget.highlighted = Some(srow);
        this.ctx.request_paint_only();
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
        this.widget.open_submenu = None;
        this.widget.kbd_submenu = None;
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
        // We hit-test rows ourselves, but must propagate so an open submenu
        // fly-out (nested under a row's node) can receive its own pointer
        // events; the rows' label/icon children stay pointer-transparent, so
        // events over them still bubble back to us for hit-testing.
        true
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
                match self.hit_hoverable(local) {
                    Some(row) => {
                        if self.hover_index != Some(row) {
                            self.hover_index = Some(row);
                            ctx.request_paint_only();
                        }
                        if self.rows[row].is_submenu {
                            // Hovering a submenu row opens its fly-out (closing
                            // any other open one).
                            if self.open_submenu != Some(row) {
                                self.open_submenu_at(ctx, row);
                            }
                        } else {
                            // Hovering a plain row closes any open submenu.
                            self.close_open_submenu(ctx);
                        }
                    }
                    // Off all rows (e.g. the pointer travelled into the open
                    // fly-out, which overflows our box): keep the submenu open
                    // and its row highlighted. Only clear hover when nothing is
                    // open.
                    None => {
                        if self.open_submenu.is_none() && self.hover_index.is_some() {
                            self.hover_index = None;
                            ctx.request_paint_only();
                        }
                    }
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.capture_pointer();
                // A standalone/inline menu takes focus on click so its keyboard
                // re-arms; a host-driven menu (right-click area / fly-out) does
                // NOT, leaving focus with its host (which forwards keys).
                if self.self_focus {
                    ctx.request_focus();
                }
                // Consume so an ancestor `MenuPanel` (when we're a fly-out)
                // doesn't also capture and swallow the release — which would
                // drop the selection.
                ctx.set_handled();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) if ctx.is_active() && ctx.is_hovered() => {
                let local = Self::to_local(ctx, state.logical_point());
                if let Some(i) = self.hit_row(local)
                    && let Some(id) = self.rows[i].leaf_id
                {
                    ctx.submit_action::<Self::Action>(MenuAction::Selected(id));
                    ctx.set_handled();
                }
            }
            // Pointer left our box. Keep an open submenu open (the pointer may
            // have crossed into its fly-out, which lives outside our box);
            // otherwise drop the hover highlight.
            PointerEvent::Leave(_) if self.open_submenu.is_none() && self.hover_index.is_some() => {
                self.hover_index = None;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    /// When focused (the inline / standalone case), forward navigation keys to
    /// [`Self::handle_menu_key`] — run via `mutate_self_later` so it can recurse
    /// into open fly-outs and emit through a `MutateCtx`. (A right-click menu is
    /// driven the same way by `ContextMenuArea`, which holds focus.)
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
        // Tab dismisses the menu but is left UNHANDLED so masonry's focus
        // traversal moves focus out in tab order (WAI-ARIA menu pattern);
        // Escape, by contrast, consumes the key. Shift+Tab dismisses too.
        if matches!(key.key, Key::Named(NamedKey::Tab)) {
            ctx.mutate_self_later(|mut w| {
                let mut w = w.downcast::<MenuPanel>();
                MenuPanel::dismiss(&mut w);
            });
            return;
        }
        if let Some(menu_key) = menu_key_from(&key.key) {
            ctx.mutate_self_later(move |mut w| {
                let mut w = w.downcast::<MenuPanel>();
                MenuPanel::handle_menu_key(&mut w, menu_key);
            });
            ctx.set_handled();
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
            // Being hidden (menu closed, or a fly-out collapsed) resets the
            // highlight and forgets any open submenu so nothing lingers across
            // re-opens. Only on *stash* (true), not un-stash — un-stashing a
            // fly-out must NOT wipe the highlight that keyboard-open just set.
            Update::StashedChanged(true) => {
                self.highlighted = None;
                self.open_submenu = None;
                self.kbd_submenu = None;
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
                Length::px(self.menu_pad_v() * 2.0 + content)
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

        let mut y = self.menu_pad_v();
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

        let corner = f64::from(self.theme.radius.small);
        let bg_rect = RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box().size(), corner);
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

    fn action_id(id: usize, label: &str) -> MenuRowSpec {
        MenuRowSpec::Action {
            id,
            label: label.into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: None,
            disabled: false,
        }
    }

    fn action(label: &str) -> MenuRowSpec {
        action_id(0, label)
    }

    fn disabled(label: &str) -> MenuRowSpec {
        MenuRowSpec::Action {
            id: 0,
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
    fn submenu_row_is_hoverable_but_not_selectable() {
        let theme = Theme::default();
        let specs = vec![MenuRowSpec::Submenu {
            label: "More".into(),
            icon: None,
            children: vec![action_id(5, "leaf")],
        }];
        let panel = MenuPanel::new(specs, &theme);
        assert!(panel.rows[0].is_submenu);
        assert!(
            !panel.rows[0].selectable,
            "a submenu row isn't click-selectable"
        );
        assert!(
            panel.rows[0].hoverable(),
            "but it is hoverable (opens on hover)"
        );
        assert!(
            panel.rows[0].leaf_id.is_none(),
            "and carries no leaf id of its own"
        );
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
        assert!(panel.hoverable_indices().is_empty());
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

    fn press_char(h: &mut TestHarness<MenuPanel>, ch: &str) -> Handled {
        h.process_text_event(TextEvent::key_down(Key::Character(ch.into())))
    }

    fn highlight(h: &mut TestHarness<MenuPanel>) -> Option<usize> {
        h.edit_root_widget(|wm| wm.widget.highlighted)
    }

    #[test]
    fn type_ahead_jumps_to_a_matching_label_and_cycles() {
        // rows: Save, Save As, Print, Preview.
        let mut h = harness(vec![
            action_id(0, "Save"),
            action_id(1, "Save As"),
            action_id(2, "Print"),
            action_id(3, "Preview"),
        ]);

        press_char(&mut h, "p");
        assert_eq!(highlight(&mut h), Some(2), "jumps to Print");
        press_char(&mut h, "p");
        assert_eq!(highlight(&mut h), Some(3), "repeat cycles to Preview");
        press_char(&mut h, "p");
        assert_eq!(highlight(&mut h), Some(2), "wraps back to Print");

        // Case-insensitive; the search advances from the current highlight.
        press_char(&mut h, "S");
        assert_eq!(highlight(&mut h), Some(0), "uppercase S matches Save");
        press_char(&mut h, "s");
        assert_eq!(highlight(&mut h), Some(1), "the next s-match is Save As");
    }

    #[test]
    fn type_ahead_ignores_nonmatches_and_skips_nonhoverable_rows() {
        let mut h = harness(vec![
            action("Apple"),
            MenuRowSpec::Separator,
            disabled("Banana"), // not hoverable → never a type-ahead target
            action("Cherry"),
        ]);

        press_char(&mut h, "z");
        assert_eq!(highlight(&mut h), None, "no match leaves the highlight");
        press_char(&mut h, "b");
        assert_eq!(highlight(&mut h), None, "disabled Banana is not a target");
        press_char(&mut h, "c");
        assert_eq!(highlight(&mut h), Some(3), "matches the hoverable Cherry");
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
        assert_eq!(
            highlight(&mut h),
            Some(0),
            "down enters at the first selectable"
        );
        press(&mut h, NamedKey::ArrowDown);
        assert_eq!(
            highlight(&mut h),
            Some(3),
            "skips the separator and disabled row"
        );
        press(&mut h, NamedKey::ArrowDown);
        assert_eq!(highlight(&mut h), Some(0), "wraps past the end to the top");
        press(&mut h, NamedKey::ArrowUp);
        assert_eq!(
            highlight(&mut h),
            Some(3),
            "up from the top wraps to the bottom"
        );
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
        // Leaf ids 10/11 (distinct from row indices) to prove selection emits
        // the leaf id, not the row position.
        let mut h = harness(vec![action_id(10, "a"), action_id(11, "b")]);
        press(&mut h, NamedKey::ArrowDown);
        press(&mut h, NamedKey::ArrowDown); // highlight row 1
        press(&mut h, NamedKey::Enter);
        let (action, _) = h
            .pop_action::<MenuAction>()
            .expect("Enter on a highlighted row selects it");
        assert!(matches!(action, MenuAction::Selected(11)));
    }

    #[test]
    fn enter_without_a_highlight_selects_nothing() {
        let mut h = harness(vec![action("a")]);
        assert_eq!(press(&mut h, NamedKey::Enter), Handled::Yes);
        assert!(h.pop_action::<MenuAction>().is_none());
    }

    #[test]
    fn right_arrow_enters_submenu_then_enter_selects_a_child() {
        // top: [ submenu "S" -> [ leaf id 7 ] ]
        let theme = Theme::default();
        let sub = MenuRowSpec::Submenu {
            label: "S".into(),
            icon: None,
            children: vec![action_id(7, "child")],
        };
        let widget = MenuPanel::new(vec![sub], &theme);
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        h.focus_on(Some(h.root_id()));

        press(&mut h, NamedKey::ArrowDown); // highlight the submenu row
        assert_eq!(highlight(&mut h), Some(0));
        press(&mut h, NamedKey::ArrowRight); // enter the fly-out (highlights its first item)
        assert_eq!(
            h.edit_root_widget(|wm| wm.widget.dbg_kbd_submenu()),
            Some(0),
            "Right should give the fly-out keyboard focus"
        );
        press(&mut h, NamedKey::Enter); // select the fly-out's leaf
        let (action, _) = h
            .pop_action::<MenuAction>()
            .expect("Enter in the fly-out selects its child");
        assert!(matches!(action, MenuAction::Selected(7)));
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

    #[test]
    fn tab_dismisses_the_menu() {
        // WAI-ARIA menu pattern: Tab dismisses the menu. The menu leaves the
        // key unhandled (so masonry's focus traversal moves focus out in tab
        // order — unlike Escape, which the menu consumes); here we assert the
        // observable dismissal: a `Dismissed` action and a cleared highlight.
        let mut h = harness(vec![action("a")]);
        press(&mut h, NamedKey::ArrowDown); // highlight row 0
        assert_eq!(highlight(&mut h), Some(0));

        press(&mut h, NamedKey::Tab);
        assert!(matches!(
            h.pop_action::<MenuAction>().map(|(a, _)| a),
            Some(MenuAction::Dismissed)
        ));
        assert_eq!(highlight(&mut h), None, "dismiss clears the highlight");
    }

    // --- per-row accessibility (TestHarness) ---

    #[test]
    fn rows_expose_per_item_accessibility_semantics() {
        use masonry::accesskit::{Role, Toggled};

        let checkable = MenuRowSpec::Action {
            id: 1,
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
