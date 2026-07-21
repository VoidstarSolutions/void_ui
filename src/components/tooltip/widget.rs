//! Masonry widget that hosts a child widget and pops a tooltip
//! [`Layer`](masonry::core::Layer) after the pointer has been idle over
//! it for a configurable delay.
//!
//! Built on top of masonry's overlay infrastructure: [`EventCtx::create_layer`]
//! creates a window-level layer, [`masonry::layers::Tooltip`] is the layer
//! widget itself (it dismisses itself on the next pointer activity via
//! `Layer::capture_pointer_event`), and the delay is a hand-rolled
//! `Instant`/`Duration` loop driven by `request_anim_frame()`.
//!
//! We use `create_layer` rather than `create_attached_layer` because the
//! latter tracks the layer in an `attached_layers` map keyed by the host
//! widget. When `TooltipLayer` self-removes via `capture_pointer_event` it
//! does not clean up that map entry, so a subsequent `create_attached_layer`
//! call would emit a spurious `RemoveLayer` for the already-gone layer and
//! panic. Instead we track `layer_id` ourselves and clear it in
//! `on_pointer_event` (which fires after each `capture_pointer_event`).

use std::time::Duration;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size, Vec2};
use masonry::layers::Tooltip as TooltipLayer;
use masonry::layout::{LenReq, Length};
use masonry::properties::types::CrossAxisAlignment;
use masonry::properties::{
    Background, BorderColor, BorderWidth, ContentColor, CornerRadius, Gap, Padding,
};
use masonry::util::Instant;
use masonry::widgets::{Flex, Label, SizedBox};

use super::view::{TooltipContent, TooltipRow};
use crate::Theme;

/// Offset of the tooltip layer from the cursor: slightly right, well below
/// the typical button-press hand-shape so the label is readable.
const CURSOR_OFFSET: Vec2 = Vec2::new(12.0, 20.0);
/// Border thickness on the tooltip surface.
const BORDER_WIDTH: Length = Length::const_px(1.0);
/// Padding inside the tooltip surface around the label.
const PADDING: Length = Length::const_px(6.0);
/// Diameter of a rows-tooltip legend dot, matching the sidebar's own
/// status-dot size so the legend reads as the same visual language.
const ROW_DOT_SIZE: Length = Length::const_px(8.0);
/// Corner radius that turns a [`ROW_DOT_SIZE`] square into a circle.
const ROW_DOT_RADIUS: Length = Length::const_px(4.0);
/// Gap between a row's dot and its label.
const ROW_DOT_LABEL_GAP: Length = Length::const_px(6.0);
/// Vertical gap between rows in a rows-style tooltip.
const ROW_LINE_GAP: Length = Length::const_px(4.0);

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
    content: TooltipContent,
    theme: Theme,
    delay: Duration,
    last_pointer_move: Option<Instant>,
    last_cursor_pos: Point,
    /// ID of the currently-live tooltip layer, if any.
    /// Cleared in `on_pointer_event` so we don't try to create a layer
    /// while one is already showing.
    layer_id: Option<WidgetId>,
}

// --- MARK: BUILDERS
impl TooltipHost {
    /// Creates a new tooltip host wrapping `child`.
    #[must_use]
    pub(crate) fn new(
        child: NewWidget<impl Widget + ?Sized>,
        content: TooltipContent,
        theme: &Theme,
        delay: Duration,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            content,
            theme: *theme,
            delay,
            last_pointer_move: None,
            last_cursor_pos: Point::ZERO,
            layer_id: None,
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

    /// Replaces the tooltip content shown on the layer.
    pub(crate) fn set_content(this: &mut WidgetMut<'_, Self>, content: TooltipContent) {
        this.widget.content = content;
        this.ctx.request_accessibility_update();
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
        let surface = match &self.content {
            TooltipContent::Text(text) => self.build_text_surface(text),
            TooltipContent::Rows(rows) => self.build_rows_surface(rows),
        };

        let mut tooltip = NewWidget::new(TooltipLayer::new(surface));
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

    /// A single line of text, styled to match the tooltip surface.
    fn build_text_surface(&self, text: &ArcStr) -> NewWidget<dyn Widget> {
        let mut label = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(self.theme.typography.size_body))
            .prepare();
        label
            .properties
            .insert(ContentColor::new(self.theme.palette.text));
        label.erased()
    }

    /// A vertical list of dot+label rows, e.g. a status-colour legend —
    /// each row is the same coloured-dot shape the sidebar uses for its own
    /// status dots, so the legend reads as the same visual language as the
    /// thing it explains.
    fn build_rows_surface(&self, rows: &[TooltipRow]) -> NewWidget<dyn Widget> {
        let mut column = Flex::column().cross_axis_alignment(CrossAxisAlignment::Start);
        for row in rows {
            let mut dot = SizedBox::empty()
                .width(ROW_DOT_SIZE)
                .height(ROW_DOT_SIZE)
                .prepare();
            dot.properties.insert(Background::Color(row.color));
            dot.properties.insert(CornerRadius::all(ROW_DOT_RADIUS));

            let mut label = Label::new(row.label.clone())
                .with_style(StyleProperty::FontSize(self.theme.typography.size_body))
                .prepare();
            label
                .properties
                .insert(ContentColor::new(self.theme.palette.text));

            let mut line = Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .with_fixed(dot.erased())
                .with_fixed(label.erased())
                .prepare();
            line.properties.insert(Gap::new(ROW_DOT_LABEL_GAP));

            column = column.with_fixed(line.erased());
        }
        let mut column = column.prepare();
        column.properties.insert(Gap::new(ROW_LINE_GAP));
        column.erased()
    }

    /// Flattened text used for the accessibility description and the
    /// window layer's debug tag. A rows tooltip loses its colour coding in
    /// this form (screen readers don't convey colour anyway), but keeps the
    /// row labels in order.
    fn description(&self) -> String {
        match &self.content {
            TooltipContent::Text(text) => text.to_string(),
            TooltipContent::Rows(rows) => rows
                .iter()
                .map(|row| row.label.as_ref())
                .collect::<Vec<_>>()
                .join(", "),
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
            self.last_cursor_pos = current.logical_point();
            // The TooltipLayer's capture_pointer_event fires before this handler
            // and has already queued RemoveLayer. Clear our tracking so the
            // next anim-frame won't think a layer is still live.
            self.layer_id = None;
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
            // Guard: on_pointer_event clears layer_id whenever the pointer
            // moves (which also triggers the layer's self-dismissal), so
            // this should always be None here. Belt-and-suspenders.
            if self.layer_id.is_none() {
                let layer = self.build_layer();
                let layer_id = layer.id();
                let pos = self.last_cursor_pos + CURSOR_OFFSET;
                ctx.create_layer::<TooltipLayer>(
                    LayerType::Tooltip(self.description()),
                    layer,
                    pos,
                );
                self.layer_id = Some(layer_id);
            }
            // Disarm the timer. The next PointerEvent::Move re-arms via
            // on_pointer_event (and clears layer_id so we can show again).
            self.last_pointer_move = None;
        } else {
            ctx.request_anim_frame();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            // When an *interactive* child (a button) is hovered, it — not
            // TooltipHost — is the directly-hovered widget, so the host sees
            // `ChildHoveredChanged` rather than `HoveredChanged`. Use the
            // child signal to disarm the timer so we don't accumulate live
            // timers across siblings.
            Update::ChildHoveredChanged(false) => {
                self.last_pointer_move = None;
            }
            // Hover loss on the host itself. A non-interactive child (a plain
            // label or icon) never becomes the hovered widget — the host does —
            // so `ChildHoveredChanged` never fires for it, only `HoveredChanged`.
            // Without this match arm the host's `layer_id` goes stale on leave,
            // and the `on_anim_frame` guard (`if self.layer_id.is_none()`) then
            // blocks *every* future tooltip — the glyph shows a tip once and
            // never again. The visible layer self-dismisses via the leaving
            // pointer move (`TooltipLayer::capture_pointer_event`), so we only
            // clear our own tracking here; calling `remove_layer` on the
            // already-gone layer would `debug_panic!`.
            Update::HoveredChanged(false) => {
                self.last_pointer_move = None;
                self.layer_id = None;
            }
            // Keyboard users never produce pointer events, so focus is the
            // equivalent "arm the timer" signal: anchor the tooltip at the
            // child's bottom-left corner and start the same idle countdown
            // used for hover.
            Update::ChildFocusChanged(true) => {
                let rect = ctx.border_box();
                self.last_cursor_pos = ctx.to_window(Point::new(rect.x0, rect.y1));
                if let Some(layer_id) = self.layer_id.take() {
                    ctx.remove_layer(layer_id);
                }
                self.last_pointer_move = Some(Instant::now());
                ctx.request_anim_frame();
            }
            // Unlike hover, losing focus has no follow-up pointer event to
            // trigger the layer's self-dismissal, so remove it explicitly.
            Update::ChildFocusChanged(false) => {
                self.last_pointer_move = None;
                if let Some(layer_id) = self.layer_id.take() {
                    ctx.remove_layer(layer_id);
                }
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
        node: &mut Node,
    ) {
        // Exposes the tooltip text to assistive tech regardless of whether
        // the layer is currently shown, mirroring the alt-text pattern used
        // by `Image`/`Canvas`/`Svg`.
        node.set_description(self.description());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

// --- MARK: TESTS

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::*;
    use crate::components::button::widget::ThemedButton;

    /// Builds a `TooltipHost` wrapping a focusable button child, returning
    /// the harness and the child's `WidgetId`.
    fn harness(delay: Duration) -> (TestHarness<TooltipHost>, WidgetId) {
        let theme = Theme::dark();
        let child = NewWidget::new(ThemedButton::new(
            NewWidget::new(Label::new("Hover me")).erased(),
            &theme,
        ))
        .erased();
        let child_id = child.id();
        let widget = TooltipHost::new(
            child,
            TooltipContent::Text("Tip text".into()),
            &theme,
            delay,
        );
        let h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        (h, child_id)
    }

    #[test]
    fn accessibility_exposes_tooltip_text_as_description() {
        let (mut h, _) = harness(Duration::from_millis(300));
        h.redraw();

        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.description(), Some("Tip text".to_string()));
    }

    #[test]
    fn set_content_updates_accessibility_description() {
        let (mut h, _) = harness(Duration::from_millis(300));

        h.edit_root_widget(|mut wm| {
            TooltipHost::set_content(&mut wm, TooltipContent::Text("New text".into()));
        });
        h.redraw();

        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.description(), Some("New text".to_string()));
    }

    /// Keyboard users never produce pointer-move events, so focus gain must
    /// arm the same idle timer hover does.
    #[test]
    fn child_focus_gain_shows_layer_after_delay() {
        let (mut h, child_id) = harness(Duration::ZERO);

        h.focus_on(Some(child_id));
        h.animate_ms(1);

        assert!(h.edit_root_widget(|wm| wm.widget.layer_id.is_some()));
    }

    /// Losing focus has no follow-up pointer event to trigger the layer's
    /// self-dismissal, so it must be removed explicitly.
    #[test]
    fn child_focus_loss_removes_layer() {
        let (mut h, child_id) = harness(Duration::ZERO);

        h.focus_on(Some(child_id));
        h.animate_ms(1);
        assert!(h.edit_root_widget(|wm| wm.widget.layer_id.is_some()));

        h.focus_on(None);
        assert!(h.edit_root_widget(|wm| wm.widget.layer_id.is_none()));
    }

    /// Builds a `TooltipHost` wrapping a NON-interactive child (a plain
    /// `Label`). Such a child never becomes the hovered widget — `TooltipHost`
    /// itself does — so the tooltip must arm/disarm off the host's own hovered
    /// status, not the child's.
    fn label_harness(delay: Duration) -> TestHarness<TooltipHost> {
        let theme = Theme::dark();
        let child = NewWidget::new(Label::new("plain")).erased();
        let widget = TooltipHost::new(
            child,
            TooltipContent::Text("Tip text".into()),
            &theme,
            delay,
        );
        TestHarness::create(default_property_set(), NewWidget::new(widget))
    }

    /// Number of overlay (non-root) layers actually painted — the
    /// user-visible signal that a tooltip is on screen, independent of the
    /// host's internal `layer_id` bookkeeping.
    fn visible_overlay_layers(h: &mut TestHarness<TooltipHost>) -> usize {
        h.redraw().0.overlay_layers().count()
    }

    /// Hovering an icon/label (non-interactive child) must still show the
    /// tooltip — regression for backfill-error reasons that were invisible on
    /// hover because the info glyph is a plain `Label`.
    #[test]
    fn hover_over_noninteractive_child_shows_layer_after_delay() {
        let mut h = label_harness(Duration::ZERO);
        let root = h.root_id();

        h.mouse_move_to(root);
        h.animate_ms(1);

        assert!(
            h.edit_root_widget(|wm| wm.widget.layer_id.is_some()),
            "hovering a non-interactive child must show the tooltip"
        );
        assert_eq!(
            visible_overlay_layers(&mut h),
            1,
            "the tooltip layer must actually be painted"
        );
    }

    /// Leaving the host (pointer moves away entirely) must remove the layer —
    /// a non-interactive child produces no `ChildHoveredChanged`, so the host's
    /// own hover-loss is the only disarm signal.
    #[test]
    fn leaving_host_over_noninteractive_child_removes_layer() {
        let mut h = label_harness(Duration::ZERO);
        let root = h.root_id();

        h.mouse_move_to(root);
        h.animate_ms(1);
        assert!(h.edit_root_widget(|wm| wm.widget.layer_id.is_some()));
        assert_eq!(
            visible_overlay_layers(&mut h),
            1,
            "precondition: the tooltip layer is painted before leaving"
        );

        h.mouse_move((10_000.0, 10_000.0));
        assert!(
            h.edit_root_widget(|wm| wm.widget.layer_id.is_none()),
            "leaving the host must clear the host's layer tracking"
        );
        assert_eq!(
            visible_overlay_layers(&mut h),
            0,
            "leaving the host must remove the visible tooltip layer"
        );
    }

    /// A rows tooltip's accessibility description joins row labels in order —
    /// screen readers get the same information as a text tooltip, just
    /// without the colour coding a sighted user gets from the dots.
    #[test]
    fn rows_tooltip_description_joins_labels_in_order() {
        let theme = Theme::dark();
        let child = NewWidget::new(Label::new("plain")).erased();
        let rows = vec![
            TooltipRow::new(masonry::peniko::Color::from_rgb8(34, 197, 94), "Ready"),
            TooltipRow::new(masonry::peniko::Color::from_rgb8(239, 68, 68), "Error"),
        ];
        let widget = TooltipHost::new(
            child,
            TooltipContent::Rows(rows),
            &theme,
            Duration::from_millis(300),
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        h.redraw();

        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.description(), Some("Ready, Error".to_string()));
    }

    /// A rows tooltip must pop the same overlay layer a text tooltip does —
    /// the richer content shape doesn't change the hover/show mechanics.
    #[test]
    fn rows_tooltip_shows_layer_after_delay() {
        let theme = Theme::dark();
        let child = NewWidget::new(Label::new("plain")).erased();
        let rows = vec![TooltipRow::new(
            masonry::peniko::Color::from_rgb8(34, 197, 94),
            "Ready",
        )];
        let widget = TooltipHost::new(child, TooltipContent::Rows(rows), &theme, Duration::ZERO);
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        let root = h.root_id();

        h.mouse_move_to(root);
        h.animate_ms(1);

        assert!(h.edit_root_widget(|wm| wm.widget.layer_id.is_some()));
        assert_eq!(visible_overlay_layers(&mut h), 1);
    }
}
