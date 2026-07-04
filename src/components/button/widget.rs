//! Masonry widget that owns the Tessera `.tb-btn` state machine.
//!
//! The widget tracks pointer hover, press, and focus through masonry's widget
//! context, then paints background + border itself from a `Theme` it holds.
//! Going through the property-stack system would require reaching into
//! `PropertyArena` from a xilem `View`, which is not exposed today; painting
//! directly keeps `Theme` as a value flowing through `app_logic` rather than
//! global state.
//!
//! Emits [`ButtonPress`] (the same action type as `masonry::widgets::Button`)
//! on primary-pointer release inside the widget and on Space/Enter while
//! focused.

use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent,
    Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, RoundedRectRadii, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::peniko::Color;
use masonry::widgets::{ButtonPress, Label};

use super::ButtonVariant;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::components::spinner::widget::SpinnerWidget;

/// Corner radius (`border-radius: 5px`).
///
/// Single source of truth for the button corner radius — the view-layer
/// fallback and the button-group outer corners both reference this.
pub(crate) const CORNER_RADIUS: f64 = 5.0;
/// Border thickness for the active and focus states.
const BORDER_WIDTH: f64 = 1.0;
/// Inset of the focus ring from the button edge.
const FOCUS_RING_INSET: f64 = 2.0;
/// Gap between a leading icon and the label.
const ICON_GAP: f64 = 5.0;

/// Themed, interactive button widget.
///
/// Owns its child (typically a `Label`) and a [`Theme`] value used to
/// resolve background / border / text colors at paint time. The host drives
/// the `active` flag for "currently-selected toggle" semantics. Pointer state
/// (hovered, pressed) is read from the widget context.
pub struct ThemedButton {
    child: WidgetPod<dyn Widget>,
    theme: Theme,
    /// Host-controlled toggle — Tessera's `.tb-btn.active`.
    active: bool,
    /// When true, all interaction is suppressed and colors are muted.
    disabled: bool,
    /// Visual style variant.
    variant: ButtonVariant,
    /// Optional leading icon rendered as a `Label` child using the Lucide font.
    icon: Option<WidgetPod<Label>>,
    /// Optional trailing icon (e.g. a dropdown caret).
    trailing_icon: Option<WidgetPod<Label>>,
    /// When true, shows a spinner and blocks all interaction.
    loading: bool,
    /// Child widget pod for the spinner, present when `loading` is true.
    spinner: Option<WidgetPod<SpinnerWidget>>,
    /// Explicit accessibility label for icon-only buttons.
    accessibility_label: Option<ArcStr>,
    /// When set, written to the system clipboard whenever the button fires.
    clipboard_payload: Option<ArcStr>,
    /// Per-corner radii for the background and focus-ring shapes.
    /// Defaults to a uniform `CORNER_RADIUS`; button groups override this to
    /// round only the outer edges of the group.
    corners: RoundedRectRadii,
}

// --- MARK: BUILDERS
impl ThemedButton {
    /// Creates a new themed button with the supplied child and theme.
    ///
    /// The child should be a non-interactive widget — typically a `Label`. An
    /// interactive child would steal pointer capture from the button.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, theme: &Theme) -> Self {
        Self {
            child: child.erased().to_pod(),
            theme: *theme,
            active: false,
            disabled: false,
            variant: ButtonVariant::Default,
            icon: None,
            trailing_icon: None,
            loading: false,
            spinner: None,
            accessibility_label: None,
            clipboard_payload: None,
            corners: RoundedRectRadii::from_single_radius(CORNER_RADIUS),
        }
    }

    /// Marks the button as the currently-selected toggle.
    #[must_use]
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Suppresses all interaction and mutes visual appearance.
    #[must_use]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Sets the visual style variant.
    #[must_use]
    pub fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Attaches a leading icon widget (a Lucide-font `Label`).
    #[must_use]
    pub fn with_icon(mut self, icon: NewWidget<Label>) -> Self {
        self.icon = Some(icon.to_pod());
        self
    }

    /// Attaches a trailing icon widget (e.g. a dropdown caret).
    #[must_use]
    pub fn with_trailing_icon(mut self, icon: NewWidget<Label>) -> Self {
        self.trailing_icon = Some(icon.to_pod());
        self
    }

    /// Shows a spinner and blocks all interaction.
    #[must_use]
    pub fn with_loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        if loading {
            let icon_size = f64::from(self.theme.density.ui_font_size);
            self.spinner = Some(WidgetPod::new(SpinnerWidget::new(
                self.theme.palette.text_muted,
                icon_size,
            )));
        } else {
            self.spinner = None;
        }
        self
    }

    /// Sets an explicit accessibility label (required for icon-only buttons).
    #[must_use]
    pub fn with_accessibility_label(mut self, name: Option<ArcStr>) -> Self {
        self.accessibility_label = name;
        self
    }

    /// Sets a string to write to the system clipboard whenever the button fires.
    #[must_use]
    pub fn with_clipboard_payload(mut self, payload: Option<ArcStr>) -> Self {
        self.clipboard_payload = payload;
        self
    }

    /// Overrides the corner radii for the button background and focus ring.
    ///
    /// Used by button groups to round only the outer edges of the group.
    #[must_use]
    pub fn with_corners(mut self, corners: RoundedRectRadii) -> Self {
        self.corners = corners;
        self
    }
}

// --- MARK: WIDGETMUT
impl ThemedButton {
    /// Replaces the theme. Requests layout + repaint if the value changed.
    /// Icon style updates are handled by the view layer via [`Self::icon_mut`].
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Toggles the host-driven `active` flag. Requests a repaint on change.
    pub fn set_active(this: &mut WidgetMut<'_, Self>, active: bool) {
        if this.widget.active != active {
            this.widget.active = active;
            this.ctx.request_paint_only();
        }
    }

    /// Sets the disabled state. Propagates to masonry's system-level
    /// disabled flag (for event routing + accessibility) and requests a repaint.
    /// Icon color updates on disabled change are handled by the view layer via
    /// [`Self::icon_mut`] / [`Self::trailing_icon_mut`].
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled != disabled {
            this.widget.disabled = disabled;
            this.ctx.set_disabled(disabled);
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the visual variant. Requests a repaint on change.
    pub fn set_variant(this: &mut WidgetMut<'_, Self>, variant: ButtonVariant) {
        if this.widget.variant != variant {
            this.widget.variant = variant;
            this.ctx.request_paint_only();
        }
    }

    /// Replaces the leading icon child. Removes any existing icon first.
    pub fn attach_icon(this: &mut WidgetMut<'_, Self>, icon: NewWidget<Label>) {
        if let Some(old) = this.widget.icon.take() {
            this.ctx.remove_child(old);
        }
        this.widget.icon = Some(icon.to_pod());
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Removes the leading icon child.
    pub fn detach_icon(this: &mut WidgetMut<'_, Self>) {
        if let Some(old) = this.widget.icon.take() {
            this.ctx.remove_child(old);
            this.ctx.children_changed();
            this.ctx.request_layout();
        }
    }

    /// Replaces the trailing icon child. Removes any existing trailing icon first.
    pub fn attach_trailing_icon(this: &mut WidgetMut<'_, Self>, icon: NewWidget<Label>) {
        if let Some(old) = this.widget.trailing_icon.take() {
            this.ctx.remove_child(old);
        }
        this.widget.trailing_icon = Some(icon.to_pod());
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Removes the trailing icon child.
    pub fn detach_trailing_icon(this: &mut WidgetMut<'_, Self>) {
        if let Some(old) = this.widget.trailing_icon.take() {
            this.ctx.remove_child(old);
            this.ctx.children_changed();
            this.ctx.request_layout();
        }
    }

    /// Returns a mutable reference to the leading icon label, if present.
    pub fn icon_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> Option<WidgetMut<'t, Label>> {
        this.widget.icon.as_mut().map(|p| this.ctx.get_mut(p))
    }

    /// Returns a mutable reference to the trailing icon label, if present.
    pub fn trailing_icon_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> Option<WidgetMut<'t, Label>> {
        this.widget
            .trailing_icon
            .as_mut()
            .map(|p| this.ctx.get_mut(p))
    }

    /// Sets the loading state. Attaches or removes the spinner child pod on change.
    pub fn set_loading(this: &mut WidgetMut<'_, Self>, loading: bool) {
        if this.widget.loading != loading {
            this.widget.loading = loading;
            if loading {
                let icon_size = f64::from(this.widget.theme.density.ui_font_size);
                let color = this.widget.theme.palette.text_muted;
                this.widget.spinner = Some(WidgetPod::new(SpinnerWidget::new(color, icon_size)));
                this.ctx.children_changed();
            } else if let Some(old) = this.widget.spinner.take() {
                this.ctx.remove_child(old);
                this.ctx.children_changed();
            }
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the spinner widget, if loading.
    pub fn spinner_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> Option<WidgetMut<'t, SpinnerWidget>> {
        this.widget.spinner.as_mut().map(|p| this.ctx.get_mut(p))
    }

    /// Updates the accessibility label. Requests an accessibility update on change.
    pub fn set_accessibility_label(this: &mut WidgetMut<'_, Self>, name: Option<ArcStr>) {
        if this.widget.accessibility_label != name {
            this.widget.accessibility_label = name;
            this.ctx.request_accessibility_update();
        }
    }

    /// Updates the clipboard payload. No visual side effects — only matters at event time.
    pub fn set_clipboard_payload(this: &mut WidgetMut<'_, Self>, payload: Option<ArcStr>) {
        this.widget.clipboard_payload = payload;
    }

    /// Replaces the corner radii. Requests a repaint on change.
    pub fn set_corners(this: &mut WidgetMut<'_, Self>, corners: RoundedRectRadii) {
        if this.widget.corners != corners {
            this.widget.corners = corners;
            this.ctx.request_paint_only();
        }
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

// --- MARK: PAINT STATE
impl ThemedButton {
    fn icon_size(&self) -> f64 {
        f64::from(self.theme.density.ui_font_size)
    }

    /// Resolves `(background, border)` colors for the current state.
    ///
    /// | state           | Default      | Danger       | Primary    | Ghost        | Warning      |
    /// |-----------------|--------------|--------------|------------|--------------|--------------|
    /// | disabled        | transparent  | transparent  | transparent| transparent  | transparent  |
    /// | rest            | transparent  | transparent  | teal_soft  | transparent/border| transparent |
    /// | hover           | `surface_2`  | `coral_soft` | `teal`     | `surface_2`  | `amber_soft` |
    /// | pressed         | `surface_hi` | `coral`      | `teal`     | `surface_hi` | `amber`      |
    /// | active (toggle) | `surface_2`  | `coral_soft` | `teal_soft`| `surface_2`  | `amber_soft` |
    fn resolve_colors(&self, hovered: bool, pressed: bool) -> (Color, Color) {
        let p = &self.theme.palette;
        if self.disabled {
            return (Color::TRANSPARENT, Color::TRANSPARENT);
        }
        match self.variant {
            ButtonVariant::Default => {
                let bg = if pressed {
                    p.surface_hi
                } else if self.active || hovered {
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
            // Ghost: always-visible border, fill on hover/press/active.
            ButtonVariant::Ghost => {
                let bg = if pressed {
                    p.surface_hi
                } else if self.active || hovered {
                    p.surface_2
                } else {
                    Color::TRANSPARENT
                };
                let border = if self.active || hovered {
                    p.border_strong
                } else {
                    p.border
                };
                (bg, border)
            }
            // Link: no background or border.
            ButtonVariant::Link => (Color::TRANSPARENT, Color::TRANSPARENT),
            // Text: subtle press feedback only — no hover fill, no border.
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
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedButton {
    type Action = ButtonPress;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled || self.loading {
            return;
        }
        // Shared Down→capture / Up-iff-active-and-hovered recognizer
        // (drag out of the button to cancel the press).
        match click::primary_click(ctx, event) {
            Some(ClickPhase::Down) => {
                ctx.request_focus();
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up(completed)) => {
                if completed.is_some() {
                    if let Some(payload) = &self.clipboard_payload {
                        ctx.set_clipboard(payload.to_string());
                    }
                    ctx.submit_action::<Self::Action>(ButtonPress {
                        button: Some(PointerButton::Primary),
                    });
                }
                // Repaint either way: the release ends the pressed state.
                ctx.request_paint_only();
            }
            None => (),
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.disabled || self.loading {
            return;
        }
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.set_handled();
            if let Some(payload) = &self.clipboard_payload {
                ctx.set_clipboard(payload.to_string());
            }
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if self.disabled || self.loading {
            return;
        }
        if event.action == accesskit::Action::Click {
            if let Some(payload) = &self.clipboard_payload {
                ctx.set_clipboard(payload.to_string());
            }
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            // Propagate the host-supplied initial `disabled` to masonry on
            // widget add. `with_disabled` only sets the widget-internal field;
            // without this, the first build leaves masonry's `is_disabled()`
            // out of sync with paint state, breaking event routing and the
            // accessibility pass that drives `node.set_disabled()`.
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
        ctx.register_child(&mut self.child);
        if let Some(icon) = &mut self.icon {
            ctx.register_child(icon);
        }
        if let Some(ti) = &mut self.trailing_icon {
            ctx.register_child(ti);
        }
        if let Some(s) = &mut self.spinner {
            ctx.register_child(s);
        }
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
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let (main_pad, cross_pad) = match axis {
            Axis::Horizontal => (2.0 * pad_h, 2.0 * pad_v),
            Axis::Vertical => (2.0 * pad_v, 2.0 * pad_h),
        };
        let inner_cross = cross_length.map(|c| Length::px((c.get() - cross_pad).max(0.0)));
        let auto_length = len_req.into();
        let context_size = LayoutSize::maybe(axis.cross(), inner_cross);
        let child_length = ctx.compute_length(
            &mut self.child,
            auto_length,
            context_size,
            axis,
            inner_cross,
        );
        let has_label = child_length.get() > 0.0;
        // Measure icon children so masonry tracks them even if we override their size in layout.
        let icon_sz = Length::px(self.icon_size());
        let icon_ctx = LayoutSize::maybe(axis.cross(), Some(icon_sz));
        if let Some(icon) = &mut self.icon {
            let _ = ctx.compute_length(icon, len_req.into(), icon_ctx, axis, Some(icon_sz));
        }
        if let Some(ti) = &mut self.trailing_icon {
            let _ = ctx.compute_length(ti, len_req.into(), icon_ctx, axis, Some(icon_sz));
        }
        let icon_extra = if (self.icon.is_some() || self.loading) && axis == Axis::Horizontal {
            let (slots, inner_gap) = if self.loading && self.icon.is_some() {
                (2.0, ICON_GAP)
            } else {
                (1.0, 0.0)
            };
            slots * self.icon_size() + inner_gap + if has_label { ICON_GAP } else { 0.0 }
        } else {
            0.0
        };
        let trailing_extra = if self.trailing_icon.is_some() && axis == Axis::Horizontal {
            self.icon_size() + if has_label { ICON_GAP } else { 0.0 }
        } else {
            0.0
        };
        Length::px(child_length.get() + main_pad + icon_extra + trailing_extra)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let icon_base = if self.loading && self.icon.is_some() {
            self.icon_size() * 2.0 + ICON_GAP
        } else if self.icon.is_some() || self.loading {
            self.icon_size()
        } else {
            0.0
        };
        let trailing_base = if self.trailing_icon.is_some() {
            self.icon_size()
        } else {
            0.0
        };
        let inner = Size::new(
            (size.width - 2.0 * pad_h - icon_base - trailing_base).max(0.0),
            (size.height - 2.0 * pad_v).max(0.0),
        );
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(inner), inner.into());
        ctx.run_layout(&mut self.child, child_size);

        let gap = if child_size.width > 0.0 {
            ICON_GAP
        } else {
            0.0
        };
        let icon_extra = if icon_base > 0.0 {
            icon_base + gap
        } else {
            0.0
        };
        let leading_gap = icon_extra - icon_base;
        let trailing_gap = if trailing_base > 0.0 { gap } else { 0.0 };
        let label_area = (inner.width - leading_gap - trailing_gap).max(0.0);
        // Label is centered in the space between the leading and trailing icon areas.
        let child_x = pad_h + icon_extra + ((label_area - child_size.width) * 0.5).max(0.0);
        let child_y = pad_v + ((inner.height - child_size.height) * 0.5).max(0.0);
        ctx.place_child(&mut self.child, Point::new(child_x, child_y));
        ctx.derive_baselines(&self.child);

        let icon_sz = self.icon_size();
        if let Some(icon) = &mut self.icon {
            ctx.run_layout(icon, Size::new(icon_sz, icon_sz));
            let icon_y = (size.height - icon_sz) * 0.5;
            let icon_x = if self.loading {
                pad_h + icon_sz + ICON_GAP
            } else {
                pad_h
            };
            ctx.place_child(icon, Point::new(icon_x, icon_y));
        }
        if let Some(ti) = &mut self.trailing_icon {
            ctx.run_layout(ti, Size::new(icon_sz, icon_sz));
            let icon_y = (size.height - icon_sz) * 0.5;
            let icon_x = size.width - pad_h - icon_sz;
            ctx.place_child(ti, Point::new(icon_x, icon_y));
        }
        if let Some(s) = &mut self.spinner {
            ctx.run_layout(s, Size::new(icon_sz, icon_sz));
            let icon_y = (size.height - icon_sz) * 0.5;
            ctx.place_child(s, Point::new(pad_h, icon_y));
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let hovered = ctx.is_hovered();
        let pressed = ctx.is_active() && hovered;
        let focused = ctx.is_focus_target();
        let (bg, border) = self.resolve_colors(hovered, pressed);

        let rect = RoundedRect::from_origin_size(Point::ORIGIN, size, self.corners);
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
                RoundedRectRadii::new(
                    (self.corners.top_left - inset).max(0.0),
                    (self.corners.top_right - inset).max(0.0),
                    (self.corners.bottom_right - inset).max(0.0),
                    (self.corners.bottom_left - inset).max(0.0),
                ),
            );
            crate::focus_ring::paint_focus_ring(painter, focus_rect, &self.theme);
        }
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
        // Masonry's accessibility pass calls `node.set_disabled()` whenever
        // `ctx.is_disabled()` is true (see masonry passes/accessibility.rs).
        // We additionally suppress the Click action so AT clients don't see
        // a disabled button as an actionable target.
        if !self.disabled && !self.loading {
            node.add_action(accesskit::Action::Click);
        }
        if let Some(name) = &self.accessibility_label {
            node.set_label(name.as_ref());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        let mut ids = vec![self.child.id()];
        if let Some(i) = &self.icon {
            ids.push(i.id());
        }
        if let Some(ti) = &self.trailing_icon {
            ids.push(ti.id());
        }
        if let Some(s) = &self.spinner {
            ids.push(s.id());
        }
        ChildrenIds::from_slice(&ids)
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
