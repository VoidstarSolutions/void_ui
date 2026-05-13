//! Masonry widget that hosts a child widget and pops a tooltip
//! [`Layer`](masonry::core::Layer) after the pointer has been idle over
//! it for a configurable delay.
//!
//! Built on top of masonry's overlay infrastructure: [`EventCtx::create_attached_layer`]
//! creates a window-level layer anchored to the host, [`masonry::layers::Tooltip`]
//! is the layer widget itself (it dismisses itself on the next pointer activity
//! via `Layer::capture_pointer_event`), and the delay is a hand-rolled
//! `Instant`/`Duration` loop driven by `request_anim_frame()`.

use std::time::Duration;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size, Vec2};
use masonry::layers::Tooltip as TooltipLayer;
use masonry::layout::LenReq;
use masonry::properties::{Background, BorderColor, BorderWidth, ContentColor, Padding};
use masonry::util::Instant;
use masonry::widgets::Label;

use crate::Theme;

/// Offset of the tooltip layer from the cursor: slightly right, well below
/// the typical button-press hand-shape so the label is readable.
const CURSOR_OFFSET: Vec2 = Vec2::new(12.0, 20.0);
/// Border thickness on the tooltip surface.
const BORDER_WIDTH: f64 = 1.0;
/// Padding inside the tooltip surface around the label.
const PADDING: f64 = 6.0;

/// Hosts a child widget and creates a tooltip layer on hover-idle.
///
/// Tracks the most recent pointer-move time in `last_pointer_move` and the
/// cursor position in `last_cursor_pos`. While `last_pointer_move` is `Some`,
/// the widget polls via `request_anim_frame` until the configured `delay`
/// has elapsed, then materializes a [`masonry::layers::Tooltip`] layer at
/// the cursor position. The layer dismisses itself on the next pointer
/// activity (see [`masonry::layers::Tooltip::capture_pointer_event`]);
/// when the pointer leaves the host the timer is cleared so a new idle
/// period starts cleanly on re-entry.
pub struct TooltipHost {
    child: WidgetPod<dyn Widget>,
    text: ArcStr,
    theme: Theme,
    delay: Duration,
    last_pointer_move: Option<Instant>,
    last_cursor_pos: Point,
}

// --- MARK: BUILDERS
impl TooltipHost {
    /// Creates a new tooltip host wrapping `child`.
    #[must_use]
    pub fn new(
        child: NewWidget<impl Widget + ?Sized>,
        text: ArcStr,
        theme: &Theme,
        delay: Duration,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            text,
            theme: *theme,
            delay,
            last_pointer_move: None,
            last_cursor_pos: Point::ZERO,
        }
    }
}

// --- MARK: WIDGETMUT
impl TooltipHost {
    /// Replaces the theme used to style the tooltip surface.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
        }
    }

    /// Replaces the tooltip text shown on the layer.
    pub fn set_text(this: &mut WidgetMut<'_, Self>, text: ArcStr) {
        this.widget.text = text;
    }

    /// Replaces the hover-idle delay before the tooltip appears.
    pub fn set_delay(this: &mut WidgetMut<'_, Self>, delay: Duration) {
        this.widget.delay = delay;
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: LAYER BUILDER
impl TooltipHost {
    /// Builds the tooltip layer widget freshly each time it is shown.
    /// Properties are applied per-instance because the theme may have
    /// changed since the last presentation.
    fn build_layer(&self) -> NewWidget<TooltipLayer> {
        let mut label = Label::new(self.text.clone())
            .with_style(StyleProperty::FontSize(self.theme.typography.size_body))
            .prepare();
        label
            .properties
            .insert(ContentColor::new(self.theme.palette.text));

        let mut tooltip = NewWidget::new(TooltipLayer::new(label));
        tooltip.properties.insert(BorderWidth::all(BORDER_WIDTH));
        tooltip
            .properties
            .insert(BorderColor::new(self.theme.palette.border_strong));
        tooltip
            .properties
            .insert(Background::Color(self.theme.palette.surface_hi));
        tooltip.properties.insert(Padding::all(PADDING));
        tooltip
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
            self.last_cursor_pos = current.logical_point();
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
            // Only one tooltip layer per host: `create_attached_layer` keys
            // by `TypeId::of::<TooltipLayer>()` and auto-replaces the prior.
            let layer = self.build_layer();
            let pos = self.last_cursor_pos + CURSOR_OFFSET;
            ctx.create_attached_layer::<TooltipLayer>(
                LayerType::Tooltip(self.text.to_string()),
                layer,
                pos,
            );
            // The layer is up; do not request another anim frame. The next
            // `Move` (which masonry's `Tooltip` will use to dismiss itself)
            // will re-arm the loop via `on_pointer_event`.
        } else {
            ctx.request_anim_frame();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::HoveredChanged(false) = event {
            self.last_pointer_move = None;
            if let Some(layer_id) = ctx.get_attached_layer::<TooltipLayer>() {
                ctx.remove_layer(layer_id);
            }
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
        cross_length: Option<f64>,
    ) -> f64 {
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
