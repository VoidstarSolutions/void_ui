//! Shared, pointer-interactive item-row widget for the overlay list
//! substrate (autocomplete suggestions, dropdown menu items). Unlike the
//! former `SuggestionItem` (paint-only; hover/highlight painted centrally by
//! `LabelList` against a shared `item_rects`), this widget handles its own
//! hover and paints its own highlight/hover fill against its own bounds —
//! `CollectionListWidget` (the parent) never has a reliable, centralized
//! rect for a materialized row once `VirtualScroll` owns real, continuous
//! scrolling, but a row always knows its own bounds.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;
use crate::focus_ring::paint_focus_ring_inset;

pub(crate) struct OverlayListItem {
    label: WidgetPod<Label>,
    text: ArcStr,
    highlighted: bool,
    theme: Theme,
    role: Role,
}

impl OverlayListItem {
    fn new(text: ArcStr, highlighted: bool, theme: &Theme, role: Role) -> Self {
        let mut lbl = Label::new(text.clone())
            .with_style(StyleProperty::FontSize(theme.density.ui_font_size))
            .prepare();
        lbl.properties.insert(ContentColor::new(theme.palette.text));
        Self {
            label: lbl.to_pod(),
            text,
            highlighted,
            theme: *theme,
            role,
        }
    }
}

/// The row-content builder both `SuggestionList` and `MenuContent` pass to
/// their `CollectionListWidget`. `role` is the per-row accessibility role
/// (`Role::ListBoxOption` for autocomplete, `Role::MenuItem` for a dropdown).
pub(crate) fn render_overlay_list_item(
    text: &ArcStr,
    highlighted: bool,
    theme: &Theme,
    role: Role,
) -> NewWidget<dyn Widget> {
    NewWidget::new(OverlayListItem::new(text.clone(), highlighted, theme, role)).erased()
}

impl OverlayListItem {
    pub(crate) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        this.widget.theme = *theme;
        {
            let mut lbl = this.ctx.get_mut(&mut this.widget.label);
            lbl.insert_prop(ContentColor::new(theme.palette.text));
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(theme.density.ui_font_size),
            );
        }
        this.ctx.request_paint_only();
    }

    pub(crate) fn set_highlighted(this: &mut WidgetMut<'_, Self>, highlighted: bool) {
        if this.widget.highlighted != highlighted {
            this.widget.highlighted = highlighted;
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
    }
}

impl Widget for OverlayListItem {
    type Action = ArcStr;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.capture_pointer();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) if ctx.is_active() && ctx.is_hovered() => {
                ctx.submit_action::<Self::Action>(self.text.clone());
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // No dedicated `hovered` field: masonry's ctx.is_hovered() already
        // reflects pointer presence per-widget (unlike LabelList's central
        // hover_index, needed only because one widget used to cover every
        // row), so `paint` reads it directly — this just triggers the
        // repaint when it changes.
        if matches!(event, Update::HoveredChanged(_)) {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.label);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(
            &mut self.label,
            len_req.into(),
            context_size,
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.label, size);
        ctx.place_child(&mut self.label, Point::ZERO);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let p = &self.theme.palette;
        let rect = ctx.border_box();
        if ctx.is_hovered() {
            painter.fill(rect, p.surface_2).draw();
        }
        if self.highlighted {
            paint_focus_ring_inset(painter, rect.size(), &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        self.role
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.text.to_string());
        node.set_selected(self.highlighted);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.label.id()])
    }
}

#[cfg(test)]
mod tests {
    use masonry::accesskit::Role;
    use masonry::core::NewWidget;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;

    use super::{OverlayListItem, render_overlay_list_item};
    use crate::Theme;

    #[test]
    fn overlay_list_item_builds_in_a_harness_without_panicking() {
        let text: masonry::core::ArcStr = "Apple".into();
        let widget = OverlayListItem::new(text, true, &Theme::default(), Role::ListBoxOption);
        let _harness = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (100, 24),
        );
    }

    #[test]
    fn render_overlay_list_item_returns_a_constructible_widget() {
        let text: masonry::core::ArcStr = "Apple".into();
        let widget = render_overlay_list_item(&text, true, &Theme::default(), Role::ListBoxOption);
        // NewWidget<dyn Widget> can't go into TestHarness (which requires
        // Widget: Sized) — this just confirms the call succeeds and returns
        // a real widget (its id is obtainable) without panicking.
        let _id = widget.id();
    }

    use masonry::core::PointerButton;
    use masonry::kurbo::Point;

    #[test]
    fn clicking_a_row_submits_its_own_text() {
        let text: masonry::core::ArcStr = "Banana".into();
        let widget = OverlayListItem::new(
            text.clone(),
            false,
            &crate::Theme::default(),
            Role::ListBoxOption,
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (100, 24),
        );
        harness.mouse_move(Point::new(10.0, 10.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert_eq!(
            harness
                .pop_action::<masonry::core::ArcStr>()
                .map(|(a, _)| a),
            Some(text),
        );
    }
}
