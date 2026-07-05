//! Masonry widget for the separator component.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef, RegisterCtx,
    UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

use super::view::{Orientation, SeparatorStyle};

/// Length of the drawn segment in a dashed separator's pattern — dash-pattern
/// chrome, not density-scaled.
const DASH_ON: f64 = 4.0;
/// Length of the gap segment in a dashed separator's pattern — dash-pattern
/// chrome, not density-scaled.
const DASH_OFF: f64 = 2.0;

pub struct SeparatorWidget {
    pub(super) orientation: Orientation,
    pub(super) style: SeparatorStyle,
    pub(super) color: Color,
}

impl SeparatorWidget {
    pub(super) fn set_color(this: &mut WidgetMut<'_, Self>, color: Color) {
        if this.widget.color != color {
            this.widget.color = color;
            this.ctx.request_paint_only();
        }
    }

    pub(super) fn set_style(this: &mut WidgetMut<'_, Self>, style: SeparatorStyle) {
        if this.widget.style != style {
            this.widget.style = style;
            this.ctx.request_paint_only();
        }
    }

    pub(super) fn set_orientation(this: &mut WidgetMut<'_, Self>, orientation: Orientation) {
        if this.widget.orientation != orientation {
            this.widget.orientation = orientation;
            this.ctx.request_layout();
        }
    }
}

impl Widget for SeparatorWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let thin = match self.orientation {
            Orientation::Horizontal => axis == Axis::Vertical,
            Orientation::Vertical => axis == Axis::Horizontal,
        };
        if thin {
            Length::px(1.0)
        } else {
            // Fill available space in the main axis.
            match len_req {
                LenReq::FitContent(available) => available,
                _ => Length::px(0.0),
            }
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        match self.style {
            SeparatorStyle::Solid => {
                painter
                    .fill(Rect::from_origin_size(Point::ORIGIN, size), self.color)
                    .draw();
            }
            SeparatorStyle::Dashed => match self.orientation {
                Orientation::Horizontal => {
                    let mut x = 0.0_f64;
                    while x < size.width {
                        let w = DASH_ON.min(size.width - x);
                        painter
                            .fill(
                                Rect::from_origin_size(
                                    Point::new(x, 0.0),
                                    Size::new(w, size.height),
                                ),
                                self.color,
                            )
                            .draw();
                        x += DASH_ON + DASH_OFF;
                    }
                }
                Orientation::Vertical => {
                    let mut y = 0.0_f64;
                    while y < size.height {
                        let h = DASH_ON.min(size.height - y);
                        painter
                            .fill(
                                Rect::from_origin_size(
                                    Point::new(0.0, y),
                                    Size::new(size.width, h),
                                ),
                                self.color,
                            )
                            .draw();
                        y += DASH_ON + DASH_OFF;
                    }
                }
            },
        }
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
}
