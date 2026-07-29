//! `MenuContent` — pure chrome (rounded-rect background/border, capped-height
//! sizing) wrapping whatever `overlay_list(...)` (`crate::collection`, Task
//! 5) builds — a virtualized, keyboard-navigable listbox widget
//! (`CollectionListWidget`). Mirrors
//! `crate::components::autocomplete::widget::SuggestionList`'s shape
//! exactly: hover/highlight painting, click selection, and materialization
//! all live in that shared substrate now; this widget only still paints its
//! own background/border chrome and caps the vertical natural size.
//!
//! Generic over the wrapped widget type `W` — mirroring `SuggestionList<W>`
//! (`autocomplete/widget.rs`) and `CollapsibleWidget<W>`
//! (`components/collapsible/widget.rs`), not an erased `WidgetPod<dyn
//! Widget>` — so `super::view::MenuContentView` (generic over the child
//! *view*) can forward `rebuild`/`teardown`/`message` straight through via
//! `this.ctx.get_mut(&mut this.widget.list)`, with no downcast needed at
//! all. `W` gets erased exactly once, one level up, wherever this widget is
//! actually embedded: `ThemedDropdownButton`'s in-tree `AnchoredOverlay`
//! overlay slot and the portal's `Passthrough` wrapper both already take
//! `NewWidget<dyn Widget>`, so genericity here costs nothing at those
//! boundaries.
//!
//! Unlike `SuggestionList`, this widget owns no keyboard handling of its own
//! at all (no Enter/Escape/Tab): `ThemedDropdownButton` keeps real keyboard
//! focus on its trigger button throughout the whole menu interaction
//! (roving-highlight model, not autocomplete's Tab-into-listbox model), so
//! arrow-key navigation is driven entirely by `ThemedDropdownButton::
//! on_text_event`, which reaches into the wrapped `CollectionListWidget` via
//! [`MenuContent::child_mut`] to push highlight state — see its doc comment
//! and `crate::collection`'s re-export of `CollectionListWidget` for why
//! that widget needs to be nameable outside `crate::collection` at all here,
//! unlike autocomplete.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length};

use crate::Theme;

/// Border width of the menu's background chrome — hairline chrome, not density-scaled.
const BORDER_WIDTH: f64 = 1.0;

/// Chrome widget for a dropdown menu: background/border painting and a
/// capped-height `measure()`. Wraps whatever `overlay_list(...)` built.
pub(crate) struct MenuContent<W: Widget> {
    list: WidgetPod<W>,
    theme: Theme,
}

impl<W: Widget> MenuContent<W> {
    #[must_use]
    pub(crate) fn new(list: NewWidget<W>, theme: &Theme) -> Self {
        Self {
            list: list.to_pod(),
            theme: *theme,
        }
    }

    /// Returns a mutable reference to the wrapped listbox — lets
    /// `super::view::MenuContentView` forward `rebuild`/`teardown`/
    /// `message` straight through, and lets `ThemedDropdownButton::
    /// set_highlight` reach `CollectionListWidget::set_highlight` directly
    /// (see the module doc for why `dropdown_button`, unlike autocomplete,
    /// needs this from outside the view layer too).
    pub(crate) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.list)
    }
}

// --- MARK: WIDGETMUT SETTERS
impl<W: Widget> MenuContent<W> {
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme == *theme {
            return;
        }
        this.widget.theme = *theme;
        this.ctx.request_paint_only();
    }
}

impl<W: Widget> Widget for MenuContent<W> {
    type Action = NoAction;

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.list);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        let natural = ctx
            .compute_length(
                &mut self.list,
                LenDef::MaxContent,
                context_size,
                axis,
                cross_length,
            )
            .get();
        match axis {
            Axis::Vertical => {
                Length::px(natural.min(crate::components::autocomplete::MAX_LIST_HEIGHT))
            }
            Axis::Horizontal => Length::px(natural),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.list, size);
        ctx.place_child(&mut self.list, Point::ORIGIN);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;

        // Background/border chrome — the only thing this widget still
        // paints itself; hover/highlight fills now live on the materialized
        // `OverlayListItem` rows (`crate::collection::item_row`).
        let corner = f64::from(self.theme.radius.small);
        let bg_rect = RoundedRect::from_origin_size(Point::ORIGIN, ctx.border_box().size(), corner);
        painter.fill(bg_rect, p.surface_hi).draw();
        painter
            .stroke(bg_rect, &Stroke::new(BORDER_WIDTH), p.border_strong)
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        // Not `Role::Menu` — `overlay_list`'s own `CollectionListWidget`
        // (constructed with `container_role: Role::Menu`) owns that now,
        // same split as `SuggestionList`/`Role::GenericContainer` vs
        // `CollectionListWidget`/`Role::ListBox`.
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
        ChildrenIds::from_slice(&[self.list.id()])
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;

    use super::*;
    use crate::collection::CollectionListWidget;

    /// The height cap this task adds: `measure()` previously sized to
    /// `item_height * item_count` unbounded (pre-virtualization
    /// `MenuContent::measure`); a menu with 200 items must now measure
    /// capped at `MAX_LIST_HEIGHT`, not to the full (much taller) natural
    /// content height.
    #[test]
    fn measure_caps_vertical_height_at_max_list_height() {
        use masonry::layout::UnitPoint;
        use masonry::widgets::Align;

        let theme = Theme::default();
        let vs = NewWidget::new(VirtualScrollWidget::new(0, 200));
        let list = CollectionListWidget::new(vs, 200, Role::Menu);
        let menu = NewWidget::new(MenuContent::new(NewWidget::new(list), &theme));
        let menu_id = menu.id();
        // Wrapped in `Align` (root sizing otherwise forces the root widget
        // to fill the whole window, masking `measure()`'s own natural/capped
        // size entirely) so `menu`'s own laid-out size reflects what
        // `measure()` actually returned — mirrors
        // `ThemedDropdownButton`'s own test fixtures' use of `Align` for the
        // same reason (`widget.rs`'s `portal_selection_close_respects_controlled_mode`).
        let root = Align::new(UnitPoint::TOP_LEFT, menu.erased());
        let mut h = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(root),
            (300, 4000),
        );
        h.render();
        let size = h.get_widget_with_id(menu_id).ctx().border_box().size();
        assert!(
            size.height <= crate::components::autocomplete::MAX_LIST_HEIGHT + 0.5,
            "200 items should measure capped at MAX_LIST_HEIGHT ({}), got {}",
            crate::components::autocomplete::MAX_LIST_HEIGHT,
            size.height
        );
    }

    #[test]
    fn set_theme_is_a_noop_when_the_theme_is_unchanged() {
        let theme = Theme::default();
        let vs = NewWidget::new(VirtualScrollWidget::new(0, 3));
        let list = CollectionListWidget::new(vs, 3, Role::Menu);
        let menu = MenuContent::new(NewWidget::new(list), &theme);
        let mut h =
            TestHarness::create_with_size(default_property_set(), NewWidget::new(menu), (300, 300));
        // No assertion beyond "doesn't panic" — set_theme's early-return
        // path just needs to be exercised; behavior-visible coverage for
        // the changed case lives at the `ThemedDropdownButton::set_theme`
        // level now (theme forwarding moved to the view layer).
        h.edit_root_widget(|mut w| MenuContent::set_theme(&mut w, &theme));
    }
}
