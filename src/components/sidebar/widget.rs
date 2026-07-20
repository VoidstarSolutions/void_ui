//! Masonry widget for the sidebar nav item.
//!
//! A full-width, left-aligned nav row. When `selected`, a 3 px accent bar
//! is painted on the left edge and the label renders in the full text color.
//! Pointer state (hover, press) is read from the widget context, matching the
//! same paint-driven pattern as [`crate::components::button::widget::ThemedButton`].
//!
//! Emits [`ButtonPress`] on primary-pointer release inside the widget and on
//! Space / Enter while focused.

use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::widgets::ButtonPress;

use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::interaction::{self, InteractionState};
use crate::focus_ring::{FOCUS_RING_OUTSET, paint_focus_ring};

/// Width of the active-state left accent bar — accent-bar chrome (stroke-like),
/// not density-scaled.
const ACCENT_WIDTH: f64 = 3.0;
/// Corner radius of the accent bar — accent-bar chrome, not density-scaled.
const ACCENT_RADIUS: f64 = 1.5;

/// Themed, interactive sidebar navigation item.
///
/// Owns its child (typically a `Label`) and a [`Theme`] value used to
/// resolve background and accent colors at paint time. The `selected` flag is
/// host-controlled; pointer state (hovered, pressed) is read from the widget
/// context.
pub struct ThemedSidebarItem {
    child: WidgetPod<dyn Widget>,
    theme: Theme,
    /// Host-controlled selected-row state.
    selected: bool,
    /// True for the span between a Space/Enter key-down and its matching
    /// key-up (or an intervening focus loss) — the keyboard equivalent of
    /// the pointer-driven `pressed` flag read from the widget context, so
    /// keyboard activation shows the same pressed fill a pointer click does.
    keyboard_pressed: bool,
    /// Host-controlled disabled state.
    disabled: bool,
}

// --- MARK: BUILDERS
impl ThemedSidebarItem {
    /// Creates a new sidebar item with the supplied child and theme.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            child: child.erased().to_pod(),
            theme: *theme,
            selected: false,
            keyboard_pressed: false,
            disabled: false,
        }
    }

    /// Marks this item as the currently-selected nav entry.
    #[must_use]
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Suppresses all interaction and mutes the visual appearance.
    #[must_use]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// --- MARK: WIDGETMUT
impl ThemedSidebarItem {
    /// Replaces the theme. Requests layout + repaint if the value changed.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Toggles the host-driven `selected` flag. Requests a repaint on change.
    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Sets the disabled state. Syncs with masonry's event-routing flag.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: PAINT STATE
impl ThemedSidebarItem {
    /// Resolves the row background color for the current interaction state.
    ///
    /// | state          | bg           |
    /// |----------------|--------------|
    /// | default        | transparent  |
    /// | hover          | `surface_2`  |
    /// | pressed        | `surface_hi` |
    /// | selected       | `surface_hi` |
    ///
    /// `selected` and `hover` resolve to distinct fills (rather than sharing
    /// one) because they're independent per-row widget states: hovering one
    /// row while a different row is selected must not make the two
    /// indistinguishable.
    fn resolve_bg(&self, hovered: bool, pressed: bool) -> Color {
        if self.disabled {
            return Color::TRANSPARENT;
        }
        let p = &self.theme.palette;
        if pressed || self.selected {
            p.surface_hi
        } else if hovered {
            p.surface_2
        } else {
            Color::TRANSPARENT
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedSidebarItem {
    type Action = ButtonPress;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled {
            return;
        }
        match click::primary_click(ctx, event) {
            Some(ClickPhase::Down(_)) => {
                ctx.request_focus();
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up { completed, .. }) => {
                if completed {
                    ctx.submit_action::<Self::Action>(ButtonPress {
                        button: Some(PointerButton::Primary),
                    });
                }
                ctx.request_paint_only();
            }
            None => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.disabled {
            return;
        }
        if interaction::keyboard_press_start(event, true) {
            ctx.set_handled();
            self.keyboard_pressed = true;
            ctx.request_paint_only();
        } else if interaction::keyboard_activate(event, true) {
            ctx.set_handled();
            self.keyboard_pressed = false;
            ctx.request_paint_only();
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if self.disabled {
            return;
        }
        if interaction::is_access_click(event) {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(false)) {
            // Losing focus mid-press (e.g. Tab away while Space is still
            // held) would otherwise leave `keyboard_pressed` stuck true
            // with no matching key-up ever arriving to clear it.
            self.keyboard_pressed = false;
        }
        match event {
            // Sync masonry's disabled flag on first attach (matches the
            // checkbox/button pattern; previously missing here).
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
            }
            Update::HoveredChanged(_) | Update::FocusChanged(_) | Update::DisabledChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
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
        let pad_h = f64::from(self.theme.density.pad_h);
        let pad_v = f64::from(self.theme.density.pad_v);
        let (main_pad, cross_pad) = match axis {
            Axis::Horizontal => (ACCENT_WIDTH + 2.0 * pad_h, 2.0 * pad_v),
            Axis::Vertical => (2.0 * pad_v, ACCENT_WIDTH + 2.0 * pad_h),
        };
        let inner_cross = cross_length.map(|c| Length::px((c.get() - cross_pad).max(0.0)));
        let auto_length = len_req.into();
        let context_size = LayoutSize::maybe(axis.cross(), inner_cross);
        let child_length = ctx.compute_length(
            &mut self.child,
            auto_length,
            context_size,
            axis,
            inner_cross,
        );
        Length::px(child_length.get() + main_pad)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_h = f64::from(self.theme.density.pad_h);
        let pad_v = f64::from(self.theme.density.pad_v);
        let inner = Size::new(
            (size.width - ACCENT_WIDTH - 2.0 * pad_h).max(0.0),
            (size.height - 2.0 * pad_v).max(0.0),
        );
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(inner), inner.into());
        ctx.run_layout(&mut self.child, child_size);
        let child_x = ACCENT_WIDTH + pad_h;
        let child_y = pad_v + ((inner.height - child_size.height) * 0.5).max(0.0);
        ctx.place_child(&mut self.child, Point::new(child_x, child_y));
        ctx.derive_baselines(&self.child);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box().size();
        let InteractionState {
            hovered,
            pressed,
            focused,
        } = InteractionState::from_paint_ctx(ctx);
        let pressed = pressed || self.keyboard_pressed;
        let p = &self.theme.palette;

        let bg = self.resolve_bg(hovered, pressed);
        let bg_rect = RoundedRect::from_origin_size(Point::ORIGIN, size, 0.0);
        if bg.components[3] > 0.0 {
            painter.fill(bg_rect, bg).draw();
        }

        if self.selected && !self.disabled {
            let accent = RoundedRect::from_origin_size(
                Point::ORIGIN,
                Size::new(ACCENT_WIDTH, size.height),
                ACCENT_RADIUS,
            );
            painter.fill(accent, p.accent).draw();
        }

        if focused {
            let inset = FOCUS_RING_OUTSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                0.0,
            );
            paint_focus_ring(painter, focus_rect, &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if !self.disabled {
            node.add_action(accesskit::Action::Click);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        !self.disabled
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{NewWidget, PointerButton, TextEvent};
    use masonry::kurbo::Point;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::{ButtonPress, Label};

    use super::ThemedSidebarItem;
    use crate::Theme;

    fn harness() -> TestHarness<ThemedSidebarItem> {
        let widget = ThemedSidebarItem::new(NewWidget::new(Label::new("Nav")), &Theme::dark());
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (160, 28))
    }

    #[test]
    fn selected_and_hovered_resolve_to_different_fills() {
        // Regression for #95: a selected row and a separately-hovered row
        // are different widget instances, so `resolve_bg` must not give
        // them the same fill or the two become visually indistinguishable.
        let theme = Theme::dark();
        let selected =
            ThemedSidebarItem::new(NewWidget::new(Label::new("A")), &theme).with_selected(true);
        let hovered_unselected = ThemedSidebarItem::new(NewWidget::new(Label::new("B")), &theme);

        let selected_bg = selected.resolve_bg(false, false);
        let hovered_bg = hovered_unselected.resolve_bg(true, false);

        assert_ne!(selected_bg, hovered_bg);
        assert_eq!(selected_bg, theme.palette.surface_hi);
        assert_eq!(hovered_bg, theme.palette.surface_2);
    }

    #[test]
    fn pressed_takes_priority_over_selected() {
        let theme = Theme::dark();
        let widget =
            ThemedSidebarItem::new(NewWidget::new(Label::new("A")), &theme).with_selected(true);
        assert_eq!(widget.resolve_bg(true, true), theme.palette.surface_hi);
    }

    #[test]
    fn pointer_click_submits_press() {
        let mut h = harness();
        h.mouse_move(Point::new(80.0, 14.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn drag_out_cancels_the_press() {
        let mut h = harness();
        h.mouse_move(Point::new(80.0, 14.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_move(Point::new(400.0, 400.0));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<ButtonPress>().is_none());
    }

    #[test]
    fn space_and_enter_activate_when_focused() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.pop_action::<ButtonPress>().is_some());

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn space_key_down_shows_the_pressed_fill_until_key_up() {
        // Regression: on_text_event used to only ever fire on key-up, so
        // Space/Enter "clicking" showed no pressed-fill feedback the way a
        // pointer click does.
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(h.pop_action::<ButtonPress>().is_none(), "not yet activated");

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn losing_focus_mid_press_clears_the_keyboard_pressed_flag() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.keyboard_pressed));

        h.focus_on(None);
        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
    }

    #[test]
    fn disabled_suppresses_click() {
        let theme = Theme::default();
        let widget =
            ThemedSidebarItem::new(NewWidget::new(Label::new("Nav")), &theme).with_disabled(true);
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (200, 32),
        );
        h.mouse_move(Point::new(20.0, 16.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            h.pop_action::<ButtonPress>().is_none(),
            "disabled sidebar item must not emit ButtonPress"
        );
    }
}
