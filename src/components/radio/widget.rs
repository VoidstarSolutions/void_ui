//! Masonry widget for the themed radio button.
//!
//! Paints a circle ring with an inner dot when selected. Layout is
//! `[circle] [gap] [label]` — the host controls the `selected` flag to indicate
//! which option in a group is currently selected.
//!
//! Emits [`ButtonPress`] on primary-pointer release and on Space while focused.

use masonry::accesskit;
use masonry::accesskit::{Node, Role, Toggled};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Circle, Point, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::widgets::ButtonPress;

use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::interaction::{self, InteractionState};
use crate::focus_ring::{FOCUS_RING_OUTSET, paint_focus_ring};

/// Diameter of the radio circle in logical pixels.
const RADIO_DIAMETER: f64 = 14.0;
/// Gap between the circle and the label.
const RADIO_GAP: f64 = 6.0;
/// Circle border stroke width.
const BORDER_WIDTH: f64 = 1.5;
/// Radius of the inner selection dot (drawn when `selected`).
const DOT_RADIUS: f64 = 3.5;

/// Themed radio button widget.
///
/// Owns its label child and a [`Theme`]. The host drives `selected` to indicate
/// the selected option; pointer and focus state are read from the widget context.
/// Group mutual-exclusion is host-managed: each radio in a group gets
/// `.selected(selected_value == my_value)` and fires a callback that updates the
/// selection in app state.
pub struct ThemedRadio {
    child: WidgetPod<dyn Widget>,
    theme: Theme,
    /// Host-controlled selected state.
    selected: bool,
    /// When true, all interaction is suppressed and colors are muted.
    disabled: bool,
    /// True for the span between a Space key-down and its matching key-up
    /// (or an intervening focus loss) — the keyboard equivalent of the
    /// pointer-driven `pressed` flag read from the widget context, so
    /// keyboard activation shows the same pressed ring a pointer click does.
    keyboard_pressed: bool,
}

// --- MARK: BUILDERS
impl ThemedRadio {
    /// Creates a new themed radio button with the supplied child and theme.
    ///
    /// The child should be a non-interactive widget — typically a `Label`.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            child: child.erased().to_pod(),
            theme: *theme,
            selected: false,
            disabled: false,
            keyboard_pressed: false,
        }
    }

    /// Marks this radio as the currently-selected option.
    #[must_use]
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Suppresses all interaction and mutes visual appearance.
    #[must_use]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

// --- MARK: WIDGETMUT
impl ThemedRadio {
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

    /// Sets the disabled state. Propagates to masonry's system-level disabled
    /// flag and requests a repaint.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the label child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: PAINT STATE
impl ThemedRadio {
    /// Resolves `(ring_color, dot_color)` for the current interaction state.
    ///
    /// `dot_color` is only meaningful when `selected == true`; callers should
    /// skip painting the dot otherwise.
    fn resolve_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (p.text_faint, p.text_faint);
        }
        let ring = if self.selected || pressed {
            p.accent
        } else if hovered {
            p.border_strong
        } else {
            p.border
        };
        let dot = p.accent;
        (ring, dot)
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedRadio {
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
        // ARIA radio-group convention: Space toggles the focused radio;
        // Enter is deliberately NOT an activation key for radios (it is
        // reserved for the form's default action, and arrow keys move the
        // selection). Hence `accept_enter: false`, unlike checkbox/toggle/
        // button. See WAI-ARIA Authoring Practices, "Radio Group".
        if interaction::keyboard_press_start(event, false) {
            ctx.set_handled();
            self.keyboard_pressed = true;
            ctx.request_paint_only();
        } else if interaction::keyboard_activate(event, false) {
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
        // Losing focus mid-press (e.g. Tab away while Space is still held)
        // would otherwise leave `keyboard_pressed` stuck true with no
        // matching key-up ever arriving to clear it.
        if matches!(event, Update::FocusChanged(false)) {
            self.keyboard_pressed = false;
        }
        interaction::interaction_update(ctx, event, self.disabled);
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
        // The circle occupies [RADIO_DIAMETER + RADIO_GAP] of horizontal space.
        // On the vertical axis it doesn't stack, so contributes 0 length but
        // reduces the horizontal cross-space available to the label.
        let circle_h = Length::px(RADIO_DIAMETER + RADIO_GAP);
        let circle_v = Length::px(0.0);

        let extra_main = match axis {
            Axis::Horizontal => circle_h,
            Axis::Vertical => circle_v,
        };
        let extra_cross = match axis {
            Axis::Horizontal => circle_v,
            Axis::Vertical => circle_h,
        };

        let cross_space = cross_length.map(|c| Length::px((c.get() - extra_cross.get()).max(0.0)));
        let auto_length = len_req.reduce(extra_main).into();
        let context_size = LayoutSize::maybe(axis.cross(), cross_space);

        let label_len = ctx.compute_length(
            &mut self.child,
            auto_length,
            context_size,
            axis,
            cross_space,
        );

        match axis {
            Axis::Horizontal => Length::px(label_len.get() + circle_h.get()),
            Axis::Vertical => Length::px(label_len.get().max(RADIO_DIAMETER) + circle_v.get()),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let label_x = RADIO_DIAMETER + RADIO_GAP;
        let label_space = Size::new((size.width - label_x).max(0.0), size.height);
        let label_size = ctx.compute_size(
            &mut self.child,
            SizeDef::fit(label_space),
            label_space.into(),
        );
        ctx.run_layout(&mut self.child, label_size);

        let label_y = ((size.height - label_size.height) * 0.5).max(0.0);
        ctx.place_child(&mut self.child, Point::new(label_x, label_y));
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
        let pressed = pressed || self.keyboard_pressed;

        let (ring_color, dot_color) = self.resolve_colors(hovered, pressed);

        let cx = RADIO_DIAMETER * 0.5;
        let cy = size.height * 0.5;
        let center = Point::new(cx, cy);

        let ring_radius = (RADIO_DIAMETER - BORDER_WIDTH) * 0.5;
        painter
            .stroke(
                Circle::new(center, ring_radius),
                &Stroke::new(BORDER_WIDTH),
                ring_color,
            )
            .draw();

        if self.selected {
            painter
                .fill(Circle::new(center, DOT_RADIUS), dot_color)
                .draw();
        }

        if focused && !self.disabled {
            let focus_radius = (RADIO_DIAMETER * 0.5) + FOCUS_RING_OUTSET;
            paint_focus_ring(painter, Circle::new(center, focus_radius), &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::RadioButton
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
        node.set_toggled(if self.selected {
            Toggled::True
        } else {
            Toggled::False
        });
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

    use super::ThemedRadio;
    use crate::Theme;

    fn harness() -> TestHarness<ThemedRadio> {
        let widget = ThemedRadio::new(NewWidget::new(Label::new("choice")), &Theme::dark());
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (120, 24))
    }

    #[test]
    fn pointer_click_submits_press() {
        let mut h = harness();
        h.mouse_move(Point::new(60.0, 12.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn space_activates_when_focused() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    /// ARIA radio convention: Enter must NOT activate a radio (it is
    /// reserved for the form's default action; arrows move selection).
    #[test]
    fn enter_does_not_activate() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.pop_action::<ButtonPress>().is_none());
    }

    #[test]
    fn space_key_down_shows_the_pressed_ring_until_key_up() {
        // Regression: on_text_event used to only ever fire on key-up, so
        // Space "clicking" showed no pressed-ring feedback the way a
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
}
