//! `PopoverHost` — transparent trigger wrapper that opens a `PopoverLayer` on click.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetId,
    WidgetMut, WidgetPod,
};

/// Action emitted by [`PopoverHost`] when the popover is dismissed.
///
/// The [`PopoverView`](super::view::PopoverView) handles this to trigger a
/// `RequestRebuild` so `pending_content` is refreshed before the next open.
#[derive(Debug)]
pub struct PopoverClosed;
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};

use super::PopoverAnchor;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::popover_layer::{OnOutsideClick, PopoverLayer};

/// Transparent wrapper around a trigger child that opens a [`PopoverLayer`] on click.
///
/// Clicking the trigger toggles the popover.  Clicking outside the open
/// popover closes it via [`PopoverLayer::capture_pointer_event`].  Pressing
/// Escape while the trigger has focus also closes the popover.
pub struct PopoverHost {
    trigger: WidgetPod<dyn Widget>,
    /// Pre-built content widget, consumed when the popover opens.  Replaced
    /// by the view layer on each rebuild so the next open cycle shows fresh
    /// content.
    pending_content: Option<NewWidget<dyn Widget>>,
    pub(super) open: bool,
    pub(super) layer_id: Option<WidgetId>,
    /// Latched at pointer-Down so that the `PopoverLayer`'s `mutate_later`
    /// (which resets `open = false` between Down and Up) doesn't cause the
    /// toggle on Up to incorrectly re-open the popover.
    was_open_at_down: bool,
    pub(super) anchor: PopoverAnchor,
    pub(super) theme: Theme,
}

// --- MARK: BUILDERS
impl PopoverHost {
    #[must_use]
    pub fn new(
        trigger: NewWidget<impl Widget + ?Sized>,
        content: NewWidget<impl Widget + ?Sized>,
        anchor: PopoverAnchor,
        theme: &Theme,
    ) -> Self {
        Self {
            trigger: trigger.erased().to_pod(),
            pending_content: Some(content.erased()),
            open: false,
            layer_id: None,
            was_open_at_down: false,
            anchor,
            theme: *theme,
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl PopoverHost {
    /// Update the theme.  Has no effect on an already-open layer.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
        }
    }

    /// Replace the pending content widget.  If the popover is currently open
    /// the new widget is queued and used the next time the popover opens.
    pub fn set_pending_content(this: &mut WidgetMut<'_, Self>, content: NewWidget<dyn Widget>) {
        this.widget.pending_content = Some(content);
    }

    /// Change the anchor without closing the popover.  Takes effect on the
    /// next open.
    pub fn set_anchor(this: &mut WidgetMut<'_, Self>, anchor: PopoverAnchor) {
        this.widget.anchor = anchor;
    }

    /// Mutable access to the trigger child for the view layer.
    pub fn trigger_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.trigger)
    }
}

// --- MARK: OPEN / CLOSE
impl PopoverHost {
    fn open_popover(&mut self, ctx: &mut EventCtx<'_>) {
        let Some(content) = self.pending_content.take() else {
            return;
        };
        let creator_id = ctx.widget_id();
        let bb = ctx.border_box();
        let trigger_size = bb.size();
        let pos = ctx.to_window(bb.origin());
        let close_cb: OnOutsideClick = Arc::new(|mut w, layer_id| {
            let mut w = w.downcast::<PopoverHost>();
            w.widget.open = false;
            w.widget.layer_id = None;
            w.ctx.remove_layer(layer_id);
            // Notify the view layer so it calls rebuild() and restores pending_content.
            w.ctx.submit_action::<PopoverClosed>(PopoverClosed);
        });
        let layer = NewWidget::new(PopoverLayer::new(
            content,
            creator_id,
            self.theme.palette.surface_hi,
            self.theme.palette.border_strong,
            self.anchor,
            trigger_size,
            close_cb,
        ));
        let layer_id = layer.id();
        ctx.create_layer(LayerType::Other, layer, pos);
        self.layer_id = Some(layer_id);
        self.open = true;
    }

    fn close_popover(&mut self, ctx: &mut EventCtx<'_>) {
        if let Some(id) = self.layer_id.take() {
            ctx.remove_layer(id);
        }
        self.open = false;
        ctx.submit_action::<PopoverClosed>(PopoverClosed);
    }
}

// --- MARK: IMPL WIDGET
impl Widget for PopoverHost {
    type Action = PopoverClosed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match click::primary_click(ctx, event) {
            Some(ClickPhase::Down) => {
                self.was_open_at_down = self.open;
                ctx.request_focus();
            }
            Some(ClickPhase::Up(Some(_))) => {
                if self.was_open_at_down {
                    if self.open {
                        self.close_popover(ctx);
                    }
                } else {
                    self.open_popover(ctx);
                }
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &masonry::core::TextEvent,
    ) {
        use masonry::core::TextEvent;
        use masonry::core::keyboard::{Key, NamedKey};
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && event.key == Key::Named(NamedKey::Escape)
            && self.open
        {
            self.close_popover(ctx);
        }
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &Update,
    ) {
        if matches!(event, Update::WidgetAdded | Update::FocusChanged(_)) {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.trigger);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.trigger, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.trigger, size);
        ctx.place_child(&mut self.trigger, Point::ORIGIN);
        ctx.derive_baselines(&self.trigger);
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
        ChildrenIds::from_slice(&[self.trigger.id()])
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
