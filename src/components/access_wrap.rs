//! Shared "transparent single-child view/widget that exists only to attach
//! one piece of accesskit state" plumbing.
//!
//! Pure-composition views (`flex_row`, `label`, ...) have no way to override
//! their own accessibility node. A handful of call sites need exactly that:
//! breadcrumb's navigation landmark and current-page marker, and decorative
//! icons that should be hidden from assistive tech entirely. [`annotate`]
//! wraps any `WidgetView` in a minimal masonry widget — layout/paint fully
//! delegate to the child, mirroring `xilem_masonry`'s own `sized_box` — that
//! applies one [`AccessAnnotation`] to the accesskit node.

use masonry::accesskit::{AriaCurrent, Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// One piece of accesskit state [`annotate`] can attach to an otherwise-plain
/// child that has no accessibility knob of its own to reach for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccessAnnotation {
    /// Report `Role::Navigation` — a navigation landmark (e.g. breadcrumb).
    Navigation,
    /// Mark as `AriaCurrent::Page` — the current-location segment of a
    /// breadcrumb or similar trail.
    CurrentPage,
    /// Hide entirely from assistive tech — a purely decorative glyph with no
    /// semantic content of its own (e.g. a chevron used only as a visual
    /// separator).
    Hidden,
}

/// Wrap `inner` in a transparent widget that applies `annotation` to the
/// accesskit node the child has no way to set itself.
pub(crate) fn annotate<State, Action, V>(
    inner: V,
    annotation: AccessAnnotation,
) -> impl WidgetView<State, Action> + use<State, Action, V>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    AccessAnnotateView { inner, annotation }
}

struct AccessAnnotateView<V> {
    inner: V,
    annotation: AccessAnnotation,
}

impl<V> ViewMarker for AccessAnnotateView<V> {}

impl<State, Action, V> View<State, Action, ViewCtx> for AccessAnnotateView<V>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<AccessAnnotateWidget>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.inner.build(ctx, state);
        let widget = AccessAnnotateWidget::new(child.new_widget, self.annotation);
        (ctx.create_pod(widget), child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        if self.annotation != prev.annotation {
            AccessAnnotateWidget::set_annotation(&mut element, self.annotation);
        }
        let mut child = AccessAnnotateWidget::child_mut(&mut element);
        self.inner
            .rebuild(&prev.inner, view_state, ctx, child.downcast(), state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = AccessAnnotateWidget::child_mut(&mut element);
        self.inner.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        let mut child = AccessAnnotateWidget::child_mut(&mut element);
        self.inner
            .message(view_state, message, child.downcast(), state)
    }
}

pub(crate) struct AccessAnnotateWidget {
    child: WidgetPod<dyn Widget>,
    annotation: AccessAnnotation,
}

impl AccessAnnotateWidget {
    fn new(child: NewWidget<impl Widget + ?Sized>, annotation: AccessAnnotation) -> Self {
        Self {
            child: child.erased().to_pod(),
            annotation,
        }
    }

    fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    fn set_annotation(this: &mut WidgetMut<'_, Self>, annotation: AccessAnnotation) {
        if this.widget.annotation != annotation {
            this.widget.annotation = annotation;
            this.ctx.request_accessibility_update();
        }
    }
}

impl Widget for AccessAnnotateWidget {
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
        match self.annotation {
            AccessAnnotation::Navigation => Role::Navigation,
            AccessAnnotation::CurrentPage | AccessAnnotation::Hidden => Role::GenericContainer,
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        match self.annotation {
            AccessAnnotation::Navigation => {}
            AccessAnnotation::CurrentPage => node.set_aria_current(AriaCurrent::Page),
            AccessAnnotation::Hidden => node.set_hidden(),
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::widgets::SizedBox;

    use super::*;

    fn widget(annotation: AccessAnnotation) -> AccessAnnotateWidget {
        AccessAnnotateWidget::new(NewWidget::new(SizedBox::empty()), annotation)
    }

    #[test]
    fn navigation_reports_navigation_role() {
        assert_eq!(
            widget(AccessAnnotation::Navigation).accessibility_role(),
            Role::Navigation
        );
    }

    #[test]
    fn current_page_and_hidden_report_generic_container_role() {
        assert_eq!(
            widget(AccessAnnotation::CurrentPage).accessibility_role(),
            Role::GenericContainer
        );
        assert_eq!(
            widget(AccessAnnotation::Hidden).accessibility_role(),
            Role::GenericContainer
        );
    }
}
