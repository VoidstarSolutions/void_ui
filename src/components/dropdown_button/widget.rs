//! `ThemedDropdownButton` — split-button masonry widget.
//!
//! A single widget that renders a main action zone (left) and a chevron
//! toggle zone (right). All pointer interaction is handled at this level
//! without child button pods, which avoids xilem action-routing ambiguity.
//!
//! When the chevron is clicked, the widget creates a [`DropdownMenuLayer`]
//! window-level layer via [`EventCtx::create_layer`] so the menu floats
//! above all other content. The layer communicates item selection back via
//! [`EventCtx::mutate_later`], which calls [`EventCtx::submit_action`] on
//! this widget's context so the action bubbles to the registered xilem view.

use std::sync::{Arc, LazyLock};

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{
    Affine, Axis, BezPath, Point, RoundedRect, RoundedRectRadii, Size, Stroke, Vec2,
};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::menu_layer::DropdownMenuLayer;
use crate::Theme;
use crate::components::button::ButtonVariant;
use crate::components::button::widget::CORNER_RADIUS;

/// Corner radius for both button zones (matches [`CORNER_RADIUS`]).
const FOCUS_RING_WIDTH: f64 = 1.5;
const FOCUS_RING_INSET: f64 = 2.0;
const BORDER_WIDTH: f64 = 1.0;
const DIVIDER_WIDTH: f64 = 1.0;
const ICON_GAP: f64 = 5.0;

/// Down-pointing caret icon in unit-square (0..1) space.
static CARET_PATH: LazyLock<Arc<BezPath>> = LazyLock::new(|| {
    let mut p = BezPath::new();
    p.move_to((0.2, 0.35));
    p.line_to((0.5, 0.65));
    p.line_to((0.8, 0.35));
    Arc::new(p)
});

/// Action type emitted by [`ThemedDropdownButton`].
///
/// Both variants route to the owning [`super::view::DropdownButtonView`] via
/// xilem's message dispatch.
#[derive(Debug)]
pub enum DropdownButtonAction {
    /// The primary (main-zone) button was pressed.
    MainPressed,
    /// Menu item at `index` was selected.
    ItemSelected(usize),
}

/// Which split zone the current click started in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Zone {
    Main,
    Chevron,
}

/// Themed split-button widget — main action zone + chevron dropdown toggle.
pub struct ThemedDropdownButton {
    label: WidgetPod<dyn Widget>,
    icon: Option<Arc<BezPath>>,
    items: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    // Open/close state
    pub(super) open: bool,
    pub(super) menu_layer_id: Option<WidgetId>,
    // Per-zone interaction tracking (None = not hovered)
    hover_zone: Option<Zone>,
    click_zone: Option<Zone>,
}

// --- MARK: BUILDERS
impl ThemedDropdownButton {
    #[must_use]
    pub fn new(
        label_text: ArcStr,
        icon: Option<Arc<BezPath>>,
        items: Vec<ArcStr>,
        variant: ButtonVariant,
        disabled: bool,
        theme: &Theme,
    ) -> Self {
        let text_color = Self::text_color_for(theme, variant, disabled);
        let mut lbl = Label::new(label_text)
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties.insert(ContentColor::new(text_color));
        Self {
            label: lbl.erased().to_pod(),
            icon,
            items,
            variant,
            disabled,
            theme: *theme,
            open: false,
            menu_layer_id: None,
            hover_zone: None,
            click_zone: None,
        }
    }

    fn text_color_for(theme: &Theme, variant: ButtonVariant, disabled: bool) -> Color {
        if disabled {
            theme.palette.text_faint
        } else if variant == ButtonVariant::Link {
            theme.palette.teal
        } else {
            theme.palette.text
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl ThemedDropdownButton {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: ButtonVariant) {
        if this.widget.variant != variant {
            this.widget.variant = variant;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_items(this: &mut WidgetMut<'_, Self>, items: Vec<ArcStr>) {
        this.widget.items = items;
    }

    pub fn set_icon(this: &mut WidgetMut<'_, Self>, icon: Option<Arc<BezPath>>) {
        this.widget.icon = icon;
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }

    pub fn label_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.label)
    }
}

// --- MARK: INTERNAL HELPERS
impl ThemedDropdownButton {
    fn icon_size(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
    }

    fn chevron_zone_width(&self) -> f64 {
        2.0 * f64::from(self.theme.density.button_pad_h) + self.icon_size()
    }

    fn pad_v(&self) -> f64 {
        f64::from(self.theme.density.button_pad_v)
    }

    fn pad_h(&self) -> f64 {
        f64::from(self.theme.density.button_pad_h)
    }

    fn resolve_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (Color::TRANSPARENT, Color::TRANSPARENT);
        }
        match self.variant {
            ButtonVariant::Default => {
                let bg = if pressed {
                    p.surface_hi
                } else if hovered {
                    p.surface_2
                } else {
                    p.surface
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Danger => {
                let bg = if pressed {
                    p.coral_deep
                } else if hovered {
                    p.coral
                } else {
                    p.coral_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Primary => {
                let bg = if pressed {
                    p.teal_deep
                } else if hovered {
                    p.teal
                } else {
                    p.teal_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Warning => {
                let bg = if pressed {
                    p.amber_deep
                } else if hovered {
                    p.amber
                } else {
                    p.amber_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Secondary => {
                let bg = if pressed {
                    p.violet_deep
                } else if hovered {
                    p.violet
                } else {
                    p.violet_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Success => {
                let bg = if pressed {
                    p.green_deep
                } else if hovered {
                    p.green
                } else {
                    p.green_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Info => {
                let bg = if pressed {
                    p.blue_deep
                } else if hovered {
                    p.blue
                } else {
                    p.blue_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Ghost => {
                let bg = if pressed {
                    p.surface_hi
                } else if hovered {
                    p.surface_2
                } else {
                    Color::TRANSPARENT
                };
                let border = if hovered { p.border_strong } else { p.border };
                (bg, border)
            }
            ButtonVariant::Link => (Color::TRANSPARENT, Color::TRANSPARENT),
            ButtonVariant::Text => {
                let bg = if pressed {
                    p.surface_hi
                } else {
                    Color::TRANSPARENT
                };
                (bg, Color::TRANSPARENT)
            }
        }
    }

    fn zone_at(&self, pos: Point, widget_width: f64) -> Zone {
        if pos.x < widget_width - self.chevron_zone_width() {
            Zone::Main
        } else {
            Zone::Chevron
        }
    }

    fn open_dropdown(&mut self, ctx: &mut EventCtx<'_>) {
        let menu_widget =
            NewWidget::new(DropdownMenuLayer::new(self.items.clone(), ctx.widget_id(), &self.theme));
        let layer_id = menu_widget.id();
        let border_box = ctx.border_box();
        let pos =
            ctx.to_window(border_box.origin()) + Vec2::new(0.0, border_box.size().height);
        ctx.create_layer(LayerType::Other, menu_widget, pos);
        self.menu_layer_id = Some(layer_id);
        self.open = true;
    }

    fn close_dropdown(&mut self, ctx: &mut EventCtx<'_>) {
        if let Some(id) = self.menu_layer_id.take() {
            ctx.remove_layer(id);
        }
        self.open = false;
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedDropdownButton {
    type Action = DropdownButtonAction;

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
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let pos = current.logical_point();
                let origin = ctx.to_window(Point::ZERO);
                let local_pos = pos - origin.to_vec2();
                let width = ctx.border_box().size().width;
                let new_zone = Some(self.zone_at(local_pos, width));
                if self.hover_zone != new_zone {
                    self.hover_zone = new_zone;
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Leave(_) if self.hover_zone.is_some() => {
                self.hover_zone = None;
                ctx.request_paint_only();
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                ctx.capture_pointer();
                let pos = state.logical_point();
                let origin = ctx.to_window(Point::ZERO);
                let local_pos = pos - origin.to_vec2();
                let width = ctx.border_box().size().width;
                self.click_zone = Some(self.zone_at(local_pos, width));
                ctx.request_focus();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state: _,
                ..
            }) => {
                if ctx.is_active() && ctx.is_hovered() {
                    if let Some(zone) = self.click_zone.take() {
                        match zone {
                            Zone::Main => {
                                ctx.submit_action::<Self::Action>(DropdownButtonAction::MainPressed);
                            }
                            Zone::Chevron => {
                                if self.open {
                                    self.close_dropdown(ctx);
                                } else {
                                    self.open_dropdown(ctx);
                                }
                            }
                        }
                    }
                } else {
                    self.click_zone = None;
                }
                ctx.request_paint_only();
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
        if self.disabled {
            return;
        }
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.submit_action::<Self::Action>(DropdownButtonAction::MainPressed);
        }
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &Update,
    ) {
        match event {
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
            }
            Update::HoveredChanged(_) | Update::DisabledChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.label);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let pad_v = self.pad_v();
        let pad_h = self.pad_h();
        let (main_pad, cross_pad) = match axis {
            Axis::Horizontal => (2.0 * pad_h, 2.0 * pad_v),
            Axis::Vertical => (2.0 * pad_v, 2.0 * pad_h),
        };
        let inner_cross = cross_length.map(|c| Length::px((c.get() - cross_pad).max(0.0)));
        let auto_length = len_req.into();
        let context_size = LayoutSize::maybe(axis.cross(), inner_cross);
        let child_length = ctx.compute_length(
            &mut self.label,
            auto_length,
            context_size,
            axis,
            inner_cross,
        );

        let icon_extra = if self.icon.is_some() && axis == Axis::Horizontal {
            self.icon_size() + ICON_GAP
        } else {
            0.0
        };
        let chevron_extra = if axis == Axis::Horizontal {
            self.chevron_zone_width()
        } else {
            0.0
        };

        Length::px(child_length.get() + main_pad + icon_extra + chevron_extra)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_v = self.pad_v();
        let pad_h = self.pad_h();
        let chevron_w = self.chevron_zone_width();
        let icon_base = if self.icon.is_some() {
            self.icon_size()
        } else {
            0.0
        };

        let main_zone_w = (size.width - chevron_w).max(0.0);
        let label_inner = Size::new(
            (main_zone_w - 2.0 * pad_h - icon_base - if self.icon.is_some() { ICON_GAP } else { 0.0 }).max(0.0),
            (size.height - 2.0 * pad_v).max(0.0),
        );
        let label_size = ctx.compute_size(&mut self.label, SizeDef::fit(label_inner), label_inner.into());
        ctx.run_layout(&mut self.label, label_size);

        let icon_x_offset = if icon_base > 0.0 { icon_base + ICON_GAP } else { 0.0 };
        let label_x = pad_h + icon_x_offset;
        let label_y = pad_v + ((label_inner.height - label_size.height) * 0.5).max(0.0);
        ctx.place_child(&mut self.label, Point::new(label_x, label_y));
        ctx.derive_baselines(&self.label);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let focused = ctx.is_focus_target();
        let active = ctx.is_active();
        let p = &self.theme.palette;
        let icon_size = self.icon_size();
        let pad_h = self.pad_h();
        let chevron_w = self.chevron_zone_width();
        let main_zone_w = (size.width - chevron_w).max(0.0);

        let main_pressed = active && self.click_zone == Some(Zone::Main);
        let chevron_pressed = active && self.click_zone == Some(Zone::Chevron);

        let (main_bg, main_border) =
            self.resolve_colors(self.hover_zone == Some(Zone::Main), main_pressed);
        let (chev_bg, chev_border) =
            self.resolve_colors(self.hover_zone == Some(Zone::Chevron), chevron_pressed || self.open);

        let r = CORNER_RADIUS;

        // Main zone — rounded left, square right
        let main_rect = RoundedRect::new(
            0.0, 0.0, main_zone_w, size.height,
            RoundedRectRadii::new(r, 0.0, 0.0, r),
        );
        if main_bg.components[3] > 0.0 {
            painter.fill(main_rect, main_bg).draw();
        }
        if main_border.components[3] > 0.0 {
            painter.stroke(main_rect, &Stroke::new(BORDER_WIDTH), main_border).draw();
        }

        // Chevron zone — square left, rounded right
        let chev_rect = RoundedRect::new(
            main_zone_w, 0.0, size.width, size.height,
            RoundedRectRadii::new(0.0, r, r, 0.0),
        );
        if chev_bg.components[3] > 0.0 {
            painter.fill(chev_rect, chev_bg).draw();
        }
        if chev_border.components[3] > 0.0 {
            painter.stroke(chev_rect, &Stroke::new(BORDER_WIDTH), chev_border).draw();
        }

        // Divider between zones
        let divider_color = if self.disabled { p.border } else { p.border_strong };
        let mut div = BezPath::new();
        div.move_to(Point::new(main_zone_w, 4.0));
        div.line_to(Point::new(main_zone_w, size.height - 4.0));
        painter.stroke(&div, &Stroke::new(DIVIDER_WIDTH), divider_color).draw();

        // Focus ring around the whole button
        if focused && !self.disabled {
            let inset = FOCUS_RING_INSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                r - inset,
            );
            painter
                .stroke(focus_rect, &Stroke::new(FOCUS_RING_WIDTH), p.teal)
                .draw();
        }

        // Leading icon in main zone
        let icon_color = if self.disabled { p.text_faint } else { p.text };
        if let Some(icon) = &self.icon {
            let icon_y = (size.height - icon_size) * 0.5;
            let transform = Affine::translate((pad_h, icon_y)) * Affine::scale(icon_size);
            painter.fill(transform * icon.as_ref(), icon_color).draw();
        }

        // Chevron icon — centered in chevron zone
        let caret = &**CARET_PATH;
        let caret_y = (size.height - icon_size) * 0.5;
        let caret_x = main_zone_w + (chevron_w - icon_size) * 0.5;
        let transform = Affine::translate((caret_x, caret_y)) * Affine::scale(icon_size);
        let caret_color = if self.disabled { p.text_faint } else { p.text };
        painter.stroke(transform * caret, &Stroke::new(1.5), caret_color).draw();
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
            node.add_action(masonry::accesskit::Action::Click);
        }
        node.add_action(masonry::accesskit::Action::Expand);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.label.id()])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        !self.disabled
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
