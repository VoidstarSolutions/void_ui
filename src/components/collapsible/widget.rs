//! Masonry widget for the animated collapsible section.
//!
//! [`CollapsibleWidget`] renders a clickable header row (title + chevron) above
//! an [`AnimatedClip`] body that slides vertically between its natural height
//! (open) and 0 (closed) over 250 ms. The header is always visible so users
//! can reopen the section.
//!
//! Emits [`CollapsibleTogglePressed`] when the header is clicked.

use std::any::TypeId;

use masonry::accesskit::{self, Node, Role};
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
use crate::animated_clip::AnimatedClip;
use crate::components::click::{self, ClickPhase};
use crate::components::icon::{IconName, icon};
use crate::components::interaction;
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};

// --- MARK: CONSTANTS

/// Gap between the title label and the trailing chevron — 4px matches no
/// density token; the header already scales via `ui_font_size` + `pad_v`/`pad_h`.
const CHEVRON_GAP: f64 = 4.0;
/// Thickness of the separator line below the header — hairline chrome, not density-scaled.
const SEPARATOR_WIDTH: f64 = 1.0;

// --- MARK: ACTION

/// Action emitted by [`CollapsibleWidget`] when the header row is clicked.
#[derive(Debug, Clone)]
pub struct CollapsibleTogglePressed;

/// Pointer interaction state of the header row, combined into a single
/// field so the widget doesn't accumulate independent bool flags.
#[derive(Debug, Clone, Copy, Default)]
struct HeaderPointerState {
    hovered: bool,
    pressed: bool,
}

// --- MARK: WIDGET HELPERS

fn make_chevron(open: bool, theme: &Theme, disabled: bool) -> NewWidget<Label> {
    let name = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let color = if disabled {
        theme.palette.text_faint
    } else {
        theme.palette.text_muted
    };
    icon(name).color(color).build_widget(theme)
}

fn make_title(text: ArcStr, theme: &Theme, disabled: bool) -> NewWidget<Label> {
    let color = if disabled {
        theme.palette.text_faint
    } else {
        theme.palette.text
    };
    let mut lbl = Label::new(text)
        .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
        .prepare();
    lbl.properties.insert(ContentColor::new(color));
    lbl
}

// --- MARK: CollapsibleWidget

/// Collapsible section with an animated body and a persistent clickable header.
///
/// The header always occupies a fixed row height (icon-font-size + padding).
/// The body transitions between 0 and its natural height when `open` changes
/// via [`AnimatedClip`].
///
/// Emits [`CollapsibleTogglePressed`] when the header row is clicked.
// `open`, `disabled`, and `header_keyboard_pressed` are independent flags with
// no shared state machine to fold them into; pointer hover/press live together
// in `header_state`.
pub struct CollapsibleWidget<W: Widget + ?Sized> {
    title: WidgetPod<Label>,
    chevron: WidgetPod<Label>,
    body: WidgetPod<AnimatedClip<W>>,
    theme: Theme,
    open: bool,
    disabled: bool,
    /// Header hover/press state, combined to keep the bool-field count down.
    header_state: HeaderPointerState,
    /// True for the span between a Space/Enter key-down and its matching
    /// key-up (or an intervening focus loss) — the keyboard equivalent of
    /// `header_state.pressed`, so keyboard activation shows the same pressed
    /// fill a pointer click does.
    header_keyboard_pressed: bool,
    /// Header height in the last layout pass (icon-font-size + 2 × `density.pad_v`).
    current_header_height: f64,
    /// Widget width from the last layout pass.
    current_width: f64,
}

// --- MARK: BUILDERS

impl<W: Widget + ?Sized> CollapsibleWidget<W> {
    #[must_use]
    pub fn new(
        title: ArcStr,
        child: NewWidget<W>,
        theme: &Theme,
        open: bool,
        disabled: bool,
    ) -> Self {
        Self {
            title: make_title(title, theme, disabled).to_pod(),
            chevron: make_chevron(open, theme, disabled).to_pod(),
            body: WidgetPod::new(AnimatedClip::new(child, Axis::Vertical, open)),
            theme: *theme,
            open,
            disabled,
            header_state: HeaderPointerState::default(),
            header_keyboard_pressed: false,
            current_header_height: 0.0,
            current_width: 0.0,
        }
    }
}

// --- MARK: WIDGETMUT

impl<W: Widget + ?Sized> CollapsibleWidget<W> {
    pub fn body_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, AnimatedClip<W>> {
        this.ctx.get_mut(&mut this.widget.body)
    }
}

impl<W: Widget + ?Sized> CollapsibleWidget<W> {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            let disabled = this.widget.disabled;
            let title_color = if disabled {
                theme.palette.text_faint
            } else {
                theme.palette.text
            };
            let chevron_color = if disabled {
                theme.palette.text_faint
            } else {
                theme.palette.text_muted
            };
            {
                let mut chevron = this.ctx.get_mut(&mut this.widget.chevron);
                chevron.insert_prop(ContentColor::new(chevron_color));
                Label::insert_style(
                    &mut chevron,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
            {
                let mut title = this.ctx.get_mut(&mut this.widget.title);
                title.insert_prop(ContentColor::new(title_color));
                Label::insert_style(
                    &mut title,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
            this.ctx.request_layout();
        }
    }

    pub fn set_title(this: &mut WidgetMut<'_, Self>, text: ArcStr) {
        let mut title = this.ctx.get_mut(&mut this.widget.title);
        Label::set_text(&mut title, text);
    }

    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            let theme = this.widget.theme;
            let title_color = if disabled {
                theme.palette.text_faint
            } else {
                theme.palette.text
            };
            let chevron_color = if disabled {
                theme.palette.text_faint
            } else {
                theme.palette.text_muted
            };
            {
                let mut title = this.ctx.get_mut(&mut this.widget.title);
                title.insert_prop(ContentColor::new(title_color));
            }
            {
                let mut chevron = this.ctx.get_mut(&mut this.widget.chevron);
                chevron.insert_prop(ContentColor::new(chevron_color));
            }
            this.ctx.request_paint_only();
        }
    }

    pub fn set_open(this: &mut WidgetMut<'_, Self>, open: bool) {
        if this.widget.open != open {
            this.widget.open = open;
            this.ctx.request_paint_only();
            let new_icon = if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            };
            let new_char = ArcStr::from(String::from(char::from(new_icon)));
            {
                let mut chevron = this.ctx.get_mut(&mut this.widget.chevron);
                Label::set_text(&mut chevron, new_char);
            }
            let mut body = this.ctx.get_mut(&mut this.widget.body);
            AnimatedClip::set_open(&mut body, open);
        }
    }
}

// --- MARK: HEADER PAINT STATE

impl<W: Widget + ?Sized> CollapsibleWidget<W> {
    fn header_bg(&self) -> Color {
        if self.disabled {
            return Color::TRANSPARENT;
        }
        let p = &self.theme.palette;
        if (self.header_state.pressed && self.header_state.hovered) || self.header_keyboard_pressed
        {
            p.surface_hi
        } else if self.header_state.hovered {
            p.surface_2
        } else {
            Color::TRANSPARENT
        }
    }

    fn header_height(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size) + 2.0 * f64::from(self.theme.density.pad_v)
    }
}

// --- MARK: IMPL WIDGET

impl<W: Widget + ?Sized> Widget for CollapsibleWidget<W> {
    type Action = CollapsibleTogglePressed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled {
            return;
        }
        match event {
            PointerEvent::Move(update) => {
                let pos = ctx.local_position(update.current.position);
                let in_header = pos.y < self.current_header_height;
                if in_header != self.header_state.hovered {
                    self.header_state.hovered = in_header;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Leave(_) if self.header_state.hovered => {
                self.header_state.hovered = false;
                ctx.request_paint_only();
            }
            // Shared primary-click recognizer; the capture guard only
            // accepts presses on the header row, so a press in the body
            // never captures — capturing there would steal the pointer
            // from interactive body children (events bubble up here).
            _ => match click::primary_click_when(ctx, event, |ctx, state| {
                ctx.local_position(state.position).y < self.current_header_height
            }) {
                Some(ClickPhase::Down(_)) => {
                    self.header_state.pressed = true;
                    ctx.request_focus();
                    ctx.request_paint_only();
                }
                Some(ClickPhase::Up { state, completed }) if self.header_state.pressed => {
                    let pos = ctx.local_position(state.position);
                    let in_header = pos.y < self.current_header_height;
                    if completed && in_header {
                        ctx.submit_action::<Self::Action>(CollapsibleTogglePressed);
                    }
                    self.header_state.pressed = false;
                    self.header_state.hovered = in_header;
                    ctx.request_paint_only();
                }
                Some(ClickPhase::Up { .. }) | None => {}
            },
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.disabled || !ctx.is_focus_target() {
            return;
        }
        if interaction::keyboard_press_start(event, true) {
            ctx.set_handled();
            self.header_keyboard_pressed = true;
            ctx.request_paint_only();
        } else if interaction::keyboard_activate(event, true) {
            ctx.set_handled();
            self.header_keyboard_pressed = false;
            ctx.request_paint_only();
            ctx.submit_action::<Self::Action>(CollapsibleTogglePressed);
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if self.disabled {
            return;
        }
        if ctx.target() == ctx.widget_id() && interaction::is_access_click(event) {
            ctx.submit_action::<Self::Action>(CollapsibleTogglePressed);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => ctx.set_disabled(self.disabled),
            Update::HoveredChanged(false) if self.header_state.hovered => {
                self.header_state.hovered = false;
                ctx.request_paint_only();
            }
            Update::FocusChanged(false) => {
                // Losing focus mid-press (e.g. Tab away while Space is still
                // held) would otherwise leave `header_keyboard_pressed` stuck
                // true with no matching key-up ever arriving to clear it.
                self.header_keyboard_pressed = false;
                ctx.request_paint_only();
            }
            Update::DisabledChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.title);
        ctx.register_child(&mut self.chevron);
        ctx.register_child(&mut self.body);
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
        let icon_sz = Length::px(f64::from(self.theme.density.ui_font_size));
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);

        // Measure chevron at icon-font-size square (result discarded — needed for intrinsics tracking).
        let _ = ctx.compute_length(
            &mut self.chevron,
            len_req.into(),
            LayoutSize::maybe(axis.cross(), Some(icon_sz)),
            axis,
            Some(icon_sz),
        );
        // Measure title (result discarded on horizontal pass).
        let _ = ctx.compute_length(
            &mut self.title,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        );
        // Body drives width; header adapts to allocated width.
        let body_length = ctx.compute_length(
            &mut self.body,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        );

        if axis == Axis::Vertical {
            Length::px(self.header_height() + body_length.get())
        } else {
            body_length
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let icon_sz = f64::from(self.theme.density.ui_font_size);
        let pad_h = f64::from(self.theme.density.pad_h);
        let pad_v = f64::from(self.theme.density.pad_v);
        let header_h = self.header_height();
        self.current_header_height = header_h;
        self.current_width = size.width;

        // Title: left side of header, leaving room for chevron + gaps.
        let title_w = (size.width - pad_h * 2.0 - CHEVRON_GAP - icon_sz).max(0.0);
        ctx.run_layout(&mut self.title, Size::new(title_w, header_h));
        ctx.place_child(&mut self.title, Point::new(pad_h, pad_v));

        // Chevron: right-aligned, vertically centred in the header row.
        let chevron_x = size.width - pad_h - icon_sz;
        let chevron_y = (header_h - icon_sz) * 0.5;
        ctx.run_layout(&mut self.chevron, Size::new(icon_sz, icon_sz));
        ctx.place_child(&mut self.chevron, Point::new(chevron_x, chevron_y));

        // Body: immediately below the header.
        let body_h = (size.height - header_h - SEPARATOR_WIDTH).max(0.0);
        ctx.run_layout(&mut self.body, Size::new(size.width, body_h));
        ctx.place_child(&mut self.body, Point::new(0.0, header_h + SEPARATOR_WIDTH));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let h = self.current_header_height;
        let w = self.current_width;
        let p = &self.theme.palette;

        // Header background (hover/press feedback).
        let bg = self.header_bg();
        if bg.components[3] > 0.0 {
            painter
                .fill(Rect::from_origin_size(Point::ORIGIN, Size::new(w, h)), bg)
                .draw();
        }

        // Keyboard-focus ring around the header.
        if ctx.is_focus_target() {
            let inset = FOCUS_RING_INSET;
            let focus_rect = Rect::from_origin_size(
                Point::new(inset, inset),
                Size::new((w - 2.0 * inset).max(0.0), (h - 2.0 * inset).max(0.0)),
            );
            paint_focus_ring(painter, focus_rect, &self.theme);
        }

        // Separator below the header.
        painter
            .fill(
                Rect::from_origin_size(Point::new(0.0, h), Size::new(w, SEPARATOR_WIDTH)),
                p.border,
            )
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if !self.disabled {
            node.add_action(accesskit::Action::Click);
        }
        node.set_expanded(self.open);
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids = [self.title.id(), self.chevron.id(), self.body.id()];
        ChildrenIds::from_slice(&ids)
    }

    fn propagates_pointer_interaction(&self) -> bool {
        true
    }

    fn accepts_focus(&self) -> bool {
        !self.disabled
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("CollapsibleWidget", id = id.trace())
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{ArcStr, NewWidget, PointerButton, TextEvent, Widget};
    use masonry::kurbo::Point;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{CollapsibleTogglePressed, CollapsibleWidget};
    use crate::Theme;

    fn harness(disabled: bool) -> TestHarness<CollapsibleWidget<dyn Widget>> {
        let body = NewWidget::new(Label::new("body")).erased();
        let widget = CollapsibleWidget::new(
            ArcStr::from("Section"),
            body,
            &Theme::dark(),
            true,
            disabled,
        );
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (200, 120))
    }

    /// Center of the header row, read from the last layout.
    fn header_center(h: &mut TestHarness<CollapsibleWidget<dyn Widget>>) -> Point {
        let header_h = h.edit_root_widget(|wm| wm.widget.current_header_height);
        Point::new(100.0, header_h * 0.5)
    }

    #[test]
    fn clicking_the_header_toggles() {
        let mut h = harness(false);
        let target = header_center(&mut h);
        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<CollapsibleTogglePressed>().is_some());
    }

    #[test]
    fn clicking_the_body_does_not_toggle() {
        let mut h = harness(false);
        let header_h = h.edit_root_widget(|wm| wm.widget.current_header_height);
        h.mouse_move(Point::new(100.0, header_h + 30.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<CollapsibleTogglePressed>().is_none());
    }

    #[test]
    fn dragging_from_header_into_body_cancels() {
        let mut h = harness(false);
        let target = header_center(&mut h);
        let header_h = h.edit_root_widget(|wm| wm.widget.current_header_height);
        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_move(Point::new(100.0, header_h + 30.0));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<CollapsibleTogglePressed>().is_none());
    }

    #[test]
    fn space_and_enter_activate_when_focused() {
        let mut h = harness(false);
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.pop_action::<CollapsibleTogglePressed>().is_some());

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.pop_action::<CollapsibleTogglePressed>().is_some());
    }

    #[test]
    fn space_key_down_shows_the_pressed_fill_until_key_up() {
        // Regression: on_text_event used to only ever act on key-up, so
        // Space/Enter "clicking" the header showed no pressed-fill feedback
        // the way a pointer click does.
        let mut h = harness(false);
        h.focus_on(Some(h.root_id()));

        assert!(!h.edit_root_widget(|wm| wm.widget.header_keyboard_pressed));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.header_keyboard_pressed));
        assert!(
            h.pop_action::<CollapsibleTogglePressed>().is_none(),
            "not yet activated"
        );

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(!h.edit_root_widget(|wm| wm.widget.header_keyboard_pressed));
        assert!(h.pop_action::<CollapsibleTogglePressed>().is_some());
    }

    #[test]
    fn losing_focus_mid_press_clears_the_keyboard_pressed_flag() {
        let mut h = harness(false);
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.header_keyboard_pressed));

        h.focus_on(None);
        assert!(!h.edit_root_widget(|wm| wm.widget.header_keyboard_pressed));
    }

    /// Keyboard activation is covered separately: masonry withholds focus (and
    /// therefore text/keyboard events) from disabled widgets framework-side,
    /// so there's no keyboard path here left to suppress at this layer.
    #[test]
    fn disabled_suppresses_header_click() {
        let mut h = harness(true);
        let target = header_center(&mut h);
        h.mouse_move(target);
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            h.pop_action::<CollapsibleTogglePressed>().is_none(),
            "disabled collapsible must not toggle on click"
        );
    }
}

#[cfg(test)]
mod density_tests {
    use masonry::widgets::SizedBox;

    use super::*;
    use crate::theme::Density;

    fn header_height_at(density: Density) -> f64 {
        let child = NewWidget::new(SizedBox::empty());
        let theme = Theme::default().with_density(density);
        CollapsibleWidget::new("title".into(), child, &theme, true, false).header_height()
    }

    #[test]
    fn header_height_scales_with_density() {
        // Balanced must keep the pre-token geometry: ui_font_size 12 + 2 × PAD_V(6).
        assert!((header_height_at(Density::balanced()) - (12.0 + 2.0 * 6.0)).abs() < 1e-6);
        assert!(header_height_at(Density::compact()) < header_height_at(Density::balanced()));
        assert!(header_height_at(Density::balanced()) < header_height_at(Density::airy()));
        let delta = header_height_at(Density::airy()) - header_height_at(Density::compact());
        // font spread + pad_v spread
        assert!((delta - ((13.0 - 11.0) + 2.0 * (8.0 - 4.0))).abs() < 1e-6);
    }
}
