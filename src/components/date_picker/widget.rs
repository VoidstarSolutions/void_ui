//! `DatePickerTrigger` (the clickable input-box trigger) and
//! `ThemedDatePickerWidget` (the full in-tree composite: trigger +
//! `CalendarBodyWidget` inside an `AnchoredOverlay`).
//!
//! This file covers the widget layer only. The view layer (`DatePickerView`)
//! lives in `view.rs` (Task 5). Portal hosting is added in Task 7.

use chrono::NaiveDate;
use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ArcStr, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::click::{ClickPhase, primary_click};
use crate::components::date_picker::calendar_body::{CalendarBodyAction, CalendarBodyWidget};
use crate::components::icon::{IconName, icon};
use crate::focus_ring::paint_focus_ring;
use crate::overlay::OverlayAnchor;

// ─────────────────────────────────────────────────────────────────────────────
// DatePickerTrigger
// ─────────────────────────────────────────────────────────────────────────────

/// Action type emitted by [`DatePickerTrigger`].
#[derive(Debug)]
pub(crate) enum DatePickerTriggerAction {
    /// Toggle the calendar open/closed.
    Toggle,
    /// Clear the selected date (only fired when the widget is clearable and a
    /// value is set).
    Clear,
}

/// Hand-rolled input-box-style trigger for the date picker.
///
/// Three regions left-to-right:
/// - Calendar icon (fixed-width square on the left).
/// - Selected date text or placeholder (fills remaining space).
/// - Optional clear button "×" (fixed-width square on the right, only when
///   `cleanable && has_value`).
pub(crate) struct DatePickerTrigger {
    icon: WidgetPod<Label>,
    text: WidgetPod<Label>,
    clear_icon: Option<WidgetPod<Label>>,
    disabled: bool,
    /// Bounding rect of the clear icon, computed each layout pass.
    clear_rect: Option<Rect>,
    /// Whether the pointer is currently hovering over the clear icon.
    hover_clear: bool,
    theme: Theme,
}

impl DatePickerTrigger {
    /// Constructs a new trigger.
    ///
    /// `text` is the display string (formatted date or placeholder).
    /// `has_value` controls whether a clear button is added.
    /// `cleanable` must be `true` for a clear button to appear.
    #[must_use]
    pub(crate) fn new(
        text: ArcStr,
        _placeholder: ArcStr,
        has_value: bool,
        cleanable: bool,
        disabled: bool,
        theme: &Theme,
    ) -> Self {
        let icon_color = if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text_muted
        };
        let text_color = if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text
        };

        let icon_pod = icon(IconName::Calendar)
            .color(icon_color)
            .build_widget(theme)
            .to_pod();

        let mut text_lbl = Label::new(text).prepare();
        text_lbl.properties.insert(ContentColor::new(text_color));
        let text_pod = text_lbl.to_pod();

        let clear_icon = if cleanable && has_value {
            Some(Self::build_clear_icon(theme, disabled))
        } else {
            None
        };

        Self {
            icon: icon_pod,
            text: text_pod,
            clear_icon,
            disabled,
            clear_rect: None,
            hover_clear: false,
            theme: *theme,
        }
    }

    fn build_clear_icon(theme: &Theme, disabled: bool) -> WidgetPod<Label> {
        let color = if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text_muted
        };
        icon(IconName::X).color(color).build_widget(theme).to_pod()
    }

    /// Returns the icon square side length (height of the widget).
    fn icon_side(theme: &Theme) -> f64 {
        f64::from(theme.density.row_height) + 2.0 * f64::from(theme.density.pad_v)
    }
}

// ── MARK: WidgetMut setters ──────────────────────────────────────────────────

impl DatePickerTrigger {
    /// Update the display text of the trigger.
    pub(crate) fn set_text(this: &mut WidgetMut<'_, Self>, text: ArcStr) {
        let mut lbl = this.ctx.get_mut(&mut this.widget.text);
        Label::set_text(&mut lbl, text);
    }

    /// Add or remove the clear button depending on whether a value is set.
    pub(crate) fn set_has_value(this: &mut WidgetMut<'_, Self>, has_value: bool, cleanable: bool) {
        let should_show = cleanable && has_value;
        let currently_shown = this.widget.clear_icon.is_some();
        if should_show == currently_shown {
            return;
        }
        if should_show {
            let theme = this.widget.theme;
            let disabled = this.widget.disabled;
            let pod = Self::build_clear_icon(&theme, disabled);
            this.widget.clear_icon = Some(pod);
            this.ctx.children_changed();
        } else {
            let pod = this.widget.clear_icon.take().unwrap();
            this.ctx.remove_child(pod);
        }
        this.widget.clear_rect = None;
        this.ctx.request_layout();
    }

    /// Enable or disable the trigger (and the clear button if present).
    pub(crate) fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled == disabled {
            return;
        }
        this.widget.disabled = disabled;
        let icon_color = if disabled {
            this.widget.theme.palette.text_faint
        } else {
            this.widget.theme.palette.text_muted
        };
        let text_color = if disabled {
            this.widget.theme.palette.text_faint
        } else {
            this.widget.theme.palette.text
        };
        {
            let mut lbl = this.ctx.get_mut(&mut this.widget.icon);
            lbl.insert_prop(ContentColor::new(icon_color));
        }
        {
            let mut lbl = this.ctx.get_mut(&mut this.widget.text);
            lbl.insert_prop(ContentColor::new(text_color));
        }
        if let Some(pod) = this.widget.clear_icon.as_mut() {
            let mut lbl = this.ctx.get_mut(pod);
            lbl.insert_prop(ContentColor::new(icon_color));
        }
        this.ctx.request_paint_only();
    }

    /// Re-apply a new theme to the trigger and all children.
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        let disabled = this.widget.disabled;
        let icon_color = if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text_muted
        };
        let text_color = if disabled {
            theme.palette.text_faint
        } else {
            theme.palette.text
        };
        {
            let mut lbl = this.ctx.get_mut(&mut this.widget.icon);
            lbl.insert_prop(ContentColor::new(icon_color));
        }
        {
            let mut lbl = this.ctx.get_mut(&mut this.widget.text);
            lbl.insert_prop(ContentColor::new(text_color));
        }
        if let Some(pod) = this.widget.clear_icon.as_mut() {
            let mut lbl = this.ctx.get_mut(pod);
            lbl.insert_prop(ContentColor::new(icon_color));
        }
        this.ctx.request_layout();
        this.ctx.request_paint_only();
    }
}

// ── MARK: Widget impl ────────────────────────────────────────────────────────

impl Widget for DatePickerTrigger {
    type Action = DatePickerTriggerAction;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.icon);
        ctx.register_child(&mut self.text);
        if let Some(pod) = &mut self.clear_icon {
            ctx.register_child(pod);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            Update::DisabledChanged(d) => {
                self.disabled = *d;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.disabled {
            return;
        }

        // Hover tracking for clear icon.
        if let PointerEvent::Move(PointerUpdate { current, .. }) = event {
            let pos = ctx.local_position(current.position);
            let new_hover_clear = self.clear_rect.is_some_and(|r| r.contains(pos));
            if new_hover_clear != self.hover_clear {
                self.hover_clear = new_hover_clear;
                ctx.request_paint_only();
            }
            return;
        }
        if let PointerEvent::Leave(_) = event {
            if self.hover_clear {
                self.hover_clear = false;
                ctx.request_paint_only();
            }
            return;
        }

        let Some(phase) = primary_click(ctx, event) else {
            return;
        };
        match phase {
            ClickPhase::Down(_) => {
                ctx.request_focus();
                ctx.request_paint_only();
            }
            ClickPhase::Up { state, completed } => {
                if completed {
                    let pos = ctx.local_position(state.position);
                    if self.clear_rect.is_some_and(|r| r.contains(pos)) {
                        ctx.submit_action::<Self::Action>(DatePickerTriggerAction::Clear);
                    } else {
                        ctx.submit_action::<Self::Action>(DatePickerTriggerAction::Toggle);
                    }
                }
                self.hover_clear = false;
                ctx.request_paint_only();
            }
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.disabled {
            return;
        }
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        let is_space = matches!(&key.key, Key::Character(c) if c == " ");
        let is_enter = matches!(&key.key, Key::Named(NamedKey::Enter));
        if is_space || is_enter {
            ctx.submit_action::<Self::Action>(DatePickerTriggerAction::Toggle);
            ctx.set_handled();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        // Fixed height; defer horizontal to the parent (redirect to primary
        // child so the trigger measures as wide as the overlay host needs it).
        let h = Self::icon_side(&self.theme);
        match axis {
            Axis::Horizontal => ctx.redirect_measurement(&mut self.icon, axis, cross_length),
            Axis::Vertical => {
                // Still measure children so their intrinsics are tracked.
                let _ = ctx.redirect_measurement(&mut self.text, axis, cross_length);
                Length::px(h)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let h = size.height;
        let icon_w = h; // square
        let clear_w = if self.clear_icon.is_some() { h } else { 0.0 };
        let text_w = (size.width - icon_w - clear_w).max(0.0);

        // Calendar icon (left).
        let icon_size = Size::new(icon_w, h);
        ctx.run_layout(&mut self.icon, icon_size);
        ctx.place_child(&mut self.icon, Point::ORIGIN);

        // Text (center).
        let text_size = Size::new(text_w, h);
        ctx.run_layout(&mut self.text, text_size);
        ctx.place_child(&mut self.text, Point::new(icon_w, 0.0));

        // Clear icon (right, optional).
        if let Some(pod) = &mut self.clear_icon {
            let clear_size = Size::new(clear_w, h);
            ctx.run_layout(pod, clear_size);
            let clear_x = icon_w + text_w;
            ctx.place_child(pod, Point::new(clear_x, 0.0));
            self.clear_rect = Some(Rect::from_origin_size(Point::new(clear_x, 0.0), clear_size));
        } else {
            self.clear_rect = None;
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let p = &self.theme.palette;
        let radius = f64::from(self.theme.radius.small);
        let rrect = RoundedRect::from_origin_size(Point::ORIGIN, size, radius);

        let fill = if ctx.is_hovered() && !self.disabled {
            p.surface_2
        } else {
            p.surface
        };
        painter.fill(rrect, fill).draw();
        painter.stroke(rrect, &Stroke::new(1.0), p.border).draw();

        if ctx.is_focus_target() && !self.disabled {
            let inset = 1.5_f64;
            let ring = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                radius,
            );
            paint_focus_ring(painter, ring, &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ComboBox
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
        let mut ids = vec![self.icon.id(), self.text.id()];
        if let Some(pod) = &self.clear_icon {
            ids.push(pod.id());
        }
        ChildrenIds::from_slice(&ids)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ThemedDatePickerWidget
// ─────────────────────────────────────────────────────────────────────────────

/// Action type emitted by [`ThemedDatePickerWidget`].
#[derive(Debug)]
pub enum DatePickerAction {
    /// The selected date changed (or was cleared: `None`).
    DateChanged(Option<NaiveDate>),
    /// The calendar panel was opened (`true`) or closed (`false`).
    OpenChanged(bool),
}

/// In-tree composite date-picker widget.
///
/// Hosts a [`DatePickerTrigger`] and a [`CalendarBodyWidget`] inside an
/// [`AnchoredOverlay`]. The overlay opens and closes in response to trigger
/// clicks, date selections, Escape, and focus loss.
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent boolean props, not a state machine"
)]
pub struct ThemedDatePickerWidget {
    overlay_host: WidgetPod<AnchoredOverlay>,
    selected: Option<NaiveDate>,
    placeholder: ArcStr,
    date_format: &'static str,
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
    disabled: bool,
    cleanable: bool,
    theme: Theme,
    open: bool,
    /// When `true`, open/close state is controlled by the host; internal
    /// actions submit [`DatePickerAction::OpenChanged`] but do not
    /// self-mutate `open` or the overlay.
    controlled: bool,
}

impl ThemedDatePickerWidget {
    /// Build the display text for the trigger from the current selection and
    /// format string, or fall back to the placeholder.
    fn display_text(&self) -> ArcStr {
        self.selected.map_or_else(
            || self.placeholder.clone(),
            |d| ArcStr::from(d.format(self.date_format).to_string()),
        )
    }

    /// Construct an in-tree date picker widget.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        selected: Option<NaiveDate>,
        placeholder: ArcStr,
        date_format: &'static str,
        min_date: Option<NaiveDate>,
        max_date: Option<NaiveDate>,
        disabled: bool,
        cleanable: bool,
        theme: &Theme,
    ) -> Self {
        let display = selected.map_or_else(
            || placeholder.clone(),
            |d| ArcStr::from(d.format(date_format).to_string()),
        );

        let trigger = DatePickerTrigger::new(
            display,
            placeholder.clone(),
            selected.is_some(),
            cleanable,
            disabled,
            theme,
        );
        let body = CalendarBodyWidget::new(selected, min_date, max_date, theme);
        let overlay_host = AnchoredOverlay::new(
            NewWidget::new(trigger),
            NewWidget::new(body),
            false,
            OverlayAnchor::BottomStart,
        );

        Self {
            overlay_host: NewWidget::new(overlay_host).to_pod(),
            selected,
            placeholder,
            date_format,
            min_date,
            max_date,
            disabled,
            cleanable,
            theme: *theme,
            open: false,
            controlled: false,
        }
    }

    /// Set the initial open state and whether it is controlled by the host.
    #[must_use]
    pub fn with_open_state(mut self, open: bool, controlled: bool) -> Self {
        self.open = open;
        self.controlled = controlled;
        self
    }
}

// ── MARK: Navigation macros ──────────────────────────────────────────────────

/// Navigate from `$this: &mut WidgetMut<'_, ThemedDatePickerWidget>` down to
/// the `DatePickerTrigger` and run `$body`.
macro_rules! with_trigger {
    ($this:ident, |$trig:ident| $body:block) => {{
        let mut ov = $this.ctx.get_mut(&mut $this.widget.overlay_host);
        let mut primary = AnchoredOverlay::primary_mut(&mut ov);
        let mut $trig = primary.downcast::<DatePickerTrigger>();
        $body
    }};
}

/// Navigate from `$this: &mut WidgetMut<'_, ThemedDatePickerWidget>` down to
/// the `CalendarBodyWidget` and run `$body`.
macro_rules! with_body {
    ($this:ident, |$body_var:ident| $body:block) => {{
        let mut ov = $this.ctx.get_mut(&mut $this.widget.overlay_host);
        let mut overlay = AnchoredOverlay::overlay_mut(&mut ov);
        let mut $body_var = overlay.downcast::<CalendarBodyWidget>();
        $body
    }};
}

// ── MARK: WidgetMut setters ──────────────────────────────────────────────────

impl ThemedDatePickerWidget {
    /// Update the selected date, refreshing both the trigger display and the
    /// calendar body.
    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: Option<NaiveDate>) {
        if this.widget.selected == selected {
            return;
        }
        this.widget.selected = selected;
        let display = this.widget.display_text();
        let has_value = selected.is_some();
        let cleanable = this.widget.cleanable;
        with_trigger!(this, |trig| {
            DatePickerTrigger::set_text(&mut trig, display);
            DatePickerTrigger::set_has_value(&mut trig, has_value, cleanable);
        });
        with_body!(this, |body| {
            CalendarBodyWidget::set_selected(&mut body, selected);
        });
        this.ctx.request_paint_only();
    }

    /// Show or hide the calendar panel.
    pub fn set_open(this: &mut WidgetMut<'_, Self>, open: bool) {
        if this.widget.open == open {
            return;
        }
        if open && this.widget.disabled {
            return;
        }
        this.widget.open = open;
        let overlay_id = this.widget.overlay_host.id();
        this.ctx.mutate_later(overlay_id, move |mut w| {
            let mut ov = w.downcast::<AnchoredOverlay>();
            AnchoredOverlay::set_overlay_visible(&mut ov, open);
        });
        this.ctx.request_paint_only();
    }

    /// Switch between controlled and uncontrolled mode.
    pub fn set_controlled(this: &mut WidgetMut<'_, Self>, controlled: bool) {
        this.widget.controlled = controlled;
    }

    /// Update the placeholder text.
    pub fn set_placeholder(this: &mut WidgetMut<'_, Self>, placeholder: ArcStr) {
        if this.widget.placeholder == placeholder {
            return;
        }
        this.widget.placeholder = placeholder;
        if this.widget.selected.is_none() {
            let display = this.widget.display_text();
            with_trigger!(this, |trig| {
                DatePickerTrigger::set_text(&mut trig, display);
            });
        }
    }

    /// Update the date format string used for display.
    pub fn set_date_format(this: &mut WidgetMut<'_, Self>, fmt: &'static str) {
        if this.widget.date_format == fmt {
            return;
        }
        this.widget.date_format = fmt;
        let display = this.widget.display_text();
        with_trigger!(this, |trig| {
            DatePickerTrigger::set_text(&mut trig, display);
        });
    }

    /// Update the minimum selectable date.
    pub fn set_min_date(this: &mut WidgetMut<'_, Self>, min: Option<NaiveDate>) {
        if this.widget.min_date == min {
            return;
        }
        this.widget.min_date = min;
        let max = this.widget.max_date;
        with_body!(this, |body| {
            CalendarBodyWidget::set_min_max(&mut body, min, max);
        });
    }

    /// Update the maximum selectable date.
    pub fn set_max_date(this: &mut WidgetMut<'_, Self>, max: Option<NaiveDate>) {
        if this.widget.max_date == max {
            return;
        }
        this.widget.max_date = max;
        let min = this.widget.min_date;
        with_body!(this, |body| {
            CalendarBodyWidget::set_min_max(&mut body, min, max);
        });
    }

    /// Enable or disable the date picker.
    pub fn set_disabled(this: &mut WidgetMut<'_, Self>, disabled: bool) {
        if this.widget.disabled == disabled {
            return;
        }
        this.widget.disabled = disabled;
        this.ctx.set_disabled(disabled);

        // Force close if disabling while open.
        if disabled && this.widget.open {
            this.widget.open = false;
            let overlay_id = this.widget.overlay_host.id();
            this.ctx.mutate_later(overlay_id, |mut w| {
                let mut ov = w.downcast::<AnchoredOverlay>();
                AnchoredOverlay::set_overlay_visible(&mut ov, false);
            });
            this.ctx
                .submit_action::<DatePickerAction>(DatePickerAction::OpenChanged(false));
        }

        with_trigger!(this, |trig| {
            DatePickerTrigger::set_disabled(&mut trig, disabled);
        });
        this.ctx.request_paint_only();
    }

    /// Update whether a "clear" button is shown inside the trigger.
    pub fn set_cleanable(this: &mut WidgetMut<'_, Self>, cleanable: bool) {
        if this.widget.cleanable == cleanable {
            return;
        }
        this.widget.cleanable = cleanable;
        let has_value = this.widget.selected.is_some();
        with_trigger!(this, |trig| {
            DatePickerTrigger::set_has_value(&mut trig, has_value, cleanable);
        });
    }

    /// Re-apply a new theme to the whole widget tree.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        with_trigger!(this, |trig| {
            DatePickerTrigger::set_theme(&mut trig, theme);
        });
        with_body!(this, |body| {
            CalendarBodyWidget::set_theme(&mut body, theme);
        });
        this.ctx.request_paint_only();
    }
}

// ── MARK: Widget impl ────────────────────────────────────────────────────────

impl Widget for ThemedDatePickerWidget {
    type Action = DatePickerAction;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.overlay_host);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
                if self.open {
                    let overlay_id = self.overlay_host.id();
                    ctx.mutate_later(overlay_id, |mut w| {
                        let mut ov = w.downcast::<AnchoredOverlay>();
                        AnchoredOverlay::set_overlay_visible(&mut ov, true);
                    });
                }
            }
            Update::ChildFocusChanged(false) if self.open => {
                if !self.controlled {
                    self.open = false;
                    let overlay_id = self.overlay_host.id();
                    ctx.mutate_later(overlay_id, |mut w| {
                        let mut ov = w.downcast::<AnchoredOverlay>();
                        AnchoredOverlay::set_overlay_visible(&mut ov, false);
                    });
                }
                ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(false));
                ctx.request_paint_only();
            }
            Update::StashedChanged(true) if self.open => {
                self.open = false;
                let overlay_id = self.overlay_host.id();
                ctx.mutate_later(overlay_id, |mut w| {
                    let mut ov = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut ov, false);
                });
                ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(false));
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        if let Some(DatePickerTriggerAction::Toggle) = action.downcast_ref() {
            if !self.disabled {
                let desired = !self.open;
                if !self.controlled {
                    self.open = desired;
                    let overlay_id = self.overlay_host.id();
                    ctx.mutate_later(overlay_id, move |mut w| {
                        let mut ov = w.downcast::<AnchoredOverlay>();
                        AnchoredOverlay::set_overlay_visible(&mut ov, desired);
                    });
                }
                ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(desired));
            }
            ctx.set_handled();
            ctx.request_paint_only();
            return;
        }

        if let Some(DatePickerTriggerAction::Clear) = action.downcast_ref() {
            if !self.disabled {
                if !self.controlled {
                    self.selected = None;
                    let display = self.placeholder.clone();
                    let cleanable = self.cleanable;
                    let overlay_id = self.overlay_host.id();
                    ctx.mutate_later(overlay_id, move |mut w| {
                        let mut ov = w.downcast::<AnchoredOverlay>();
                        {
                            let mut primary = AnchoredOverlay::primary_mut(&mut ov);
                            let mut trig = primary.downcast::<DatePickerTrigger>();
                            DatePickerTrigger::set_text(&mut trig, display);
                            DatePickerTrigger::set_has_value(&mut trig, false, cleanable);
                        }
                        {
                            let mut overlay = AnchoredOverlay::overlay_mut(&mut ov);
                            let mut body = overlay.downcast::<CalendarBodyWidget>();
                            CalendarBodyWidget::set_selected(&mut body, None);
                        }
                    });
                }
                ctx.submit_action::<Self::Action>(DatePickerAction::DateChanged(None));
            }
            ctx.set_handled();
            ctx.request_paint_only();
            return;
        }

        if let Some(CalendarBodyAction::DateSelected(date)) = action.downcast_ref() {
            let date = *date;
            if !self.controlled {
                self.selected = Some(date);
                self.open = false;
                let display = ArcStr::from(date.format(self.date_format).to_string());
                let cleanable = self.cleanable;
                let overlay_id = self.overlay_host.id();
                ctx.mutate_later(overlay_id, move |mut w| {
                    let mut ov = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut ov, false);
                    {
                        let mut primary = AnchoredOverlay::primary_mut(&mut ov);
                        let mut trig = primary.downcast::<DatePickerTrigger>();
                        DatePickerTrigger::set_text(&mut trig, display);
                        DatePickerTrigger::set_has_value(&mut trig, true, cleanable);
                    }
                });
            }
            ctx.submit_action::<Self::Action>(DatePickerAction::DateChanged(Some(date)));
            ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(false));
            ctx.set_handled();
            ctx.request_paint_only();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if !self.open {
            return;
        }
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        if let Key::Named(NamedKey::Escape) = &key.key {
            if !self.controlled {
                self.open = false;
                let overlay_id = self.overlay_host.id();
                ctx.mutate_later(overlay_id, |mut w| {
                    let mut ov = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut ov, false);
                });
            }
            ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(false));
            ctx.set_handled();
            ctx.request_paint_only();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.overlay_host, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.overlay_host, size);
        ctx.place_child(&mut self.overlay_host, Point::ORIGIN);
        ctx.derive_baselines(&self.overlay_host);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — children paint themselves.
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.overlay_host.id()])
    }
}
