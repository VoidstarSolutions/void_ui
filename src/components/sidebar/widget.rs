//! Masonry widget for the Tessera sidebar nav item.
//!
//! A full-width, left-aligned nav row. When `active`, a 3 px teal accent bar
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
/// resolve background and accent colors at paint time. The `active` flag is
/// host-controlled; pointer state (hovered, pressed) is read from the widget
/// context.
pub struct ThemedSidebarItem {
    child: WidgetPod<dyn Widget>,
    theme: Theme,
    /// Host-controlled selected-row state.
    active: bool,
}

// --- MARK: BUILDERS
impl ThemedSidebarItem {
    /// Creates a new sidebar item with the supplied child and theme.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            child: child.erased().to_pod(),
            theme: *theme,
            active: false,
        }
    }

    /// Marks this item as the currently-selected nav entry.
    #[must_use]
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
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

    /// Toggles the host-driven `active` flag. Requests a repaint on change.
    pub fn set_active(this: &mut WidgetMut<'_, Self>, active: bool) {
        if this.widget.active != active {
            this.widget.active = active;
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
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
    /// | active         | `surface_2`  |
    fn resolve_bg(&self, hovered: bool, pressed: bool) -> Color {
        let p = &self.theme.palette;
        if pressed && hovered {
            p.surface_hi
        } else if self.active || hovered {
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
        if interaction::keyboard_activate(event, true) {
            ctx.set_handled();
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if interaction::is_access_click(event) {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // NOTE: unlike button/checkbox/toggle/radio, the sidebar item has
        // no `disabled` field, so there is no WidgetAdded disabled sync
        // and no DisabledChanged repaint here (it cannot adopt
        // `interaction::interaction_update`). Known API inconsistency —
        // adding `disabled` to sidebar items is tracked in the
        // API-consistency plan.
        match event {
            Update::HoveredChanged(_) | Update::FocusChanged(_) => {
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
        let size = ctx.border_box_size();
        let InteractionState {
            hovered,
            pressed,
            focused,
        } = InteractionState::from_paint_ctx(ctx);
        let p = &self.theme.palette;

        let bg = self.resolve_bg(hovered, pressed);
        let bg_rect = RoundedRect::from_origin_size(Point::ORIGIN, size, 0.0);
        if bg.components[3] > 0.0 {
            painter.fill(bg_rect, bg).draw();
        }

        if self.active {
            let accent = RoundedRect::from_origin_size(
                Point::ORIGIN,
                Size::new(ACCENT_WIDTH, size.height),
                ACCENT_RADIUS,
            );
            painter.fill(accent, p.teal).draw();
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
        node.add_action(accesskit::Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        true
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
}
