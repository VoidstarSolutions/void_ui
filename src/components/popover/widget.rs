//! `PopoverHost` — transparent trigger wrapper that opens floating content
//! on click. Two hosting modes:
//!
//! - **Portal** (scope ancestor present): the content view is mounted by the
//!   scope's own view into [`crate::overlay_portal::PortalSlot`] — the
//!   scope's always-last-painted child — so the open popover paints above
//!   everything inside the scope. The content stays a real xilem view child
//!   (of the scope), so rebuilds, theme swaps, and button callbacks all work;
//!   this host only pushes open/placement as plain data via `mutate_later`
//!   (the `Send` bound on `mutate_later` closures is irrelevant to data).
//!   Dismissal is the slot's backdrop (click anywhere in the scope outside
//!   the popover) plus Escape on the focused trigger — *not*
//!   `ChildFocusChanged`, since the content is not our descendant in this
//!   mode and clicking it would otherwise close the popover.
//! - **In-tree** (fallback, no scope): content permanently mounted in an
//!   [`AnchoredOverlay`] below the trigger, toggled via
//!   `set_overlay_visible`; paints above earlier-painted siblings only.
//!
//! **Dismissal asymmetry:** in-tree popovers also close when focus leaves the
//! trigger subtree (Tab away), because the content is a descendant and
//! `ChildFocusChanged(false)` fires reliably. Portal popovers close only via
//! backdrop click, Escape, or trigger toggle — the content is not a
//! descendant, so Tab-away focus loss does not reach the host.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget, WidgetId, WidgetMut, WidgetPod,
    AccessCtx, ChildrenIds, ComposeCtx, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, RoundedRect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::Padding;
use masonry::widgets::ButtonPress;

use super::PopoverAnchor;
use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::components::click::{self, ClickPhase};
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// Corner radius of the popover surface's chrome.
const CORNER_RADIUS: f64 = 5.0;
/// Border width of the popover surface's chrome.
const BORDER_WIDTH: f64 = 1.0;

/// Gap between the trigger and the popover surface, scaled with density.
fn surface_gap(theme: &Theme) -> Length {
    Length::px(f64::from(theme.density.pad) / 3.0)
}

/// How this host mounts its content: permanently in-tree (fallback, no
/// scope ancestor), or portal-mounted in the nearest scope's `PortalSlot`
/// (content is a view child of the *scope*; we only hold the key).
enum Hosting {
    InTree {
        overlay_host: WidgetPod<AnchoredOverlay>,
    },
    Portal {
        trigger: WidgetPod<dyn Widget>,
        scope: OverlayScopeHandle,
        key: u64,
        /// Last window-space anchor rect pushed to the slot; `compose` uses
        /// it to re-anchor only when we actually moved (mirrors
        /// `ThemedDropdownButton::compose`).
        last_anchor_rect_window: Option<Rect>,
    },
}

/// Transparent wrapper around a trigger child that opens floating content on
/// click. See module docs for the hosting strategy.
///
/// Clicking the trigger toggles the popover. Pressing Escape while focused
/// closes it; so does clicking outside (via focus loss in-tree, via the
/// portal slot's backdrop in portal mode).
pub struct PopoverHost {
    hosting: Hosting,
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
            hosting: Hosting::InTree {
                overlay_host: NewWidget::new(overlay_host).to_pod(),
            },
            open: false,
            anchor,
            theme: *theme,
        }
    }

    /// Portal-mode constructor: the content lives in the scope's slot under
    /// `key`; we host only the trigger.
    #[must_use]
    pub(crate) fn new_portal(
        trigger: NewWidget<impl Widget + ?Sized>,
        anchor: PopoverAnchor,
        theme: &Theme,
        scope: OverlayScopeHandle,
        key: u64,
    ) -> Self {
        Self {
            hosting: Hosting::Portal {
                trigger: trigger.erased().to_pod(),
                scope,
                key,
                last_anchor_rect_window: None,
            },
            open: false,
            anchor,
            theme: *theme,
            trigger_id,
        }
    }
}

// --- MARK: WIDGETMUT SETTERS
impl PopoverHost {
    /// Update the theme. In-tree this refreshes the permanently-mounted
    /// surface's chrome immediately; in portal mode the surface chrome flows
    /// through the registry → scope rebuild, so we only store the theme and
    /// re-push the (gap-bearing) open state if currently open.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut overlay_host = this.ctx.get_mut(overlay_host);
                AnchoredOverlay::set_gap(&mut overlay_host, surface_gap(theme));
                let mut overlay = AnchoredOverlay::overlay_mut(&mut overlay_host);
                let mut surface = overlay.downcast::<PopoverSurface>();
                PopoverSurface::set_theme(&mut surface, theme);
            }
            Hosting::Portal { .. } => Self::refresh_portal(this),
        }
    }

    /// Change the anchor — forwarded immediately to the live `AnchoredOverlay`
    /// in-tree, re-pushed to the scope's slot (if open) in portal mode.
    pub fn set_anchor(this: &mut WidgetMut<'_, Self>, anchor: PopoverAnchor) {
        if this.widget.anchor == anchor {
            return;
        }
        this.widget.anchor = anchor;
        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => {
                let mut overlay_host = this.ctx.get_mut(overlay_host);
                AnchoredOverlay::set_anchor(&mut overlay_host, anchor);
            }
            Hosting::Portal { .. } => Self::refresh_portal(this),
        }
    }

    /// Re-push current open/placement state to the scope's slot (portal mode,
    /// open popovers only). Window rect is recomputed from our current box.
    fn refresh_portal(this: &mut WidgetMut<'_, Self>) {
        if !this.widget.open {
            return;
        }
        let Hosting::Portal {
            scope,
            key,
            last_anchor_rect_window,
            ..
        } = &mut this.widget.hosting
        else {
            return;
        };
        let Some(scope_id) = scope.widget_id() else {
            return;
        };
        let key = *key;
        let owner = Some(this.ctx.widget_id());
        let anchor = this.widget.anchor;
        let gap = surface_gap(&this.widget.theme).get();
        let rect = Rect::from_origin_size(
            this.ctx.to_window(Point::ORIGIN),
            this.ctx.border_box_size(),
        );
        *last_anchor_rect_window = Some(rect);
        this.ctx.mutate_later(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_visible(&mut scope, key, true, owner, rect, anchor, gap);
        });
    }

    /// Sync the host's `open` flag after the portal slot dismissed its
    /// content (backdrop click). The slot already hid the content; this
    /// only keeps the trigger's notion of "open" honest so the next click
    /// re-opens instead of "closing" an already-closed popover.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if this.widget.open {
            this.widget.open = false;
            this.ctx.request_paint_only();
        }
    }

    /// Mutable access to the `overlay_host` for the view layer's in-tree
    /// paths, which thread both the trigger
    /// (`overlay_host_mut → AnchoredOverlay::primary_mut`) and the content
    /// (`overlay_host_mut → AnchoredOverlay::overlay_mut → downcast::<PopoverSurface> → content_mut`)
    /// through it.
    ///
    /// # Panics
    ///
    /// Panics for portal-mode popovers, which have no `overlay_host` — the
    /// view layer must match on its own binding and use [`Self::trigger_mut`]
    /// instead.
    pub fn overlay_host_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, AnchoredOverlay> {
        match &mut this.widget.hosting {
            Hosting::InTree { overlay_host } => this.ctx.get_mut(overlay_host),
            Hosting::Portal { .. } => {
                panic!("overlay_host_mut is only valid for in-tree popovers")
            }
        }
    }

    /// Trigger access for portal-mode popovers (in-tree mode reaches the
    /// trigger via `overlay_host_mut` → `AnchoredOverlay::primary_mut`).
    ///
    /// # Panics
    ///
    /// Panics for in-tree popovers — the view layer must match on its own
    /// binding and use [`Self::overlay_host_mut`] instead.
    pub fn trigger_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        match &mut this.widget.hosting {
            Hosting::Portal { trigger, .. } => this.ctx.get_mut(trigger),
            Hosting::InTree { .. } => panic!("trigger_mut is only valid for portal popovers"),
        }
    }
}

// --- MARK: INTERNAL HELPERS
impl PopoverHost {
    /// Push `open` to whichever host mounts the content. `EventCtx` flavor —
    /// used by click and Escape handling.
    fn push_open_state(&mut self, ctx: &mut EventCtx<'_>, open: bool) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => {
                ctx.mutate_child_later(overlay_host, move |mut w| {
                    AnchoredOverlay::set_overlay_visible(&mut w, open);
                });
            }
            Hosting::Portal {
                scope,
                key,
                last_anchor_rect_window,
                ..
            } => {
                // Unreachable in practice: the OnceLock fills at the scope's
                // `WidgetAdded`, before any descendant event can fire.
                let Some(scope_id) = scope.widget_id() else {
                    return;
                };
                let key = *key;
                let owner = Some(ctx.widget_id());
                let anchor = self.anchor;
                let gap = surface_gap(&self.theme).get();
                let rect =
                    Rect::from_origin_size(ctx.to_window(Point::ORIGIN), ctx.border_box_size());
                *last_anchor_rect_window = Some(rect);
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    OverlayScope::set_portal_visible(
                        &mut scope, key, open, owner, rect, anchor, gap,
                    );
                });
                if open {
                    ctx.request_compose();
                }
            }
        }
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
                self.push_open_state(ctx, open);
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
            self.push_open_state(ctx, false);
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
    /// path.
    ///
    /// **In-tree only:** there the open content is a permanently-mounted
    /// descendant (inside `overlay_host`), so this only fires for genuine
    /// outside clicks — exactly mirrors `ThemedDropdownButton::update`. In
    /// portal mode the content lives in the scope's slot, *not* under us, so
    /// clicking it would clear our child-focus and wrongly close the popover;
    /// the slot's backdrop handles outside-click dismissal instead.
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            Update::ChildFocusChanged(false) if self.open => {
                if let Hosting::InTree { overlay_host } = &mut self.hosting {
                    self.open = false;
                    ctx.mutate_child_later(overlay_host, |mut w| {
                        AnchoredOverlay::set_overlay_visible(&mut w, false);
                    });
                    ctx.request_paint_only();
                }
            }
            // A trigger stashed mid-open (e.g. a tab/panel container hiding us
            // without tearing us down) can no longer be clicked to dismiss its
            // popover, and in portal mode the slot child would stay visible while
            // we stop painting. Close eagerly — mirrors `ThemedDropdownButton`'s
            // disabled-mid-open close. In-tree mode self-heals (the overlay is a
            // stashed descendant), so only portal mode needs the push.
            Update::StashedChanged(true) if self.open => {
                self.open = false;
                if let Hosting::Portal { scope, key, .. } = &self.hosting {
                    if let Some(scope_id) = scope.widget_id() {
                        let key = *key;
                        ctx.mutate_later(scope_id, move |mut w| {
                            let mut scope = w.downcast::<OverlayScope>();
                            OverlayScope::set_portal_visible(
                                &mut scope,
                                key,
                                false,
                                None,
                                Rect::ZERO,
                                PopoverAnchor::BottomStart,
                                0.0,
                            );
                        });
                    }
                }
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    /// Re-anchors a still-open portal popover as we move in window space —
    /// e.g. while the user scrolls a `ScrollContainer` containing us. The
    /// content lives in a structurally separate subtree (the scope's portal
    /// slot), so — unlike `AnchoredOverlay`, which tracks for free as a
    /// rigidly-attached descendant — nothing re-places it automatically.
    ///
    /// Self-renews only while we're actually moving: each call compares our
    /// current window-space anchor rect to the last one we pushed, and only
    /// re-arms (`mutate_self_later` → `request_compose`) when it changed.
    /// `ComposeCtx` exposes neither `transform_has_changed` nor
    /// `request_compose` (both are `MutateCtx`-only), hence the comparison —
    /// it gives the same "stop cleanly once stable, no busy-loop" behavior.
    /// Mirrors `ThemedDropdownButton::compose`.
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        if !self.open {
            return;
        }
        let Hosting::Portal {
            scope,
            key,
            last_anchor_rect_window,
            ..
        } = &mut self.hosting
        else {
            return;
        };
        let Some(scope_id) = scope.widget_id() else {
            return;
        };
        let rect = Rect::from_origin_size(ctx.to_window(Point::ZERO), ctx.border_box_size());
        if *last_anchor_rect_window == Some(rect) {
            return;
        }
        *last_anchor_rect_window = Some(rect);
        let key = *key;
        ctx.mutate_later(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_placement(&mut scope, key, rect);
        });
        ctx.mutate_self_later(|mut w| {
            w.ctx.request_compose();
        });
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        match &mut self.hosting {
            Hosting::InTree { overlay_host } => ctx.register_child(overlay_host),
            Hosting::Portal { trigger, .. } => ctx.register_child(trigger),
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
        match &self.hosting {
            Hosting::InTree { overlay_host } => ChildrenIds::from_slice(&[overlay_host.id()]),
            Hosting::Portal { trigger, .. } => ChildrenIds::from_slice(&[trigger.id()]),
        }
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
pub(crate) struct PopoverSurface {
    content: WidgetPod<dyn Widget>,
    bg: Color,
    border: Color,
    pad: f32,
}

impl PopoverSurface {
    pub(crate) fn new(content: NewWidget<dyn Widget>, theme: &Theme) -> Self {
        Self {
            content: content.to_pod(),
            bg: theme.palette.surface_hi,
            border: theme.palette.border_strong,
            pad: theme.density.pad,
        }
    }

    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
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
    pub(crate) fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
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
#[cfg(test)]
mod tests {
    use masonry::core::PointerButton;
    use masonry::testing::TestHarness;

    use super::*;

    /// End-to-end portal wiring at the widget layer, assembled the way the
    /// view layer does it: an `OverlayScope` whose content holds a
    /// portal-mode `PopoverHost` (sharing the scope's handle + key) and whose
    /// slot holds the surface-wrapped content. Exercises the full
    /// owner-notification loop — trigger click → `mutate_later(scope)` →
    /// `set_portal_visible`, then backdrop click → `mutate_later(owner)` →
    /// downcast to `PopoverHost` → `mark_closed` — which would only fail at
    /// runtime (wrong-type downcast panics) if the wiring regressed.
    #[test]
    fn portal_open_and_backdrop_close_roundtrip() {
        let theme = Theme::default();
        let key = 1;
        let handle = OverlayScopeHandle::new();

        let trigger = masonry::widgets::Label::new("trigger").prepare().erased();
        let host = NewWidget::new(PopoverHost::new_portal(
            trigger,
            PopoverAnchor::BottomStart,
            &theme,
            handle.clone(),
            key,
        ));

        let content = masonry::widgets::Label::new("popover body")
            .prepare()
            .erased();
        let surface = NewWidget::new(PopoverSurface::new(content, &theme)).erased();

        // The handle's `OnceLock` fills at the scope's `WidgetAdded`, so
        // `scope.widget_id()` resolves for every event after harness creation.
        let scope = OverlayScope::new(handle, host.erased(), vec![(key, surface)]);
        let mut harness = TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(scope),
        );

        // Click the trigger (the host fills the scope, so any point works
        // while the popover is closed): toggles `open` and pushes visibility
        // into the scope's portal slot via `mutate_later`.
        harness.mouse_move(Point::new(200.0, 200.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));

        harness.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "slot child must be visible and placed after the trigger click"
            );
        });
        harness.edit_root_widget(|mut wm| {
            let mut content = OverlayScope::content_mut(&mut wm);
            let host = content.downcast::<PopoverHost>();
            assert!(host.widget.open, "host must consider itself open");
        });

        // Click "outside" the popover: the anchor rect is the host's full
        // box (the whole scope here, BottomStart), so the placed content sits
        // below the window — every in-window point is outside it. The press
        // lands on the now-unstashed backdrop, which dismisses the slot child
        // and notifies the owner (`mutate_later` → `mark_closed`).
        harness.mouse_move(Point::new(390.0, 390.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));

        harness.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_none(),
                "slot child must be hidden after a backdrop click"
            );
        });
        harness.edit_root_widget(|mut wm| {
            let mut content = OverlayScope::content_mut(&mut wm);
            let host = content.downcast::<PopoverHost>();
            assert!(
                !host.widget.open,
                "backdrop dismissal must sync the host's open flag via mark_closed"
            );
        });
    }
}
