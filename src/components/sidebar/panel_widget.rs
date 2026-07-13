//! Masonry widget for the animated collapsible sidebar panel.
//!
//! [`ThemedSidebarPanel`] wraps any child widget and renders a narrow toggle
//! strip on its right edge. The strip contains a `‹` chevron when expanded and
//! `›` when collapsed. Clicking the strip animates the content width between
//! its natural size (expanded) and 0 (collapsed) over 250 ms via
//! [`AnimatedClip`]; the strip itself is always visible so users can always
//! reopen the sidebar.

use std::any::TypeId;

use crate::animated_clip::AnimatedClip;
use crate::components::icon::IconName;
use crate::focus_ring::paint_focus_ring;
use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, TextEvent,
    Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::icon::icon;
use crate::components::interaction;

// --- MARK: CONSTANTS

/// Width of the collapse/expand toggle strip on the right edge — a minimum
/// hit-target (like `resizable`'s `GRAB_HALF`), not density-scaled.
const STRIP_WIDTH: f64 = 20.0;
/// Width of the strip's left separator line — hairline chrome, not density-scaled.
const SEPARATOR_WIDTH: f64 = 1.0;

// --- MARK: ACTION

/// Action emitted by [`ThemedSidebarPanel`] when the toggle strip is clicked.
#[derive(Debug, Clone)]
pub struct SidebarTogglePressed;

// --- MARK: CHEVRON HELPER

fn make_chevron(collapsed: bool, theme: &Theme) -> NewWidget<Label> {
    let name = if collapsed {
        IconName::ChevronRight
    } else {
        IconName::ChevronLeft
    };
    icon(name)
        .color(theme.palette.text_muted)
        .build_widget(theme)
}

// --- MARK: ThemedSidebarPanel

/// Sidebar container that animates its content width and renders a persistent
/// toggle strip on its right edge.
///
/// The strip always occupies [`STRIP_WIDTH`] pixels; the content area
/// transitions between 0 and its natural width when `collapsed` changes via
/// [`AnimatedClip`].
///
/// Emits [`SidebarTogglePressed`] when the strip is clicked.
// `collapsed`/`strip_hovered`/`strip_pressed`/`strip_keyboard_pressed` are
// independent flags with no shared state machine to fold them into — each
// can be true or false regardless of the others (e.g. collapsed-while-
// hovered, pointer-pressed-while-keyboard-pressed mid-transition) — so an
// enum would just relocate the same four independent facts, not simplify
// them.
#[allow(clippy::struct_excessive_bools)]
pub struct ThemedSidebarPanel<W: Widget + ?Sized> {
    content: WidgetPod<AnimatedClip<W>>,
    chevron: WidgetPod<Label>,
    theme: Theme,
    collapsed: bool,
    /// True while the pointer is inside the strip area.
    strip_hovered: bool,
    /// True while the strip is being pressed.
    strip_pressed: bool,
    /// True for the span between a Space/Enter key-down and its matching
    /// key-up (or an intervening focus loss) — the keyboard equivalent of
    /// `strip_pressed`, so keyboard activation shows the same pressed fill
    /// a pointer click does.
    strip_keyboard_pressed: bool,
    /// Left x coordinate of the strip in the last layout pass.
    current_strip_x: f64,
    /// Widget height from the last layout pass.
    current_height: f64,
}

// --- MARK: BUILDERS
impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    #[must_use]
    pub fn new(child: NewWidget<W>, theme: &Theme, collapsed: bool) -> Self {
        Self {
            content: WidgetPod::new(AnimatedClip::new(child, Axis::Horizontal, !collapsed)),
            chevron: make_chevron(collapsed, theme).to_pod(),
            theme: *theme,
            collapsed,
            strip_hovered: false,
            strip_pressed: false,
            strip_keyboard_pressed: false,
            current_strip_x: 0.0,
            current_height: 0.0,
        }
    }
}

// --- MARK: WIDGETMUT
impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    pub fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, AnimatedClip<W>> {
        this.ctx.get_mut(&mut this.widget.content)
    }
}

impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            {
                let mut chevron = this.ctx.get_mut(&mut this.widget.chevron);
                chevron.insert_prop(ContentColor::new(theme.palette.text_muted));
                Label::insert_style(
                    &mut chevron,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
            this.ctx.request_layout();
        }
    }

    pub fn set_collapsed(this: &mut WidgetMut<'_, Self>, collapsed: bool) {
        if this.widget.collapsed != collapsed {
            this.widget.collapsed = collapsed;
            this.ctx.request_paint_only();
            let new_icon = if collapsed {
                IconName::ChevronRight
            } else {
                IconName::ChevronLeft
            };
            let new_char = ArcStr::from(String::from(char::from(new_icon)));
            {
                let mut chevron = this.ctx.get_mut(&mut this.widget.chevron);
                Label::set_text(&mut chevron, new_char);
            }
            let mut content = this.ctx.get_mut(&mut this.widget.content);
            AnimatedClip::set_open(&mut content, !collapsed);
        }
    }
}

// --- MARK: STRIP PAINT STATE
impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    fn strip_bg(&self) -> Color {
        let p = &self.theme.palette;
        if (self.strip_pressed && self.strip_hovered) || self.strip_keyboard_pressed {
            p.surface_hi
        } else if self.strip_hovered {
            p.surface_2
        } else {
            Color::TRANSPARENT
        }
    }
}

// --- MARK: IMPL WIDGET
impl<W: Widget + ?Sized> Widget for ThemedSidebarPanel<W> {
    type Action = SidebarTogglePressed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Positional hover: only the trailing strip is interactive, so track
        // whether the pointer sits over it — this drives the strip's paint
        // state and is not part of the shared press machine.
        match event {
            PointerEvent::Move(update) => {
                let pos = ctx.local_position(update.current.position);
                let in_strip = pos.x >= self.current_strip_x;
                if in_strip != self.strip_hovered {
                    self.strip_hovered = in_strip;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Leave(_) if self.strip_hovered => {
                self.strip_hovered = false;
                ctx.request_paint_only();
            }
            _ => {}
        }

        // Shared press machine, with the strip's hit test as the capture guard
        // so a press aimed at the panel *content* never arms the toggle.
        let strip_x = self.current_strip_x;
        let phase = click::primary_click_when(ctx, event, |ctx, state| {
            ctx.local_position(state.position).x >= strip_x
        });
        match phase {
            Some(ClickPhase::Down(_)) => {
                self.strip_pressed = true;
                ctx.request_focus();
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up { completed, .. }) => {
                // `completed` is widget-level; refine it positionally — the
                // toggle only fires if the release is still over the strip.
                if completed && self.strip_hovered {
                    ctx.submit_action::<Self::Action>(SidebarTogglePressed);
                }
                self.strip_pressed = false;
                ctx.request_paint_only();
            }
            None => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if interaction::keyboard_press_start(event, true) {
            ctx.set_handled();
            self.strip_keyboard_pressed = true;
            ctx.request_paint_only();
        } else if interaction::keyboard_activate(event, true) {
            ctx.set_handled();
            self.strip_keyboard_pressed = false;
            ctx.request_paint_only();
            ctx.submit_action::<Self::Action>(SidebarTogglePressed);
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if interaction::is_access_click(event) {
            ctx.submit_action::<Self::Action>(SidebarTogglePressed);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::HoveredChanged(false) = event
            && self.strip_hovered
        {
            self.strip_hovered = false;
            ctx.request_paint_only();
        }
        if matches!(event, Update::FocusChanged(false)) {
            // Losing focus mid-press (e.g. Tab away while Space is still
            // held) would otherwise leave `strip_keyboard_pressed` stuck
            // true with no matching key-up ever arriving to clear it.
            self.strip_keyboard_pressed = false;
        }
        if let Update::FocusChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.content);
        ctx.register_child(&mut self.chevron);
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
        let content_length = ctx.compute_length(
            &mut self.content,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        );
        // Measure chevron at a fixed icon-font-size square.
        let chevron_sz = Length::px(f64::from(self.theme.density.ui_font_size));
        let chevron_ctx = LayoutSize::maybe(axis.cross(), Some(chevron_sz));
        let _ = ctx.compute_length(
            &mut self.chevron,
            len_req.into(),
            chevron_ctx,
            axis,
            Some(chevron_sz),
        );
        if axis == Axis::Horizontal {
            Length::px(content_length.get() + STRIP_WIDTH)
        } else {
            content_length
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let content_width = (size.width - STRIP_WIDTH).max(0.0);
        let content_size = Size::new(content_width, size.height);
        ctx.run_layout(&mut self.content, content_size);
        ctx.place_child(&mut self.content, Point::ORIGIN);
        self.current_strip_x = content_width;
        self.current_height = size.height;

        // Place chevron centered in the strip.
        let icon_sz = f64::from(self.theme.density.ui_font_size);
        ctx.run_layout(&mut self.chevron, Size::new(icon_sz, icon_sz));
        let chevron_x = content_width + STRIP_WIDTH * 0.5 - icon_sz * 0.5;
        let chevron_y = (size.height - icon_sz) * 0.5;
        ctx.place_child(&mut self.chevron, Point::new(chevron_x, chevron_y));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let strip_x = self.current_strip_x;
        let h = self.current_height;
        let p = &self.theme.palette;

        // Separator line on the left edge of the strip.
        let sep_rect =
            Rect::from_origin_size(Point::new(strip_x, 0.0), Size::new(SEPARATOR_WIDTH, h));
        painter.fill(sep_rect, p.border).draw();

        // Strip background (hover/press feedback).
        let bg = self.strip_bg();
        if bg.components[3] > 0.0 {
            let bg_rect = Rect::from_origin_size(
                Point::new(strip_x + SEPARATOR_WIDTH, 0.0),
                Size::new(STRIP_WIDTH - SEPARATOR_WIDTH, h),
            );
            painter.fill(bg_rect, bg).draw();
        }

        // Chevron is a self-painting Label child placed during layout.

        if ctx.is_focus_target() {
            let focus_rect = Rect::from_origin_size(
                Point::new(strip_x + SEPARATOR_WIDTH, 0.0),
                Size::new(STRIP_WIDTH - SEPARATOR_WIDTH, h),
            );
            paint_focus_ring(painter, focus_rect, &self.theme);
        }
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
        node.add_action(accesskit::Action::Click);
        let label = if self.collapsed {
            "Expand sidebar"
        } else {
            "Collapse sidebar"
        };
        node.set_label(label);
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids = [self.content.id(), self.chevron.id()];
        ChildrenIds::from_slice(&ids)
    }

    fn propagates_pointer_interaction(&self) -> bool {
        true
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("ThemedSidebarPanel", id = id.trace())
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{NewWidget, TextEvent};
    use masonry::layout::Length;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::SizedBox;

    use super::{SidebarTogglePressed, ThemedSidebarPanel};
    use crate::Theme;

    fn harness() -> TestHarness<ThemedSidebarPanel<SizedBox>> {
        let widget = ThemedSidebarPanel::new(
            NewWidget::new(SizedBox::empty().size(Length::px(80.0), Length::px(200.0))),
            &Theme::dark(),
            false,
        );
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (100, 200))
    }

    #[test]
    fn space_key_down_shows_the_pressed_fill_until_key_up() {
        // Regression: on_text_event used to only ever act on key-up, so
        // Space/Enter "clicking" the strip showed no pressed-fill feedback
        // the way a pointer click does.
        let mut h = harness();
        h.focus_on(Some(h.root_id()));

        assert!(!h.edit_root_widget(|wm| wm.widget.strip_keyboard_pressed));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.strip_keyboard_pressed));
        assert!(
            h.pop_action::<SidebarTogglePressed>().is_none(),
            "not yet activated"
        );

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(!h.edit_root_widget(|wm| wm.widget.strip_keyboard_pressed));
        assert!(h.pop_action::<SidebarTogglePressed>().is_some());
    }

    #[test]
    fn enter_also_activates() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.pop_action::<SidebarTogglePressed>().is_some());
    }

    #[test]
    fn losing_focus_mid_press_clears_the_keyboard_pressed_flag() {
        let mut h = harness();
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.strip_keyboard_pressed));

        h.focus_on(None);
        assert!(!h.edit_root_widget(|wm| wm.widget.strip_keyboard_pressed));
    }
}
