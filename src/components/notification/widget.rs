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
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
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
    /// Create a host wrapping `child`, arming a `timeout` countdown that
    /// elapses `timeout` after `armed_at`.
    ///
    /// `armed_at` is the toast's own creation/appearance time, owned by the
    /// host application (e.g. stored on its toast entry); see
    /// [`Self::set_timeout`].
    #[must_use]
    pub fn new(
        child: NewWidget<impl Widget + ?Sized>,
        timeout: Option<Duration>,
        armed_at: Option<Instant>,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            timeout,
            armed_at,
        }
    }

    /// Mutable access to the wrapped child for the [`View`](xilem::core::View) layer.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Update the auto-dismiss timeout and its starting instant.
    ///
    /// `flex_col`'s positional diffing can reuse this host for a different
    /// logical toast (e.g. when an earlier toast is dismissed and the list
    /// shifts), so `Update::WidgetAdded` won't fire again for the new toast.
    /// Rather than have the host guess whether it's been handed a new toast,
    /// `armed_at` is the toast's own creation time, supplied fresh by the
    /// view on every rebuild — overwriting it here is always correct,
    /// whether this host is continuing to serve the same toast or has been
    /// reused for a different one.
    pub fn set_timeout(
        this: &mut WidgetMut<'_, Self>,
        timeout: Option<Duration>,
        armed_at: Option<Instant>,
    ) {
        this.widget.timeout = timeout;
        this.widget.armed_at = armed_at;
        if timeout.is_some() {
            this.ctx.request_anim_frame();
        }
    }
}

impl Widget for NotificationHost {
    type Action = NotificationTimeout;

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::WidgetAdded = event
            && self.timeout.is_some()
        {
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

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::*;

    fn harness(
        timeout: Option<Duration>,
        armed_at: Option<Instant>,
    ) -> TestHarness<NotificationHost> {
        let child = NewWidget::new(Label::new("toast")).erased();
        let widget = NotificationHost::new(child, timeout, armed_at);
        TestHarness::create(default_property_set(), NewWidget::new(widget))
    }

    /// `flex_col`'s positional diffing reuses a host for a different logical
    /// toast when an earlier one is dismissed and the list shifts (see
    /// [`NotificationHost::set_timeout`]'s doc comment). `armed_at` is the
    /// new toast's own creation time, supplied by the view — `set_timeout`
    /// must adopt it unconditionally, even when the new toast happens to
    /// have the same configured `timeout` as the old one, otherwise the new
    /// toast inherits the old toast's stale `armed_at` and fires too early.
    #[test]
    fn reusing_host_for_new_toast_adopts_its_armed_at() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(10);
        let mut h = harness(Some(Duration::from_millis(50)), Some(t0));

        // toast1 slides into this slot, configured with the same 50ms
        // timeout as toast0 but created at t1 — `NotificationView::rebuild`
        // calls `set_timeout` exactly like this.
        h.edit_root_widget(|mut wm| {
            NotificationHost::set_timeout(&mut wm, Some(Duration::from_millis(50)), Some(t1));
        });

        let armed_at = h.edit_root_widget(|wm| wm.widget.armed_at);

        assert_eq!(
            armed_at,
            Some(t1),
            "reusing the host for a new toast must adopt the new toast's own \
             creation time, even when the new toast's timeout duration \
             matches the old one"
        );
    }

    /// The flip side of the previous test: when *this* toast is unchanged
    /// but a *different* toast elsewhere in the stack is added/removed, the
    /// whole `flex_col` rebuilds and `set_timeout` is called again for this
    /// host with the same `timeout` and `armed_at` it already had. Since
    /// `armed_at` is this toast's own creation time and hasn't changed, the
    /// countdown must not effectively restart — otherwise a toast can never
    /// expire as long as other toasts keep appearing/disappearing around it.
    #[test]
    fn unrelated_rebuild_with_unchanged_armed_at_does_not_reset_the_timer() {
        // Already past the 50ms timeout when the host is created, so the
        // very first anim frame fires unless `set_timeout` resets `armed_at`
        // to "now" (which would make it not-yet-elapsed and require a real
        // 50ms wait — exactly the bug this test guards against).
        let t0 = Instant::now()
            .checked_sub(Duration::from_millis(100))
            .unwrap();
        let mut h = harness(Some(Duration::from_millis(50)), Some(t0));

        // Same toast (same creation time), same timeout — as if a sibling
        // toast's dismissal triggered a stack-wide rebuild.
        h.edit_root_widget(|mut wm| {
            NotificationHost::set_timeout(&mut wm, Some(Duration::from_millis(50)), Some(t0));
        });

        h.animate_ms(1);

        assert_eq!(
            h.pop_action::<NotificationTimeout>().map(|(_, id)| id),
            Some(h.root_id()),
            "an unrelated rebuild must not restart this toast's countdown"
        );
    }
}
