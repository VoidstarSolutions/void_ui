//! Masonry widget owning the checkbox state machine.
//!
//! Paints the box, border, checkmark, and optional focus ring directly from
//! a `Theme` value. The optional text label is a masonry `Label` child widget
//! so text layout is handled by the framework.
//!
//! Emits [`CheckboxPress`] on primary-pointer release inside the widget and
//! on Space/Enter while focused.

use masonry::accesskit::{self, Node, Role, Toggled};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, BezPath, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;

use super::CheckboxPress;
use crate::Theme;

/// Corner radius of the checkbox box.
const BOX_RADIUS: f64 = 3.0;
/// Stroke width of the box border.
const BOX_BORDER: f64 = 1.0;
/// Focus-ring stroke width.
const FOCUS_RING_WIDTH: f64 = 1.5;
/// Gap between the box edge and the focus ring.
const FOCUS_RING_INSET: f64 = 1.5;
/// Gap between the box and an optional text label.
const LABEL_GAP: f64 = 6.0;
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
    /// Optional text label rendered to the right of the box.
    label: Option<WidgetPod<dyn Widget>>,
}

// --- MARK: BUILDERS
impl CheckboxWidget {
    /// Creates a new checkbox in the given state.
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
    pub fn with_label(mut self, label: NewWidget<impl Widget + ?Sized>) -> Self {
        self.label = Some(label.erased().to_pod());
        self
    }
}

// --- MARK: WIDGETMUT
impl CheckboxWidget {
    /// Sets the checked state. Requests a repaint and accessibility update on change.
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
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the theme. Requests layout + repaint on change.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
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

    /// Resolves `(background, border)` for the checkbox box in the current state.
    fn resolve_box_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (Color::TRANSPARENT, p.border);
        }
        if self.checked {
            let bg = if pressed || hovered { p.teal } else { p.teal_soft };
            (bg, p.teal)
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
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                // Fire only if the pointer was captured here and is still inside.
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<Self::Action>(CheckboxPress);
                }
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
        if self.disabled {
            return;
        }
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
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
        if event.action == accesskit::Action::Click {
            ctx.submit_action::<Self::Action>(CheckboxPress);
        }
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &Update,
    ) {
        match event {
            // Sync masonry's disabled flag on first attach (matches button pattern).
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
            }
            Update::HoveredChanged(_) | Update::DisabledChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
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
        let box_sz = self.box_size();

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
                Length::px(2.0 * PAD + box_sz + label_w)
            }
            Axis::Vertical => {
                let label_h = if let Some(label) = &mut self.label {
                    let avail_w = cross_length.map(|c| {
                        Length::px((c.get() - 2.0 * PAD - box_sz - LABEL_GAP).max(0.0))
                    });
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
        if let Some(label) = &mut self.label {
            let content_h = (size.height - 2.0 * PAD).max(0.0);
            let avail = Size::new(
                (size.width - 2.0 * PAD - box_sz - LABEL_GAP).max(0.0),
                content_h,
            );
            let label_size = ctx.compute_size(label, SizeDef::fit(avail), avail.into());
            ctx.run_layout(label, label_size);
            let label_x = PAD + box_sz + LABEL_GAP;
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
        let hovered = ctx.is_hovered();
        let pressed = ctx.is_active() && hovered;
        let focused = ctx.is_focus_target();
        let p = &self.theme.palette;
        let box_sz = self.box_size();

        let content_h = (size.height - 2.0 * PAD).max(0.0);
        let box_x = PAD;
        let box_y = PAD + ((content_h - box_sz) * 0.5).max(0.0);

        let box_rect = RoundedRect::from_origin_size(
            Point::new(box_x, box_y),
            Size::new(box_sz, box_sz),
            BOX_RADIUS,
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
                BOX_RADIUS + inset,
            );
            painter
                .stroke(focus_rect, &Stroke::new(FOCUS_RING_WIDTH), p.teal)
                .draw();
        }

        if self.checked {
            let check_color = if self.disabled { p.text_faint } else { p.teal };
            let m = box_sz * 0.18;
            let mut check = BezPath::new();
            check.move_to((box_x + m, box_y + box_sz * 0.52));
            check.line_to((box_x + box_sz * 0.40, box_y + box_sz * 0.78));
            check.line_to((box_x + box_sz - m, box_y + box_sz * 0.26));
            painter
                .stroke(&check, &Stroke::new(1.5), check_color)
                .draw();
        }
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
