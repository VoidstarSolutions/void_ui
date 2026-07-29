//! Shared, pointer-interactive item-row widget for the overlay list
//! substrate (autocomplete suggestions, dropdown menu items). Unlike the
//! former `SuggestionItem` (paint-only; hover/highlight painted centrally by
//! `LabelList` against a shared `item_rects`), this widget handles its own
//! hover and paints its own highlight/hover fill against its own bounds —
//! `CollectionListWidget` (the parent) never has a reliable, centralized
//! rect for a materialized row once `VirtualScroll` owns real, continuous
//! scrolling, but a row always knows its own bounds.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty,
    Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
// Only `render_overlay_list_item` (test-only, see below) needs `NewWidget` at
// this module scope — `mod tests` imports its own copy separately.
#[cfg(test)]
use masonry::core::NewWidget;
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use crate::Theme;
use crate::components::item_list;
use crate::focus_ring::paint_focus_ring_inset;

/// Synchronous, `EventCtx`-level side effect run immediately after a pointer
/// click completes a row selection — before the resulting selection action
/// even reaches the xilem driver. Exists because `on_select`
/// (`item_row_view.rs`) fires later, from inside `View::message`, which only
/// ever has `MutateCtx`-level access (no `EventCtx`/`ActionCtx`) — masonry's
/// `set_focus`/`request_focus` are only available from `EventCtx`/`ActionCtx`
/// (see `masonry_core::core::contexts`), so a consumer that needs to move
/// real focus (e.g. autocomplete returning focus to its text field after a
/// click-selection) has no way to do that from `on_select` at all. This hook
/// gives it exactly one synchronous opportunity, at the same point the
/// original click was detected — mirroring how the pre-virtualization
/// `LabelList::on_pointer_event` used to call `refocus_input` directly,
/// inline with submitting the selection action.
///
/// Only wired for pointer clicks: keyboard Enter-selection
/// (`CollectionListWidget::on_text_event`) activates a row via a
/// `mutate_self_later` closure, which is `MutateCtx`-only — there's no
/// `EventCtx` available there either, so keyboard-driven refocus/close is
/// each consumer's own responsibility at a layer that does have one
/// (autocomplete's `SuggestionList::on_text_event` handles Enter/Escape/Tab
/// directly via bubbling, entirely independent of this hook).
pub(crate) type OnActivated = Arc<dyn for<'a> Fn(&mut EventCtx<'a>) + Send + Sync>;

pub(crate) struct OverlayListItem {
    label: WidgetPod<Label>,
    text: ArcStr,
    highlighted: bool,
    theme: Theme,
    role: Role,
    on_activated: Option<OnActivated>,
}

impl OverlayListItem {
    pub(crate) fn new(
        text: ArcStr,
        highlighted: bool,
        theme: &Theme,
        role: Role,
        on_activated: Option<OnActivated>,
    ) -> Self {
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
            on_activated,
        }
    }
}

/// Builds a plain, imperatively-constructed `NewWidget<dyn Widget>` row.
/// Test-only: production row content goes through `overlay_list_item`/
/// `OverlayListItemView` instead (a real `View`, not a bare widget — see
/// `item_row_view.rs`), since `overlay_list_body`'s `virtual_scroll` needs
/// `WidgetView`-typed content. Kept as a `pub(crate)` convenience for
/// `autocomplete::widget`'s and `dropdown_button::widget`'s own
/// `#[cfg(test)]` fixtures, which materialize rows directly at the widget
/// layer without standing up a full `ViewCtx`. `role` is the per-row
/// accessibility role (`Role::ListBoxOption` for autocomplete,
/// `Role::MenuItem` for a dropdown).
#[cfg(test)]
pub(crate) fn render_overlay_list_item(
    text: &ArcStr,
    highlighted: bool,
    theme: &Theme,
    role: Role,
    on_activated: Option<OnActivated>,
) -> NewWidget<dyn Widget> {
    NewWidget::new(OverlayListItem::new(
        text.clone(),
        highlighted,
        theme,
        role,
        on_activated,
    ))
    .erased()
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

    /// Replaces the row's displayed text — needed because `virtual_scroll`
    /// (the View wrapping this widget) rebuilds already-materialized rows'
    /// content on every ordinary rebuild pass, so an existing row can be
    /// asked to show different text at the same index (e.g. a same-length
    /// filtered result on a new keystroke) without being torn down.
    pub(crate) fn set_text(this: &mut WidgetMut<'_, Self>, text: ArcStr) {
        if this.widget.text == text {
            return;
        }
        this.widget.text = text.clone();
        {
            let mut lbl = this.ctx.get_mut(&mut this.widget.label);
            Label::set_text(&mut lbl, text);
        }
        this.ctx.request_layout();
        this.ctx.request_accessibility_update();
    }

    /// Synthesizes the same selection action a pointer click would submit —
    /// used by `CollectionListWidget::on_text_event`'s Enter handler to
    /// select the highlighted row without a real pointer event. Deliberately
    /// does not also run `on_activated` (that hook needs `EventCtx`, and this
    /// runs from a `mutate_self_later` closure, which only has `MutateCtx`);
    /// keyboard-driven refocus/close is each consumer's own responsibility
    /// elsewhere (see `on_activated`'s doc comment).
    pub(crate) fn activate(this: &mut WidgetMut<'_, Self>) {
        this.ctx.submit_action::<ArcStr>(this.widget.text.clone());
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
                if let Some(on_activated) = &self.on_activated {
                    on_activated(ctx);
                }
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

    /// Vertical: a fixed, explicit row height from `item_list::item_height`
    /// — the same "single-line list item row height" formula
    /// `context_menu::MenuItemNode` already uses for its own rows — rather
    /// than the label's own bare natural height. This row previously had no
    /// vertical padding at all (label height only), which produced a real,
    /// live-reproduced bug: `CollectionListWidget::measure` estimates the
    /// *container's* natural height as `item_count * item_list::item_height`
    /// (see its doc comment), and with rows genuinely shorter than that
    /// per-row budget, `VirtualScroll` only filled part of it — leaving
    /// unfilled space at the bottom of the (correctly-sized, per that
    /// formula) container. Giving rows a real height matching the same
    /// formula the container's estimate already assumes closes that gap at
    /// the source, and also fixes the rows themselves rendering with an
    /// uncomfortably small click target (no breathing room around the text).
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Vertical => Length::px(item_list::item_height(&self.theme.density)),
            Axis::Horizontal => {
                let context_size = LayoutSize::maybe(Axis::Vertical, cross_length);
                ctx.compute_length(
                    &mut self.label,
                    len_req.into(),
                    context_size,
                    Axis::Horizontal,
                    cross_length,
                )
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let label_size = ctx.compute_size(&mut self.label, SizeDef::fit(size), size.into());
        ctx.run_layout(&mut self.label, label_size);
        let label_y = (size.height - label_size.height) * 0.5;
        ctx.place_child(&mut self.label, Point::new(0.0, label_y));
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
        // `aria-selected` (accesskit's `set_selected`) is a valid attribute
        // for `Role::ListBoxOption` (autocomplete's rows) but not for
        // `Role::MenuItem` (dropdown's rows) — the pre-virtualization
        // paint-only menu rows never exposed per-row `aria-selected` either.
        if self.role == Role::ListBoxOption {
            node.set_selected(self.highlighted);
        }
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
        let widget = OverlayListItem::new(text, true, &Theme::default(), Role::ListBoxOption, None);
        let _harness = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (100, 24),
        );
    }

    #[test]
    fn render_overlay_list_item_returns_a_constructible_widget() {
        let text: masonry::core::ArcStr = "Apple".into();
        let widget =
            render_overlay_list_item(&text, true, &Theme::default(), Role::ListBoxOption, None);
        // NewWidget<dyn Widget> can't go into TestHarness (which requires
        // Widget: Sized) — this just confirms the call succeeds and returns
        // a real widget (its id is obtainable) without panicking.
        let _id = widget.id();
    }

    use masonry::core::PointerButton;
    use masonry::kurbo::Point;

    #[test]
    fn set_text_replaces_the_displayed_text() {
        let widget = OverlayListItem::new(
            "Apple".into(),
            false,
            &Theme::default(),
            Role::ListBoxOption,
            None,
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (100, 24),
        );
        let id = harness.root_id();
        harness.edit_root_widget(|mut w| {
            OverlayListItem::set_text(&mut w, "Banana".into());
        });
        harness.redraw();
        // Confirm via accessibility snapshot (the widget's own accessibility()
        // sets node.set_label(self.text.to_string())) rather than reaching
        // into Label internals directly.
        let node = harness.access_node(id).expect("node exists");
        assert_eq!(node.label(), Some("Banana".to_string()));
    }

    #[test]
    fn clicking_a_row_submits_its_own_text() {
        let text: masonry::core::ArcStr = "Banana".into();
        let widget = OverlayListItem::new(
            text.clone(),
            false,
            &crate::Theme::default(),
            Role::ListBoxOption,
            None,
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
