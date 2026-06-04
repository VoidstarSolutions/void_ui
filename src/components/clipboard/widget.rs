//! Icon-only child widget for the clipboard button.
//!
//! Paints a copy icon at rest and a checkmark after being activated. The
//! checkmark reverts automatically after [`COPIED_DURATION`] seconds, driven
//! by `request_anim_frame` — no timer thread required.
//!
//! This widget is passive: it emits no actions and handles no events.
//! Interaction (pointer, keyboard, focus, background, focus ring) is owned by
//! the [`ThemedButton`] parent that wraps it.
//!
//! [`ThemedButton`]: crate::components::button::widget::ThemedButton

use std::sync::LazyLock;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, BezPath, Stroke};
use masonry::layout::{LenReq, Length};

use crate::Theme;

/// Seconds the checkmark is shown before reverting to the copy icon.
const COPIED_DURATION: f64 = 1.5;

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

/// Passive icon widget for the clipboard button.
///
/// Paints the copy or check icon; the parent `ThemedButton` owns all
/// interaction, background painting, and focus handling.
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
    /// loop — including when already copied, so repeated activations restart
    /// the countdown. Setting `false` cancels an in-progress countdown
    /// immediately.
    pub fn set_copied(this: &mut WidgetMut<'_, Self>, copied: bool) {
        if this.widget.copied != copied || copied {
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
    type Action = NoAction;

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        if self.copied {
            let interval_ns = u32::try_from(interval).unwrap_or(u32::MAX);
            self.copied_t += f64::from(interval_ns) * 1e-9;
            if self.copied_t >= COPIED_DURATION {
                self.copied = false;
                self.copied_t = 0.0;
            } else {
                ctx.request_anim_frame();
            }
            ctx.request_paint_only();
        }
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &masonry::core::Update,
    ) {
    }

    fn register_children(&mut self, _ctx: &mut masonry::core::RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(f64::from(self.theme.density.ui_font_size))
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &PropertiesRef<'_>,
        _size: masonry::kurbo::Size,
    ) {
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let p = &self.theme.palette;
        let (icon, color) = if self.copied {
            (&*CHECK_PATH, p.teal)
        } else {
            (&*COPY_PATH, p.text_muted)
        };

        let icon_size = f64::from(self.theme.density.ui_font_size);
        let stroke_width = icon_size / 10.0;
        let icon_x = (size.width - icon_size) * 0.5;
        let icon_y = (size.height - icon_size) * 0.5;
        let transform = Affine::translate((icon_x, icon_y)) * Affine::scale(icon_size);

        painter
            .stroke(transform * icon, &Stroke::new(stroke_width), color)
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
