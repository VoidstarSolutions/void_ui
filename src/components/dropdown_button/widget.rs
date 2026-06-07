//! `ThemedDropdownButton` — button with trailing chevron that opens a floating menu.
//!
//! Visually and behaviourally identical to `ThemedButton` with a `trailing_icon`,
//! except that clicking anywhere on the button (label area or chevron) toggles the
//! floating [`crate::popover_layer::PopoverLayer`] instead of firing a primary action. There is no split-zone
//! concept: the whole surface is one click target.

use std::sync::{Arc, LazyLock};

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty,
    Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, BezPath, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::menu_layer::{MenuContent, MenuItemSelected};
use crate::AnchoredOverlay;
use crate::Theme;
use crate::components::button::ButtonVariant;
use crate::components::button::widget::CORNER_RADIUS;
use crate::components::click::{self, ClickPhase};
use crate::components::popover::PopoverAnchor;

const FOCUS_RING_WIDTH: f64 = 1.5;
const FOCUS_RING_INSET: f64 = 2.0;
const BORDER_WIDTH: f64 = 1.0;
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
#[derive(Debug)]
pub enum DropdownButtonAction {
    /// Menu item at `index` was selected.
    ItemSelected(usize),
}

/// Button widget that opens a floating dropdown menu on click.
///
/// Renders as a standard button with a trailing chevron icon. Clicking anywhere
/// on the button (label area or chevron) toggles the menu layer.
pub struct ThemedDropdownButton {
    overlay_host: WidgetPod<AnchoredOverlay>,
    icon: Option<Arc<BezPath>>,
    items: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    theme: Theme,
    pub(super) open: bool,
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
        let overlay_host = AnchoredOverlay::new(
            lbl.erased(),
            NewWidget::new(MenuContent::new(items.clone(), theme)),
            false,
            PopoverAnchor::BottomStart,
        );
        Self {
            overlay_host: NewWidget::new(overlay_host).to_pod(),
            icon,
            items,
            variant,
            disabled,
            theme: *theme,
            open: false,
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

            let text_color =
                Self::text_color_for(theme, this.widget.variant, this.widget.disabled);
            {
                let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
                let mut lbl = AnchoredOverlay::primary_mut(&mut overlay_host);
                lbl.insert_prop(ContentColor::new(text_color));
                let mut lbl = lbl.downcast::<Label>();
                Label::insert_style(
                    &mut lbl,
                    StyleProperty::FontSize(theme.density.ui_font_size),
                );
            }
            {
                let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
                let mut menu = AnchoredOverlay::overlay_mut(&mut overlay_host);
                let mut menu = menu.downcast::<MenuContent>();
                MenuContent::set_theme(&mut menu, theme);
            }
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
        this.widget.items.clone_from(&items);
        let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
        let mut menu = AnchoredOverlay::overlay_mut(&mut overlay_host);
        let mut menu = menu.downcast::<MenuContent>();
        MenuContent::set_items(&mut menu, items);
    }

    pub fn set_icon(this: &mut WidgetMut<'_, Self>, icon: Option<Arc<BezPath>>) {
        this.widget.icon = icon;
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }
}

// --- MARK: INTERNAL HELPERS
impl ThemedDropdownButton {
    fn icon_size(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
    }

    fn trailing_icon_width(&self) -> f64 {
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

    fn set_overlay_visible(&mut self, ctx: &mut EventCtx<'_>, visible: bool) {
        ctx.mutate_child_later(&mut self.overlay_host, move |mut w| {
            AnchoredOverlay::set_overlay_visible(&mut w, visible);
        });
    }

    fn open_dropdown(&mut self, ctx: &mut EventCtx<'_>) {
        self.open = true;
        self.set_overlay_visible(ctx, true);
        ctx.request_paint_only();
    }

    fn close_dropdown(&mut self, ctx: &mut EventCtx<'_>) {
        self.open = false;
        self.set_overlay_visible(ctx, false);
        ctx.request_paint_only();
    }

    fn toggle_dropdown(&mut self, ctx: &mut EventCtx<'_>) {
        if self.open {
            self.close_dropdown(ctx);
        } else {
            self.open_dropdown(ctx);
        }
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
        // While open, `MenuContent` (now a real descendant — see the removed
        // `propagates_pointer_interaction` override) handles its own pointer
        // events, which then bubble up here. `is_hovered` — "the pointer is
        // directly over *my* border box," not a descendant's — is `false` for
        // those bubbled events (the menu can legally extend outside our box
        // via `AnchoredOverlay`'s unclamped placement), so this skips them and
        // leaves `MenuContent` to handle its own clicks/selection untouched.
        if self.open && !ctx.is_hovered() {
            return;
        }
        match click::primary_click(ctx, event) {
            Some(ClickPhase::Down) => {
                ctx.request_focus();
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up(Some(_))) => {
                self.toggle_dropdown(ctx);
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up(None)) => ctx.request_paint_only(),
            None => {}
        }
    }

    /// Handles [`MenuItemSelected`] bubbling up from `MenuContent` (nested
    /// inside `overlay_host`): closes the dropdown and re-emits the selection
    /// as our own [`DropdownButtonAction::ItemSelected`].
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        if let Some(&MenuItemSelected(index)) = action.downcast_ref::<MenuItemSelected>() {
            self.open = false;
            ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                AnchoredOverlay::set_overlay_visible(&mut w, false);
            });
            ctx.submit_action::<Self::Action>(DropdownButtonAction::ItemSelected(index));
            ctx.set_handled();
            ctx.request_paint_only();
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
            self.toggle_dropdown(ctx);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
            }
            // A click landing outside our subtree clears our focus — masonry's
            // standard "click outside to dismiss" signal. This only works
            // because the menu remains a *descendant*: clicks on the trigger
            // or inside the menu keep our subtree focused (see
            // `event.rs::226-235` for the ancestor-chain check), so the menu
            // only closes for genuine outside clicks.
            Update::FocusChanged(false) if self.open => {
                self.open = false;
                ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                });
                ctx.request_paint_only();
            }
            Update::HoveredChanged(_)
            | Update::ActiveChanged(_)
            | Update::DisabledChanged(_)
            | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.overlay_host);
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
        let child_length = ctx.compute_length(
            &mut self.overlay_host,
            len_req.into(),
            LayoutSize::maybe(axis.cross(), inner_cross),
            axis,
            inner_cross,
        );

        let icon_extra = if self.icon.is_some() && axis == Axis::Horizontal {
            self.icon_size() + ICON_GAP
        } else {
            0.0
        };
        // Trailing chevron always occupies its fixed slot on the horizontal axis.
        let chevron_extra = if axis == Axis::Horizontal {
            self.trailing_icon_width()
        } else {
            0.0
        };

        Length::px(child_length.get() + main_pad + icon_extra + chevron_extra)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_v = self.pad_v();
        let pad_h = self.pad_h();
        let icon_base = if self.icon.is_some() {
            self.icon_size()
        } else {
            0.0
        };
        let icon_gap = if self.icon.is_some() { ICON_GAP } else { 0.0 };
        let trailing_w = self.trailing_icon_width();

        let label_inner = Size::new(
            (size.width - 2.0 * pad_h - icon_base - icon_gap - trailing_w).max(0.0),
            (size.height - 2.0 * pad_v).max(0.0),
        );
        let label_size = ctx.compute_size(
            &mut self.overlay_host,
            SizeDef::fit(label_inner),
            label_inner.into(),
        );
        ctx.run_layout(&mut self.overlay_host, label_size);

        let label_x = pad_h + icon_base + icon_gap;
        let label_y = pad_v + ((label_inner.height - label_size.height) * 0.5).max(0.0);
        ctx.place_child(&mut self.overlay_host, Point::new(label_x, label_y));
        ctx.derive_baselines(&self.overlay_host);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let focused = ctx.is_focus_target();
        let hovered = ctx.has_hovered();
        let pressed = ctx.has_active() && hovered;
        let p = &self.theme.palette;
        let icon_size = self.icon_size();
        let pad_h = self.pad_h();

        // When the dropdown is open, show the button in its hover state so it's
        // visually clear that it's active.
        let (bg, border) = self.resolve_colors(hovered || self.open, pressed);

        let rect = RoundedRect::from_origin_size(Point::ORIGIN, size, CORNER_RADIUS);
        if bg.components[3] > 0.0 {
            painter.fill(rect, bg).draw();
        }
        if border.components[3] > 0.0 {
            painter
                .stroke(rect, &Stroke::new(BORDER_WIDTH), border)
                .draw();
        }

        if focused && !self.disabled {
            let inset = FOCUS_RING_INSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                CORNER_RADIUS - inset,
            );
            painter
                .stroke(focus_rect, &Stroke::new(FOCUS_RING_WIDTH), p.teal)
                .draw();
        }

        let icon_color = if self.disabled { p.text_faint } else { p.text };

        if let Some(icon) = &self.icon {
            let icon_y = (size.height - icon_size) * 0.5;
            let transform = Affine::translate((pad_h, icon_y)) * Affine::scale(icon_size);
            painter.fill(transform * icon.as_ref(), icon_color).draw();
        }

        // Trailing chevron — same placement as ThemedButton's trailing_icon.
        let caret_x = size.width - pad_h - icon_size;
        let caret_y = (size.height - icon_size) * 0.5;
        let transform = Affine::translate((caret_x, caret_y)) * Affine::scale(icon_size);
        painter
            .stroke(transform * &**CARET_PATH, &Stroke::new(1.5), icon_color)
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
            node.add_action(masonry::accesskit::Action::Click);
        }
        node.add_action(masonry::accesskit::Action::Expand);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.overlay_host.id()])
    }

    fn accepts_focus(&self) -> bool {
        !self.disabled
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}
