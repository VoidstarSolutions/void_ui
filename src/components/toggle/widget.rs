//! Masonry widget for the toggle switch.
//!
//! Paints a pill-shaped track and a circular thumb directly from a `Theme`.
//! The optional text label is a masonry `Label` child widget. The `checked`
//! flag is host-controlled.
//!
//! Emits [`TogglePress`] on primary-pointer release inside the widget and
//! on Space/Enter while focused.

use masonry::accesskit::{self, Node, Role, Toggled};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Circle, Point, RoundedRect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::TogglePress;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::interaction::{self, InteractionState};
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};

/// Gap between the thumb edge and the track edge, in logical pixels.
const THUMB_INSET: f64 = 2.0;
/// Gap between the track edge and the focus ring.
/// Gap between the track and an optional text label.
const LABEL_GAP: f64 = 6.0;
/// Uniform padding around the widget — provides clearance for the focus ring.
const PAD: f64 = 2.0;

/// Interactive toggle switch widget.
pub struct ToggleWidget {
    checked: bool,
    disabled: bool,
    theme: Theme,
    /// Optional text label rendered to the right of the track.
    label: Option<WidgetPod<dyn Widget>>,
}

// --- MARK: BUILDERS
impl ToggleWidget {
    /// Creates a new toggle in the given state.
    #[must_use]
    pub fn new(theme: &Theme, checked: bool, disabled: bool) -> Self {
        Self {
            checked,
            disabled,
            theme: *theme,
            label: None,
        }
    }

    /// Attaches a text label child widget.
    #[must_use]
    pub fn with_label(mut self, label: NewWidget<Label>) -> Self {
        self.label = Some(label.erased().to_pod());
        self
    }
}

// --- MARK: SIZING
impl ToggleWidget {
    fn track_height(&self) -> f64 {
        (f64::from(self.theme.density.ui_font_size) * 4.0 / 3.0).round()
    }

    fn track_width(&self) -> f64 {
        (f64::from(self.theme.density.ui_font_size) * 3.0).round()
    }

    fn thumb_radius(&self) -> f64 {
        (self.track_height() / 2.0 - THUMB_INSET).max(1.0)
    }
}

// --- MARK: WIDGETMUT
impl ToggleWidget {
    fn text_color(disabled: bool, theme: &Theme) -> Color {
        if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text
        }
    }

    /// Sets the checked state. Requests repaint and accessibility update on change.
    pub fn set_checked(this: &mut WidgetMut<'_, Self>, checked: bool) {
        if this.widget.checked != checked {
            this.widget.checked = checked;
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets the disabled state. Syncs with masonry's event-routing flag.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            let color = Self::text_color(disabled, &this.widget.theme);
            if let Some(mut lbl) = Self::label_mut(this) {
                lbl.insert_prop(ContentColor::new(color));
            }
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the theme. Requests layout + repaint on change.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            let color = Self::text_color(this.widget.disabled, theme);
            if let Some(mut lbl) = Self::label_mut(this) {
                lbl.insert_prop(ContentColor::new(color));
                let mut typed = lbl.downcast::<Label>();
                Label::insert_style(
                    &mut typed,
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
    pub fn attach_label(this: &mut WidgetMut<'_, Self>, label: NewWidget<Label>) {
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
impl ToggleWidget {
    /// Resolves the track fill color for the current interaction state.
    fn resolve_track_fill(&self, hovered: bool, pressed: bool) -> Color {
        let p = &self.theme.palette;
        if self.disabled {
            return if self.checked {
                p.teal_soft
            } else {
                p.surface_2
            };
        }
        if self.checked {
            if pressed { p.teal_deep } else { p.teal }
        } else if pressed || hovered {
            p.surface_hi
        } else {
            p.surface_2
        }
    }

    fn resolve_thumb_color(&self) -> Color {
        if self.disabled {
            self.theme.palette.text_faint
        } else {
            // Near-white with a slight cool tint — readable on both teal and gray tracks.
            crate::theme::oklch(0.97, 0.005, 240.0)
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ToggleWidget {
    type Action = TogglePress;

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
                    ctx.submit_action::<Self::Action>(TogglePress);
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
        if interaction::keyboard_activate(event, true) {
            ctx.set_handled();
            ctx.submit_action::<Self::Action>(TogglePress);
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
            ctx.submit_action::<Self::Action>(TogglePress);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        interaction::interaction_update(ctx, event, self.disabled);
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
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
        let track_h = self.track_height();
        let track_w = self.track_width();

        match axis {
            Axis::Horizontal => {
                let label_w = if let Some(label) = &mut self.label {
                    let inner_cross =
                        cross_length.map(|c| Length::px((c.get() - 2.0 * PAD).max(0.0)));
                    let context = LayoutSize::maybe(Axis::Vertical, inner_cross);
                    let w = ctx.compute_length(label, len_req.into(), context, axis, inner_cross);
                    LABEL_GAP + w.get()
                } else {
                    0.0
                };
                Length::px(2.0 * PAD + track_w + label_w)
            }
            Axis::Vertical => {
                let label_h = if let Some(label) = &mut self.label {
                    let avail_w = cross_length
                        .map(|c| Length::px((c.get() - 2.0 * PAD - track_w - LABEL_GAP).max(0.0)));
                    let context = LayoutSize::maybe(Axis::Horizontal, avail_w);
                    let h = ctx.compute_length(label, len_req.into(), context, axis, avail_w);
                    h.get()
                } else {
                    0.0
                };
                Length::px(2.0 * PAD + track_h.max(label_h))
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let track_w = self.track_width();
        let content_h = (size.height - 2.0 * PAD).max(0.0);

        if let Some(label) = &mut self.label {
            let avail = Size::new(
                (size.width - 2.0 * PAD - track_w - LABEL_GAP).max(0.0),
                content_h,
            );
            let label_size = ctx.compute_size(label, SizeDef::fit(avail), avail.into());
            ctx.run_layout(label, label_size);
            let label_x = PAD + track_w + LABEL_GAP;
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

        let track_h = self.track_height();
        let track_w = self.track_width();
        let track_radius = track_h / 2.0;
        let thumb_r = self.thumb_radius();

        let content_h = (size.height - 2.0 * PAD).max(0.0);
        let track_x = PAD;
        let track_y = PAD + ((content_h - track_h) * 0.5).max(0.0);

        let track_rect = RoundedRect::from_origin_size(
            Point::new(track_x, track_y),
            Size::new(track_w, track_h),
            track_radius,
        );

        painter
            .fill(track_rect, self.resolve_track_fill(hovered, pressed))
            .draw();

        if focused && !self.disabled {
            let inset = FOCUS_RING_INSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(track_x - inset, track_y - inset),
                Size::new(track_w + 2.0 * inset, track_h + 2.0 * inset),
                track_radius + inset,
            );
            paint_focus_ring(painter, focus_rect, &self.theme);
        }

        let center_y = track_y + track_h / 2.0;
        let thumb_cx = if self.checked {
            track_x + track_w - THUMB_INSET - thumb_r
        } else {
            track_x + THUMB_INSET + thumb_r
        };
        painter
            .fill(
                Circle::new(Point::new(thumb_cx, center_y), thumb_r),
                self.resolve_thumb_color(),
            )
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::Switch
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
            ChildrenIds::from_slice(&[label.id()])
        } else {
            ChildrenIds::from_slice(&[])
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

    use crate::Theme;
    use crate::components::toggle::TogglePress;
    use crate::theme::Density;

    use super::ToggleWidget;

    fn widget(theme: &Theme, checked: bool, disabled: bool) -> ToggleWidget {
        ToggleWidget::new(theme, checked, disabled)
    }

    // --- sizing ---

    #[test]
    fn track_height_scales_with_font_size() {
        let compact = widget(
            &Theme::dark().with_density(Density::compact()),
            false,
            false,
        );
        let balanced = widget(
            &Theme::dark().with_density(Density::balanced()),
            false,
            false,
        );
        let airy = widget(&Theme::dark().with_density(Density::airy()), false, false);
        assert!(compact.track_height() < balanced.track_height());
        assert!(balanced.track_height() < airy.track_height());
    }

    #[test]
    fn track_width_is_wider_than_height() {
        let w = widget(&Theme::dark(), false, false);
        assert!(w.track_width() > w.track_height());
    }

    #[test]
    fn thumb_fits_inside_track() {
        let w = widget(&Theme::dark(), false, false);
        assert!(w.thumb_radius() * 2.0 < w.track_height());
    }

    #[test]
    fn thumb_radius_is_positive() {
        for density in [Density::compact(), Density::balanced(), Density::airy()] {
            let w = widget(&Theme::dark().with_density(density), false, false);
            assert!(w.thumb_radius() > 0.0);
        }
    }

    // --- track fill ---

    #[test]
    fn checked_track_differs_from_unchecked() {
        let on = widget(&Theme::dark(), true, false);
        let off = widget(&Theme::dark(), false, false);
        assert_ne!(
            on.resolve_track_fill(false, false),
            off.resolve_track_fill(false, false)
        );
    }

    #[test]
    fn pressed_checked_track_differs_from_resting() {
        let w = widget(&Theme::dark(), true, false);
        assert_ne!(
            w.resolve_track_fill(false, true),
            w.resolve_track_fill(false, false)
        );
    }

    #[test]
    fn hovered_unchecked_track_differs_from_resting() {
        let w = widget(&Theme::dark(), false, false);
        assert_ne!(
            w.resolve_track_fill(true, false),
            w.resolve_track_fill(false, false)
        );
    }

    #[test]
    fn disabled_ignores_hover_and_press() {
        let w = widget(&Theme::dark(), false, true);
        assert_eq!(
            w.resolve_track_fill(false, false),
            w.resolve_track_fill(true, true)
        );
    }

    #[test]
    fn disabled_checked_track_differs_from_disabled_unchecked() {
        let on = widget(&Theme::dark(), true, true);
        let off = widget(&Theme::dark(), false, true);
        assert_ne!(
            on.resolve_track_fill(false, false),
            off.resolve_track_fill(false, false)
        );
    }

    // --- thumb color ---

    #[test]
    fn disabled_thumb_differs_from_enabled() {
        let enabled = widget(&Theme::dark(), false, false);
        let disabled = widget(&Theme::dark(), false, true);
        assert_ne!(
            enabled.resolve_thumb_color(),
            disabled.resolve_thumb_color()
        );
    }

    #[test]
    fn enabled_thumb_color_is_same_regardless_of_checked_state() {
        let on = widget(&Theme::dark(), true, false);
        let off = widget(&Theme::dark(), false, false);
        assert_eq!(on.resolve_thumb_color(), off.resolve_thumb_color());
    }

    // --- activation ---

    fn harness() -> TestHarness<ToggleWidget> {
        let w = widget(&Theme::dark(), false, false);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(w), (80, 30))
    }

    #[test]
    fn pointer_click_submits_press() {
        let mut h = harness();
        h.mouse_move(Point::new(40.0, 15.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<TogglePress>().is_some());
    }

    #[test]
    fn drag_out_cancels_the_press() {
        let mut h = harness();
        h.mouse_move(Point::new(40.0, 15.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_move(Point::new(300.0, 300.0));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<TogglePress>().is_none());
    }

    #[test]
    fn space_and_enter_activate_when_focused() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.pop_action::<TogglePress>().is_some());

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.pop_action::<TogglePress>().is_some());
    }
}
