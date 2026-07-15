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
use crate::components::interaction::{self, InteractionState};
use crate::components::spinner::widget::SpinnerWidget;

/// Border thickness for the active and focus states.
const BORDER_WIDTH: f64 = 1.0;
/// Inset of the focus ring from the button edge.
///
/// Deliberately larger than the shared
/// [`crate::focus_ring::FOCUS_RING_INSET`] (1.5): the button's ring sits
/// inside a filled, [`CORNER_RADIUS`]-rounded background (the per-corner
/// focus radii below subtract this inset from the background radii), and
/// the extra half-pixel keeps the ring visually separated from the fill
/// edge. Named distinctly so it no longer shadows the shared constant.
/// Focus-ring chrome, not density-scaled.
const BUTTON_FOCUS_RING_INSET: f64 = 2.0;
/// Gap between a leading icon and the label — 5px matches no density token
/// (`gap` = 6 would change pixels); a candidate for a future *deliberate*
/// visual change to `gap`, out of scope for this pixel-identical refactor.
const ICON_GAP: f64 = 5.0;

/// Themed, interactive button widget.
///
/// Owns its child (typically a `Label`) and a [`Theme`] value used to
/// resolve background / border / text colors at paint time. The host drives
/// the `active` flag for "currently-selected toggle" semantics. Pointer state
/// (hovered, pressed) is read from the widget context.
// `active`/`disabled`/`loading`/`keyboard_pressed` are independent flags with
// no shared state machine to fold them into — each can be true or false
// regardless of the others (e.g. disabled-while-loading, active-while-
// keyboard_pressed) — so an enum would just relocate the same four
// independent facts, not simplify them.
#[allow(clippy::struct_excessive_bools)]
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
    /// Defaults to a uniform `theme.radius.small`; button groups override
    /// this to round only the outer edges of the group.
    corners: RoundedRectRadii,
    /// True for the span between a Space/Enter key-down and its matching
    /// key-up (or an intervening focus loss) — the keyboard equivalent of
    /// the pointer-driven `pressed` flag read from the widget context, so
    /// keyboard activation shows the same pressed fill a pointer click does.
    keyboard_pressed: bool,
    /// When true, the child is sized to the button's full inner box and
    /// left-aligned instead of being sized to fit and centered.
    ///
    /// A label wants fit-and-center (the default). A composite child — a
    /// `flex_row` of columns behind [`content_button`] — wants fill: under
    /// `SizeDef::fit` a flexed child collapses to its minimum, so columns
    /// never line up across rows.
    ///
    /// [`content_button`]: super::content_button
    stretch_child: bool,
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
            corners: RoundedRectRadii::from_single_radius(f64::from(theme.radius.small)),
            keyboard_pressed: false,
            stretch_child: false,
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

    /// Sizes the child to the button's full inner box and left-aligns it,
    /// instead of sizing it to fit and centering it. See [`Self::stretch_child`].
    #[must_use]
    pub fn with_stretch_child(mut self, stretch: bool) -> Self {
        self.stretch_child = stretch;
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

    /// Toggles child stretching. Requests layout on change.
    pub fn set_stretch_child(this: &mut WidgetMut<'_, Self>, stretch: bool) {
        if this.widget.stretch_child != stretch {
            this.widget.stretch_child = stretch;
            this.ctx.request_layout();
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
    /// | state           | Default      | Danger        | Primary       | Ghost        | Warning        |
    /// |-----------------|--------------|---------------|---------------|--------------|----------------|
    /// | disabled        | transparent  | transparent   | transparent   | transparent  | transparent    |
    /// | rest            | `surface`    | `danger_soft` | `accent_soft` | transparent/border | `warning_soft` |
    /// | hover           | `surface_2`  | `danger`      | `accent`      | `surface_2`  | `warning`      |
    /// | pressed         | `surface_hi` | `danger_deep` | `accent_deep` | `surface_hi` | `warning_deep` |
    /// | active (toggle) | `surface_2`  | `danger_soft` | `accent_soft` | `surface_2`  | `warning_soft` |
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
                    p.danger_deep
                } else if hovered {
                    p.danger
                } else {
                    p.danger_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Primary => {
                let bg = if pressed {
                    p.accent_deep
                } else if hovered {
                    p.accent
                } else {
                    p.accent_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Warning => {
                let bg = if pressed {
                    p.warning_deep
                } else if hovered {
                    p.warning
                } else {
                    p.warning_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Secondary => {
                let bg = if pressed {
                    p.secondary_deep
                } else if hovered {
                    p.secondary
                } else {
                    p.secondary_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Success => {
                let bg = if pressed {
                    p.success_deep
                } else if hovered {
                    p.success
                } else {
                    p.success_soft
                };
                (bg, Color::TRANSPARENT)
            }
            ButtonVariant::Info => {
                let bg = if pressed {
                    p.info_deep
                } else if hovered {
                    p.info
                } else {
                    p.info_soft
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
            Some(ClickPhase::Down(_)) => {
                ctx.request_focus();
                ctx.request_paint_only();
            }
            Some(ClickPhase::Up { completed, .. }) => {
                if completed {
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
        if interaction::keyboard_press_start(event, true) {
            ctx.set_handled();
            self.keyboard_pressed = true;
            ctx.request_paint_only();
        } else if interaction::keyboard_activate(event, true) {
            ctx.set_handled();
            self.keyboard_pressed = false;
            ctx.request_paint_only();
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
        if interaction::is_access_click(event) {
            if let Some(payload) = &self.clipboard_payload {
                ctx.set_clipboard(payload.to_string());
            }
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Losing focus mid-press (e.g. Tab away while Space is still held)
        // would otherwise leave `keyboard_pressed` stuck true with no
        // matching key-up ever arriving to clear it.
        if matches!(event, Update::FocusChanged(false)) {
            self.keyboard_pressed = false;
        }
        interaction::interaction_update(ctx, event, self.disabled);
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
        let (child_size, child_x) = if self.stretch_child {
            // Fill: the icon columns and their gaps are reserved up front.
            // The fit path below derives its gaps from the *measured* child
            // width, which isn't available here — a stretched child's width is
            // what we're solving for, so the reservation has to come first.
            let leading_gap = if icon_base > 0.0 { ICON_GAP } else { 0.0 };
            let trailing_gap = if trailing_base > 0.0 { ICON_GAP } else { 0.0 };
            let avail = Size::new(
                (inner.width - leading_gap - trailing_gap).max(0.0),
                inner.height,
            );
            let child_size = ctx.compute_size(&mut self.child, SizeDef::fixed(avail), avail.into());
            (child_size, pad_h + icon_base + leading_gap)
        } else {
            let child_size = ctx.compute_size(&mut self.child, SizeDef::fit(inner), inner.into());
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
            (child_size, child_x)
        };
        ctx.run_layout(&mut self.child, child_size);

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
        let size = ctx.border_box().size();
        let InteractionState {
            hovered,
            pressed,
            focused,
        } = InteractionState::from_paint_ctx(ctx);
        // `pressed` alone only reflects pointer-capture; keyboard activation
        // never captures the pointer, so it needs `keyboard_pressed` folded
        // in to show the same pressed fill Space/Enter that a pointer click does.
        let pressed = pressed || self.keyboard_pressed;
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
            let inset = BUTTON_FOCUS_RING_INSET;
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

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{NewWidget, TextEvent, WidgetTag};
    use masonry::kurbo::Point;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::*;
    use crate::Theme;

    fn harness_with(disabled: bool) -> TestHarness<ThemedButton> {
        let widget = ThemedButton::new(NewWidget::new(Label::new("Go")), &Theme::dark())
            .with_disabled(disabled);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (120, 32))
    }

    /// Tags the child so the layout tests can read its geometry back out.
    const CHILD: WidgetTag<Label> = WidgetTag::named("themed-button-child");

    /// A button far wider than its "Go" label, so fit-and-center and fill are
    /// distinguishable (at natural width they would coincide).
    fn harness_with_stretch(stretch: bool) -> TestHarness<ThemedButton> {
        let child = NewWidget::new(Label::new("Go")).with_tag(CHILD);
        let widget = ThemedButton::new(child, &Theme::dark()).with_stretch_child(stretch);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (300, 40))
    }

    /// `(button width, child width, child x relative to the button)`.
    fn child_geometry(h: &TestHarness<ThemedButton>) -> (f64, f64, f64) {
        let button_x = h.root_widget().ctx().window_transform().translation().x;
        let button_w = h.root_widget().ctx().border_box().size().width;
        let child = h.get_widget(CHILD);
        let child_x = child.ctx().window_transform().translation().x;
        (
            button_w,
            child.ctx().border_box().size().width,
            child_x - button_x,
        )
    }

    #[test]
    fn pointer_click_submits_press() {
        let mut h = harness_with(false);
        h.mouse_move(Point::new(60.0, 16.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn disabled_button_ignores_clicks_and_keys() {
        let mut h = harness_with(true);
        h.mouse_move(Point::new(60.0, 16.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        assert!(h.pop_action::<ButtonPress>().is_none());
    }

    #[test]
    fn space_and_enter_activate_when_focused() {
        let mut h = harness_with(false);
        h.focus_on(Some(h.root_id()));

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.pop_action::<ButtonPress>().is_some());

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn space_key_down_shows_the_pressed_fill_until_key_up() {
        // Regression test: `on_text_event` used to only ever fire on key-up,
        // so Space/Enter "clicking" a button submitted its action correctly
        // but never showed any pressed-fill feedback — pointer clicks show a
        // pressed background because pointer-down captures the pointer;
        // keyboard activation never captures the pointer, so `pressed` in
        // `InteractionState` stayed false the whole time.
        let mut h = harness_with(false);
        h.focus_on(Some(h.root_id()));

        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));

        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(h.pop_action::<ButtonPress>().is_none(), "not yet activated");

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
        assert!(h.pop_action::<ButtonPress>().is_some());
    }

    #[test]
    fn child_is_fit_and_centered_by_default() {
        let h = harness_with_stretch(false);
        let pad_h = f64::from(Theme::dark().density.button_pad_h);
        let (button_w, child_w, child_x) = child_geometry(&h);
        let inner_w = button_w - 2.0 * pad_h;

        assert!(
            child_w < inner_w,
            "a label keeps its natural width ({child_w}) inside a {inner_w}px inner box"
        );
        // Centered: equal slack on both sides, so x is past the left padding.
        let expected_x = pad_h + (inner_w - child_w) * 0.5;
        assert!((child_x - expected_x).abs() < 0.5, "child_x {child_x}");
    }

    #[test]
    fn stretch_child_fills_the_inner_box_and_left_aligns() {
        // Why this exists: `content_button` puts a composite child (a flex_row
        // of columns) in a button that a parent stretches. Under the fit path a
        // flexed column collapses to its minimum, so columns never line up
        // across rows — filling is what makes composite rows possible.
        let h = harness_with_stretch(true);
        let pad_h = f64::from(Theme::dark().density.button_pad_h);
        let (button_w, child_w, child_x) = child_geometry(&h);

        assert!(
            (child_w - (button_w - 2.0 * pad_h)).abs() < 0.5,
            "child ({child_w}) should fill the inner box of a {button_w}px button"
        );
        assert!((child_x - pad_h).abs() < 0.5, "left-aligned, got {child_x}");
    }

    #[test]
    fn losing_focus_mid_press_clears_the_keyboard_pressed_flag() {
        // Tabbing away (or the window losing focus) while Space is still
        // physically held down means the key-up that would normally clear
        // `keyboard_pressed` never reaches this widget — without the
        // `FocusChanged(false)` handling in `update()`, the pressed fill
        // would stay stuck the next time the widget paints.
        let mut h = harness_with(false);
        h.focus_on(Some(h.root_id()));
        h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.keyboard_pressed));

        h.focus_on(None);
        assert!(!h.edit_root_widget(|wm| wm.widget.keyboard_pressed));
    }
}
