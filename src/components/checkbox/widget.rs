//! Masonry widget owning the checkbox state machine.
//!
//! Paints the box, border, checkmark, and optional focus ring directly from
//! a `Theme` value. The optional text label is a masonry `Label` child widget
//! so text layout is handled by the framework.
//!
//! Emits [`CheckboxPress`] on primary-pointer release inside the widget and
//! on Space/Enter while focused.

use masonry::accesskit::{self, Node, Role, Toggled};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::CheckboxPress;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::icon::{IconName, icon};
use crate::components::interaction::{self, InteractionState};
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};

/// Stroke width of the box border.
const BOX_BORDER: f64 = 1.0;
/// Uniform padding around the widget — provides clearance for the focus ring.
const PAD: f64 = 2.0;

/// Interactive checkbox widget.
///
/// Owns its optional label child and a [`Theme`] used to resolve colors at
/// paint time. The `checked` flag is host-controlled.
pub struct CheckboxWidget {
    checked: bool,
    disabled: bool,
    theme: Theme,
    /// Lucide Check icon — always present, transparent when unchecked.
    check_icon: WidgetPod<Label>,
    /// Optional text label rendered to the right of the box.
    label: Option<WidgetPod<dyn Widget>>,
    /// True for the span between a Space/Enter key-down and its matching
    /// key-up (or an intervening focus loss) — the keyboard equivalent of
    /// the pointer-driven `pressed` flag read from the widget context, so
    /// keyboard activation shows the same pressed fill a pointer click does.
    keyboard_pressed: bool,
}

// --- MARK: BUILDERS
impl CheckboxWidget {
    /// Creates a new checkbox in the given state.
    #[must_use]
    pub fn new(theme: &Theme, checked: bool, disabled: bool) -> Self {
        let check_color = if checked {
            if disabled {
                theme.palette.text_faint
            } else {
                theme.palette.accent
            }
        } else {
            Color::TRANSPARENT
        };
        Self {
            checked,
            disabled,
            theme: *theme,
            check_icon: icon(IconName::Check)
                .color(check_color)
                .build_widget(theme)
                .to_pod(),
            label: None,
            keyboard_pressed: false,
        }
    }

    /// Attaches a text label child widget.
    #[must_use]
    pub fn with_label(mut self, label: NewWidget<impl Widget + ?Sized>) -> Self {
        self.label = Some(label.erased().to_pod());
        self
    }
}

// --- MARK: WIDGETMUT
impl CheckboxWidget {
    fn check_color(checked: bool, disabled: bool, theme: &Theme) -> Color {
        if checked {
            if disabled {
                theme.palette.text_faint
            } else {
                theme.palette.accent
            }
        } else {
            Color::TRANSPARENT
        }
    }

    /// Sets the checked state. Requests a repaint and accessibility update on change.
    pub fn set_checked(this: &mut WidgetMut<'_, Self>, checked: bool) {
        if this.widget.checked != checked {
            this.widget.checked = checked;
            let color = Self::check_color(checked, this.widget.disabled, &this.widget.theme);
            {
                let mut check = this.ctx.get_mut(&mut this.widget.check_icon);
                check.insert_prop(ContentColor::new(color));
            }
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets the disabled state. Syncs with masonry's event-routing flag.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            if this.widget.checked {
                let color = Self::check_color(true, disabled, &this.widget.theme);
                {
                    let mut check = this.ctx.get_mut(&mut this.widget.check_icon);
                    check.insert_prop(ContentColor::new(color));
                }
            }
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the theme. Requests layout + repaint on change.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            let color = Self::check_color(this.widget.checked, this.widget.disabled, theme);
            {
                let mut check = this.ctx.get_mut(&mut this.widget.check_icon);
                check.insert_prop(ContentColor::new(color));
                Label::insert_style(
                    &mut check,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the label child, if one exists.
    pub fn label_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> Option<WidgetMut<'t, dyn Widget>> {
        this.widget.label.as_mut().map(|l| this.ctx.get_mut(l))
    }

    /// Attaches a new label child, replacing any existing one.
    pub fn attach_label(this: &mut WidgetMut<'_, Self>, label: NewWidget<impl Widget + ?Sized>) {
        if let Some(old) = this.widget.label.take() {
            this.ctx.remove_child(old);
        }
        this.widget.label = Some(label.erased().to_pod());
        this.ctx.children_changed();
    }

    /// Detaches and removes the label child.
    pub fn detach_label(this: &mut WidgetMut<'_, Self>) {
        if let Some(old) = this.widget.label.take() {
            this.ctx.remove_child(old);
            this.ctx.children_changed();
        }
    }
}

// --- MARK: PAINT STATE
impl CheckboxWidget {
    fn box_size(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
    }

    fn label_gap(&self) -> f64 {
        f64::from(self.theme.density.gap)
    }

    /// Resolves `(background, border)` for the checkbox box in the current state.
    fn resolve_box_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (Color::TRANSPARENT, p.border);
        }
        if self.checked {
            let bg = if pressed || hovered {
                p.accent
            } else {
                p.accent_soft
            };
            (bg, p.accent)
        } else {
            let bg = if pressed {
                p.surface_hi
            } else if hovered {
                p.surface_2
            } else {
                Color::TRANSPARENT
            };
            (bg, p.border)
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for CheckboxWidget {
    type Action = CheckboxPress;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled {
            return;
        }
        // Shared Down→capture / Up-iff-active-and-hovered recognizer
        // (drag out of the widget to cancel the press).
        match click::primary_click(ctx, event) {
            Some(ClickPhase::Down(_)) => {
                ctx.request_focus();
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up { completed, .. }) => {
                if completed {
                    ctx.submit_action::<Self::Action>(CheckboxPress);
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
            ctx.submit_action::<Self::Action>(CheckboxPress);
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
            ctx.submit_action::<Self::Action>(CheckboxPress);
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
        ctx.register_child(&mut self.check_icon);
        if let Some(label) = &mut self.label {
            ctx.register_child(label);
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
        let box_sz = self.box_size();
        let label_gap = self.label_gap();

        // Measure check icon at box size (inside the box, doesn't affect widget measurement).
        let check_sz = Length::px(box_sz);
        let check_ctx = LayoutSize::maybe(axis.cross(), Some(check_sz));
        let _ = ctx.compute_length(
            &mut self.check_icon,
            len_req.into(),
            check_ctx,
            axis,
            Some(check_sz),
        );

        match axis {
            Axis::Horizontal => {
                let label_w = if let Some(label) = &mut self.label {
                    let inner_cross =
                        cross_length.map(|c| Length::px((c.get() - 2.0 * PAD).max(0.0)));
                    let context = LayoutSize::maybe(Axis::Vertical, inner_cross);
                    let w = ctx.compute_length(label, len_req.into(), context, axis, inner_cross);
                    label_gap + w.get()
                } else {
                    0.0
                };
                Length::px(2.0 * PAD + box_sz + label_w)
            }
            Axis::Vertical => {
                let label_h = if let Some(label) = &mut self.label {
                    let avail_w = cross_length
                        .map(|c| Length::px((c.get() - 2.0 * PAD - box_sz - label_gap).max(0.0)));
                    let context = LayoutSize::maybe(Axis::Horizontal, avail_w);
                    let h = ctx.compute_length(label, len_req.into(), context, axis, avail_w);
                    h.get()
                } else {
                    0.0
                };
                Length::px(2.0 * PAD + box_sz.max(label_h))
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let box_sz = self.box_size();
        let label_gap = self.label_gap();
        let content_h = (size.height - 2.0 * PAD).max(0.0);
        let box_y = PAD + ((content_h - box_sz) * 0.5).max(0.0);

        // Place check icon at the box position.
        ctx.run_layout(&mut self.check_icon, Size::new(box_sz, box_sz));
        ctx.place_child(&mut self.check_icon, Point::new(PAD, box_y));

        if let Some(label) = &mut self.label {
            let avail = Size::new(
                (size.width - 2.0 * PAD - box_sz - label_gap).max(0.0),
                content_h,
            );
            let label_size = ctx.compute_size(label, SizeDef::fit(avail), avail.into());
            ctx.run_layout(label, label_size);
            let label_x = PAD + box_sz + label_gap;
            let label_y = PAD + ((content_h - label_size.height) * 0.5).max(0.0);
            ctx.place_child(label, Point::new(label_x, label_y));
        }
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
        let box_sz = self.box_size();

        let content_h = (size.height - 2.0 * PAD).max(0.0);
        let box_x = PAD;
        let box_y = PAD + ((content_h - box_sz) * 0.5).max(0.0);

        let box_radius = f64::from(self.theme.radius.tiny);
        let box_rect = RoundedRect::from_origin_size(
            Point::new(box_x, box_y),
            Size::new(box_sz, box_sz),
            box_radius,
        );

        let (bg, border) = self.resolve_box_colors(hovered, pressed);
        if bg.components[3] > 0.0 {
            painter.fill(box_rect, bg).draw();
        }
        painter
            .stroke(box_rect, &Stroke::new(BOX_BORDER), border)
            .draw();

        if focused && !self.disabled {
            let inset = FOCUS_RING_INSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(box_x - inset, box_y - inset),
                Size::new(box_sz + 2.0 * inset, box_sz + 2.0 * inset),
                box_radius + inset,
            );
            paint_focus_ring(painter, focus_rect, &self.theme);
        }

        // Check icon is a self-painting Label child placed at the box position during layout.
    }

    fn accessibility_role(&self) -> Role {
        Role::CheckBox
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
        node.set_toggled(if self.checked {
            Toggled::True
        } else {
            Toggled::False
        });
    }

    fn children_ids(&self) -> ChildrenIds {
        if let Some(label) = &self.label {
            let ids = [self.check_icon.id(), label.id()];
            ChildrenIds::from_slice(&ids)
        } else {
            ChildrenIds::from_slice(&[self.check_icon.id()])
        }
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

    use super::CheckboxWidget;
    use crate::Theme;
    use crate::components::checkbox::CheckboxPress;

    fn harness() -> TestHarness<CheckboxWidget> {
        let widget = CheckboxWidget::new(&Theme::dark(), false, false);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (60, 30))
    }

    #[test]
    fn pointer_click_submits_press() {
        let mut h = harness();
        h.mouse_move(Point::new(30.0, 15.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<CheckboxPress>().is_some());
    }

    #[test]
    fn drag_out_cancels_the_press() {
        let mut h = harness();
        h.mouse_move(Point::new(30.0, 15.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_move(Point::new(300.0, 300.0));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<CheckboxPress>().is_none());
    }

    #[test]
    fn space_and_enter_activate_when_focused() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.pop_action::<CheckboxPress>().is_some());

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(
            h.pop_action::<CheckboxPress>().is_some(),
            "checkboxes accept Enter as well as Space"
        );
    }

    #[test]
    fn space_key_down_shows_the_pressed_fill_until_key_up() {
        // Regression: on_text_event used to only ever fire on key-up, so
        // Space/Enter "clicking" showed no pressed-fill feedback the way a
        // pointer click does (pointer-down captures the pointer; keyboard
        // activation never does).
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(
            h.pop_action::<CheckboxPress>().is_none(),
            "not yet activated"
        );

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(h.pop_action::<CheckboxPress>().is_some());
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

#[cfg(test)]
mod density_tests {
    use super::*;
    use crate::theme::Density;

    #[test]
    fn label_gap_scales_with_density() {
        let gap = |d: Density| {
            CheckboxWidget::new(&Theme::dark().with_density(d), false, false).label_gap()
        };
        assert!((gap(Density::balanced()) - 6.0).abs() < 1e-6); // pre-token LABEL_GAP
        assert!(gap(Density::compact()) < gap(Density::balanced()));
        assert!(gap(Density::balanced()) < gap(Density::airy()));
    }
}
