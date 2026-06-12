//! Masonry widget that hosts a child widget and submits a
//! [`NotificationTimeout`] action once a configurable duration has elapsed.
//!
//! Pure passthrough wrapper — `measure`/`layout`/`paint` delegate entirely to
//! `child` — plus an `Instant`/`Duration` timer driven by
//! `request_anim_frame()`, modeled on
//! [`crate::components::tooltip::widget::TooltipHost`]'s hover-idle timer
//! (minus the `Layer` machinery, which `Notification` doesn't need).

use std::time::Duration;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenDef, LenReq, Length};
use masonry::util::Instant;

/// Action submitted by [`NotificationHost`] once its timeout elapses.
#[derive(Debug)]
pub struct NotificationTimeout;

/// Hosts a child widget and submits [`NotificationTimeout`] once `timeout`
/// has elapsed since the widget was added, if `timeout` is `Some`.
///
/// Self-disarms after firing — the host application is expected to remove
/// (tear down) this widget in response, so the timer never needs to fire
/// twice.
pub struct NotificationHost {
    child: WidgetPod<dyn Widget>,
    timeout: Option<Duration>,
    armed_at: Option<Instant>,
}

impl NotificationHost {
    /// Create a host wrapping `child`, arming a `timeout` countdown on
    /// mount if `Some`.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, timeout: Option<Duration>) -> Self {
        Self {
            child: child.erased().to_pod(),
            timeout,
            armed_at: None,
        }
    }

    /// Mutable access to the wrapped child for the [`View`](xilem::core::View) layer.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for NotificationHost {
    type Action = NotificationTimeout;

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::WidgetAdded = event
            && self.timeout.is_some()
        {
            self.armed_at = Some(Instant::now());
            ctx.request_anim_frame();
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _interval: u64,
    ) {
        let (Some(timeout), Some(armed)) = (self.timeout, self.armed_at) else {
            return;
        };
        if Instant::now().duration_since(armed) >= timeout {
            ctx.submit_action::<NotificationTimeout>(NotificationTimeout);
            self.armed_at = None;
        } else {
            ctx.request_anim_frame();
        }
    }

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
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Thin passthrough wrapper that reports its child's *intrinsic*
/// (`MinContent`) size to its parent, regardless of how much space the
/// parent offers.
///
/// `ZStack::layout` sizes each child via `compute_size(child,
/// SizeDef::fit(size), ...)`, i.e. a `FitContent(available_space)` request
/// per axis. `Flex`'s `FitContent` measurement is `space.max(content)` —
/// "fill the offered space, or be bigger if the content demands it" (see
/// `Flex::measure`). A toast stack (a `flex_col`) placed directly in a
/// `ZStack` therefore reports the *full window height* as its size, even
/// though it only has a couple of toasts in it. That leaves
/// `extra_height` near zero in `ZStack::layout`, so every [`UnitPoint`]
/// alignment resolves to roughly the same origin — toasts always pile up
/// at the top regardless of the chosen corner.
///
/// Wrapping the stack in [`NotificationOverlay`] fixes this: its `measure`
/// ignores the incoming `FitContent` request and instead asks the child to
/// measure under `MinContent`, which `Flex` honors as its true content
/// size. `ZStack` then sees the toast stack's real footprint, so
/// `extra_height` is meaningful and `.alignment(...)` places the stack in
/// the correct corner.
///
/// [`UnitPoint`]: masonry::layout::UnitPoint
pub struct NotificationOverlay {
    child: WidgetPod<dyn Widget>,
}

impl NotificationOverlay {
    /// Wrap `child`, reporting its intrinsic content size to the parent.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
        }
    }

    /// Mutable access to the wrapped child for the [`View`](xilem::core::View) layer.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for NotificationOverlay {
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
        let context_size = ctx.context_size();
        ctx.compute_length(
            &mut self.child,
            LenDef::MinContent,
            context_size,
            axis,
            cross_length,
        )
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
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
