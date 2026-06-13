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

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::KeyState;
use masonry::core::{
    AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

use super::widget::{MenuAction, MenuPanel, menu_key_from};

/// Action emitted when the user selects the menu row at index `0` (its position
/// in the original item list — same indexing as [`MenuAction::Selected`]).
#[derive(Debug)]
pub enum ContextMenuAction {
    /// The row at this index was selected.
    ItemSelected(usize),
}

/// Wraps `content` and opens a [`MenuPanel`] at the cursor on right-click.
pub struct ContextMenuArea {
    content: WidgetPod<dyn Widget>,
    menu: WidgetPod<MenuPanel>,
    open: bool,
    /// Cursor point (local coords) where the menu was opened.
    cursor: Point,
}

impl ContextMenuArea {
    #[must_use]
    pub fn new(content: NewWidget<dyn Widget>, menu: NewWidget<MenuPanel>) -> Self {
        Self {
            content: content.to_pod(),
            menu: menu.to_pod(),
            open: false,
            cursor: Point::ZERO,
        }
    }

    /// Mutable access to the content child for the view layer.
    pub fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.content)
    }

    /// Mutable access to the menu child for the view layer (theme/rows refresh).
    pub fn menu_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, MenuPanel> {
        this.ctx.get_mut(&mut this.widget.menu)
    }

    fn to_local(ctx: &EventCtx<'_>, window_pos: Point) -> Point {
        let origin = ctx.to_window(Point::ZERO);
        window_pos - origin.to_vec2()
    }

    /// Place the menu's top-left at `cursor`, shifted back inside `container`
    /// when it would overflow the right/bottom edge.
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
            self.cursor = Self::to_local(ctx, state.logical_point());
            self.open = true;
            // We hold focus ourselves while open (the menu can't take focus
            // while it's still stashing on open). A later click outside clears
            // our focus — our `FocusChanged(false)` dismissal — and we forward
            // navigation keys into the menu (`on_text_event`).
            ctx.request_focus();
            ctx.request_layout();
            ctx.request_paint_only();
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
        if let Some(menu_key) = menu_key_from(&key.key) {
            ctx.mutate_child_later(&mut self.menu, move |mut w| {
                MenuPanel::handle_menu_key(&mut w, menu_key);
            });
            ctx.set_handled();
        }
    }

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
        // We hold focus while open and forward keys into the menu; the menu is
        // built non-self-focusing (`hosted`) so clicking it doesn't steal our
        // focus. So `FocusChanged(false)` only fires for a genuine click
        // outside — the "click outside to dismiss" path. Also close if stashed
        // mid-open (a tab/panel hid us).
        let should_close = matches!(
            event,
            Update::FocusChanged(false) | Update::StashedChanged(true)
        );
        if should_close && self.open {
            self.open = false;
            ctx.request_layout();
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        // Registration order is paint order: content first, menu last so the
        // open menu paints on top of the content.
        ctx.register_child(&mut self.content);
        ctx.register_child(&mut self.menu);
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

        if self.open {
            ctx.set_stashed(&mut self.menu, false);
            // Snug to intrinsic content size — see `AnchoredOverlay::layout`.
            let menu_size = ctx.compute_size(&mut self.menu, SizeDef::MIN, size.into());
            ctx.run_layout(&mut self.menu, menu_size);
            let placed = Self::clamp(self.cursor, menu_size, size);
            ctx.place_child(&mut self.menu, placed);
        } else {
            ctx.set_stashed(&mut self.menu, true);
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

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.content.id(), self.menu.id()])
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
}
