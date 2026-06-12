//! `PopoverHost` — transparent trigger wrapper that opens floating content on
//! click, hosted by an in-tree [`AnchoredOverlay`] — the same fallback
//! `ThemedDropdownButton` uses when no ancestor [`OverlayScope`] is present.
//!
//! Popover content is an arbitrary, *stateful* `WidgetView`-built widget. It
//! must be built once (in `View::build`/`rebuild`, where `ViewCtx`/`&mut
//! State` are available) and live for the popover's entire lifetime — it
//! cannot be torn down and freshly reconstructed from an event-handler
//! closure the way `ThemedDropdownButton`'s stateless `MenuContent` is for
//! its scope-push path. That rules out ever pushing it into an ancestor
//! `OverlayScope` via `mutate_later`: doing so would require capturing a
//! pre-built `NewWidget<dyn Widget>` in a `Send`-bound closure, and
//! `Box`/`WidgetPod`/`NewWidget<dyn Widget>` can never be `Send` (`Widget`
//! has no `Send` supertrait upstream). So `PopoverHost` always hosts content
//! permanently inside `overlay_host`, toggled visible via
//! `AnchoredOverlay::set_overlay_visible` — it tracks our movement for free as
//! a rigidly-attached descendant, and is clipped by whatever ancestor clips
//! us (no window-bleed).
//!
//! The gallery's `with_source!` macro places the live demo *after* its
//! source-code block in paint order specifically so this in-tree overlay wins
//! against that later-occluding sibling — see `void-ui-macros`.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::Padding;
use masonry::widgets::ButtonPress;

use super::PopoverAnchor;
use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::click::{self, ClickPhase};

/// Corner radius of the popover surface's chrome.
const CORNER_RADIUS: f64 = 5.0;
/// Border width of the popover surface's chrome.
const BORDER_WIDTH: f64 = 1.0;

/// Gap between the trigger and the popover surface, scaled with density.
fn surface_gap(theme: &Theme) -> Length {
    Length::px(f64::from(theme.density.pad) / 3.0)
}

/// Transparent wrapper around a trigger child that opens floating content on
/// click. See module docs for the hosting strategy.
///
/// Clicking the trigger toggles the popover. Losing focus (e.g. clicking
/// outside) or pressing Escape while focused closes it.
pub struct PopoverHost {
    overlay_host: WidgetPod<AnchoredOverlay>,
    open: bool,
    anchor: PopoverAnchor,
    theme: Theme,
    /// The trigger's own widget id, captured at construction. Used by
    /// `on_action` to ignore bubbled `ButtonPress` actions that originate
    /// from inside the popover content rather than the trigger itself.
    trigger_id: WidgetId,
}

// --- MARK: BUILDERS
impl PopoverHost {
    #[must_use]
    pub fn new(
        trigger: NewWidget<impl Widget + ?Sized>,
        mut content: NewWidget<impl Widget + ?Sized>,
        anchor: PopoverAnchor,
        theme: &Theme,
    ) -> Self {
        let trigger = trigger.erased();
        let trigger_id = trigger.id();
        content
            .properties
            .insert(Padding::all(Length::px(f64::from(theme.density.pad))));
        let surface = NewWidget::new(PopoverSurface::new(content.erased(), theme)).erased();
        let overlay_host =
            AnchoredOverlay::new(trigger, surface, false, anchor).with_gap(surface_gap(theme));
        Self {
            overlay_host: NewWidget::new(overlay_host).to_pod(),
            open: false,
            anchor,
            theme: *theme,
            trigger_id,
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl PopoverHost {
    /// Update the theme, refreshing the permanently-mounted surface's chrome
    /// colors.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
            AnchoredOverlay::set_gap(&mut overlay_host, surface_gap(theme));
            let mut overlay = AnchoredOverlay::overlay_mut(&mut overlay_host);
            let mut surface = overlay.downcast::<PopoverSurface>();
            PopoverSurface::set_theme(&mut surface, theme);
        }
    }

    /// Change the anchor, forwarded immediately to the live `AnchoredOverlay`.
    pub fn set_anchor(this: &mut WidgetMut<'_, Self>, anchor: PopoverAnchor) {
        if this.widget.anchor != anchor {
            this.widget.anchor = anchor;
            let mut overlay_host = this.ctx.get_mut(&mut this.widget.overlay_host);
            AnchoredOverlay::set_anchor(&mut overlay_host, anchor);
        }
    }

    /// Mutable access to the `overlay_host` for the view layer, which threads
    /// both the trigger (`overlay_host_mut → AnchoredOverlay::primary_mut`)
    /// and the content
    /// (`overlay_host_mut → AnchoredOverlay::overlay_mut → downcast::<PopoverSurface> → content_mut`)
    /// through it.
    pub fn overlay_host_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, AnchoredOverlay> {
        this.ctx.get_mut(&mut this.widget.overlay_host)
    }
}

// --- MARK: IMPL WIDGET
impl Widget for PopoverHost {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match click::primary_click(ctx, event) {
            Some(ClickPhase::Down) => {
                ctx.request_focus();
            }
            Some(ClickPhase::Up(Some(_))) => {
                let open = !self.open;
                self.open = open;
                ctx.mutate_child_later(&mut self.overlay_host, move |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, open);
                });
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
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && event.key == Key::Named(NamedKey::Escape)
            && self.open
        {
            ctx.set_handled();
            self.open = false;
            ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                AnchoredOverlay::set_overlay_visible(&mut w, false);
            });
            ctx.request_paint_only();
        }
    }

    /// Routes a keyboard-issued `ButtonPress` (`button: None`, emitted on
    /// Enter/Space while the trigger is focused) into the open/close toggle,
    /// mirroring `ThemedDropdownButton::on_action`. Action bubbling is
    /// independent of `EventCtx::set_handled`, so this fires even though the
    /// trigger itself consumes the keyboard event. Pointer clicks are handled
    /// by `on_pointer_event` instead — a `ButtonPress` with `button: Some(_)`
    /// is ignored here to avoid double-toggling.
    ///
    /// `ButtonPress` actions bubble from anywhere in the subtree, including
    /// buttons inside the popover content — only react when `source` is the
    /// trigger itself, so activating a button inside the open content doesn't
    /// also toggle the popover.
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if let Some(press) = action.downcast_ref::<ButtonPress>()
            && press.button.is_none()
            && source == self.trigger_id
        {
            ctx.set_handled();
            let open = !self.open;
            self.open = open;
            ctx.mutate_child_later(&mut self.overlay_host, move |mut w| {
                AnchoredOverlay::set_overlay_visible(&mut w, open);
            });
            ctx.request_paint_only();
        }
    }

    /// Reacts to `ChildFocusChanged` — masonry's "focus entered/left my
    /// subtree" signal for ancestors — rather than `FocusChanged`, since the
    /// trigger is the actual focus target. A click landing outside our
    /// subtree clears focus from it, the standard "click outside to dismiss"
    /// path; the open content remains a permanently-mounted descendant
    /// (inside `overlay_host`), so this only fires for genuine outside clicks
    /// — exactly mirrors `ThemedDropdownButton::update`.
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            Update::ChildFocusChanged(false) if self.open => {
                self.open = false;
                ctx.mutate_child_later(&mut self.overlay_host, |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, false);
                });
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.overlay_host);
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
        ChildrenIds::from_slice(&[self.overlay_host.id()])
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

/// Transparent wrapper that paints rounded background/border chrome around
/// arbitrary popover content. `AnchoredOverlay` is purely structural — it
/// doesn't paint chrome — so whatever it hosts must paint its own (mirrors
/// `MenuContent`, which does the same for dropdown menus).
pub(super) struct PopoverSurface {
    content: WidgetPod<dyn Widget>,
    bg: Color,
    border: Color,
    pad: f32,
}

impl PopoverSurface {
    fn new(content: NewWidget<dyn Widget>, theme: &Theme) -> Self {
        Self {
            content: content.to_pod(),
            bg: theme.palette.surface_hi,
            border: theme.palette.border_strong,
            pad: theme.density.pad,
        }
    }

    fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        let bg = theme.palette.surface_hi;
        let border = theme.palette.border_strong;
        if this.widget.bg != bg || this.widget.border != border {
            this.widget.bg = bg;
            this.widget.border = border;
            this.ctx.request_paint_only();
        }
        if (this.widget.pad - theme.density.pad).abs() > f32::EPSILON {
            this.widget.pad = theme.density.pad;
            let pad = Padding::all(Length::px(f64::from(theme.density.pad)));
            Self::content_mut(this).insert_prop(pad);
        }
    }

    /// Mutable access to the wrapped content for the view layer.
    pub(super) fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.content)
    }
}

impl Widget for PopoverSurface {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.content);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.content, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.content, size);
        ctx.place_child(&mut self.content, Point::ORIGIN);
        ctx.derive_baselines(&self.content);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let rrect =
            RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box_size(), CORNER_RADIUS);
        if self.bg.components[3] > 0.0 {
            painter.fill(rrect, self.bg).draw();
        }
        if self.border.components[3] > 0.0 {
            painter
                .stroke(rrect, &Stroke::new(BORDER_WIDTH), self.border)
                .draw();
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

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
        ChildrenIds::from_slice(&[self.content.id()])
    }
}

// --- MARK: TESTS

#[cfg(test)]
mod tests {
    use masonry::core::TextEvent;
    use masonry::core::keyboard::{Key, NamedKey};
    use masonry::core::{Handled, NewWidget};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::*;
    use crate::components::button::widget::ThemedButton;

    fn button(label: &str, theme: &Theme) -> NewWidget<dyn Widget> {
        NewWidget::new(ThemedButton::new(
            NewWidget::new(Label::new(label)).erased(),
            theme,
        ))
        .erased()
    }

    /// Builds a `PopoverHost` with a button trigger and label content,
    /// returning the harness and the trigger's `WidgetId`.
    fn harness() -> (TestHarness<PopoverHost>, WidgetId) {
        let theme = Theme::dark();
        let trigger = button("Open", &theme);
        let trigger_id = trigger.id();
        let content = NewWidget::new(Label::new("Content")).erased();
        let widget = PopoverHost::new(trigger, content, PopoverAnchor::BottomStart, &theme);
        let h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        (h, trigger_id)
    }

    #[test]
    fn enter_on_the_trigger_toggles_open() {
        let (mut h, trigger_id) = harness();
        h.focus_on(Some(trigger_id));

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.edit_root_widget(|wm| wm.widget.open));

        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(!h.edit_root_widget(|wm| wm.widget.open));
    }

    #[test]
    fn space_on_the_trigger_toggles_open() {
        let (mut h, trigger_id) = harness();
        h.focus_on(Some(trigger_id));

        h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
        assert!(h.edit_root_widget(|wm| wm.widget.open));
    }

    #[test]
    fn escape_closes_and_is_handled() {
        let (mut h, trigger_id) = harness();
        h.focus_on(Some(trigger_id));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.edit_root_widget(|wm| wm.widget.open));

        let handled = h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Escape)));
        assert_eq!(handled, Handled::Yes);
        assert!(!h.edit_root_widget(|wm| wm.widget.open));
    }

    /// A keyboard activation of a button inside the open popover content
    /// bubbles a `ButtonPress { button: None }` action just like the
    /// trigger's does — `on_action` must only react when it originates from
    /// the trigger itself.
    #[test]
    fn button_press_from_content_does_not_toggle_popover() {
        let theme = Theme::dark();
        let trigger = button("Open", &theme);
        let trigger_id = trigger.id();
        let content = button("Inside", &theme);
        let content_id = content.id();
        let widget = PopoverHost::new(trigger, content, PopoverAnchor::BottomStart, &theme);
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));

        h.focus_on(Some(trigger_id));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(h.edit_root_widget(|wm| wm.widget.open));

        h.focus_on(Some(content_id));
        h.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert!(
            h.edit_root_widget(|wm| wm.widget.open),
            "activating a button inside the popover content must not toggle the popover"
        );
    }
}
