//! `DatePickerTrigger` (the clickable input-box trigger) and
//! `ThemedDatePickerWidget` (the full composite: trigger +
//! `CalendarBodyWidget` inside an `AnchoredOverlay` for in-tree hosting, or
//! trigger-only with a portal-registered `CalendarBodyView` for portal hosting).
//!
//! This file covers the widget layer only. The view layer (`DatePickerView`)
//! lives in `view.rs` (Task 5). Portal hosting is added in Task 7.

use chrono::NaiveDate;
use masonry::accesskit::{Action as AccessKitAction, ActionData, CustomAction, Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ActionCtx, ArcStr, ChildrenIds, ComposeCtx, ErasedAction, EventCtx,
    LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PointerEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use masonry::properties::ContentColor;
use masonry::widgets::{Label, Passthrough};

use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::click::{ClickPhase, primary_click};
use crate::components::date_picker::calendar_body::CalendarHeaderHandle;
use crate::components::date_picker::calendar_body::{
    CalendarBodyAction, CalendarBodyWidget, CalendarNavKey,
};
use crate::components::icon::{IconName, icon};
use crate::components::interaction::is_access_click;
use crate::focus_ring::paint_focus_ring;
use crate::overlay::OverlayAnchor;
use crate::overlay::binding::{self, PortalBinding, PortalCtx, PortalOpenCtx};
use crate::overlay_portal::PortalSlot;
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

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

/// Stable id for the clear affordance's accesskit custom action, used to
/// match `AccessEvent::data` in `on_access_event` back to
/// [`DatePickerTriggerAction::Clear`].
const CLEAR_CUSTOM_ACTION_ID: i32 = 1;

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

    /// Gap reserved after the text before the box's right edge. The clear
    /// icon's square column already reads as trailing padding when present,
    /// so this is only needed without it.
    fn trailing_pad(theme: &Theme, has_clear_icon: bool) -> f64 {
        if has_clear_icon {
            0.0
        } else {
            2.0 * f64::from(theme.density.pad_h)
        }
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

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if self.disabled {
            return;
        }
        if is_access_click(event) {
            ctx.submit_action::<Self::Action>(DatePickerTriggerAction::Toggle);
            ctx.set_handled();
            return;
        }
        if event.action == AccessKitAction::CustomAction
            && matches!(
                event.data,
                Some(ActionData::CustomAction(id)) if id == CLEAR_CUSTOM_ACTION_ID
            )
        {
            ctx.submit_action::<Self::Action>(DatePickerTriggerAction::Clear);
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
        let h = Self::icon_side(&self.theme);
        match axis {
            Axis::Horizontal => {
                // Icon and clear columns are fixed squares (see `layout`); only
                // the text needs measuring. Redirecting to the icon alone (as
                // this used to) left the trigger's natural width equal to the
                // icon's width, ignoring the text entirely — a `flex_row` or
                // other content-sized parent would shrink the trigger down to
                // a bare icon square.
                let icon_w = h;
                let clear_w = if self.clear_icon.is_some() { h } else { 0.0 };
                let trailing_pad = Self::trailing_pad(&self.theme, self.clear_icon.is_some());
                let context_size = LayoutSize::maybe(Axis::Vertical, Some(Length::px(h)));
                let text_len = ctx.compute_length(
                    &mut self.text,
                    len_req.into(),
                    context_size,
                    axis,
                    Some(Length::px(h)),
                );
                Length::px(icon_w + clear_w + trailing_pad + text_len.get())
            }
            Axis::Vertical => {
                // Still measure children so their intrinsics are tracked.
                let _ = ctx.redirect_measurement(&mut self.text, axis, cross_length);
                Length::px(h)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let h = size.height;
        let icon_w = h; // square column
        let clear_w = if self.clear_icon.is_some() { h } else { 0.0 };
        // The clear icon's square column already reads as trailing padding;
        // without it, the text would otherwise run flush to the box edge.
        let trailing_pad = Self::trailing_pad(&self.theme, self.clear_icon.is_some());
        let text_w = (size.width - icon_w - clear_w - trailing_pad).max(0.0);

        // Icons are laid out at their natural glyph size and centered within
        // their square column — running them at the full column size would
        // leave `Label` (which only centers text horizontally, and only when
        // `text_alignment` is `Center`) pinning the glyph to the top-left.
        let glyph = f64::from(self.theme.density.ui_font_size);
        let glyph_size = Size::new(glyph, glyph);

        // Calendar icon (left).
        ctx.run_layout(&mut self.icon, glyph_size);
        ctx.place_child(
            &mut self.icon,
            Point::new((icon_w - glyph) * 0.5, (h - glyph) * 0.5),
        );

        // Text: fills the remaining width (left-aligned within it), but its
        // height is left to its natural line height and vertically centered —
        // same reasoning as the icons above.
        let text_size = ctx.compute_size(
            &mut self.text,
            SizeDef::fixed(Size::new(text_w, h)).with_height(LenDef::FitContent(Length::px(h))),
            Size::new(text_w, h).into(),
        );
        ctx.run_layout(&mut self.text, text_size);
        ctx.place_child(
            &mut self.text,
            Point::new(icon_w, (h - text_size.height) * 0.5),
        );

        // Clear icon (right, optional).
        if let Some(pod) = &mut self.clear_icon {
            ctx.run_layout(pod, glyph_size);
            let clear_x = icon_w + text_w;
            ctx.place_child(
                pod,
                Point::new(clear_x + (clear_w - glyph) * 0.5, (h - glyph) * 0.5),
            );
            self.clear_rect = Some(Rect::from_origin_size(
                Point::new(clear_x, 0.0),
                Size::new(clear_w, h),
            ));
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
        let size = ctx.border_box().size();
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
            node.add_action(AccessKitAction::Click);
            if self.clear_icon.is_some() {
                node.add_action(AccessKitAction::CustomAction);
                node.push_custom_action(CustomAction {
                    id: CLEAR_CUSTOM_ACTION_ID,
                    description: "Clear".into(),
                });
            }
        }
        node.add_action(AccessKitAction::Expand);
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

widget_id_handle!(
    /// Self-filling handle to a [`ThemedDatePickerWidget`]'s widget id.
    /// Given to portal-mounted [`super::view::CalendarBodyView`] so a day
    /// selection can `mutate_later` back to close the picker and sync the
    /// trigger display.
    DatePickerHandle
);

/// How this date picker mounts its calendar panel: permanently in-tree
/// (fallback, no scope ancestor), or portal-mounted in the nearest scope's
/// `PortalSlot` (the calendar body is a view child of the scope; we only hold
/// the key).
enum Hosting {
    InTree {
        overlay_host: WidgetPod<AnchoredOverlay>,
    },
    Portal {
        trigger: WidgetPod<DatePickerTrigger>,
        binding: PortalBinding,
    },
}

/// Navigate from `$this: &mut WidgetMut<'_, ThemedDatePickerWidget>` down to
/// the `DatePickerTrigger` and run `$body`.
///
/// Works for both `InTree` (navigates through `AnchoredOverlay::primary_mut`)
/// and `Portal` (navigates directly to the trigger pod).
macro_rules! with_trigger {
    ($this:ident, |$trig:ident| $body:block) => {
        match &mut $this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut ov = $this.ctx.get_mut(overlay_host);
                let mut primary = AnchoredOverlay::primary_mut(&mut ov);
                let mut $trig = primary.downcast::<DatePickerTrigger>();
                $body
            }
            Hosting::Portal { trigger, .. } => {
                let mut trig_wm = $this.ctx.get_mut(trigger);
                let mut $trig = trig_wm.downcast::<DatePickerTrigger>();
                $body
            }
        }
    };
}

/// Navigate from `$this: &mut WidgetMut<'_, ThemedDatePickerWidget>` down to
/// the `CalendarBodyWidget` and run `$body`. Only valid in `InTree` mode.
macro_rules! with_body {
    ($this:ident, |$body_var:ident| $body:block) => {{
        if let Hosting::InTree { overlay_host } = &mut $this.widget.hosting {
            let mut ov = $this.ctx.get_mut(overlay_host);
            let mut overlay = AnchoredOverlay::overlay_mut(&mut ov);
            let mut $body_var = overlay.downcast::<CalendarBodyWidget>();
            $body
        }
    }};
}

/// Composite date-picker widget.
///
/// In-tree mode: hosts a [`DatePickerTrigger`] and a [`CalendarBodyWidget`]
/// inside an [`AnchoredOverlay`].
///
/// Portal mode: hosts only a [`DatePickerTrigger`]; the calendar body lives in
/// the scope's `PortalSlot`, mounted via [`crate::overlay_portal::OverlayPortal`].
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent boolean props, not a state machine"
)]
pub struct ThemedDatePickerWidget {
    hosting: Hosting,
    handle: DatePickerHandle,
    /// Handle to the portal-mounted calendar's first header button
    /// (`header_prev`), used to move real keyboard focus there when Tab is
    /// pressed while the picker is open — natural masonry Tab traversal
    /// then walks forward through the rest of the header row into the grid
    /// on subsequent Tab presses. `None` for `InTree` hosting (Tab
    /// traversal already reaches the in-tree calendar correctly without
    /// it).
    header_handle: Option<CalendarHeaderHandle>,
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
        let body = CalendarBodyWidget::new(
            chrono::Local::now().date_naive(),
            selected,
            min_date,
            max_date,
            theme,
            &CalendarHeaderHandle::new(),
        );
        let overlay_host = AnchoredOverlay::new(
            NewWidget::new(trigger),
            NewWidget::new(body),
            false,
            OverlayAnchor::BottomStart,
        );

        Self {
            hosting: Hosting::InTree {
                overlay_host: NewWidget::new(overlay_host).to_pod(),
            },
            handle: DatePickerHandle::new(),
            header_handle: None,
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

    /// Portal-mode constructor: the calendar body lives in the scope's slot
    /// under `key`, registered by the view layer as a `CalendarBodyView`; we
    /// host only the trigger. `handle` is filled at `Update::WidgetAdded` and
    /// given to the registered `CalendarBodyView` so it can notify us back.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(crate) fn new_portal(
        selected: Option<NaiveDate>,
        placeholder: ArcStr,
        date_format: &'static str,
        min_date: Option<NaiveDate>,
        max_date: Option<NaiveDate>,
        disabled: bool,
        cleanable: bool,
        theme: &Theme,
        handle: DatePickerHandle,
        scope: OverlayScopeHandle,
        key: u64,
        header_handle: CalendarHeaderHandle,
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
        Self {
            hosting: Hosting::Portal {
                trigger: NewWidget::new(trigger).to_pod(),
                binding: PortalBinding::new(scope, key, date_picker_dismiss_hook),
            },
            handle,
            header_handle: Some(header_handle),
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

// ── MARK: WidgetMut setters ──────────────────────────────────────────────────

impl ThemedDatePickerWidget {
    /// Update the selected date, refreshing both the trigger display and the
    /// calendar body (in-tree mode only — portal mode rebuilds the calendar
    /// body via the view layer).
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
        if open {
            this.widget.open_menu(&mut this.ctx);
        } else {
            this.widget.close_menu(&mut this.ctx);
        }
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
            this.widget.close_menu(&mut this.ctx);
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

    /// Sync `open` state after the portal slot dismissed the calendar without
    /// going through our own event handlers (outside press;
    /// `PortalSlot::dismiss_outside`), called via `mutate_later(handle)` from
    /// [`date_picker_dismiss_hook`]. The slot has *already hidden* the content,
    /// so both `open` in both modes must go `false`. Controlled hosts observe
    /// [`DatePickerAction::OpenChanged`] and re-apply `true` via
    /// [`Self::set_open`] if they disagree.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if this.widget.open {
            this.widget.open = false;
            // In portal mode the slot already hid the content — close_menu
            // is a redundant re-push. InTree mode must close the AnchoredOverlay.
            if matches!(this.widget.hosting, Hosting::InTree { .. }) {
                this.widget.close_menu(&mut this.ctx);
            }
            this.ctx
                .submit_action::<DatePickerAction>(DatePickerAction::OpenChanged(false));
            this.ctx.request_paint_only();
        }
    }

    /// Close after a portal-mounted `CalendarBodyView` handled a day
    /// selection (see `super::view::CalendarBodyView::message`, which calls
    /// this via `mutate_later(handle)`). Unlike [`Self::mark_closed`], nothing
    /// has pre-hidden the slot content here — this IS the close — so it must
    /// honor controlled mode: gate the self-mutation behind `!controlled`,
    /// always report both `DateChanged` and `OpenChanged(false)`.
    pub(crate) fn close_for_selection(this: &mut WidgetMut<'_, Self>, date: NaiveDate) {
        if !this.widget.controlled {
            this.widget.selected = Some(date);
            this.widget.open = false;
            let display = ArcStr::from(date.format(this.widget.date_format).to_string());
            let cleanable = this.widget.cleanable;
            this.widget.close_menu(&mut this.ctx);
            with_trigger!(this, |trig| {
                DatePickerTrigger::set_text(&mut trig, display);
                DatePickerTrigger::set_has_value(&mut trig, true, cleanable);
            });
        }
        this.ctx
            .submit_action::<DatePickerAction>(DatePickerAction::DateChanged(Some(date)));
        this.ctx
            .submit_action::<DatePickerAction>(DatePickerAction::OpenChanged(false));
        this.ctx.request_paint_only();
    }

    /// Closes the calendar in response to Escape pressed *inside* it
    /// (Portal mode only — called via `mutate_later` from
    /// `CalendarBodyWidget::on_text_event`, since the calendar isn't a
    /// descendant of the trigger in that mode). Unlike `mark_closed`, this
    /// must actually hide the portal slot itself — nothing else has done so
    /// yet, unlike the outside-press-dismiss case `mark_closed` handles.
    pub(crate) fn close_from_calendar(this: &mut WidgetMut<'_, Self>) {
        if !this.widget.open {
            return;
        }
        if !this.widget.controlled {
            this.widget.open = false;
            this.widget.close_menu(&mut this.ctx);
        }
        this.ctx
            .submit_action::<DatePickerAction>(DatePickerAction::OpenChanged(false));
        this.ctx.request_paint_only();
    }
}

/// Dismiss hook registered with the portal slot (see
/// [`crate::overlay_portal::DismissHook`]): syncs `open` after an
/// outside-press dismissal via [`ThemedDatePickerWidget::mark_closed`].
pub(crate) fn date_picker_dismiss_hook(mut w: WidgetMut<'_, dyn Widget>) {
    let mut picker = w.downcast::<ThemedDatePickerWidget>();
    ThemedDatePickerWidget::mark_closed(&mut picker);
}

// ── MARK: Internal helpers ───────────────────────────────────────────────────

impl ThemedDatePickerWidget {
    /// Close the calendar panel in whichever host mounts it. Shared by every
    /// close path; generic over the context — see [`PortalCtx`].
    fn close_menu(&mut self, ctx: &mut impl PortalCtx) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.queue_mutate(overlay_host.id(), |mut w| {
                    let mut ov = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut ov, false);
                });
            }
            Hosting::Portal { binding, .. } => binding.close(ctx),
        }
    }

    /// Open the calendar panel in whichever host mounts it (trigger toggle
    /// only).
    fn open_menu(&mut self, ctx: &mut impl PortalOpenCtx) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.queue_mutate(overlay_host.id(), |mut w| {
                    let mut ov = w.downcast::<AnchoredOverlay>();
                    AnchoredOverlay::set_overlay_visible(&mut ov, true);
                });
            }
            Hosting::Portal { binding, .. } => {
                binding.open(ctx, OverlayAnchor::BottomStart, 0.0);
            }
        }
    }
}

// ── MARK: Widget impl ────────────────────────────────────────────────────────

impl Widget for ThemedDatePickerWidget {
    type Action = DatePickerAction;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => ctx.register_child(overlay_host),
            Hosting::Portal { trigger, .. } => ctx.register_child(trigger),
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                ctx.set_disabled(self.disabled);
                self.handle.set(ctx.widget_id());
                if self.open {
                    if self.disabled {
                        self.open = false;
                    } else {
                        self.open_menu(ctx);
                    }
                }
            }
            // The trigger is the actual focus target, so we react to
            // `ChildFocusChanged` — masonry's "focus entered/left my subtree"
            // signal for ancestors — rather than `FocusChanged`. Only
            // meaningful in-tree: there the calendar is a *descendant*
            // (clicks on the trigger or inside the calendar keep our subtree
            // focused), so this only fires for genuine outside clicks. In
            // portal mode the calendar lives in the scope's slot, not under
            // us — the slot's own outside-press dismissal handles it instead
            // (see `PortalSlot::dismiss_outside`).
            Update::ChildFocusChanged(false)
                if self.open && matches!(self.hosting, Hosting::InTree { .. }) =>
            {
                if !self.controlled {
                    self.open = false;
                    self.close_menu(ctx);
                }
                ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(false));
                ctx.request_paint_only();
            }
            // A trigger stashed mid-open (e.g. a tab/panel container hiding
            // us without tearing us down) can no longer be clicked to dismiss
            // its calendar, and the calendar would stay visible/painted. Close
            // eagerly for both hosting modes.
            Update::StashedChanged(true) if self.open => {
                self.open = false;
                self.close_menu(ctx);
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
                    if desired {
                        self.open_menu(ctx);
                    } else {
                        self.close_menu(ctx);
                    }
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
                    // In-tree: update trigger and body directly via the
                    // overlay host. Portal: trigger is our direct child; body
                    // is rebuilt by the view layer.
                    match &self.hosting {
                        Hosting::InTree { overlay_host } => {
                            let display = self.placeholder.clone();
                            let cleanable = self.cleanable;
                            let overlay_id = overlay_host.id();
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
                        Hosting::Portal { trigger, .. } => {
                            let display = self.placeholder.clone();
                            let cleanable = self.cleanable;
                            let trig_id = trigger.id();
                            ctx.mutate_later(trig_id, move |mut w| {
                                let mut trig = w.downcast::<DatePickerTrigger>();
                                DatePickerTrigger::set_text(&mut trig, display);
                                DatePickerTrigger::set_has_value(&mut trig, false, cleanable);
                            });
                        }
                    }
                }
                ctx.submit_action::<Self::Action>(DatePickerAction::DateChanged(None));
            }
            ctx.set_handled();
            ctx.request_paint_only();
            return;
        }

        // `CalendarBodyAction::DateSelected` only bubbles here in in-tree
        // mode (the calendar is our descendant). In portal mode that action
        // is handled by `CalendarBodyView::message` → `mutate_later` →
        // `close_for_selection` instead.
        if let Some(CalendarBodyAction::DateSelected(date)) = action.downcast_ref() {
            let date = *date;
            if !self.controlled {
                self.selected = Some(date);
                self.open = false;
                let display = ArcStr::from(date.format(self.date_format).to_string());
                let cleanable = self.cleanable;
                let overlay_id = match &self.hosting {
                    Hosting::InTree { overlay_host } => overlay_host.id(),
                    Hosting::Portal { .. } => {
                        // Should not happen in portal mode (see comment above),
                        // but if it did, there's nothing to do here.
                        ctx.submit_action::<Self::Action>(DatePickerAction::DateChanged(Some(
                            date,
                        )));
                        ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(false));
                        ctx.set_handled();
                        ctx.request_paint_only();
                        return;
                    }
                };
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
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        // Tab (no Shift): move real focus onto the calendar's first header
        // button in Portal mode — native Tab search can't discover the
        // calendar on its own (it's mounted in the scope's PortalSlot,
        // structurally distant from the trigger, not adjacent to it; see
        // the design spec's Context section). Once focus is on
        // `header_prev`, natural masonry Tab traversal walks forward
        // through the rest of the header row (month, year, next) and into
        // the grid on its own, since `CalendarBodyWidget::children_ids`
        // already lists them in that reading order — no further
        // interception is needed for that leg. InTree mode doesn't need any
        // of this: `AnchoredOverlay` already registers the calendar right
        // after the trigger, so native Tab already works there today.
        if key.key == Key::Named(NamedKey::Tab)
            && !key.modifiers.shift()
            && self.open
            && let Hosting::Portal { .. } = &self.hosting
            && let Some(header_id) = self
                .header_handle
                .as_ref()
                .and_then(CalendarHeaderHandle::widget_id)
        {
            ctx.set_focus(header_id);
            ctx.set_handled();
            return;
        }

        // Escape: close the calendar (regardless of whether nav keys work).
        if matches!(&key.key, Key::Named(NamedKey::Escape)) {
            if !self.open {
                return;
            }
            if !self.controlled {
                self.open = false;
                self.close_menu(ctx);
            }
            ctx.submit_action::<Self::Action>(DatePickerAction::OpenChanged(false));
            ctx.set_handled();
            ctx.request_paint_only();
            return;
        }

        if !self.open {
            return;
        }

        let nav_key = match &key.key {
            Key::Named(NamedKey::ArrowUp) => Some(CalendarNavKey::Up),
            Key::Named(NamedKey::ArrowDown) => Some(CalendarNavKey::Down),
            Key::Named(NamedKey::ArrowLeft) => Some(CalendarNavKey::Left),
            Key::Named(NamedKey::ArrowRight) => Some(CalendarNavKey::Right),
            Key::Named(NamedKey::Home) => Some(CalendarNavKey::Home),
            Key::Named(NamedKey::End) => Some(CalendarNavKey::End),
            Key::Named(NamedKey::Enter) => Some(CalendarNavKey::Activate),
            Key::Named(NamedKey::PageUp) if key.modifiers.shift() => Some(CalendarNavKey::PrevYear),
            Key::Named(NamedKey::PageUp) => Some(CalendarNavKey::PrevMonth),
            Key::Named(NamedKey::PageDown) if key.modifiers.shift() => {
                Some(CalendarNavKey::NextYear)
            }
            Key::Named(NamedKey::PageDown) => Some(CalendarNavKey::NextMonth),
            _ => None,
        };

        let Some(nav) = nav_key else {
            return;
        };

        match &self.hosting {
            Hosting::InTree { overlay_host } => {
                let overlay_id = overlay_host.id();
                ctx.mutate_later(overlay_id, move |mut w| {
                    let mut ov = w.downcast::<AnchoredOverlay>();
                    let mut overlay = AnchoredOverlay::overlay_mut(&mut ov);
                    let mut body = overlay.downcast::<CalendarBodyWidget>();
                    CalendarBodyWidget::handle_nav_key(&mut body, nav);
                });
            }
            Hosting::Portal { binding, .. } => {
                let Some(scope_id) = binding.scope_widget_id() else {
                    return;
                };
                let key_val = binding.key();
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                    let Some(mut child) = PortalSlot::child_mut(&mut slot, key_val) else {
                        return;
                    };
                    // BareTrigger placement: child IS the Passthrough wrapping
                    // CalendarBodyWidget (see CalendarBodyView::build).
                    let mut passthrough = child.downcast::<Passthrough>();
                    let mut body_wm = Passthrough::child_mut(&mut passthrough);
                    let mut body = body_wm.downcast::<CalendarBodyWidget>();
                    CalendarBodyWidget::handle_nav_key(&mut body, nav);
                });
            }
        }
        ctx.set_handled();
    }

    /// Re-anchors a still-open portal-mode calendar as we move in window space.
    /// Mirrors `ThemedDropdownButton::compose`; no-op in-tree.
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        let binding = match &mut self.hosting {
            Hosting::Portal { binding, .. } => Some(binding),
            Hosting::InTree { .. } => None,
        };
        binding::compose_reanchor(ctx, self.open, binding);
    }

    /// Keeps a still-open portal-mode calendar's [`Self::compose`] running
    /// every frame — see [`binding::arm_reanchor_on_anim_frame`].
    fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _: u64) {
        binding::arm_reanchor_on_anim_frame(
            ctx,
            self.open,
            matches!(self.hosting, Hosting::Portal { .. }),
        );
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.redirect_measurement(overlay_host, axis, cross_length)
            }
            Hosting::Portal { trigger, .. } => {
                ctx.redirect_measurement(trigger, axis, cross_length)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.run_layout(overlay_host, size);
                ctx.place_child(overlay_host, Point::ORIGIN);
                ctx.derive_baselines(overlay_host);
            }
            Hosting::Portal { trigger, .. } => {
                ctx.run_layout(trigger, size);
                ctx.place_child(trigger, Point::ORIGIN);
                ctx.derive_baselines(trigger);
            }
        }
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
        match &self.hosting {
            Hosting::InTree { overlay_host } => ChildrenIds::from_slice(&[overlay_host.id()]),
            Hosting::Portal { trigger, .. } => ChildrenIds::from_slice(&[trigger.id()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::date_picker::calendar_body::CalendarHeaderHandle;
    use masonry::testing::TestHarness;

    /// Builds an `OverlayScope`-rooted harness hosting a Portal-mode
    /// `ThemedDatePickerWidget` as `content` and its `CalendarBodyWidget` as
    /// the pre-registered slot child under `key` (mirrors
    /// `dropdown_button/widget.rs`'s `portal_selection_close_respects_controlled_mode`
    /// setup), clicks the trigger to open it via the real `on_action` Toggle
    /// path, and returns `(harness, trigger_id, header_id)`.
    fn open_portal_picker_for_test() -> (TestHarness<OverlayScope>, WidgetId, WidgetId) {
        use masonry::core::NewWidget;
        use masonry::kurbo::Point;
        use masonry::layout::AsUnit;
        use masonry::theme::default_property_set;
        use xilem::view::PointerButton;

        let theme = Theme::default();
        let scope_handle = OverlayScopeHandle::new();
        let trigger_handle = DatePickerHandle::new();
        let header_handle = CalendarHeaderHandle::new();
        let key = 1;

        let trigger = ThemedDatePickerWidget::new_portal(
            None,
            ArcStr::from("Pick a date"),
            "%Y-%m-%d",
            None,
            None,
            false,
            false,
            &theme,
            trigger_handle.clone(),
            scope_handle.clone(),
            key,
            header_handle.clone(),
        )
        .with_open_state(false, false);
        let sized = masonry::widgets::SizedBox::new(NewWidget::new(trigger).erased())
            .width(100.0.px())
            .height(40.0.px())
            .prepare();
        let content =
            masonry::widgets::Align::new(masonry::layout::UnitPoint::TOP_LEFT, sized.erased())
                .prepare()
                .erased();
        let body = NewWidget::new(CalendarBodyWidget::new_with_trigger_handle(
            chrono::Local::now().date_naive(),
            None,
            None,
            None,
            &theme,
            &header_handle,
            Some(trigger_handle.clone()),
        ))
        .erased();
        let scope = OverlayScope::new(
            scope_handle,
            content,
            vec![(
                key,
                body,
                crate::overlay_portal::PortalPlacement::BareTrigger,
            )],
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(scope));

        let trigger_id = trigger_handle
            .widget_id()
            .expect("trigger reports its id at Update::WidgetAdded during harness construction");
        h.mouse_move(Point::new(50.0, 20.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        let header_id = header_handle
            .widget_id()
            .expect("grid reports its id at Update::WidgetAdded during harness construction");

        (h, trigger_id, header_id)
    }

    #[test]
    fn selecting_a_day_returns_focus_to_the_trigger() {
        let (mut h, trigger_id, header_id) = open_portal_picker_for_test();
        // Focus stays on the trigger right after opening (per Task 1's
        // Tab-interception design — there is no auto-focus-on-open). Press
        // Tab first to move real focus onto the grid, matching how a real
        // user would reach it before selecting a day via the keyboard.
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        assert_eq!(h.focused_widget_id(), Some(header_id));

        // Seed the roving focus onto a real day cell first — ArrowDown from
        // no prior focus lands on the first non-disabled cell (unlike
        // ArrowRight/Left/Up, which need an existing reference cell and
        // no-op when there isn't one yet).
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));

        assert_eq!(
            h.focused_widget_id(),
            Some(trigger_id),
            "Enter-selecting a day should return real focus to the trigger"
        );
    }

    #[test]
    fn escape_in_open_calendar_returns_focus_to_the_trigger() {
        let (mut h, trigger_id, header_id) = open_portal_picker_for_test();
        // Same as above: Tab first to get real focus onto the grid, since
        // opening alone no longer does this.
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        assert_eq!(h.focused_widget_id(), Some(header_id));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));

        assert_eq!(
            h.focused_widget_id(),
            Some(trigger_id),
            "Escape while the calendar holds focus should return real focus to the trigger"
        );
    }

    #[test]
    fn tab_after_opening_moves_real_focus_to_the_header_row() {
        let (mut h, _trigger_id, header_id) = open_portal_picker_for_test();
        // `_trigger_id` (from `DatePickerHandle`) identifies the outer
        // composite `ThemedDatePickerWidget`, which does not itself accept
        // focus — the click that opened the picker gave real focus to the
        // inner `DatePickerTrigger` sub-widget instead (its own
        // `ctx.request_focus()` in `on_pointer_event`), which has no handle
        // exposed to this test module. So this asserts the behavioral claim
        // directly — real focus did NOT jump onto the header row on open,
        // and something real still holds it — rather than pinning to an id
        // that was never the actual focus target.
        let focused_after_open = h.focused_widget_id();
        assert_ne!(
            focused_after_open,
            Some(header_id),
            "opening the picker should NOT auto-move focus onto the header row — the trigger keeps it until Tab is pressed"
        );
        assert_ne!(
            focused_after_open, None,
            "the trigger should hold real focus after the click that opened the picker"
        );

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));

        assert_eq!(
            h.focused_widget_id(),
            Some(header_id),
            "Tab, pressed while the trigger holds focus and the calendar is open, should move real focus onto the first header button (header_prev) — matching natural top-to-bottom reading order, not the grid"
        );
    }

    #[test]
    fn tab_from_header_row_walks_forward_into_the_grid() {
        // Once Tab has moved focus onto `header_prev` (the interception's
        // job), the remaining hops — through month, year, next, and into
        // the grid — are *not* intercepted; they rely on native masonry Tab
        // traversal following `CalendarBodyWidget::children_ids`'s
        // documented order (prev, month, year, next, weekday labels, grid).
        // This proves that structural ordering actually holds, rather than
        // just asserting the single interception hop.
        let (mut h, _trigger_id, header_id) = open_portal_picker_for_test();
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        assert_eq!(h.focused_widget_id(), Some(header_id));

        let mut seen = Vec::new();
        for _ in 0..4 {
            h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
            seen.push(h.focused_widget_id());
        }

        // month, year, next are each distinct real widgets, and none of
        // them repeat header_id or each other.
        for id in &seen[..3] {
            assert_ne!(*id, Some(header_id));
            assert_ne!(*id, None);
        }
        assert!(
            seen[..3]
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                == 3,
            "month/year/next should be three distinct real widgets, not the same one revisited: {seen:?}"
        );

        let landed_on_grid = seen[3].is_some_and(|id| {
            h.get_widget_with_id(id)
                .type_name()
                .contains("CalendarGridWidget")
        });
        assert!(
            landed_on_grid,
            "the fourth forward Tab from header_prev should land on CalendarGridWidget: {seen:?}"
        );
    }

    #[test]
    fn shift_tab_from_header_row_leaves_the_header_button() {
        // `header_prev` is now the Tab-interception's landing target, so
        // Shift+Tab from it is no longer the primary way to discover the
        // header row (that's Tab, asserted above). This just confirms
        // Shift+Tab from the interception's own landing spot doesn't get
        // silently swallowed — it still lands on a real widget somewhere
        // (native masonry backward traversal, uninterupted for this leg).
        let (mut h, _trigger_id, header_id) = open_portal_picker_for_test();
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        assert_eq!(h.focused_widget_id(), Some(header_id));

        h.process_text_event(TextEvent::Keyboard(
            masonry::core::keyboard::KeyboardEvent {
                state: KeyState::Down,
                key: Key::Named(NamedKey::Tab),
                modifiers: masonry::core::keyboard::Modifiers::SHIFT,
                ..Default::default()
            },
        ));

        let after = h.focused_widget_id();
        assert_ne!(
            after, None,
            "Shift+Tab from header_prev should land on a real focusable widget, not nothing"
        );
    }

    #[test]
    fn intree_mode_tab_from_trigger_still_reaches_calendar_content() {
        use masonry::kurbo::Point;
        use masonry::theme::default_property_set;
        use xilem::view::PointerButton;

        let theme = Theme::default();
        let widget = ThemedDatePickerWidget::new(
            None,
            ArcStr::from("Pick a date"),
            "%Y-%m-%d",
            None,
            None,
            false,
            false,
            &theme,
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.edit_root_widget(|mut wm| {
            wm.widget.open = true;
            let Hosting::InTree { overlay_host } = &mut wm.widget.hosting else {
                panic!("expected InTree hosting");
            };
            let mut ov = wm.ctx.get_mut(overlay_host);
            AnchoredOverlay::set_overlay_visible(&mut ov, true);
        });

        // `h.root_id()` is the outer composite `ThemedDatePickerWidget`, which
        // never overrides `accepts_focus` (only the inner `DatePickerTrigger`
        // sub-widget does) — `h.focus_on` on a non-accepting id doesn't give
        // it real focus, so `find_next_focusable` never has a real anchor and
        // silently degenerates into "first focusable widget in the tree",
        // which happens to be the trigger regardless of whether Tab can
        // actually reach the calendar. Mirror
        // `tab_in_intree_mode_does_not_hit_the_portal_only_branch`'s
        // technique instead: a real mouse press (deliberately not released,
        // so `DatePickerTriggerAction::Toggle` never fires and re-closes the
        // picker we just opened above) gives the trigger genuine focus via
        // `ClickPhase::Down`'s `ctx.request_focus()`.
        h.mouse_move(Point::new(10.0, 10.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        let focused_after_press = h.focused_widget_id();
        assert_ne!(
            focused_after_press, None,
            "pressing on the InTree trigger should give it real focus"
        );

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));

        let focused_after_tab = h.focused_widget_id();
        assert_ne!(
            focused_after_tab, None,
            "Tab from an open InTree-mode picker's trigger should land on a real focusable \
             widget, not nothing"
        );
        assert_ne!(
            focused_after_tab, focused_after_press,
            "Tab from an open InTree-mode picker's trigger should move real focus into the \
             calendar content, not leave it on the trigger"
        );
    }

    #[test]
    fn tab_in_intree_mode_does_not_hit_the_portal_only_branch() {
        use masonry::core::NewWidget;
        use masonry::kurbo::Point;
        use masonry::theme::default_property_set;
        use xilem::view::PointerButton;

        let theme = Theme::default();
        let widget = ThemedDatePickerWidget::new(
            None,
            ArcStr::from("Pick a date"),
            "%Y-%m-%d",
            None,
            None,
            false,
            false,
            &theme,
        );
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        // Actually open the InTree picker via direct `WidgetMut`
        // manipulation, so `self.open` is genuinely `true` when Tab is
        // pressed below — the whole point of this test is to exercise the
        // Tab branch's `Hosting::Portal` guard, which sits *behind* an
        // `self.open` check in the match. If we never open the picker
        // (as the original, buggy version of this test did — press-only,
        // no release, so `self.open` stayed `false` throughout), the
        // `self.open` check alone already short-circuits the whole guard
        // and the `Hosting::Portal` clause is never evaluated, making the
        // test pass identically whether or not that clause exists. This
        // mirrors `open_menu`'s own `Hosting::InTree` branch (flip `open`,
        // make the `AnchoredOverlay`'s overlay child visible) without going
        // through a real click, so a subsequent press below doesn't need to
        // *also* complete a `Toggle` action that would re-close it.
        h.edit_root_widget(|mut wm| {
            wm.widget.open = true;
            let Hosting::InTree { overlay_host } = &mut wm.widget.hosting else {
                panic!("expected InTree hosting");
            };
            let mut ov = wm.ctx.get_mut(overlay_host);
            AnchoredOverlay::set_overlay_visible(&mut ov, true);
        });

        // `h.root_id()` is the outer composite `ThemedDatePickerWidget`,
        // which does not itself accept focus (only the inner
        // `DatePickerTrigger` sub-widget does) — `h.focus_on` on a
        // non-accepting id doesn't give it real focus, so a real pointer
        // press is used instead to match how focus actually gets set in
        // practice. Deliberately press-only, no release: a full click
        // completes `DatePickerTriggerAction::Toggle`, which would
        // re-toggle (close) the picker we just opened above.
        // `ClickPhase::Down` alone already calls `ctx.request_focus()`
        // without completing the click, so this gives the trigger real
        // focus while leaving `open` (and the now-visible overlay) as we
        // set it.
        h.mouse_move(Point::new(10.0, 10.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        let focused_after_press = h.focused_widget_id();
        assert_ne!(
            focused_after_press, None,
            "pressing on the InTree trigger should give it real focus"
        );

        // With the picker genuinely open, the Tab branch's `self.open` check
        // now passes, so this is the first time the `Hosting::Portal` guard
        // is actually evaluated — and it should fail (`InTree`, not
        // `Portal`), letting the event fall through to whatever InTree's
        // existing (unchanged) on_text_event logic does with Tab (nothing —
        // it was never decoded there either, before or after this plan) and
        // then on to native Tab traversal, which — because `AnchoredOverlay`
        // registers the now-visible calendar right after the trigger — can
        // discover it on its own.
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));

        let focused_after_tab = h.focused_widget_id();
        assert_ne!(
            focused_after_tab, None,
            "Tab must not leave focus in an invalid state; if the Portal-only branch had \
             incorrectly fired here, ctx.set_handled() would have blocked native Tab search \
             from running at all"
        );
        assert_ne!(
            focused_after_tab, focused_after_press,
            "Tab, with the InTree picker genuinely open, should move real focus off the \
             trigger and into the open calendar content via native Tab traversal — proving \
             the new Portal-only branch's Hosting::Portal guard correctly did not intercept \
             it here"
        );
    }

    /// Regression coverage through the *actual* xilem View pipeline —
    /// `demo::panel(...)` exactly as the gallery composes it (six
    /// `date_picker`s sharing one `overlay_scope`, wrapped in a
    /// `scroll_container`) — rather than `open_portal_picker_for_test`'s
    /// hand-built `OverlayScope::new(...)` widget tree. Added after a live
    /// report that Tab didn't reach the header row despite the hand-built
    /// harness's tests passing: the hand-built tree turned out to still be
    /// faithful (both this test and the hand-built ones now agree), but
    /// this exercises the real `date_picker()`/`overlay_scope()`/
    /// `scroll_container()` composition the hand-built harness never
    /// touches, so a future divergence between the two would be caught
    /// here.
    #[test]
    fn real_view_pipeline_tab_reaches_the_header_row_then_the_grid() {
        use crate::components::date_picker::demo;
        use masonry::core::NewWidget;
        use masonry::testing::TestHarness;
        use xilem::ViewCtx;
        use xilem::core::{RawProxy, View};
        use xilem::view::PointerButton;

        struct S;
        let mut state = S;

        let theme = crate::theme::Theme::default();
        let view = demo::panel(&theme);

        let proxy: std::sync::Arc<dyn RawProxy> = crate::test_support::noop_proxy();
        let runtime = crate::test_support::current_thread_runtime();
        let mut ctx = ViewCtx::new(proxy, runtime);
        let (pod, _view_state) = view.build(&mut ctx, &mut state);

        let root = masonry::widgets::Flex::column().with_fixed(pod.new_widget);
        let mut h = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(root),
            (900, 1400),
        );

        h.render();
        let mut trigger_ids = Vec::new();
        h.inspect_widgets(|w| {
            if w.type_name().contains("DatePickerTrigger") {
                trigger_ids.push(w.id());
            }
        });
        let first_trigger = trigger_ids[0];

        h.mouse_click_on(first_trigger, Some(PointerButton::Primary));
        h.render();

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        let after_tab = h.focused_widget_id();
        assert!(
            after_tab.is_some_and(|id| h
                .get_widget_with_id(id)
                .type_name()
                .contains("ThemedButton")),
            "Tab from the real gallery panel's first trigger should land on a real header button: {after_tab:?}"
        );

        // The interception's single hop only reaches `header_prev` (the
        // first of 4 header buttons); the remaining 3 buttons (month, year,
        // next) plus the final hop into the grid are all native masonry
        // traversal — 4 more Tab presses in total.
        for _ in 0..4 {
            h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        }
        let after_five_tabs = h.focused_widget_id();
        assert!(
            after_five_tabs.is_some_and(|id| h
                .get_widget_with_id(id)
                .type_name()
                .contains("CalendarGridWidget")),
            "five Tabs total (one interception hop into header_prev, then four native hops \
             through month/year/next and into the grid) should land on the grid: {after_five_tabs:?}"
        );
    }
}
