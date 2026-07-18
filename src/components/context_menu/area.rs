//! `ContextMenuArea` — wraps arbitrary content and pops a [`MenuPanel`] at the
//! cursor on secondary (right) click.
//!
//! Hosting strategy mirrors `dropdown_button`'s in-tree fallback: the menu is a
//! *descendant* anchored over the content (free to overflow it, clipped only by
//! a real clipping ancestor), shown/hidden via an internal `open` flag. Because
//! the menu lives in our subtree:
//!
//! - **Keyboard + focus**: we request focus on ourselves on open and own roving
//!   arrow navigation, pushing the highlight into the [`MenuPanel`] (which just
//!   paints it) — mirroring how `ThemedDropdownButton` drives its menu. The
//!   menu isn't focusable here, so clicking it doesn't move focus; a click
//!   *outside* clears ours, which is the "click outside to dismiss" path
//!   ([`Update::FocusChanged(false)`](masonry::core::Update)). Escape closes.
//! - **Selection**: a pointer click in the menu emits [`MenuAction::Selected`],
//!   which bubbles to our [`Widget::on_action`]; keyboard activation we handle
//!   directly. Either way we close and re-emit a [`ContextMenuAction::ItemSelected`].
//!
//! Unlike the trigger-anchored overlays (`AnchoredOverlay`/popover), the menu is
//! placed at the *cursor point* where the right-click landed, clamped so it
//! stays inside the area's box.

use masonry::accesskit::{Action, HasPopup, Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::widgets::Passthrough;

use super::widget::{MenuAction, MenuPanel, menu_key_from};
use crate::overlay::binding::{PortalBinding, PortalCtx};
use crate::overlay_portal::PortalSlot;
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

widget_id_handle!(
    /// Self-filling handle to a [`ContextMenuArea`]'s widget id, filled at
    /// `Update::WidgetAdded` — mirrors
    /// `dropdown_button::widget::DropdownButtonHandle`.
    ///
    /// Given to a portal-mounted `ContextMenuContentView`
    /// (`super::view`) so a selection or dismissal can `mutate_later` back
    /// into the area: in portal mode the menu is not a descendant of the
    /// area, so normal action bubbling never reaches
    /// [`ContextMenuArea::on_action`].
    ContextMenuHandle
);

/// Action emitted when the user selects the menu row at index `0` (its position
/// in the original item list — same indexing as [`MenuAction::Selected`]).
#[derive(Debug)]
pub enum ContextMenuAction {
    /// The row at this index was selected.
    ItemSelected(usize),
}

/// How this area mounts its menu: permanently in-tree (fallback, no scope
/// ancestor), or portal-mounted in the nearest scope's `PortalSlot` (the
/// menu is a view child of the *scope*; we only hold the binding). Mirrors
/// `dropdown_button::widget::Hosting`.
enum Hosting {
    InTree { menu: WidgetPod<MenuPanel> },
    Portal { binding: PortalBinding },
}

/// Wraps `content` and opens a [`MenuPanel`] at the cursor on right-click.
pub struct ContextMenuArea {
    content: WidgetPod<dyn Widget>,
    hosting: Hosting,
    handle: ContextMenuHandle,
    open: bool,
    /// Cursor point (local coords) where the menu was opened.
    cursor: Point,
}

impl ContextMenuArea {
    /// In-tree constructor (fallback, no scope ancestor): the menu is a
    /// direct descendant, shown/hidden via `open`/`layout`.
    #[must_use]
    pub fn new(content: NewWidget<dyn Widget>, menu: NewWidget<MenuPanel>) -> Self {
        Self {
            content: content.to_pod(),
            hosting: Hosting::InTree {
                menu: menu.to_pod(),
            },
            handle: ContextMenuHandle::new(),
            open: false,
            cursor: Point::ZERO,
        }
    }

    /// Portal-mode constructor: the menu lives in the scope's slot under
    /// `key`, registered by the view layer as a `ContextMenuContentView`; we
    /// host only `content`. `handle` is filled at `Update::WidgetAdded` and
    /// given to the registered content view. Mirrors
    /// `ThemedDropdownButton::new_portal`.
    #[must_use]
    pub(crate) fn new_portal(
        content: NewWidget<dyn Widget>,
        handle: ContextMenuHandle,
        scope: OverlayScopeHandle,
        key: u64,
    ) -> Self {
        Self {
            content: content.to_pod(),
            hosting: Hosting::Portal {
                binding: PortalBinding::new(scope, key, context_menu_dismiss_hook),
            },
            handle,
            open: false,
            cursor: Point::ZERO,
        }
    }

    /// Mutable access to the content child for the view layer.
    pub fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.content)
    }

    /// Mutable access to the in-tree menu child for the view layer's
    /// theme/rows refresh. `None` in portal mode — there the registered
    /// `ContextMenuContentView` is refreshed via `OverlayPortal::update`
    /// instead (see `super::view`).
    pub fn menu_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> Option<WidgetMut<'t, MenuPanel>> {
        match &mut this.widget.hosting {
            Hosting::InTree { menu } => Some(this.ctx.get_mut(menu)),
            Hosting::Portal { .. } => None,
        }
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        let origin = ctx.to_window(Point::ZERO);
        window_pos - origin.to_vec2()
    }

    /// Open the menu at `cursor` (local coords) and take focus — shared by
    /// the right-click handler and the accessibility `ShowContextMenu`
    /// invoke.
    fn open_at(&mut self, ctx: &mut EventCtx<'_>, cursor: Point) {
        self.cursor = cursor;
        self.open = true;
        ctx.request_focus();
        ctx.request_layout();
        ctx.request_paint_only();
        if let Hosting::Portal { binding } = &mut self.hosting {
            let window_point = ctx.to_window(cursor);
            binding.open_at_point(ctx, window_point);
        }
    }

    /// Close the portal-mounted menu, if hosted that way — in-tree mode
    /// needs nothing beyond `self.open = false` (checked directly by
    /// `layout`), so this is a no-op there.
    fn close_menu(&mut self, ctx: &mut impl PortalCtx) {
        if let Hosting::Portal { binding } = &mut self.hosting {
            binding.close(ctx);
        }
    }

    /// Mutate the currently-hosted `MenuPanel`, regardless of hosting mode —
    /// `mutate_child_later` for the in-tree child, or a `mutate_later` reach
    /// into the scope's portal slot by key for the portal-mounted one (the
    /// same `Passthrough`-erasure downcast every portal content view needs;
    /// see `ThemedDropdownButton::set_highlight`).
    fn mutate_menu(
        &mut self,
        ctx: &mut EventCtx<'_>,
        f: impl FnOnce(&mut WidgetMut<'_, MenuPanel>) + Send + 'static,
    ) {
        match &mut self.hosting {
            Hosting::InTree { menu } => {
                ctx.mutate_child_later(menu, move |mut w| f(&mut w));
            }
            Hosting::Portal { binding } => {
                let Some(scope_id) = binding.scope_widget_id() else {
                    return;
                };
                let key = binding.key();
                ctx.mutate_later(scope_id, move |mut w| {
                    let mut scope = w.downcast::<OverlayScope>();
                    let mut slot = OverlayScope::portal_slot_mut(&mut scope);
                    if let Some(mut child) = PortalSlot::child_mut(&mut slot, key) {
                        let mut pass = child.downcast::<Passthrough>();
                        let mut menu = Passthrough::child_mut(&mut pass);
                        let mut menu = menu.downcast::<MenuPanel>();
                        f(&mut menu);
                    }
                });
            }
        }
    }

    /// Place the menu's top-left at `cursor`, shifted back inside
    /// `container` when it would overflow the right/bottom edge. In-tree
    /// only — portal mode gets the equivalent clamp generically from
    /// `PortalSlot::layout` (see Task 1).
    fn clamp(cursor: Point, menu: Size, container: Size) -> Point {
        let x = if cursor.x + menu.width > container.width {
            (container.width - menu.width).max(0.0)
        } else {
            cursor.x
        };
        let y = if cursor.y + menu.height > container.height {
            (container.height - menu.height).max(0.0)
        } else {
            cursor.y
        };
        Point::new(x.max(0.0), y.max(0.0))
    }
}

/// Close the menu after the scope's `PortalSlot::dismiss_outside` hid a
/// portal-mounted context menu on an outside press (see
/// `crate::overlay_portal::DismissHook`). Also reused directly by
/// `ContextMenuContentView::message` (see `super::view`) for the
/// selection/Escape close path — unlike `dropdown_button`, `ContextMenuArea`
/// has no host-controlled `open` prop, so both paths share this one
/// unconditional close.
pub(crate) fn context_menu_dismiss_hook(mut w: WidgetMut<'_, dyn Widget>) {
    let mut area = w.downcast::<ContextMenuArea>();
    ContextMenuArea::mark_closed(&mut area);
}

impl ContextMenuArea {
    /// Sync `open` after an external close notification (outside-press
    /// dismissal via [`context_menu_dismiss_hook`], or a portal-mounted
    /// selection/Escape via `ContextMenuContentView::message`). No-op if
    /// already closed.
    pub(crate) fn mark_closed(this: &mut WidgetMut<'_, Self>) {
        if this.widget.open {
            this.widget.open = false;
            this.widget.close_menu(&mut this.ctx);
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }
}

impl Widget for ContextMenuArea {
    type Action = ContextMenuAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Right-click anywhere in the area opens the menu at the cursor.
        // Handled on Down (matching native context menus) and consumed so it
        // doesn't reach the content beneath.
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Secondary),
            state,
            ..
        }) = event
        {
            // We hold focus ourselves while open (the menu can't take focus
            // while it's still stashing on open). A later click outside clears
            // our focus — our `FocusChanged(false)` dismissal — and we forward
            // navigation keys into the menu (`on_text_event`).
            self.open_at(ctx, Self::to_local(ctx, state.logical_point()));
            ctx.set_handled();
        }
    }

    /// An AT "show context menu" invoke (e.g. NVDA/VoiceOver's context-menu
    /// gesture, or the keyboard context-menu key) opens the menu centered on
    /// the area, exactly as a right-click would.
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::ShowContextMenu && !self.open {
            let size = ctx.border_box().size();
            self.open_at(ctx, Point::new(size.width / 2.0, size.height / 2.0));
            ctx.set_handled();
        }
    }

    /// We hold focus while open, so navigation keys land here; forward them to
    /// the menu's own [`MenuPanel::handle_menu_key`], which owns highlight
    /// movement, submenu open/close, and selection/dismissal (emitted as a
    /// `MenuAction` that bubbles back to our `on_action`).
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
        // Tab dismisses (closes) the menu and is left unhandled so focus moves
        // on in tab order (WAI-ARIA menu pattern). The child's Dismissed action
        // bubbles to `on_action`, which clears `open`.
        if matches!(key.key, Key::Named(NamedKey::Tab)) {
            self.mutate_menu(ctx, MenuPanel::dismiss);
            return;
        }
        if let Some(menu_key) = menu_key_from(&key.key) {
            self.mutate_menu(ctx, move |w| MenuPanel::handle_menu_key(w, menu_key));
            ctx.set_handled();
        }
    }

    /// Only reachable in `Hosting::InTree` — there the menu is our
    /// descendant, so its `MenuAction` bubbles here. In `Hosting::Portal`
    /// the menu lives in the scope's slot instead; its `MenuAction` is
    /// handled by `super::view::ContextMenuContentView::message`, which
    /// notifies us via `mark_closed` through `mutate_later` instead of
    /// bubbling.
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        _source: WidgetId,
    ) {
        // The menu emits `MenuAction` from a pointer click or a forwarded key.
        // Either way: a selection closes us and re-emits to the view layer; a
        // dismissal (Escape) just closes.
        match action.downcast_ref::<MenuAction>() {
            Some(&MenuAction::Selected(index)) => {
                self.open = false;
                ctx.submit_action::<Self::Action>(ContextMenuAction::ItemSelected(index));
                ctx.set_handled();
                ctx.request_layout();
                ctx.request_paint_only();
            }
            Some(MenuAction::Dismissed) => {
                self.open = false;
                ctx.set_handled();
                ctx.request_layout();
                ctx.request_paint_only();
            }
            None => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => {
                self.handle.set(ctx.widget_id());
            }
            // We hold focus while open and forward keys into the menu; the
            // menu is built non-self-focusing (`hosted`) so clicking it
            // doesn't steal our focus. So `FocusChanged(false)` only fires
            // for a genuine click outside — the "click outside to dismiss"
            // path. Also close if stashed mid-open (a tab/panel hid us).
            Update::FocusChanged(false) | Update::StashedChanged(true) if self.open => {
                self.open = false;
                self.close_menu(ctx);
                ctx.request_layout();
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        // Registration order is paint order: content first, then the
        // in-tree menu (if any) so it paints on top. Portal mode registers
        // only `content` here — the menu lives in the scope's slot instead,
        // painted above everything in the scope regardless of sibling order
        // (see `overlay_scope.rs`'s module doc — this is the actual fix for
        // #77).
        ctx.register_child(&mut self.content);
        if let Hosting::InTree { menu } = &mut self.hosting {
            ctx.register_child(menu);
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
        // Footprint is the content's footprint — the menu never reflows the
        // area (mirrors `AnchoredOverlay::measure`).
        ctx.compute_length(
            &mut self.content,
            len_req.into(),
            LayoutSize::maybe(axis.cross(), cross_length),
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.content, size);
        ctx.place_child(&mut self.content, Point::ORIGIN);
        ctx.derive_baselines(&self.content);

        if let Hosting::InTree { menu } = &mut self.hosting {
            if self.open {
                ctx.set_stashed(menu, false);
                // Snug to intrinsic content size — see `AnchoredOverlay::layout`.
                let menu_size = ctx.compute_size(menu, SizeDef::MIN, size.into());
                ctx.run_layout(menu, menu_size);
                let placed = Self::clamp(self.cursor, menu_size, size);
                ctx.place_child(menu, placed);
            } else {
                ctx.set_stashed(menu, true);
            }
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — both children paint themselves.
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    /// Exposes the right-click menu to ATs: `has_popup`/`expanded` mirror a
    /// submenu row's semantics, and `ShowContextMenu` lets an AT (or the
    /// keyboard context-menu key) open the menu without a real right-click.
    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_has_popup(HasPopup::Menu);
        node.set_expanded(self.open);
        node.add_action(Action::ShowContextMenu);
    }

    fn children_ids(&self) -> ChildrenIds {
        match &self.hosting {
            Hosting::InTree { menu } => ChildrenIds::from_slice(&[self.content.id(), menu.id()]),
            Hosting::Portal { .. } => ChildrenIds::from_slice(&[self.content.id()]),
        }
    }

    /// Focusable only while open, so the area isn't a tab stop at rest but can
    /// hold the focus we request on open — which drives both keyboard
    /// navigation and outside-click dismissal.
    fn accepts_focus(&self) -> bool {
        self.open
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_menu_at_cursor_when_it_fits() {
        let p = ContextMenuArea::clamp(
            Point::new(20.0, 30.0),
            Size::new(100.0, 80.0),
            Size::new(400.0, 400.0),
        );
        assert_eq!(p, Point::new(20.0, 30.0));
    }

    #[test]
    fn clamp_shifts_menu_back_inside_the_right_and_bottom_edges() {
        let p = ContextMenuArea::clamp(
            Point::new(380.0, 360.0),
            Size::new(100.0, 80.0),
            Size::new(400.0, 400.0),
        );
        assert_eq!(p, Point::new(300.0, 320.0));
    }

    #[test]
    fn clamp_never_goes_negative_for_a_menu_larger_than_the_container() {
        let p = ContextMenuArea::clamp(
            Point::new(10.0, 10.0),
            Size::new(500.0, 500.0),
            Size::new(400.0, 400.0),
        );
        assert_eq!(p, Point::new(0.0, 0.0));
    }

    /// End-to-end: a right-click opens the menu and transfers focus to it
    /// (`set_focus`), so a following ArrowDown+Enter — routed to the focused
    /// menu — selects the first row and closes the area. This would fail at
    /// runtime if the focus transfer regressed (the keys wouldn't reach the
    /// menu and no action would be produced).
    #[test]
    fn right_click_opens_focuses_menu_and_keyboard_selects() {
        use masonry::core::keyboard::{Key, NamedKey};
        use masonry::core::{NewWidget, TextEvent, Widget as _};
        use masonry::layout::AsUnit;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;
        use masonry::widgets::{Label, SizedBox};
        use xilem::view::PointerButton;

        use crate::Theme;
        use crate::components::context_menu::widget::{MenuPanel, MenuRowSpec};

        let theme = Theme::default();
        let row = |id: usize, label: &str| MenuRowSpec::Action {
            id,
            label: label.into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: None,
            disabled: false,
        };
        let content = SizedBox::new(Label::new("area").prepare().erased())
            .width(200.0.px())
            .height(120.0.px())
            .prepare()
            .erased();
        let menu = MenuPanel::new(vec![row(0, "Copy"), row(1, "Paste")], &theme).hosted();
        let area = ContextMenuArea::new(content, NewWidget::new(menu));
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(area));

        // Right-click inside the area opens the menu.
        h.mouse_move(Point::new(40.0, 30.0));
        h.mouse_button_press(Some(PointerButton::Secondary));
        h.mouse_button_release(Some(PointerButton::Secondary));
        assert!(
            h.edit_root_widget(|wm| wm.widget.open),
            "right-click must open the menu"
        );

        // We hold focus on open, so the arrow reaches our own keyboard handler;
        // ArrowDown highlights the first row, Enter selects it.
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        let (action, _) = h
            .pop_action::<ContextMenuAction>()
            .expect("keyboard selection must re-emit a ContextMenuAction");
        let ContextMenuAction::ItemSelected(index) = action;
        assert_eq!(index, 0, "ArrowDown then Enter selects the first row");
        assert!(
            !h.edit_root_widget(|wm| wm.widget.open),
            "selecting a row must close the menu"
        );
    }

    /// An AT "show context menu" invoke (the accessibility action a screen
    /// reader sends for its context-menu gesture / the keyboard menu key)
    /// opens the menu and focuses the area, exactly like a right-click.
    #[test]
    fn show_context_menu_access_action_opens_the_menu() {
        use masonry::accesskit::{Action, ActionRequest, TreeId};
        use masonry::core::{NewWidget, Widget as _};
        use masonry::layout::AsUnit;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;
        use masonry::widgets::{Label, SizedBox};

        use crate::Theme;
        use crate::components::context_menu::widget::{MenuPanel, MenuRowSpec};

        let theme = Theme::default();
        let row = |id: usize, label: &str| MenuRowSpec::Action {
            id,
            label: label.into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: None,
            disabled: false,
        };
        let content = SizedBox::new(Label::new("area").prepare().erased())
            .width(200.0.px())
            .height(120.0.px())
            .prepare()
            .erased();
        let menu = MenuPanel::new(vec![row(0, "Copy")], &theme).hosted();
        let area = ContextMenuArea::new(content, NewWidget::new(menu));
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(area));
        let root_id = h.root_id();

        h.process_access_event(ActionRequest {
            action: Action::ShowContextMenu,
            target_tree: TreeId::ROOT,
            target_node: root_id.into(),
            data: None,
        });

        assert!(
            h.edit_root_widget(|wm| wm.widget.open),
            "ShowContextMenu must open the menu"
        );
        assert_eq!(
            h.focused_widget_id(),
            Some(root_id),
            "the area must take focus so it can forward navigation keys"
        );
    }

    /// WAI-ARIA menu pattern: Tab dismisses (closes) the open menu, and the
    /// key is left unhandled so focus continues in document tab order.
    #[test]
    fn tab_dismisses_and_closes_the_area() {
        use masonry::core::keyboard::{Key, NamedKey};
        use masonry::core::{NewWidget, TextEvent, Widget as _};
        use masonry::layout::AsUnit;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;
        use masonry::widgets::{Label, SizedBox};
        use xilem::view::PointerButton;

        use crate::Theme;
        use crate::components::context_menu::widget::{MenuPanel, MenuRowSpec};

        let theme = Theme::default();
        let row = |id: usize, label: &str| MenuRowSpec::Action {
            id,
            label: label.into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: None,
            disabled: false,
        };
        let content = SizedBox::new(Label::new("area").prepare().erased())
            .width(200.0.px())
            .height(120.0.px())
            .prepare()
            .erased();
        let menu = MenuPanel::new(vec![row(0, "Copy"), row(1, "Paste")], &theme).hosted();
        let area = ContextMenuArea::new(content, NewWidget::new(menu));
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(area));

        h.mouse_move(Point::new(40.0, 30.0));
        h.mouse_button_press(Some(PointerButton::Secondary));
        h.mouse_button_release(Some(PointerButton::Secondary));
        assert!(
            h.edit_root_widget(|wm| wm.widget.open),
            "right-click must open the menu"
        );

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        assert!(
            !h.edit_root_widget(|wm| wm.widget.open),
            "Tab must dismiss (close) the menu"
        );
    }

    /// The right-click area advertises `aria-haspopup=menu` so assistive tech
    /// knows it opens a menu, and reflects its open/closed state via
    /// `aria-expanded`. (The behavior shipped with #41; this pins it against
    /// regression, closing the a11y test gap called out in #78.)
    #[test]
    fn area_advertises_has_popup_menu_and_expanded_state() {
        use masonry::accesskit::HasPopup;
        use masonry::core::{NewWidget, Widget as _};
        use masonry::layout::AsUnit;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;
        use masonry::widgets::{Label, SizedBox};
        use xilem::view::PointerButton;

        use crate::Theme;
        use crate::components::context_menu::widget::{MenuPanel, MenuRowSpec};

        let theme = Theme::default();
        let content = SizedBox::new(Label::new("area").prepare().erased())
            .width(200.0.px())
            .height(120.0.px())
            .prepare()
            .erased();
        let menu = MenuPanel::new(
            vec![MenuRowSpec::Action {
                id: 0,
                label: "Copy".into(),
                subtitle: None,
                icon: None,
                shortcut: None,
                checked: None,
                disabled: false,
            }],
            &theme,
        )
        .hosted();
        let area = ContextMenuArea::new(content, NewWidget::new(menu));
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(area));
        let root = h.root_id();
        h.redraw();

        let node = h
            .access_node(root)
            .expect("area exposes an accessibility node");
        assert_eq!(
            node.data().has_popup(),
            Some(HasPopup::Menu),
            "the area must advertise that it opens a menu"
        );
        assert_eq!(
            node.data().is_expanded(),
            Some(false),
            "closed by default → not expanded"
        );

        // Opening the menu flips aria-expanded to true.
        h.mouse_move(Point::new(40.0, 30.0));
        h.mouse_button_press(Some(PointerButton::Secondary));
        h.mouse_button_release(Some(PointerButton::Secondary));
        h.redraw();
        let node = h.access_node(root).expect("area node after open");
        assert_eq!(
            node.data().is_expanded(),
            Some(true),
            "aria-expanded reflects the open menu"
        );
    }

    // --- Portal-mode tests ---

    use crate::Theme;
    use crate::components::context_menu::widget::MenuRowSpec;
    use crate::overlay_portal::PortalPlacement;
    use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

    fn portal_test_row(id: usize, label: &str) -> MenuRowSpec {
        MenuRowSpec::Action {
            id,
            label: label.into(),
            subtitle: None,
            icon: None,
            shortcut: None,
            checked: None,
            disabled: false,
        }
    }

    /// Builds a scope whose `content` is a portal-mode `ContextMenuArea`
    /// (200×120 inner filler) and whose `portal_children` holds the matching
    /// `MenuPanel`, registered by hand at `key` exactly as the view layer
    /// would register it via `ContextMenuContentView` (see Task 3) — but
    /// without going through the view layer, mirroring
    /// `dropdown_button::widget::tests::portal_selection_close_respects_controlled_mode`.
    fn portal_scope_harness(key: u64) -> (masonry::testing::TestHarness<OverlayScope>, Theme) {
        use masonry::core::NewWidget;
        use masonry::layout::AsUnit;
        use masonry::testing::TestHarness;
        use masonry::theme::default_property_set;
        use masonry::widgets::{Label, SizedBox};

        let theme = Theme::default();
        let inner = SizedBox::new(Label::new("area").prepare().erased())
            .width(200.0.px())
            .height(120.0.px())
            .prepare()
            .erased();
        let scope_handle = OverlayScopeHandle::new();
        let area =
            ContextMenuArea::new_portal(inner, ContextMenuHandle::new(), scope_handle.clone(), key);
        let content = NewWidget::new(area).erased();
        let menu = MenuPanel::new(
            vec![portal_test_row(0, "Copy"), portal_test_row(1, "Paste")],
            &theme,
        )
        .hosted();
        // Matches what the view layer produces (`ContextMenuContentView` is
        // typed to a `Pod<Passthrough>` element — see
        // `overlay_scope.rs::wrap_portal_content`), and what `mutate_menu`'s
        // `Hosting::Portal` branch expects to downcast through. Mirrors
        // `autocomplete::widget::tests`' identical hand-built harness.
        let menu = Passthrough::new(NewWidget::new(menu).erased());
        let scope = OverlayScope::new(
            scope_handle,
            content,
            vec![(
                key,
                NewWidget::new(menu).erased(),
                PortalPlacement::BareTrigger,
            )],
        );
        let harness = TestHarness::create(default_property_set(), NewWidget::new(scope));
        (harness, theme)
    }

    fn with_portal_area<R>(
        h: &mut masonry::testing::TestHarness<OverlayScope>,
        f: impl FnOnce(&mut WidgetMut<'_, ContextMenuArea>) -> R,
    ) -> R {
        h.edit_root_widget(|mut wm| {
            let mut content = OverlayScope::content_mut(&mut wm);
            let mut area = content.downcast::<ContextMenuArea>();
            f(&mut area)
        })
    }

    #[test]
    fn portal_mode_right_click_marks_the_slot_child_visible_at_the_cursor() {
        use xilem::view::PointerButton;

        let (mut h, _theme) = portal_scope_harness(5);

        h.mouse_move(Point::new(40.0, 30.0));
        h.mouse_button_press(Some(PointerButton::Secondary));
        h.mouse_button_release(Some(PointerButton::Secondary));

        assert!(
            with_portal_area(&mut h, |a| a.widget.open),
            "right-click must open the area in portal mode too"
        );
        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            let placed = slot
                .widget
                .placed_rect(5)
                .expect("slot child must be visible and placed after the right-click");
            assert!(
                (placed.x0 - 40.0).abs() < 1e-9 && (placed.y0 - 30.0).abs() < 1e-9,
                "menu top-left must land exactly on the cursor point, got {placed:?}"
            );
        });
    }

    #[test]
    fn portal_mode_right_click_near_the_edge_clamps_the_menu_on_screen() {
        use xilem::view::PointerButton;

        let (mut h, _theme) = portal_scope_harness(5);

        h.mouse_move(Point::new(395.0, 395.0));
        h.mouse_button_press(Some(PointerButton::Secondary));
        h.mouse_button_release(Some(PointerButton::Secondary));

        h.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            let placed = slot
                .widget
                .placed_rect(5)
                .expect("slot child must be placed");
            assert!(
                placed.x1 <= 400.0 + 1e-9,
                "menu must not overflow the right edge, got {placed:?}"
            );
            assert!(
                placed.y1 <= 400.0 + 1e-9,
                "menu must not overflow the bottom edge, got {placed:?}"
            );
        });
    }

    #[test]
    fn portal_mode_keyboard_selection_reaches_the_slot_mounted_panel() {
        use masonry::core::TextEvent;
        use masonry::core::keyboard::{Key, NamedKey};
        use xilem::view::PointerButton;

        let (mut h, _theme) = portal_scope_harness(5);

        h.mouse_move(Point::new(40.0, 30.0));
        h.mouse_button_press(Some(PointerButton::Secondary));
        h.mouse_button_release(Some(PointerButton::Secondary));
        assert!(with_portal_area(&mut h, |a| a.widget.open));

        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));

        let (action, _) = h
            .pop_action::<MenuAction>()
            .expect("ArrowDown+Enter, forwarded into the slot-mounted panel, must select a row");
        assert!(
            matches!(action, MenuAction::Selected(0)),
            "ArrowDown then Enter selects the first row, got {action:?}"
        );

        // In production this notification comes from
        // `ContextMenuContentView::message` via `mutate_later` (Task 3);
        // exercised directly here since that view doesn't exist without
        // going through the full xilem View layer.
        with_portal_area(&mut h, ContextMenuArea::mark_closed);
        assert!(
            !with_portal_area(&mut h, |a| a.widget.open),
            "mark_closed must close the area"
        );
    }

    #[test]
    fn portal_mode_outside_click_dismisses_via_the_scope() {
        use xilem::view::PointerButton;

        let (mut h, _theme) = portal_scope_harness(5);

        h.mouse_move(Point::new(40.0, 30.0));
        h.mouse_button_press(Some(PointerButton::Secondary));
        h.mouse_button_release(Some(PointerButton::Secondary));
        assert!(with_portal_area(&mut h, |a| a.widget.open));

        // A left-click far from the menu's placed rect (near (40,30)) bubbles
        // through the scope's own pointer handling, which calls
        // `PortalSlot::dismiss_outside` and notifies us via
        // `context_menu_dismiss_hook`.
        h.mouse_move(Point::new(390.0, 390.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));

        assert!(
            !with_portal_area(&mut h, |a| a.widget.open),
            "an outside press must dismiss the portal-mounted menu via context_menu_dismiss_hook"
        );
    }
}
