//! Masonry widget for the animated collapsible section.
//!
//! [`CollapsibleWidget`] renders a clickable header row (title + chevron) above
//! an [`AnimatedClip`] body that slides vertically between its natural height
//! (open) and 0 (closed) over 250 ms. The header is always visible so users
//! can reopen the section.
//!
//! Emits [`CollapsibleTogglePressed`] when the header is clicked.

use std::any::TypeId;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;
use crate::animated_clip::AnimatedClip;
use crate::components::icon::{IconName, icon};

// --- MARK: CONSTANTS

/// Vertical padding inside the header row.
const PAD_V: f64 = 6.0;
/// Horizontal padding inside the header row.
const PAD_H: f64 = 8.0;
/// Gap between the title label and the trailing chevron.
const CHEVRON_GAP: f64 = 4.0;
/// Thickness of the separator line below the header.
const SEPARATOR_WIDTH: f64 = 1.0;

// --- MARK: ACTION

/// Action emitted by [`CollapsibleWidget`] when the header row is clicked.
#[derive(Debug, Clone)]
pub struct CollapsibleTogglePressed;

// --- MARK: WIDGET HELPERS

fn make_chevron(open: bool, theme: &Theme) -> NewWidget<Label> {
    let name = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    icon(name)
        .color(theme.palette.text_muted)
        .build_widget(theme)
}

fn make_title(text: ArcStr, theme: &Theme) -> NewWidget<Label> {
    let mut lbl = Label::new(text)
        .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
        .prepare();
    lbl.properties.insert(ContentColor::new(theme.palette.text));
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
pub struct CollapsibleWidget<W: Widget + ?Sized> {
    title: WidgetPod<Label>,
    chevron: WidgetPod<Label>,
    body: WidgetPod<AnimatedClip<W>>,
    theme: Theme,
    open: bool,
    /// True while the pointer is inside the header area.
    header_hovered: bool,
    /// True while the header is being pressed.
    header_pressed: bool,
    /// Header height in the last layout pass (icon-font-size + 2 × `PAD_V`).
    current_header_height: f64,
    /// Widget width from the last layout pass.
    current_width: f64,
}

// --- MARK: BUILDERS

impl<W: Widget + ?Sized> CollapsibleWidget<W> {
    #[must_use]
    pub fn new(title: ArcStr, child: NewWidget<W>, theme: &Theme, open: bool) -> Self {
        Self {
            title: make_title(title, theme).to_pod(),
            chevron: make_chevron(open, theme).to_pod(),
            body: WidgetPod::new(AnimatedClip::new(child, Axis::Vertical, open)),
            theme: *theme,
            open,
            header_hovered: false,
            header_pressed: false,
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
            {
                let mut chevron = this.ctx.get_mut(&mut this.widget.chevron);
                chevron.insert_prop(ContentColor::new(theme.palette.text_muted));
                Label::insert_style(
                    &mut chevron,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
            {
                let mut title = this.ctx.get_mut(&mut this.widget.title);
                title.insert_prop(ContentColor::new(theme.palette.text));
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
        let p = &self.theme.palette;
        if self.header_pressed && self.header_hovered {
            p.surface_hi
        } else if self.header_hovered {
            p.surface_2
        } else {
            Color::TRANSPARENT
        }
    }

    fn header_height(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size) + 2.0 * PAD_V
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
        match event {
            PointerEvent::Move(update) => {
                let pos = ctx.local_position(update.current.position);
                let in_header = pos.y < self.current_header_height;
                if in_header != self.header_hovered {
                    self.header_hovered = in_header;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if pos.y < self.current_header_height {
                    self.header_pressed = true;
                    ctx.capture_pointer();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) if self.header_pressed => {
                let pos = ctx.local_position(state.position);
                let in_header = pos.y < self.current_header_height;
                if ctx.is_hovered() && in_header {
                    ctx.submit_action::<Self::Action>(CollapsibleTogglePressed);
                }
                self.header_pressed = false;
                self.header_hovered = in_header;
                ctx.request_paint_only();
            }
            PointerEvent::Leave(_) if self.header_hovered => {
                self.header_hovered = false;
                ctx.request_paint_only();
            }
            _ => {}
        }
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

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::HoveredChanged(false) = event
            && self.header_hovered
        {
            self.header_hovered = false;
            ctx.request_paint_only();
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
        let header_h = self.header_height();
        self.current_header_height = header_h;
        self.current_width = size.width;

        // Title: left side of header, leaving room for chevron + gaps.
        let title_w = (size.width - PAD_H * 2.0 - CHEVRON_GAP - icon_sz).max(0.0);
        ctx.run_layout(&mut self.title, Size::new(title_w, header_h));
        ctx.place_child(&mut self.title, Point::new(PAD_H, PAD_V));

        // Chevron: right-aligned, vertically centred in the header row.
        let chevron_x = size.width - PAD_H - icon_sz;
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
        _ctx: &mut PaintCtx<'_>,
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

        // Separator below the header.
        painter
            .fill(
                Rect::from_origin_size(Point::new(0.0, h), Size::new(w, SEPARATOR_WIDTH)),
                p.border,
            )
            .draw();
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
        let ids = [self.title.id(), self.chevron.id(), self.body.id()];
        ChildrenIds::from_slice(&ids)
    }

    fn propagates_pointer_interaction(&self) -> bool {
        true
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("CollapsibleWidget", id = id.trace())
    }
}
