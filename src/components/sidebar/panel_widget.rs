//! Masonry widget for the animated sidebar panel.
//!
//! [`ThemedSidebarPanel`] wraps any child widget and animates its own width
//! between 0 (collapsed) and the child's natural width (expanded). The child
//! is always laid out at its natural width; a clip path restricts what is
//! visible during the slide animation so adjacent content reflowes smoothly.

use std::any::TypeId;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length};

use crate::Theme;

/// Duration of the collapse/expand slide animation.
const SLIDE_MILLIS: f32 = 250.0;

/// Container widget that slides its content off-screen when collapsed.
///
/// The `collapsed` flag is host-controlled. When it changes, the widget
/// animates `collapse_progress` from 0.0 (fully visible) toward 1.0 (fully
/// hidden) or vice versa over [`SLIDE_MILLIS`] milliseconds. Width reported to
/// the parent tracks `natural_width * (1 − collapse_progress)` so siblings
/// fill the freed space during the animation.
pub struct ThemedSidebarPanel<W: Widget + ?Sized> {
    child: WidgetPod<W>,
    #[allow(dead_code)]
    theme: Theme,
    /// Host-controlled target: `true` → animate toward hidden.
    collapsed: bool,
    /// Animation progress: 0.0 = fully visible, 1.0 = fully hidden.
    collapse_progress: f32,
    /// Child's natural (unexpanded) width from the most recent measure pass.
    natural_width: f64,
}

// --- MARK: BUILDERS
impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    #[must_use]
    pub fn new(child: NewWidget<W>, theme: &Theme) -> Self {
        Self {
            child: child.to_pod(),
            theme: *theme,
            collapsed: false,
            collapse_progress: 0.0,
            natural_width: 0.0,
        }
    }

    #[must_use]
    pub fn with_collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        if collapsed {
            self.collapse_progress = 1.0;
        }
        self
    }
}

// --- MARK: WIDGETMUT
impl<W: Widget + FromDynWidget> ThemedSidebarPanel<W> {
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    pub fn set_collapsed(this: &mut WidgetMut<'_, Self>, collapsed: bool) {
        if this.widget.collapsed != collapsed {
            this.widget.collapsed = collapsed;
            let target: f32 = if collapsed { 1.0 } else { 0.0 };
            if (target - this.widget.collapse_progress).abs() > 1e-4 {
                this.ctx.request_anim_frame();
            }
        }
    }
}

// --- MARK: IMPL WIDGET
impl<W: Widget + ?Sized> Widget for ThemedSidebarPanel<W> {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let target: f32 = if self.collapsed { 1.0 } else { 0.0 };
        let delta = (interval as f32 / 1_000_000.0) / SLIDE_MILLIS;
        let diff = target - self.collapse_progress;
        if diff.abs() > 1e-4 {
            self.collapse_progress = if diff > 0.0 {
                (self.collapse_progress + delta).min(target)
            } else {
                (self.collapse_progress - delta).max(target)
            };
            ctx.request_layout();
            if (target - self.collapse_progress).abs() > 1e-4 {
                ctx.request_anim_frame();
            }
        }
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        let child_length =
            ctx.compute_length(&mut self.child, len_req.into(), context_size, axis, cross_length);
        if axis == Axis::Horizontal {
            let natural = child_length.get();
            if natural > 0.0 {
                self.natural_width = natural;
            }
            Length::px((natural * (1.0 - f64::from(self.collapse_progress))).max(0.0))
        } else {
            child_length
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // Always lay out the child at full natural width so content doesn't
        // reflow during the slide animation. The clip path set below
        // restricts what is actually visible to the current animated width.
        let child_size = Size::new(self.natural_width.max(size.width), size.height);
        ctx.run_layout(&mut self.child, child_size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.set_clip_path(size.to_rect());
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

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
