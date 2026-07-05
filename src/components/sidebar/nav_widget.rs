//! Masonry widget for [`crate::components::sidebar::sidebar_nav`].
//!
//! Lays out `items` (label content built by the view layer) in a vertical
//! list, paints the row background/accent/focus chrome behind each item, and
//! hit-tests pointer events against each item's placed rect — emitting
//! [`SidebarNavSelected`] directly rather than registering per-item action
//! sources (mirrors [`crate::components::tabs::widget::TabsWidget`]).
//!
//! Each item's content is wrapped in [`SidebarNavItemNode`], which exposes it
//! to accessibility as `Role::ListBoxOption` with `selected`/position state
//! and an accessible name. The list itself is a single focus stop
//! (`Role::ListBox`); Up/Down move a roving focus indicator among items
//! without changing the selection, and Enter/Space activate the focused item.

use masonry::accesskit::{self, Node, Orientation, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;

use crate::Theme;
use crate::components::item_list;
use crate::focus_ring::{FOCUS_RING_OUTSET, paint_focus_ring};

/// Width of the active-state left accent bar — accent-bar chrome (stroke-like),
/// not density-scaled.
const ACCENT_WIDTH: f64 = 3.0;
/// Corner radius of the accent bar — accent-bar chrome, not density-scaled.
const ACCENT_RADIUS: f64 = 1.5;
/// Gap between adjacent nav rows — a 2px hairline separation; no token equals
/// it and scaling a hairline is visual noise, so it doesn't scale with density.
const GAP: f64 = 2.0;

/// Action emitted when the user picks a different nav item, by pointer click
/// or Enter/Space on the roving-focused item.
#[derive(Debug, Clone, Copy)]
pub struct SidebarNavSelected(pub usize);

/// Builds the [`SidebarNavItemNode`]s handed to [`ThemedSidebarNav::new`] /
/// [`ThemedSidebarNav::set_items`].
fn wrap_items(
    items: Vec<NewWidget<dyn Widget>>,
    names: Vec<Option<ArcStr>>,
    active: usize,
) -> Vec<WidgetPod<SidebarNavItemNode>> {
    let n = items.len();
    items
        .into_iter()
        .zip(names)
        .enumerate()
        .map(|(i, (content, name))| {
            SidebarNavItemNode::new(content, name, i == active, i, n).to_pod()
        })
        .collect()
}

/// Wraps one nav item's content for accessibility.
///
/// `Role::ListBoxOption`, like `Role::Tab`, doesn't compute its accessible
/// name from descendant text — this wrapper sets it explicitly from the
/// [`super::nav_view::SidebarNavItem`]'s label or `aria_label`. Layout and
/// paint are pure pass-through: all chrome (background, accent bar, focus
/// ring) is painted by [`ThemedSidebarNav`] around this node's placed rect.
pub struct SidebarNavItemNode {
    child: WidgetPod<dyn Widget>,
    name: Option<ArcStr>,
    selected: bool,
    index: usize,
    count: usize,
}

impl SidebarNavItemNode {
    fn new(
        child: NewWidget<dyn Widget>,
        name: Option<ArcStr>,
        selected: bool,
        index: usize,
        count: usize,
    ) -> NewWidget<Self> {
        NewWidget::new(Self {
            child: child.to_pod(),
            name,
            selected,
            index,
            count,
        })
    }

    fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_accessibility_update();
        }
    }

    /// Updates the accessible name announced for this item, e.g. when the
    /// item's label or `aria_label` changes without the item count changing.
    pub fn set_name(this: &mut WidgetMut<'_, Self>, name: Option<ArcStr>) {
        if this.widget.name != name {
            this.widget.name = name;
            this.ctx.request_accessibility_update();
        }
    }

    /// Mutable access to the wrapped content widget for the view layer.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for SidebarNavItemNode {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
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
        Role::ListBoxOption
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
        node.set_position_in_set(self.index + 1);
        node.set_size_of_set(self.count);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Masonry widget backing [`super::nav_view::SidebarNavView`].
pub struct ThemedSidebarNav {
    items: Vec<WidgetPod<SidebarNavItemNode>>,
    /// Each item's placed rect in local coordinates, set during `layout`.
    placed: Vec<Rect>,
    /// Host-controlled selected nav entry.
    active: usize,
    /// Roving keyboard-focus index among items; moved by Up/Down without
    /// changing `active`.
    focused: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    theme: Theme,
    /// Average item height from the last real `layout` pass, cached because
    /// `set_items` clears `placed` — a keypress arriving before the next
    /// layout needs `item_rect_or_estimate`'s fallback to stay reasonably
    /// accurate for variable-height items (wrapped labels, icons) rather
    /// than assuming every item is the same guessed height. `None` until
    /// the first layout ever runs.
    last_avg_item_height: Option<f64>,
}

// --- MARK: HELPERS
impl ThemedSidebarNav {
    /// Item rect for `index`, falling back to an estimate when `placed` is
    /// empty (cleared by `set_items`, not yet rebuilt by layout). Items are
    /// vertical; `y` is estimated from `last_avg_item_height` (the real
    /// average from the last layout, which tracks variable-height items far
    /// better than a fixed per-item guess) when available, falling back to a
    /// theme-metric guess only before the very first layout has ever run.
    /// `x` always spans from 0. Shared with `tabs` via
    /// [`item_list::item_rect_or_estimate`].
    fn pad_h(&self) -> f64 {
        f64::from(self.theme.density.pad_h)
    }

    fn pad_v(&self) -> f64 {
        f64::from(self.theme.density.pad_v)
    }

    fn item_rect_or_estimate(&self, index: usize) -> Rect {
        let item_h = self
            .last_avg_item_height
            .unwrap_or(f64::from(self.theme.density.ui_font_size) + 2.0 * self.pad_v());
        item_list::item_rect_or_estimate(
            &self.placed,
            index,
            item_list::EstimateGeometry {
                axis: Axis::Vertical,
                outer: 0.0,
                cross_offset: 0.0,
                main_extent: item_h,
                cross_extent: 0.0,
                gap: GAP,
            },
        )
    }
}

// --- MARK: BUILDERS
impl ThemedSidebarNav {
    #[must_use]
    pub fn new(
        items: Vec<NewWidget<dyn Widget>>,
        names: Vec<Option<ArcStr>>,
        active: usize,
        theme: &Theme,
    ) -> Self {
        let focused = active.min(items.len().saturating_sub(1));
        Self {
            items: wrap_items(items, names, active),
            placed: Vec::new(),
            active,
            focused,
            hovered: None,
            pressed: None,
            theme: *theme,
            last_avg_item_height: None,
        }
    }
}

// --- MARK: WIDGETMUT
impl ThemedSidebarNav {
    pub fn set_active(this: &mut WidgetMut<'_, Self>, active: usize) {
        if this.widget.active != active {
            let old = this.widget.active;
            this.widget.active = active;
            this.widget.focused = active;
            if let Some(item) = this.widget.items.get_mut(old) {
                let mut node = this.ctx.get_mut(item);
                SidebarNavItemNode::set_selected(&mut node, false);
            }
            if let Some(item) = this.widget.items.get_mut(active) {
                let mut node = this.ctx.get_mut(item);
                SidebarNavItemNode::set_selected(&mut node, true);
            }
            this.widget.hovered = None;
            this.widget.pressed = None;
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

    /// Mutable access to item `index`'s [`SidebarNavItemNode`] wrapper.
    ///
    /// Two-step access for the view layer: call
    /// [`SidebarNavItemNode::child_mut`] on the result to reach the content
    /// widget itself.
    pub fn item_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        index: usize,
    ) -> WidgetMut<'t, SidebarNavItemNode> {
        this.ctx.get_mut(&mut this.widget.items[index])
    }

    /// Replaces the entire item set, e.g. when the item *count* changes.
    pub fn set_items(
        this: &mut WidgetMut<'_, Self>,
        items: Vec<NewWidget<dyn Widget>>,
        names: Vec<Option<ArcStr>>,
        active: usize,
    ) {
        for old in std::mem::take(&mut this.widget.items) {
            this.ctx.remove_child(old);
        }
        this.widget.items = wrap_items(items, names, active);
        this.widget.active = active;
        this.widget.focused = active.min(this.widget.items.len().saturating_sub(1));
        this.widget.hovered = None;
        this.widget.pressed = None;
        this.widget.placed.clear();
        this.ctx.children_changed();
        this.ctx.request_layout();
    }
}

// --- MARK: HELPERS
impl ThemedSidebarNav {
    fn item_at(&self, pos: Point) -> Option<usize> {
        self.placed.iter().position(|r| r.contains(pos))
    }

    /// Resolves item `index`'s row background color for the current
    /// interaction state.
    fn resolve_bg(&self, index: usize) -> Color {
        let p = &self.theme.palette;
        let hovered = self.hovered == Some(index);
        let pressed = self.pressed == Some(index) && hovered;
        if pressed {
            p.surface_hi
        } else if index == self.active || hovered {
            p.surface_2
        } else {
            Color::TRANSPARENT
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedSidebarNav {
    type Action = SidebarNavSelected;

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
                    self.focused = i;
                    ctx.request_focus();
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
                    if self.item_at(pos) == Some(i) && i != self.active {
                        ctx.submit_action::<Self::Action>(SidebarNavSelected(i));
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

    /// Up/Down move a roving focus indicator among items, clamped at the
    /// first/last item (no wraparound). Enter/Space activate the
    /// roving-focused item, submitting [`SidebarNavSelected`] if it differs
    /// from `active`.
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if self.items.is_empty() {
            return;
        }
        match key.key {
            Key::Named(NamedKey::ArrowUp) if key.state == KeyState::Down => {
                if self.focused > 0 {
                    self.focused -= 1;
                    ctx.request_scroll_to(self.item_rect_or_estimate(self.focused));
                    ctx.request_paint_only();
                }
                ctx.set_handled();
            }
            Key::Named(NamedKey::ArrowDown) if key.state == KeyState::Down => {
                if self.focused + 1 < self.items.len() {
                    self.focused += 1;
                    ctx.request_scroll_to(self.item_rect_or_estimate(self.focused));
                    ctx.request_paint_only();
                }
                ctx.set_handled();
            }
            Key::Named(NamedKey::Enter) if key.state.is_up() => {
                ctx.set_handled();
                if self.focused != self.active {
                    ctx.submit_action::<Self::Action>(SidebarNavSelected(self.focused));
                }
            }
            Key::Character(ref c) if c == " " && key.state.is_up() => {
                ctx.set_handled();
                if self.focused != self.active {
                    ctx.submit_action::<Self::Action>(SidebarNavSelected(self.focused));
                }
            }
            _ => {}
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
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let auto_length = len_req.into();
        let n = self.items.len();
        let pad_h = self.pad_h();
        let pad_v = self.pad_v();
        match axis {
            Axis::Vertical => {
                #[allow(clippy::cast_precision_loss)]
                let mut total = GAP * n.saturating_sub(1) as f64;
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
                    total += content_h + 2.0 * pad_v;
                }
                Length::px(total)
            }
            Axis::Horizontal => {
                let mut max_w: f64 = 0.0;
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
                    max_w = max_w.max(content_w + ACCENT_WIDTH + 2.0 * pad_h);
                }
                Length::px(max_w)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_h = self.pad_h();
        let pad_v = self.pad_v();
        let inner_width = (size.width - ACCENT_WIDTH - 2.0 * pad_h).max(0.0);

        self.placed.clear();
        let mut y = 0.0;
        let mut total_height = 0.0;
        for item in &mut self.items {
            let content_size = ctx.compute_size(
                item,
                SizeDef::fit(Size::new(inner_width, 0.0)),
                Size::new(inner_width, 0.0).into(),
            );
            let item_height = content_size.height + 2.0 * pad_v;
            self.placed.push(Rect::from_origin_size(
                Point::new(0.0, y),
                Size::new(size.width, item_height),
            ));

            let cs = Size::new(inner_width, content_size.height);
            ctx.run_layout(item, cs);
            let cx = ACCENT_WIDTH + pad_h;
            let cy = y + pad_v;
            ctx.place_child(item, Point::new(cx, cy));

            y += item_height + GAP;
            total_height += item_height;
        }
        if !self.items.is_empty() {
            self.last_avg_item_height = Some(total_height / item_list::index_f64(self.items.len()));
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;

        for (i, rect) in self.placed.iter().enumerate() {
            let bg = self.resolve_bg(i);
            if bg.components[3] > 0.0 {
                let bg_rect = RoundedRect::from_rect(*rect, 0.0);
                painter.fill(bg_rect, bg).draw();
            }

            if i == self.active {
                let accent = RoundedRect::from_origin_size(
                    rect.origin(),
                    Size::new(ACCENT_WIDTH, rect.height()),
                    ACCENT_RADIUS,
                );
                painter.fill(accent, p.teal).draw();
            }

            if i == self.focused && ctx.is_focus_target() {
                let inset = rect.inset(-FOCUS_RING_OUTSET);
                paint_focus_ring(painter, RoundedRect::from_rect(inset, 0.0), &self.theme);
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ListBox
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_orientation(Orientation::Vertical);
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
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::SizedBox;

    use super::*;
    use crate::theme::Density;

    fn item(width: f64, height: f64) -> NewWidget<dyn Widget> {
        NewWidget::new(SizedBox::empty().size(Length::px(width), Length::px(height))).erased()
    }

    fn widget(items: Vec<NewWidget<dyn Widget>>, active: usize) -> ThemedSidebarNav {
        let names = vec![None; items.len()];
        ThemedSidebarNav::new(items, names, active, &Theme::default())
    }

    fn harness(items: Vec<NewWidget<dyn Widget>>, active: usize) -> TestHarness<ThemedSidebarNav> {
        TestHarness::create(
            default_property_set(),
            NewWidget::new(widget(items, active)),
        )
    }

    fn harness_with_names(
        items: Vec<NewWidget<dyn Widget>>,
        names: Vec<Option<ArcStr>>,
        active: usize,
    ) -> TestHarness<ThemedSidebarNav> {
        TestHarness::create(
            default_property_set(),
            NewWidget::new(ThemedSidebarNav::new(
                items,
                names,
                active,
                &Theme::default(),
            )),
        )
    }

    // --- item_at ---

    #[test]
    fn item_at_returns_index_of_containing_rect() {
        let mut w = widget(vec![], 0);
        w.placed = vec![
            Rect::from_origin_size(Point::ORIGIN, Size::new(100.0, 20.0)),
            Rect::from_origin_size(Point::new(0.0, 20.0), Size::new(100.0, 20.0)),
        ];
        assert_eq!(w.item_at(Point::new(10.0, 10.0)), Some(0));
        assert_eq!(w.item_at(Point::new(10.0, 30.0)), Some(1));
        assert_eq!(w.item_at(Point::new(10.0, 100.0)), None);
    }

    // --- layout ---

    #[test]
    fn layout_stacks_items_vertically_without_overlap() {
        let mut h = harness(vec![item(40.0, 20.0), item(40.0, 30.0)], 0);
        let placed = h.edit_root_widget(|wm| wm.widget.placed.clone());

        assert_eq!(placed.len(), 2);
        assert!(placed[0].y1 <= placed[1].y0 + 1e-6);
    }

    #[test]
    fn item_padding_scales_with_density() {
        let placed_height = |density: Density| {
            let items = vec![item(40.0, 20.0)];
            let widget = ThemedSidebarNav::new(
                items,
                vec![None],
                0,
                &Theme::default().with_density(density),
            );
            let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
            h.edit_root_widget(|wm| wm.widget.placed[0].height())
        };
        // Balanced must keep the pre-token geometry: content 20 + 2 × PAD_V(6).
        assert!((placed_height(Density::balanced()) - (20.0 + 2.0 * 6.0)).abs() < 1e-6);
        assert!(placed_height(Density::compact()) < placed_height(Density::balanced()));
        assert!(placed_height(Density::balanced()) < placed_height(Density::airy()));
    }

    #[test]
    fn item_rect_or_estimate_uses_real_average_height_not_uniform_guess() {
        // Wide spread so the fixed theme-metric guess can't coincidentally
        // match the real average.
        let mut h = harness(vec![item(100.0, 10.0), item(100.0, 200.0)], 0);
        let (real_avg_height, uniform_guess) = h.edit_root_widget(|wm| {
            let uniform_guess = f64::from(wm.widget.theme.density.ui_font_size)
                + 2.0 * f64::from(wm.widget.theme.density.pad_v);
            let real_avg = wm
                .widget
                .last_avg_item_height
                .expect("layout should have run and cached an average height");
            (real_avg, uniform_guess)
        });
        assert!(
            (real_avg_height - uniform_guess).abs() > 1.0,
            "test fixture should produce an average height that's clearly different \
             from the fixed guess (avg={real_avg_height}, guess={uniform_guess})"
        );

        // Simulate the transitional window set_items creates: placed
        // cleared, layout hasn't run yet.
        let estimated_height = h.edit_root_widget(|wm| {
            wm.widget.placed.clear();
            wm.widget.item_rect_or_estimate(0).height()
        });

        assert!(
            (estimated_height - real_avg_height).abs() < 1e-6,
            "estimate should track the real average item height from the last layout \
             (est={estimated_height}, expected={real_avg_height})"
        );
    }

    // --- pointer interaction ---

    #[test]
    fn clicking_an_unselected_item_emits_sidebar_nav_selected() {
        let mut h = harness(vec![item(100.0, 20.0), item(100.0, 20.0)], 0);
        let target = h.edit_root_widget(|wm| wm.widget.placed[1].center());

        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        let (action, widget_id) = h
            .pop_action::<SidebarNavSelected>()
            .expect("expected an action");
        assert_eq!(action.0, 1);
        assert_eq!(widget_id, h.root_id());
    }

    #[test]
    fn clicking_the_active_item_emits_nothing() {
        let mut h = harness(vec![item(100.0, 20.0), item(100.0, 20.0)], 0);
        let target = h.edit_root_widget(|wm| wm.widget.placed[0].center());

        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        assert!(h.pop_action::<SidebarNavSelected>().is_none());
    }

    // --- keyboard navigation ---

    #[test]
    fn arrow_keys_move_roving_focus_and_clamp_at_ends() {
        let mut h = harness(
            vec![item(100.0, 20.0), item(100.0, 20.0), item(100.0, 20.0)],
            0,
        );
        h.focus_on(Some(h.root_id()));

        // ArrowUp at the first item stays put.
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowUp)));
        assert_eq!(h.edit_root_widget(|wm| wm.widget.focused), 0);

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        assert_eq!(h.edit_root_widget(|wm| wm.widget.focused), 1);

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        // ArrowDown at the last item stays put.
        assert_eq!(h.edit_root_widget(|wm| wm.widget.focused), 2);
    }

    #[test]
    fn arrow_keys_do_not_change_active_or_emit_actions() {
        let mut h = harness(vec![item(100.0, 20.0), item(100.0, 20.0)], 0);
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));

        assert_eq!(h.edit_root_widget(|wm| wm.widget.active), 0);
        assert!(h.pop_action::<SidebarNavSelected>().is_none());
    }

    #[test]
    fn enter_activates_the_roving_focused_item() {
        let mut h = harness(vec![item(100.0, 20.0), item(100.0, 20.0)], 0);
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));

        let (action, _) = h
            .pop_action::<SidebarNavSelected>()
            .expect("expected an action");
        assert_eq!(action.0, 1);
    }

    #[test]
    fn space_activates_the_roving_focused_item() {
        let mut h = harness(vec![item(100.0, 20.0), item(100.0, 20.0)], 0);
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));

        let (action, _) = h
            .pop_action::<SidebarNavSelected>()
            .expect("expected an action");
        assert_eq!(action.0, 1);
    }

    #[test]
    fn enter_on_the_already_active_item_emits_nothing() {
        let mut h = harness(vec![item(100.0, 20.0), item(100.0, 20.0)], 0);
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));

        assert!(h.pop_action::<SidebarNavSelected>().is_none());
    }

    // --- setters ---

    #[test]
    fn set_active_updates_selected_flags_and_resets_focused() {
        let mut h = harness(vec![item(100.0, 20.0), item(100.0, 20.0)], 0);

        h.edit_root_widget(|mut wm| {
            ThemedSidebarNav::set_active(&mut wm, 1);
            assert!(!ThemedSidebarNav::item_mut(&mut wm, 0).widget.selected);
            assert!(ThemedSidebarNav::item_mut(&mut wm, 1).widget.selected);
            assert_eq!(wm.widget.focused, 1);
        });
    }

    // --- accessibility ---

    #[test]
    fn items_expose_label_selected_and_set_position() {
        let mut h = harness_with_names(
            vec![item(100.0, 20.0), item(100.0, 20.0)],
            vec![Some("First".into()), Some("Second".into())],
            1,
        );
        let ids = h.edit_root_widget(|mut wm| {
            let first = ThemedSidebarNav::item_mut(&mut wm, 0).id();
            let second = ThemedSidebarNav::item_mut(&mut wm, 1).id();
            [first, second]
        });
        h.redraw();

        let first = h.access_node(ids[0]).expect("node exists");
        assert_eq!(first.label(), Some("First".to_string()));
        assert_eq!(first.is_selected(), Some(false));
        assert_eq!(first.position_in_set(), Some(1));
        assert_eq!(first.size_of_set(), Some(2));

        let second = h.access_node(ids[1]).expect("node exists");
        assert_eq!(second.label(), Some("Second".to_string()));
        assert_eq!(second.is_selected(), Some(true));
        assert_eq!(second.position_in_set(), Some(2));
    }

    #[test]
    fn set_name_updates_accessible_label() {
        let mut h = harness_with_names(vec![item(100.0, 20.0)], vec![Some("Original".into())], 0);
        let id = h.edit_root_widget(|mut wm| ThemedSidebarNav::item_mut(&mut wm, 0).id());

        h.edit_root_widget(|mut wm| {
            let mut node = ThemedSidebarNav::item_mut(&mut wm, 0);
            SidebarNavItemNode::set_name(&mut node, Some("Updated".into()));
        });
        h.redraw();

        let node = h.access_node(id).expect("node exists");
        assert_eq!(node.label(), Some("Updated".to_string()));
    }
}
