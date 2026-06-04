//! Masonry widget for the animated collapsible sidebar panel.
//!
//! [`ThemedSidebarPanel`] wraps any child widget and renders a narrow toggle
//! strip on its right edge. The strip contains a `‹` chevron when expanded and
//! `›` when collapsed. Clicking the strip animates the content width between
//! its natural size (expanded) and 0 (collapsed) over 250 ms; the strip itself
//! is always visible so users can always reopen the sidebar.

use std::any::TypeId;
use std::borrow::Cow;

use lucide_icons::Icon as LucideIcon;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ArcStr, ChildrenIds, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx,
    NewWidget, NoAction, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, StyleProperty, TextEvent, Update, UpdateCtx, Widget, WidgetId,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::parley::{FontFamily, FontFamilyName};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;

// --- MARK: CONSTANTS

/// Width of the collapse/expand toggle strip on the right edge.
const STRIP_WIDTH: f64 = 20.0;
/// Duration of the collapse/expand slide animation.
const SLIDE_MILLIS: f32 = 250.0;
/// Width of the strip's left separator line.
const SEPARATOR_WIDTH: f64 = 1.0;

// --- MARK: ACTION

/// Action emitted by [`ThemedSidebarPanel`] when the toggle strip is clicked.
#[derive(Debug, Clone)]
pub struct SidebarTogglePressed;

// --- MARK: CHEVRON HELPER

fn make_chevron(collapsed: bool, theme: &Theme) -> NewWidget<Label> {
    let icon = if collapsed {
        LucideIcon::ChevronRight
    } else {
        LucideIcon::ChevronLeft
    };
    let ch = char::from(icon);
    let mut lbl = Label::new(ArcStr::from(String::from(ch)))
        .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
        .with_style(StyleProperty::FontFamily(FontFamily::Single(
            FontFamilyName::Named(Cow::Borrowed("lucide")),
        )))
        .prepare();
    lbl.properties
        .insert(ContentColor::new(theme.palette.text_muted));
    lbl
}

// --- MARK: SidebarContent

/// Widget that clips its child to an animated width.
///
/// Only [`ThemedSidebarPanel`] constructs it; it is public so the documented
/// path from a panel to its content ([`ThemedSidebarPanel::content_mut`] then
/// [`SidebarContent::child_mut`]) works outside the crate.
pub struct SidebarContent<W: Widget + ?Sized> {
    child: WidgetPod<W>,
    collapsed: bool,
    /// 0.0 = fully visible, 1.0 = fully hidden.
    collapse_progress: f32,
    /// Child's natural width from the most recent measure pass.
    natural_width: f64,
}

impl<W: Widget + ?Sized> SidebarContent<W> {
    pub(crate) fn new(child: NewWidget<W>, collapsed: bool) -> Self {
        Self {
            child: child.to_pod(),
            collapsed,
            collapse_progress: if collapsed { 1.0 } else { 0.0 },
            natural_width: 0.0,
        }
    }
}

impl<W: Widget + FromDynWidget> SidebarContent<W> {
    /// Returns a `WidgetMut` for the wrapped content widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget + ?Sized> SidebarContent<W> {
    pub(crate) fn set_collapsed(this: &mut WidgetMut<'_, Self>, collapsed: bool) {
        if this.widget.collapsed != collapsed {
            this.widget.collapsed = collapsed;
            let target: f32 = if collapsed { 1.0 } else { 0.0 };
            if (target - this.widget.collapse_progress).abs() > 1e-4 {
                this.ctx.request_anim_frame();
            }
        }
    }

    fn animated_width(&self) -> f64 {
        (self.natural_width * f64::from(1.0 - self.collapse_progress)).max(0.0)
    }
}

impl<W: Widget + ?Sized> Widget for SidebarContent<W> {
    type Action = NoAction;

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let target: f32 = if self.collapsed { 1.0 } else { 0.0 };
        let ms = u16::try_from(interval / 1_000_000).unwrap_or(u16::MAX);
        let delta = f32::from(ms) / SLIDE_MILLIS;
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

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
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
        let child_length = ctx.compute_length(
            &mut self.child,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        );
        if axis == Axis::Horizontal {
            let natural = child_length.get();
            if natural > 0.0 {
                self.natural_width = natural;
            }
            Length::px(self.animated_width())
        } else {
            child_length
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // Always lay out the child at full natural width so content doesn't
        // reflow during the slide animation.
        let child_width = self.natural_width.max(size.width);
        let child_size = Size::new(child_width, size.height);
        ctx.run_layout(&mut self.child, child_size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        // Clip to the currently animated width so content slides off-screen.
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

// --- MARK: ThemedSidebarPanel

/// Sidebar container that animates its content width and renders a persistent
/// toggle strip on its right edge.
///
/// The strip always occupies [`STRIP_WIDTH`] pixels; the content area
/// transitions between 0 and its natural width when `collapsed` changes.
///
/// Emits [`SidebarTogglePressed`] when the strip is clicked.
pub struct ThemedSidebarPanel<W: Widget + ?Sized> {
    content: WidgetPod<SidebarContent<W>>,
    chevron: WidgetPod<Label>,
    theme: Theme,
    collapsed: bool,
    /// True while the pointer is inside the strip area.
    strip_hovered: bool,
    /// True while the strip is being pressed.
    strip_pressed: bool,
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
            content: WidgetPod::new(SidebarContent::new(child, collapsed)),
            chevron: make_chevron(collapsed, theme).to_pod(),
            theme: *theme,
            collapsed,
            strip_hovered: false,
            strip_pressed: false,
            current_strip_x: 0.0,
            current_height: 0.0,
        }
    }
}

// --- MARK: WIDGETMUT
impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    pub fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, SidebarContent<W>> {
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
            this.ctx.request_paint_only();
        }
    }

    pub fn set_collapsed(this: &mut WidgetMut<'_, Self>, collapsed: bool) {
        if this.widget.collapsed != collapsed {
            this.widget.collapsed = collapsed;
            this.ctx.request_paint_only();
            let new_icon = if collapsed {
                LucideIcon::ChevronRight
            } else {
                LucideIcon::ChevronLeft
            };
            let new_char = ArcStr::from(String::from(char::from(new_icon)));
            {
                let mut chevron = this.ctx.get_mut(&mut this.widget.chevron);
                Label::set_text(&mut chevron, new_char);
            }
            let mut content = this.ctx.get_mut(&mut this.widget.content);
            SidebarContent::set_collapsed(&mut content, collapsed);
        }
    }
}

// --- MARK: STRIP PAINT STATE
impl<W: Widget + ?Sized> ThemedSidebarPanel<W> {
    fn strip_bg(&self) -> Color {
        let p = &self.theme.palette;
        if self.strip_pressed && self.strip_hovered {
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
        match event {
            PointerEvent::Move(update) => {
                let pos = ctx.local_position(update.current.position);
                let in_strip = pos.x >= self.current_strip_x;
                if in_strip != self.strip_hovered {
                    self.strip_hovered = in_strip;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if pos.x >= self.current_strip_x {
                    self.strip_pressed = true;
                    ctx.capture_pointer();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) if self.strip_pressed => {
                if ctx.is_hovered() && self.strip_hovered {
                    ctx.submit_action::<Self::Action>(SidebarTogglePressed);
                }
                self.strip_pressed = false;
                ctx.request_paint_only();
            }
            PointerEvent::Leave(_) if self.strip_hovered => {
                self.strip_hovered = false;
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
            && self.strip_hovered
        {
            self.strip_hovered = false;
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
        _ctx: &mut PaintCtx<'_>,
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
        let ids = [self.content.id(), self.chevron.id()];
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
        tracing::trace_span!("ThemedSidebarPanel", id = id.trace())
    }
}
