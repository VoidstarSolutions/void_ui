//! Masonry widget for the clipboard icon button.
//!
//! A square button that paints a copy icon at rest and a checkmark after being
//! clicked. The checkmark reverts automatically after [`COPIED_DURATION`]
//! seconds, driven by `request_anim_frame` — no timer thread required.
//!
//! Emits [`ClipboardPress`] on primary-pointer release inside the widget and
//! on Space / Enter while focused.

use std::sync::LazyLock;

use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, BezPath, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

use crate::Theme;

const CORNER_RADIUS: f64 = 5.0;
const FOCUS_RING_WIDTH: f64 = 1.5;
const FOCUS_RING_INSET: f64 = 2.0;
/// Seconds the checkmark is shown before reverting to the copy icon.
const COPIED_DURATION: f64 = 1.5;

/// Action emitted when the user activates the clipboard button.
#[derive(Debug)]
pub struct ClipboardPress;

/// Two overlapping squares (back-left, front-right) in unit-square space.
static COPY_PATH: LazyLock<BezPath> = LazyLock::new(|| {
    let mut p = BezPath::new();
    p.move_to((0.05, 0.30));
    p.line_to((0.05, 0.95));
    p.line_to((0.70, 0.95));
    p.line_to((0.70, 0.30));
    p.close_path();
    p.move_to((0.30, 0.05));
    p.line_to((0.95, 0.05));
    p.line_to((0.95, 0.70));
    p.line_to((0.30, 0.70));
    p.close_path();
    p
});

/// Checkmark in unit-square space.
static CHECK_PATH: LazyLock<BezPath> = LazyLock::new(|| {
    let mut p = BezPath::new();
    p.move_to((0.10, 0.50));
    p.line_to((0.40, 0.80));
    p.line_to((0.90, 0.20));
    p
});

/// Clipboard icon-button widget.
///
/// Paints a copy icon at rest; switches to a checkmark for
/// [`COPIED_DURATION`] seconds after activation, then reverts.
pub struct ClipboardWidget {
    theme: Theme,
    copied: bool,
    copied_t: f64,
}

// --- MARK: BUILDERS
impl ClipboardWidget {
    #[must_use]
    pub fn new(theme: &Theme) -> Self {
        Self {
            theme: *theme,
            copied: false,
            copied_t: 0.0,
        }
    }
}

// --- MARK: WIDGETMUT
impl ClipboardWidget {
    /// Replaces the theme. Requests a repaint on change.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_paint_only();
        }
    }

    /// Transitions into (or out of) the copied-feedback state.
    ///
    /// Setting `true` resets the elapsed timer to zero and arms the animation
    /// loop. Setting `false` cancels an in-progress countdown immediately.
    pub fn set_copied(this: &mut WidgetMut<'_, Self>, copied: bool) {
        if this.widget.copied != copied {
            this.widget.copied = copied;
            this.widget.copied_t = 0.0;
            if copied {
                this.ctx.request_anim_frame();
            }
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ClipboardWidget {
    type Action = ClipboardPress;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
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
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<Self::Action>(ClipboardPress);
                }
                ctx.request_paint_only();
            }
            _ => (),
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.submit_action::<Self::Action>(ClipboardPress);
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == accesskit::Action::Click {
            ctx.submit_action::<Self::Action>(ClipboardPress);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        if self.copied {
            self.copied_t += interval as f64 * 1e-9;
            if self.copied_t >= COPIED_DURATION {
                self.copied = false;
                self.copied_t = 0.0;
            } else {
                ctx.request_anim_frame();
            }
            ctx.request_paint_only();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let icon_size = f64::from(self.theme.density.ui_font_size);
        Length::px(match axis {
            Axis::Horizontal => 2.0 * pad_h + icon_size,
            Axis::Vertical => 2.0 * pad_v + icon_size,
        })
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

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

        let bg = if pressed {
            p.surface_hi
        } else if hovered {
            p.surface_2
        } else {
            p.surface
        };

        let rect = RoundedRect::from_origin_size(Point::ORIGIN, size, CORNER_RADIUS);
        painter.fill(rect, bg).draw();

        if focused {
            let inset = FOCUS_RING_INSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                (CORNER_RADIUS - inset).max(0.0),
            );
            painter
                .stroke(focus_rect, &Stroke::new(FOCUS_RING_WIDTH), p.teal)
                .draw();
        }

        let icon_size = f64::from(self.theme.density.ui_font_size);
        let icon_x = (size.width - icon_size) * 0.5;
        let icon_y = (size.height - icon_size) * 0.5;
        let transform = Affine::translate((icon_x, icon_y)) * Affine::scale(icon_size);

        let (icon, color): (&BezPath, Color) = if self.copied {
            (&*CHECK_PATH, p.teal)
        } else if hovered || pressed {
            (&*COPY_PATH, p.text)
        } else {
            (&*COPY_PATH, p.text_muted)
        };

        painter
            .stroke(transform * icon, &Stroke::new(1.5), color)
            .draw();
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
        node.set_label(if self.copied { "Copied" } else { "Copy" });
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
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
