//! `OverlaySurface` — transparent wrapper that paints rounded
//! background/border chrome around arbitrary overlay content.
//! [`crate::anchored_overlay::AnchoredOverlay`] and
//! [`crate::overlay_portal::PortalSlot`] are purely structural — they don't
//! paint chrome — so whatever they host must paint its own (mirrors
//! `MenuContent`, which does the same for dropdown menus).

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::Padding;

use crate::Theme;

/// Border width of the overlay surface's chrome — hairline chrome, not density-scaled.
const BORDER_WIDTH: f64 = 1.0;

/// Which corner radius an [`OverlaySurface`] paints, per
/// [`crate::theme::Radii`]'s documented usage: `small` for "cards, pills,
/// buttons" (popovers, dropdown menus), `large` for "large surfaces, dialogs".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceStyle {
    Popover,
    Dialog,
}

impl SurfaceStyle {
    fn corner_radius(self, theme: &Theme) -> f64 {
        match self {
            Self::Popover => f64::from(theme.radius.small),
            Self::Dialog => f64::from(theme.radius.large),
        }
    }
}

/// Transparent wrapper that paints rounded background/border chrome around
/// arbitrary overlay content, used by both `AnchoredOverlay` and
/// `PortalSlot` hosts.
pub(crate) struct OverlaySurface {
    content: WidgetPod<dyn Widget>,
    bg: Color,
    border: Color,
    pad: f32,
    style: SurfaceStyle,
    corner_radius: f64,
}

impl OverlaySurface {
    pub(crate) fn new(content: NewWidget<dyn Widget>, theme: &Theme, style: SurfaceStyle) -> Self {
        Self {
            content: content.to_pod(),
            bg: theme.palette.surface_hi,
            border: theme.palette.border_strong,
            pad: theme.density.pad,
            style,
            corner_radius: style.corner_radius(theme),
        }
    }

    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        let bg = theme.palette.surface_hi;
        let border = theme.palette.border_strong;
        let corner_radius = this.widget.style.corner_radius(theme);
        if this.widget.bg != bg || this.widget.border != border {
            this.widget.bg = bg;
            this.widget.border = border;
            this.ctx.request_paint_only();
        }
        if (this.widget.corner_radius - corner_radius).abs() > f64::EPSILON {
            this.widget.corner_radius = corner_radius;
            this.ctx.request_paint_only();
        }
        if (this.widget.pad - theme.density.pad).abs() > f32::EPSILON {
            this.widget.pad = theme.density.pad;
            let pad = Padding::all(Length::px(f64::from(theme.density.pad)));
            Self::content_mut(this).insert_prop(pad);
        }
    }

    /// Mutable access to the wrapped content for the view layer.
    pub(crate) fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.content)
    }
}

impl Widget for OverlaySurface {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.content);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.content, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.content, size);
        ctx.place_child(&mut self.content, Point::ORIGIN);
        ctx.derive_baselines(&self.content);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let rrect =
            RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box().size(), self.corner_radius);
        if self.bg.components[3] > 0.0 {
            painter.fill(rrect, self.bg).draw();
        }
        if self.border.components[3] > 0.0 {
            painter
                .stroke(rrect, &Stroke::new(BORDER_WIDTH), self.border)
                .draw();
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

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
        ChildrenIds::from_slice(&[self.content.id()])
    }
}
