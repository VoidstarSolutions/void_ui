//! `ContextMenuArea` — wraps arbitrary content and pops a [`MenuPanel`] at the
//! cursor on secondary (right) click.
//!
//! Hosting strategy mirrors `dropdown_button`'s in-tree fallback: the menu is a
//! *descendant* anchored over the content (free to overflow it, clipped only by
//! a real clipping ancestor), shown/hidden via an internal `open` flag. Because
//! the menu lives in our subtree:
//!
//! - **Selection** bubbles up as [`MenuItemSelected`] to our [`Widget::on_action`],
//!   which closes the menu and re-emits a [`ContextMenuAction::ItemSelected`].
//! - **Dismissal** is focus-driven: opening the menu requests focus on *this*
//!   widget, so any click that lands outside our subtree clears our focus and
//!   closes the menu (mirrors `ThemedDropdownButton`'s `ChildFocusChanged`
//!   path, but we hold focus directly rather than via a trigger child).
//! - **Escape** closes without selecting.
//!
//! Unlike the trigger-anchored overlays (`AnchoredOverlay`/popover), the menu is
//! placed at the *cursor point* where the right-click landed, clamped so it
//! stays inside the area's box.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

use super::widget::{MenuItemSelected, MenuPanel};

/// Action emitted when the user selects the menu row at index `0` (its position
/// in the original item list — same indexing as [`MenuItemSelected`]).
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

    fn close(&mut self, ctx: &mut EventCtx<'_>) {
        if self.open {
            self.open = false;
            ctx.request_layout();
            ctx.request_paint_only();
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
            self.cursor = Self::to_local(ctx, state.logical_point());
            self.open = true;
            ctx.request_focus();
            ctx.request_layout();
            ctx.request_paint_only();
            ctx.set_handled();
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
        if let TextEvent::Keyboard(key) = event
            && key.state == KeyState::Down
            && key.key == Key::Named(NamedKey::Escape)
        {
            self.close(ctx);
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
        if let Some(&MenuItemSelected(index)) = action.downcast_ref::<MenuItemSelected>() {
            self.open = false;
            ctx.submit_action::<Self::Action>(ContextMenuAction::ItemSelected(index));
            ctx.set_handled();
            ctx.request_layout();
            ctx.request_paint_only();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Two ways an open menu must close itself:
        // - `FocusChanged(false)`: we hold focus while open (requested on
        //   open), so a click outside our subtree clears it — the standard
        //   "click outside to dismiss" path. Clicks on the menu keep focus
        //   here (it's our descendant), so this only fires for genuine
        //   outside clicks.
        // - `StashedChanged(true)`: a tab/panel container hid us mid-open; the
        //   menu can no longer be clicked away, so close eagerly (mirrors
        //   `PopoverHost`'s `StashedChanged` handling).
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
    /// hold the focus we request on open (which drives outside-click dismissal).
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
}
