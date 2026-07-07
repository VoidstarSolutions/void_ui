//! Single-child wrapper widget that detects primary-button clicks
//! with modifier state and emits a [`RowClickAction`].
//!
//! Modeled on [`crate::components::data_grid::copy_shortcut::CopyOnShortcut`]
//! but for pointer events instead of keyboard events. The widget itself stays
//! dumb — it reports "primary click (or Enter/Space) happened at these
//! modifiers" — the collection view layer translates that into the right
//! [`SelectionState`](super::SelectionState) update for the affected row.
//!
//! Keyboard support makes rows operable without a pointer: the row takes
//! focus, Tab moves between rows (masonry's built-in traversal), Enter/Space
//! activates the focused row, and a focus ring is painted while focused. The
//! selected state is reported to assistive technology via accesskit's
//! `Selected` property.
//!
//! `accepts_focus = true` so subsequent Ctrl/Cmd+C on the parent
//! [`CopyOnShortcut`](crate::components::data_grid::copy_shortcut::CopyOnShortcut)
//! wrapper has a focused descendant inside the grid.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, Modifiers, NewWidget,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};

use super::single_child;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::focus_ring::{FOCUS_RING_INSET, paint_focus_ring};

/// Action emitted by `RowClickable` on primary-button release (or
/// Enter/Space activation). The receiver inspects the modifiers to decide
/// whether this is a plain click (`replace`), a multi-select toggle
/// (`action_mod`), or a shift-extend (`shift`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowClickAction {
    /// Shift modifier was held.
    pub shift: bool,
    /// Platform "action modifier" was held — Cmd on macOS, Ctrl
    /// elsewhere. Matches masonry's `TextArea` convention.
    pub action_mod: bool,
}

/// Builds a [`RowClickAction`] from a pointer or keyboard event's
/// [`Modifiers`], applying the platform's action-modifier convention (Cmd on
/// macOS, Ctrl elsewhere).
fn row_click_action(modifiers: Modifiers) -> RowClickAction {
    let action_mod = if cfg!(target_os = "macos") {
        modifiers.meta()
    } else {
        modifiers.ctrl()
    };
    RowClickAction {
        shift: modifiers.shift(),
        action_mod,
    }
}

/// Single-child wrapper that emits a [`RowClickAction`] on primary-button
/// release (or Enter/Space) inside its bounds.
pub struct RowClickable {
    child: WidgetPod<dyn Widget>,
    /// Reported via accesskit's `Selected` property — lets screen readers
    /// announce a row's selection state. See [`Self::set_selected`].
    selected: bool,
    /// Used to color the focus ring drawn in [`Self::paint`] when this row
    /// has keyboard focus.
    theme: Theme,
    /// Width (local px, from the leading edge) of a hit zone the row
    /// **defers** to an interactive child sitting there — a disclosure
    /// chevron on an expandable row, a leading row-action control (#97).
    /// A primary press with `local x < leading_hit_width` neither captures
    /// the pointer nor selects the row, so the press bubbles to the child
    /// control instead (the same defer-to-child split the collapsible header
    /// makes by `y`, here by `x`). `0.0` (the default) reserves nothing, so
    /// the whole row selects — the behavior for a plain, non-expandable row.
    ///
    /// Leading-edge (LTR) only, matching the chevron's fixed left placement;
    /// callers should reserve a zone only where an interactive control
    /// actually sits, or that strip becomes a dead zone for selection.
    leading_hit_width: f64,
}

// --- MARK: BUILDERS
impl RowClickable {
    #[must_use]
    pub fn new(
        child: NewWidget<impl Widget + ?Sized>,
        selected: bool,
        theme: &Theme,
        leading_hit_width: f64,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            selected,
            theme: *theme,
            leading_hit_width,
        }
    }
}

// --- MARK: WIDGETMUT
impl RowClickable {
    /// Returns a mutable reference to the wrapped child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Sets the row's selected state, requesting an accessibility update on
    /// change so screen readers announce it.
    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_accessibility_update();
        }
    }

    /// Replaces the theme used to color the focus ring.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_paint_only();
        }
    }

    /// Updates the leading defer-to-child hit width (see the field docs).
    /// Cheap: it only affects the next press's capture guard, so no repaint
    /// or relayout is requested.
    pub fn set_leading_hit_width(this: &mut WidgetMut<'_, Self>, width: f64) {
        this.widget.leading_hit_width = width;
    }
}

// --- MARK: IMPL WIDGET
impl Widget for RowClickable {
    type Action = RowClickAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Shared Down→capture / Up-iff-active-and-hovered recognizer, with a
        // positional capture guard that defers a leading hit zone to an
        // interactive child (chevron / row action). Outside that zone the row
        // captures and behaves exactly as before; a press inside it is not
        // captured (guard returns false ⇒ no `Down`, so the child handles it).
        let leading = self.leading_hit_width;
        match click::primary_click_when(ctx, event, |ctx, state| {
            ctx.local_position(state.position).x >= leading
        }) {
            // Rows take keyboard focus so a subsequent Ctrl/Cmd+C lands
            // inside the grid (see the module docs).
            Some(ClickPhase::Down(_)) => ctx.request_focus(),
            Some(ClickPhase::Up {
                state,
                completed: true,
            }) => {
                ctx.submit_action::<Self::Action>(row_click_action(state.modifiers));
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        // Enter or Space activates the focused row, with the same modifier
        // semantics as a click. Tab is deliberately not handled, so masonry's
        // built-in focus traversal moves between rows.
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        let is_activate = match &key.key {
            Key::Named(NamedKey::Enter) => true,
            Key::Character(s) => s == " ",
            Key::Named(_) => false,
        };
        if key.state != KeyState::Down || !is_activate {
            return;
        }
        ctx.submit_action::<Self::Action>(row_click_action(key.modifiers));
        ctx.set_handled();
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // The focus ring drawn in `paint` depends on `ctx.is_focus_target()`;
        // without this, gaining/losing focus doesn't trigger a repaint and
        // the ring never appears.
        if let Update::FocusChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        single_child::register_children(ctx, &mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        single_child::measure(ctx, &mut self.child, axis, len_req, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        single_child::layout(ctx, &mut self.child, size);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if ctx.is_focus_target() {
            let size = ctx.border_box_size();
            let inset = FOCUS_RING_INSET;
            let rect = Rect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
            );
            paint_focus_ring(painter, rect, &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ListItem
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_selected(self.selected);
    }

    fn children_ids(&self) -> ChildrenIds {
        single_child::children_ids(&self.child)
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn propagates_pointer_interaction(&self) -> bool {
        // Let an interactive leading child (a disclosure chevron, a leading
        // row-action) receive pointer hover/press independently. A press over
        // a non-interactive cell still bubbles up here, so row selection is
        // unchanged; the positional guard in `on_pointer_event` is what keeps
        // a press over the leading control from also selecting the row.
        true
    }
}

// --- MARK: XILEM VIEW WRAPPER

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Wraps a child view in `RowClickable` and routes pointer release (or
/// Enter/Space) + modifiers through the supplied `on_click` callback.
///
/// `on_click` runs synchronously against the host's app state during
/// xilem's message-handling pass. Use it to apply the right
/// [`SelectionState`](super::SelectionState) op based on the
/// [`RowClickAction`] modifier flags.
///
/// Row *content* stays at `Action = ()`; this wrapper is the boundary that
/// lifts internal intents into the host's `Action` type via
/// `Action::default()` (an action reaching the root re-runs the host's app
/// logic — `RequestRebuild` would only re-evaluate the existing, now-stale
/// view values).
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ClickableRow<V, State, Action, F> {
    child: V,
    selected: bool,
    theme: Theme,
    on_click: F,
    /// See [`RowClickable::leading_hit_width`]. `0.0` by default (whole row
    /// selects); set via [`Self::leading_hit_width`] on an expandable row so
    /// the leading chevron zone defers to the chevron control.
    leading_hit_width: f64,
    phantom: PhantomData<fn() -> (State, Action)>,
}

/// Constructor for [`ClickableRow`]. `selected` is reported to assistive
/// technology via accesskit's `Selected` property — pass the row's current
/// [`SelectionState`](super::SelectionState) membership. `theme` colors the
/// focus ring drawn when the row has keyboard focus.
pub fn clickable_row<V, State, Action, F>(
    child: V,
    selected: bool,
    theme: &Theme,
    on_click: F,
) -> ClickableRow<V, State, Action, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State, RowClickAction) + Send + Sync + 'static,
    State: 'static,
    Action: Default + 'static,
{
    ClickableRow {
        child,
        selected,
        theme: *theme,
        on_click,
        leading_hit_width: 0.0,
        phantom: PhantomData,
    }
}

impl<V, State, Action, F> ClickableRow<V, State, Action, F> {
    /// Reserves a leading hit zone of `width` local px (from the row's
    /// leading edge) that defers to an interactive child sitting there — an
    /// expandable row's disclosure chevron, a leading row-action control.
    ///
    /// A primary press inside the zone doesn't select the row; it bubbles to
    /// the child control instead. Presses outside it select as usual. Pass
    /// the chevron/control's rendered width (plus any depth indent that sits
    /// before it); `0.0` (the default) reserves nothing.
    pub fn leading_hit_width(mut self, width: f64) -> Self {
        self.leading_hit_width = width;
        self
    }
}

impl<V, State, Action, F> ViewMarker for ClickableRow<V, State, Action, F> {}

impl<V, State, Action, F> View<State, Action, ViewCtx> for ClickableRow<V, State, Action, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State, RowClickAction) + Send + Sync + 'static,
    State: 'static,
    Action: Default + 'static,
{
    type Element = Pod<RowClickable>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = RowClickable::new(
            child_pod.new_widget,
            self.selected,
            &self.theme,
            self.leading_hit_width,
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.selected != prev.selected {
            RowClickable::set_selected(&mut element, self.selected);
        }
        if self.theme != prev.theme {
            RowClickable::set_theme(&mut element, &self.theme);
        }
        if (self.leading_hit_width - prev.leading_hit_width).abs() > f64::EPSILON {
            RowClickable::set_leading_hit_width(&mut element, self.leading_hit_width);
        }
        let mut child = RowClickable::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut child = RowClickable::child_mut(&mut element);
            self.child.teardown(view_state, ctx, child.downcast());
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if let Some(action) = message.take_message::<RowClickAction>() {
            (self.on_click)(app_state, *action);
            MessageResult::Action(Action::default())
        } else {
            let mut child = RowClickable::child_mut(&mut element);
            match self
                .child
                .message(view_state, message, child.downcast(), app_state)
            {
                // Lift the ()-typed row content's action into the host's
                // Action so it still re-runs the host's app logic.
                MessageResult::Action(()) => MessageResult::Action(Action::default()),
                MessageResult::RequestRebuild => MessageResult::RequestRebuild,
                MessageResult::Nop => MessageResult::Nop,
                MessageResult::Stale => MessageResult::Stale,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
    use masonry::core::{NewWidget, PointerButton, TextEvent};
    use masonry::kurbo::Point;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{RowClickAction, RowClickable};
    use crate::Theme;

    fn key_down(key: Key, modifiers: Modifiers) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key,
            modifiers,
            ..Default::default()
        })
    }

    fn harness_with_leading(leading_hit_width: f64) -> TestHarness<RowClickable> {
        let child = NewWidget::new(Label::new("row"));
        let widget = RowClickable::new(child, false, &Theme::default(), leading_hit_width);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (120, 24))
    }

    fn harness() -> TestHarness<RowClickable> {
        harness_with_leading(0.0)
    }

    /// Presses the primary button at `x` (row is 24 px tall, so `y = 12` is
    /// mid-row), then releases at the same point, and returns whether the row
    /// emitted a [`RowClickAction`] (i.e. selected).
    fn click_at_x(harness: &mut TestHarness<RowClickable>, x: f64) -> bool {
        harness.mouse_move(Point::new(x, 12.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.pop_action::<RowClickAction>().is_some()
    }

    /// Space activates the focused row (keyboard parity with a click), so a
    /// pointer is not required to select.
    #[test]
    fn space_activates_the_focused_row() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        let handled =
            harness.process_text_event(key_down(Key::Character(" ".into()), Modifiers::empty()));
        assert!(handled.is_handled());
        assert_eq!(
            harness.pop_action::<RowClickAction>().map(|(a, _)| a),
            Some(RowClickAction::default()),
        );
    }

    /// Enter likewise activates the focused row.
    #[test]
    fn enter_activates_the_focused_row() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        let handled =
            harness.process_text_event(key_down(Key::Named(NamedKey::Enter), Modifiers::empty()));
        assert!(handled.is_handled());
        assert!(harness.pop_action::<RowClickAction>().is_some());
    }

    /// Shift held during activation carries through as a range-extend.
    #[test]
    fn shift_space_carries_the_shift_modifier() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        harness.process_text_event(key_down(Key::Character(" ".into()), Modifiers::SHIFT));
        let action = harness.pop_action::<RowClickAction>().map(|(a, _)| a);
        assert_eq!(
            action,
            Some(RowClickAction {
                shift: true,
                action_mod: false
            })
        );
    }

    /// A non-activating key (a printable character other than space) leaves
    /// the row alone, so type-ahead / other handlers still see it.
    #[test]
    fn other_keys_do_not_activate() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        harness.process_text_event(key_down(Key::Character("x".into()), Modifiers::empty()));
        assert!(harness.pop_action::<RowClickAction>().is_none());
    }

    /// Default (no leading zone reserved): a primary click anywhere on the
    /// row selects it — the plain, non-expandable-row behavior, unchanged by
    /// the defer-to-child machinery.
    #[test]
    fn default_row_selects_on_click_anywhere() {
        let mut harness = harness();
        assert!(click_at_x(&mut harness, 5.0), "near the leading edge");
        assert!(click_at_x(&mut harness, 60.0), "mid-row");
    }

    /// A press inside a reserved leading zone does NOT select the row: the
    /// capture guard rejects it so the press bubbles to the interactive
    /// leading child (the disclosure chevron) instead.
    #[test]
    fn press_in_leading_zone_does_not_select() {
        let mut harness = harness_with_leading(20.0);
        assert!(
            !click_at_x(&mut harness, 10.0),
            "a press at x=10 (< 20 px leading zone) must defer, not select",
        );
    }

    /// A press outside the reserved leading zone still selects the row, so a
    /// click on the row's content behaves exactly as it would without a
    /// leading control.
    #[test]
    fn press_outside_leading_zone_still_selects() {
        let mut harness = harness_with_leading(20.0);
        assert!(
            click_at_x(&mut harness, 50.0),
            "a press at x=50 (> 20 px leading zone) must select",
        );
    }

    /// The zone boundary is inclusive of the row side: a press exactly at
    /// `x == leading_hit_width` selects (the guard is `x >= leading`), so the
    /// reserved zone is the half-open `[0, leading)` interval.
    #[test]
    fn press_at_zone_boundary_selects() {
        let mut harness = harness_with_leading(20.0);
        assert!(
            click_at_x(&mut harness, 20.0),
            "boundary belongs to the row"
        );
    }
}
