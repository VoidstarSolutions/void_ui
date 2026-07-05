//! Masonry widgets for the breadcrumb component.
//!
//! Both are transparent single-child pass-throughs — like masonry's own
//! `Passthrough`/`SizedBox`, they add no layout or paint of their own — that
//! exist solely to attach accesskit state pure composition (`flex_row`,
//! `label`) has no way to override: [`BreadcrumbNav`] reports
//! [`Role::Navigation`] for the whole trail, so it's announced as a
//! navigation landmark; [`BreadcrumbCurrent`] marks the trailing
//! current-location segment with `AriaCurrent::Page`, the native-a11y
//! equivalent of the web's `aria-current="page"`.

use masonry::accesskit::{AriaCurrent, Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};

pub(super) struct BreadcrumbNav {
    child: WidgetPod<dyn Widget>,
}

impl BreadcrumbNav {
    pub(super) fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }

    pub(super) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for BreadcrumbNav {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::Navigation
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Marks the wrapped segment as the breadcrumb's current location —
/// `AriaCurrent::Page` — since a plain `label()` has no accessibility state
/// to attach this to. Reports `Role::GenericContainer` itself (like
/// [`BreadcrumbNav`]); the actual text is exposed by the wrapped label.
pub(super) struct BreadcrumbCurrent {
    child: WidgetPod<dyn Widget>,
}

impl BreadcrumbCurrent {
    pub(super) fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }

    pub(super) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for BreadcrumbCurrent {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_aria_current(AriaCurrent::Page);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

#[cfg(test)]
mod tests {
    use masonry::widgets::SizedBox;

    use super::*;

    #[test]
    fn reports_navigation_role() {
        let widget = BreadcrumbNav::new(NewWidget::new(SizedBox::empty()));
        assert_eq!(widget.accessibility_role(), Role::Navigation);
    }

    #[test]
    fn current_reports_generic_container_role() {
        let widget = BreadcrumbCurrent::new(NewWidget::new(SizedBox::empty()));
        assert_eq!(widget.accessibility_role(), Role::GenericContainer);
    }
}
