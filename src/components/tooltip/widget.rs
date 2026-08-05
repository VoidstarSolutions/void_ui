//! Masonry widget that hosts a child widget and shows a tooltip popup after
//! the pointer has been idle over it for a configurable delay.
//!
//! Built on the same `overlay_scope`/`overlay_portal` mechanism `popover` and
//! `dialog` use — required (no in-tree fallback), since masonry's window
//! `Layer` can't route `View::message` to arbitrary content (see
//! `docs/superpowers/specs/2026-07-21-tooltip-arbitrary-content-design.md`).
//! `binding.open_at_point`/`close` push visibility to the scope's portal
//! slot; the popup content itself is registered separately by the view layer
//! (`super::view::TooltipView`) and lives entirely in that slot.

use std::time::Duration;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size, Vec2};
use masonry::layout::{LenReq, Length};
use masonry::util::Instant;

use crate::overlay::binding::{PortalBinding, PortalCtx};
use crate::overlay_scope::OverlayScopeHandle;

/// Offset of the tooltip popup from the cursor: slightly right, well below
/// the typical button-press hand-shape so the content is readable.
const CURSOR_OFFSET: Vec2 = Vec2::new(12.0, 20.0);

/// Hosts a child widget and shows a portal-mounted tooltip popup on
/// hover-idle (or keyboard-focus-idle).
///
/// Tracks the most recent pointer-move time in `last_pointer_move` and the
/// last-known anchor point (cursor position, or the child's bottom-left
/// corner for keyboard focus) in `last_cursor_pos_window`. While
/// `last_pointer_move` is `Some`, the widget polls via `request_anim_frame`
/// until `delay` has elapsed, then opens the popup at
/// `last_cursor_pos_window + CURSOR_OFFSET` via `binding.open_at_point`.
/// `visible` mirrors whether the popup is currently shown, kept in sync by
/// `tooltip_dismiss_hook` when an outside press dismisses it.
pub struct TooltipHost {
    child: WidgetPod<dyn Widget>,
    binding: PortalBinding,
    delay: Duration,
    last_pointer_move: Option<Instant>,
    last_cursor_pos_window: Point,
    visible: bool,
}

// --- MARK: BUILDERS
impl TooltipHost {
    /// Creates a new tooltip host wrapping `child`, whose popup content is
    /// already registered under `key` in `scope`'s portal.
    #[must_use]
    pub(crate) fn new(
        child: NewWidget<impl Widget + ?Sized>,
        scope: OverlayScopeHandle,
        key: u64,
        delay: Duration,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            binding: PortalBinding::new(scope, key, tooltip_dismiss_hook),
            delay,
            last_pointer_move: None,
            last_cursor_pos_window: Point::ZERO,
            visible: false,
        }
    }
}

// --- MARK: WIDGETMUT
impl TooltipHost {
    /// Replaces the hover-idle delay before the tooltip appears.
    pub fn set_delay(this: &mut WidgetMut<'_, Self>, delay: Duration) {
        this.widget.delay = delay;
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Sync `visible` after the portal slot dismissed the popup (outside
    /// press). Mirrors `PopoverHost::mark_closed`; unlike popover, there's no
    /// action to submit — a tooltip's visibility is purely internal.
    pub(crate) fn mark_dismissed(this: &mut WidgetMut<'_, Self>) {
        this.widget.visible = false;
    }
}

/// Dismiss hook registered with the portal slot (see
/// [`crate::overlay_portal::DismissHook`]): syncs `visible` after an
/// outside-press dismissal via [`TooltipHost::mark_dismissed`].
pub(crate) fn tooltip_dismiss_hook(mut w: WidgetMut<'_, dyn Widget>) {
    let mut host = w.downcast::<TooltipHost>();
    TooltipHost::mark_dismissed(&mut host);
}

// --- MARK: INTERNAL HELPERS
impl TooltipHost {
    /// Show the popup at the current anchor point, if not already visible.
    fn show(&mut self, ctx: &mut UpdateCtx<'_>) {
        let point = self.last_cursor_pos_window + CURSOR_OFFSET;
        self.binding.open_at_point(ctx, point);
        self.visible = true;
    }

    /// Hide the popup, if currently visible. No-op otherwise, so callers can
    /// call this unconditionally on every hover/focus-loss signal.
    fn hide(&mut self, ctx: &mut impl PortalCtx) {
        if self.visible {
            self.binding.close(ctx);
            self.visible = false;
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for TooltipHost {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Move(PointerUpdate { current, .. }) = event {
            self.last_cursor_pos_window = current.logical_point();
            // Any pointer activity dismisses an already-shown tooltip,
            // including a jiggle while still hovering the same child.
            self.hide(ctx);
            self.last_pointer_move = Some(Instant::now());
            ctx.request_anim_frame();
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _interval: u64,
    ) {
        let Some(last) = self.last_pointer_move else {
            return;
        };
        if Instant::now().duration_since(last) >= self.delay {
            if !self.visible {
                self.show(ctx);
            }
            self.last_pointer_move = None;
        } else {
            ctx.request_anim_frame();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            // Hover/focus loss disarms the timer and hides an already-shown
            // popup. `ChildHoveredChanged(false)` fires when an *interactive*
            // child (a button) — not TooltipHost — was the directly-hovered
            // widget; `HoveredChanged(false)` covers the host's own hover
            // loss, which is what fires for a non-interactive child (a plain
            // label or icon) that never becomes the hovered widget itself;
            // `ChildFocusChanged(false)` is the keyboard-focus equivalent.
            Update::ChildHoveredChanged(false)
            | Update::HoveredChanged(false)
            | Update::ChildFocusChanged(false) => {
                self.last_pointer_move = None;
                self.hide(ctx);
            }
            // Keyboard users never produce pointer events, so focus is the
            // equivalent "arm the timer" signal: anchor the tooltip at the
            // child's bottom-left corner and start the same idle countdown
            // used for hover.
            Update::ChildFocusChanged(true) => {
                let rect = ctx.border_box();
                self.last_cursor_pos_window = ctx.to_window(Point::new(rect.x0, rect.y1));
                self.hide(ctx);
                self.last_pointer_move = Some(Instant::now());
                ctx.request_anim_frame();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

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

// --- MARK: TESTS

#[cfg(test)]
mod tests {
    use masonry::core::{NewWidget, WidgetId};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;
    use xilem::view::PointerButton;

    use super::*;
    use crate::Theme;
    use crate::components::button::widget::ThemedButton;
    use crate::overlay_portal::PortalPlacement;
    use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

    /// Builds an `OverlayScope` whose content is a `TooltipHost` wrapping
    /// `child`, with a `Label("Tip text")` popup registered under `key = 5`
    /// in the scope's portal — mirrors [`crate::components::context_menu::area::tests::portal_scope_harness`].
    /// Returns the harness, the popup's portal key, and the host's own
    /// widget id (needed for `mouse_move_to`/`focus_on` targets now that the
    /// scope, not the host, is the harness root).
    fn tooltip_scope_harness(
        delay: Duration,
        child: NewWidget<dyn Widget>,
    ) -> (TestHarness<OverlayScope>, u64, WidgetId) {
        let key = 5;
        let scope_handle = OverlayScopeHandle::new();
        let host = NewWidget::new(TooltipHost::new(child, scope_handle.clone(), key, delay));
        let host_id = host.id();
        let content = host.erased();
        let popup = Label::new("Tip text").prepare().erased();
        let scope = OverlayScope::new(
            scope_handle,
            content,
            vec![(key, popup, PortalPlacement::Trigger)],
        );
        let harness = TestHarness::create(default_property_set(), NewWidget::new(scope));
        (harness, key, host_id)
    }

    fn with_host<R>(
        h: &mut TestHarness<OverlayScope>,
        f: impl FnOnce(&mut WidgetMut<'_, TooltipHost>) -> R,
    ) -> R {
        h.edit_root_widget(|mut wm| {
            let mut content = OverlayScope::content_mut(&mut wm);
            let mut host = content.downcast::<TooltipHost>();
            f(&mut host)
        })
    }

    #[test]
    fn child_focus_gain_shows_after_delay() {
        let theme = Theme::dark();
        let child = NewWidget::new(ThemedButton::new(
            NewWidget::new(Label::new("Hover me")).erased(),
            &theme,
        ))
        .erased();
        let child_id = child.id();
        let (mut h, key, _host_id) = tooltip_scope_harness(Duration::ZERO, child);

        h.focus_on(Some(child_id));
        h.animate_ms(1);

        assert!(with_host(&mut h, |host| host.widget.visible));
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "the popup must actually be placed"
            );
        });
    }

    #[test]
    fn child_focus_loss_hides() {
        let theme = Theme::dark();
        let child = NewWidget::new(ThemedButton::new(
            NewWidget::new(Label::new("Hover me")).erased(),
            &theme,
        ))
        .erased();
        let child_id = child.id();
        let (mut h, _key, _host_id) = tooltip_scope_harness(Duration::ZERO, child);

        h.focus_on(Some(child_id));
        h.animate_ms(1);
        assert!(with_host(&mut h, |host| host.widget.visible));

        h.focus_on(None);
        assert!(!with_host(&mut h, |host| host.widget.visible));
    }

    #[test]
    fn hover_over_noninteractive_child_shows_after_delay() {
        let child = NewWidget::new(Label::new("plain")).erased();
        let (mut h, key, host_id) = tooltip_scope_harness(Duration::ZERO, child);

        h.mouse_move_to(host_id);
        h.animate_ms(1);

        assert!(
            with_host(&mut h, |host| host.widget.visible),
            "hovering a non-interactive child must show the tooltip"
        );
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(slot.widget.placed_rect(key).is_some());
        });
    }

    #[test]
    fn leaving_host_over_noninteractive_child_hides() {
        let child = NewWidget::new(Label::new("plain")).erased();
        let (mut h, key, host_id) = tooltip_scope_harness(Duration::ZERO, child);

        h.mouse_move_to(host_id);
        h.animate_ms(1);
        assert!(with_host(&mut h, |host| host.widget.visible));
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "precondition: popup is placed before leaving"
            );
        });

        h.mouse_move((10_000.0, 10_000.0));
        assert!(
            !with_host(&mut h, |host| host.widget.visible),
            "leaving the host must hide the tooltip"
        );
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_none(),
                "the popup must actually be hidden"
            );
        });
    }

    /// Regression test for the one case `TooltipHost`'s own Move-based
    /// dismissal can't catch: a click on the still-hovered child with no
    /// intervening pointer move. Masonry's window `Layer` used to dismiss
    /// this via its own global pointer-event capture; the portal-based
    /// replacement must get the same behavior from `dismiss_outside`'s
    /// outside-press mechanism instead, since the popup's synthetic
    /// cursor-anchor rect doesn't cover the host's own box.
    #[test]
    fn a_click_on_the_hovered_child_dismisses_via_the_scope() {
        let child = NewWidget::new(Label::new("plain")).erased();
        let (mut h, _key, host_id) = tooltip_scope_harness(Duration::ZERO, child);

        h.mouse_move_to(host_id);
        h.animate_ms(1);
        assert!(with_host(&mut h, |host| host.widget.visible));

        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        assert!(
            !with_host(&mut h, |host| host.widget.visible),
            "a click on the hovered child (no intervening move) must still dismiss the tooltip"
        );
    }
}
