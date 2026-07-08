//! Single-child wrapper widget that detects primary-button clicks and
//! keyboard activation with modifier state and emits a [`RowInteraction`].
//!
//! Modeled on [`crate::components::data_grid::copy_shortcut::CopyOnShortcut`]
//! but for pointer events instead of keyboard events. The widget itself stays
//! dumb — it reports "a select or activate happened at these modifiers" — the
//! collection view layer translates that into the right
//! [`SelectionState`](super::SelectionState) update or activation call for the
//! affected row.
//!
//! Keyboard support makes rows operable without a pointer: the row takes
//! focus, Tab moves between rows (masonry's built-in traversal), and a focus
//! ring is painted while focused. The keyboard splits two intents:
//!
//! - **Space** (and a pointer click) *selects* the focused row — the
//!   selection-background / `SelectionState` path.
//! - **Enter** *activates* ("opens") it — a distinct intent routed to the
//!   host's activation handler. When no handler is wired, Enter falls back
//!   to selection so keyboard operation never regresses. Double-click
//!   activation is deferred (the pointer path has no click-count yet).
//!
//! The selected state is reported to assistive technology via accesskit's
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

/// Modifiers carried by a row *selection* intent (a primary-button release
/// or Space). The receiver inspects them to decide whether this is a plain
/// click (`replace`), a multi-select toggle (`action_mod`), or a
/// shift-extend (`shift`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowClickAction {
    /// Shift modifier was held.
    pub shift: bool,
    /// Platform "action modifier" was held — Cmd on macOS, Ctrl
    /// elsewhere. Matches masonry's `TextArea` convention.
    pub action_mod: bool,
}

/// What a focused/clicked [`RowClickable`] reports to the view layer. The two
/// intents are deliberately distinct (see the module docs): a pointer click
/// or **Space** is a *selection* intent, while **Enter** is an *activation*
/// ("open the row") intent that the host handles separately from selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowInteraction {
    /// Primary-button click, or Space on the focused row — *select*.
    Select(RowClickAction),
    /// Enter on the focused row — *activate* ("open"). Carries the event
    /// modifiers for symmetry, though activation is modifier-agnostic today.
    Activate(RowClickAction),
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

/// A horizontal hit zone on a row that **defers** to an interactive child
/// sitting there (a disclosure chevron, a leading row-action) instead of
/// selecting the row. Occupies `[offset, offset + width)` in row-local px
/// from the leading edge.
///
/// The `offset` is what lets the zone reserve *only* the child's box when the
/// child is inset from the leading edge — a disclosure chevron sits after the
/// depth indent, so a parent reserves `offset = indent`, `width = chevron`.
/// A plain `[0, width)` zone (offset `0`) would instead swallow the blank
/// indent gutter to the chevron's left, making it a selection dead zone that
/// grows with tree depth while a leaf's indent stays selectable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeadingHitZone {
    /// Distance from the row's leading edge to the start of the zone (px).
    pub offset: f64,
    /// Width of the zone (px) — the deferred child's own width.
    pub width: f64,
}

impl LeadingHitZone {
    /// Whether row-local `x` falls in the half-open `[offset, offset + width)`
    /// interval that defers to the child. The far edge belongs to the row, so
    /// content flush against the zone still selects.
    fn contains(self, x: f64) -> bool {
        x >= self.offset && x < self.offset + self.width
    }
}

/// Single-child wrapper that emits a [`RowInteraction`] on primary-button
/// release, Space (select), or Enter (activate) inside its bounds.
pub struct RowClickable {
    child: WidgetPod<dyn Widget>,
    /// Reported via accesskit's `Selected` property — lets screen readers
    /// announce a row's selection state. See [`Self::set_selected`].
    selected: bool,
    /// Used to color the focus ring drawn in [`Self::paint`] when this row
    /// has keyboard focus.
    theme: Theme,
    /// Leading hit zone the row **defers** to an interactive child sitting
    /// there — a disclosure chevron on an expandable row, a leading row-action
    /// control (#97). A primary press inside the zone neither captures the
    /// pointer nor selects the row, so the press bubbles to the child control
    /// instead (the same defer-to-child split the collapsible header makes by
    /// `y`, here by `x`). `None` (the default) reserves nothing, so the whole
    /// row selects — the behavior for a plain, non-expandable row.
    ///
    /// The zone's [`offset`](LeadingHitZone::offset) reserves *only* the
    /// child's box even when it's inset (a chevron after the depth indent),
    /// so the gutter to its left stays selectable. LTR only.
    leading_hit: Option<LeadingHitZone>,
    /// Backs [`propagates_pointer_interaction`](Widget::propagates_pointer_interaction)
    /// — whether pointer hover/press reaches the row's children. `false` (the
    /// default) makes the row an opaque selection target, the original
    /// behavior; a collection sets it `true` only when it hosts an interactive
    /// row child (an expandable grid's disclosure chevron). It's a
    /// **collection-level** constant, not per-row: masonry caches this at
    /// widget creation and virtualization recycles a row widget across
    /// positions (a leaf slot may later render a parent), so every row in a
    /// collection that *can* defer must propagate — see [`Self::new`].
    propagates_pointer: bool,
}

// --- MARK: BUILDERS
impl RowClickable {
    /// `propagates_pointer` must be a **collection-level** decision (`true`
    /// iff the collection ever defers to a row child), not derived per row:
    /// masonry caches it at creation and can't change it, while virtualization
    /// recycles a row widget across positions, so a leaf slot that later holds
    /// a parent must already propagate.
    #[must_use]
    pub fn new(
        child: NewWidget<impl Widget + ?Sized>,
        selected: bool,
        theme: &Theme,
        leading_hit: Option<LeadingHitZone>,
        propagates_pointer: bool,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            selected,
            theme: *theme,
            leading_hit,
            propagates_pointer,
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

    /// Updates the leading defer-to-child hit zone (see the field docs).
    /// Cheap: it only affects the next press's capture guard, so no repaint
    /// or relayout is requested.
    pub fn set_leading_hit(this: &mut WidgetMut<'_, Self>, zone: Option<LeadingHitZone>) {
        this.widget.leading_hit = zone;
    }
}

// --- MARK: IMPL WIDGET
impl Widget for RowClickable {
    type Action = RowInteraction;

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
        // A pointer click is always a *selection* intent.
        let zone = self.leading_hit;
        match click::primary_click_when(ctx, event, |ctx, state| {
            // Capture (→ select) unless the press lands in the deferred
            // child's zone; an inset zone leaves the gutter to its left
            // selectable (see `LeadingHitZone`).
            let x = ctx.local_position(state.position).x;
            !zone.is_some_and(|z| z.contains(x))
        }) {
            // Rows take keyboard focus so a subsequent Ctrl/Cmd+C lands
            // inside the grid (see the module docs).
            Some(ClickPhase::Down(_)) => ctx.request_focus(),
            Some(ClickPhase::Up {
                state,
                completed: true,
            }) => {
                ctx.submit_action::<Self::Action>(RowInteraction::Select(row_click_action(
                    state.modifiers,
                )));
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
        // Space *selects* (pointer-click parity); Enter *activates* ("open").
        // Both carry the event modifiers. Tab is deliberately not handled, so
        // masonry's built-in focus traversal moves between rows.
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        let interaction = match &key.key {
            Key::Named(NamedKey::Enter) => {
                RowInteraction::Activate(row_click_action(key.modifiers))
            }
            Key::Character(s) if s == " " => {
                RowInteraction::Select(row_click_action(key.modifiers))
            }
            _ => return,
        };
        ctx.submit_action::<Self::Action>(interaction);
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
        // Collection-gated (see the `propagates_pointer` field): a plain
        // grid/list keeps `false` — an opaque selection target, the original
        // behavior — while a collection with an interactive row child (an
        // expandable grid's disclosure chevron) sets `true` so that child
        // receives hover/press. A press over a non-interactive cell still
        // bubbles up here, so row selection is unchanged; the positional guard
        // in `on_pointer_event` keeps a press over the leading control from
        // also selecting the row.
        self.propagates_pointer
    }
}

// --- MARK: XILEM VIEW WRAPPER

use std::marker::PhantomData;
use std::sync::Arc;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Boxed row-activation handler. The row id is baked in by the caller, so
/// this only needs `&mut State`. `None` on a [`ClickableRow`] means the row
/// has no activation handler, so Enter falls back to selection.
type RowActivate<State> = Arc<dyn Fn(&mut State) + Send + Sync>;

/// Wraps a child view in `RowClickable` and routes its [`RowInteraction`]
/// through the supplied callbacks: a pointer click or Space runs `on_click`
/// (selection); Enter runs the optional `on_activate` (activation), or falls
/// back to `on_click` when none is set.
///
/// Both callbacks run synchronously against the host's app state during
/// xilem's message-handling pass. Use `on_click` to apply the right
/// [`SelectionState`](super::SelectionState) op based on the
/// [`RowClickAction`] modifier flags; use [`Self::on_activate`] to "open" the
/// row.
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
    /// See [`RowClickable::leading_hit`]. `None` by default (whole row
    /// selects); set via [`Self::leading_hit`] on an expandable row so the
    /// chevron's box defers to the chevron control.
    leading_hit: Option<LeadingHitZone>,
    /// See [`RowClickable::propagates_pointer_interaction`]. `false` by default
    /// (opaque selection-target row); a collection sets it `true` via
    /// [`Self::propagate_pointer_to_children`] when it hosts an interactive row
    /// child. Collection-level, not per-row (see [`RowClickable::new`]).
    propagates_pointer: bool,
    on_activate: Option<RowActivate<State>>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

/// Constructor for [`ClickableRow`]. `selected` is reported to assistive
/// technology via accesskit's `Selected` property — pass the row's current
/// [`SelectionState`](super::SelectionState) membership. `theme` colors the
/// focus ring drawn when the row has keyboard focus. The row has no
/// activation handler by default; add one with [`ClickableRow::on_activate`].
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
        leading_hit: None,
        propagates_pointer: false,
        on_activate: None,
        phantom: PhantomData,
    }
}

impl<V, State, Action, F> ClickableRow<V, State, Action, F> {
    /// Reserves a leading hit [`zone`](LeadingHitZone) that defers to an
    /// interactive child sitting there — an expandable row's disclosure
    /// chevron, a leading row-action control.
    ///
    /// A primary press inside the zone doesn't select the row; it bubbles to
    /// the child control instead. Presses outside it — including the indent
    /// gutter to the left of an inset zone — select as usual. `None` (the
    /// default) reserves nothing.
    pub fn leading_hit(mut self, zone: Option<LeadingHitZone>) -> Self {
        self.leading_hit = zone;
        self
    }

    /// Lets pointer hover/press reach the row's children (so an interactive
    /// row child — a disclosure chevron — can receive them). `false` (the
    /// default) keeps the row an opaque selection target.
    ///
    /// Pass a **collection-level** value (`true` iff the collection ever
    /// defers to a row child), not a per-row one: it's cached at widget
    /// creation and virtualization recycles a row across positions — see
    /// [`RowClickable::new`].
    pub fn propagate_pointer_to_children(mut self, propagate: bool) -> Self {
        self.propagates_pointer = propagate;
        self
    }

    /// Sets the row's *activation* handler, run when Enter is pressed on the
    /// focused row (a distinct intent from selection — see the module docs).
    /// Without one, Enter falls back to `on_click` (selection). The row id is
    /// the caller's responsibility to bake into `on_activate`.
    pub fn on_activate(mut self, on_activate: impl Fn(&mut State) + Send + Sync + 'static) -> Self
    where
        State: 'static,
    {
        self.on_activate = Some(Arc::new(on_activate));
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
            self.leading_hit,
            self.propagates_pointer,
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
        if self.leading_hit != prev.leading_hit {
            RowClickable::set_leading_hit(&mut element, self.leading_hit);
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
        // A message addressed to *this* wrapper's `RowClickable` arrives
        // fully routed (empty remaining path) — that's our own row
        // interaction. A message with a non-empty path is bound for an
        // interactive descendant inside the row content (a disclosure chevron,
        // a leading row-action) and must be forwarded untouched: probing it
        // with `take_message` would panic ("message has not reached its
        // target"). Same guard as `CopyOnShortcutView` / `CollectionBodyView`.
        if message.remaining_path().is_empty() {
            if let Some(interaction) = message.take_message::<RowInteraction>() {
                match *interaction {
                    RowInteraction::Select(action) => (self.on_click)(app_state, action),
                    RowInteraction::Activate(action) => match self.on_activate.as_ref() {
                        Some(on_activate) => on_activate(app_state),
                        // No activation handler wired: Enter falls back to
                        // selection so keyboard operation never regresses.
                        None => (self.on_click)(app_state, action),
                    },
                }
            }
            return MessageResult::Action(Action::default());
        }
        let mut child = RowClickable::child_mut(&mut element);
        match self
            .child
            .message(view_state, message, child.downcast(), app_state)
        {
            // Lift the ()-typed row content's action into the host's Action
            // so it still re-runs the host's app logic.
            MessageResult::Action(()) => MessageResult::Action(Action::default()),
            MessageResult::RequestRebuild => MessageResult::RequestRebuild,
            MessageResult::Nop => MessageResult::Nop,
            MessageResult::Stale => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use masonry::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
    use masonry::core::{NewWidget, PointerButton, PointerEvent, TextEvent};
    use masonry::kurbo::{Axis, Point};
    use masonry::layout::Length;
    use masonry::testing::{ModularWidget, TestHarness};
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{LeadingHitZone, RowClickAction, RowClickable, RowInteraction};
    use crate::Theme;

    fn key_down(key: Key, modifiers: Modifiers) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key,
            modifiers,
            ..Default::default()
        })
    }

    fn harness_with_zone(zone: Option<LeadingHitZone>) -> TestHarness<RowClickable> {
        let child = NewWidget::new(Label::new("row"));
        // Pointer propagation is orthogonal to these selection-guard tests
        // (the child is an inert label); `true` mirrors an expandable row.
        let widget = RowClickable::new(child, false, &Theme::default(), zone, true);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (120, 24))
    }

    /// A leading-edge zone `[0, width)` — the row-action / #97 shape (offset
    /// `0`). `0.0` reserves nothing.
    fn harness_with_leading(width: f64) -> TestHarness<RowClickable> {
        harness_with_zone((width > 0.0).then_some(LeadingHitZone { offset: 0.0, width }))
    }

    fn harness() -> TestHarness<RowClickable> {
        harness_with_zone(None)
    }

    /// Presses the primary button at `x` (row is 24 px tall, so `y = 12` is
    /// mid-row), then releases at the same point, and returns whether the row
    /// emitted a *selection* intent (`RowInteraction::Select`) — a pointer
    /// click is always a select, never an activate.
    fn click_at_x(harness: &mut TestHarness<RowClickable>, x: f64) -> bool {
        harness.mouse_move(Point::new(x, 12.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        matches!(
            harness.pop_action::<RowInteraction>().map(|(a, _)| a),
            Some(RowInteraction::Select(_)),
        )
    }

    /// Space *selects* the focused row (keyboard parity with a click), so a
    /// pointer is not required to select.
    #[test]
    fn space_selects_the_focused_row() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        let handled =
            harness.process_text_event(key_down(Key::Character(" ".into()), Modifiers::empty()));
        assert!(handled.is_handled());
        assert_eq!(
            harness.pop_action::<RowInteraction>().map(|(a, _)| a),
            Some(RowInteraction::Select(RowClickAction::default())),
        );
    }

    /// Enter *activates* the focused row — a distinct intent from Space's
    /// selection (the whole point of #108).
    #[test]
    fn enter_activates_the_focused_row() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        let handled =
            harness.process_text_event(key_down(Key::Named(NamedKey::Enter), Modifiers::empty()));
        assert!(handled.is_handled());
        assert_eq!(
            harness.pop_action::<RowInteraction>().map(|(a, _)| a),
            Some(RowInteraction::Activate(RowClickAction::default())),
        );
    }

    /// Shift held during a Space selection carries through as a range-extend.
    #[test]
    fn shift_space_carries_the_shift_modifier() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        harness.process_text_event(key_down(Key::Character(" ".into()), Modifiers::SHIFT));
        let action = harness.pop_action::<RowInteraction>().map(|(a, _)| a);
        assert_eq!(
            action,
            Some(RowInteraction::Select(RowClickAction {
                shift: true,
                action_mod: false
            }))
        );
    }

    /// A non-activating key (a printable character other than space) leaves
    /// the row alone, so type-ahead / other handlers still see it.
    #[test]
    fn other_keys_do_nothing() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));

        harness.process_text_event(key_down(Key::Character("x".into()), Modifiers::empty()));
        assert!(harness.pop_action::<RowInteraction>().is_none());
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

    /// The zone is the half-open `[offset, offset + width)` interval, so its
    /// far edge belongs to the row: a press exactly at `offset + width`
    /// selects.
    #[test]
    fn press_at_zone_boundary_selects() {
        let mut harness = harness_with_leading(20.0);
        assert!(
            click_at_x(&mut harness, 20.0),
            "boundary belongs to the row"
        );
    }

    /// The tree indent-gutter fix: an *inset* zone (`offset > 0`, as a nested
    /// parent's chevron sits after its depth indent) reserves only the
    /// child's box. The gutter to the zone's left selects the row (like a
    /// leaf's indent — no depth-scaling dead zone), the zone itself defers,
    /// and content past it still selects.
    #[test]
    fn inset_zone_leaves_the_gutter_to_its_left_selectable() {
        // Chevron box [30, 50): 30 px indent (~depth 2) + 20 px chevron.
        let mut harness = harness_with_zone(Some(LeadingHitZone {
            offset: 30.0,
            width: 20.0,
        }));
        assert!(
            click_at_x(&mut harness, 15.0),
            "the indent gutter left of the chevron selects (was a dead zone)",
        );
        assert!(
            !click_at_x(&mut harness, 40.0),
            "a press on the chevron box defers, not selects",
        );
        assert!(
            click_at_x(&mut harness, 70.0),
            "content past the chevron selects as usual",
        );
        // Half-open `[30, 50)`: the near edge defers, the far edge selects.
        assert!(
            !click_at_x(&mut harness, 30.0),
            "zone start (offset) defers"
        );
        assert!(click_at_x(&mut harness, 50.0), "zone end selects");
    }

    /// Presses mid-row and reports `(child_pointer_downs, row_selected)` for a
    /// row built with the given `propagates_pointer`. The child is a minimal
    /// interactive widget that counts pointer-downs but doesn't capture, so the
    /// press still bubbles to the row.
    fn press_with_propagation(propagates: bool) -> (usize, bool) {
        let child_downs = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&child_downs);
        let child = ModularWidget::new(())
            .accepts_pointer_interaction(true)
            .measure_fn(|(), _ctx, _props, axis, _len_req, _cross| match axis {
                Axis::Horizontal => Length::px(120.0),
                Axis::Vertical => Length::px(24.0),
            })
            .pointer_event_fn(move |(), _ctx, _props, event| {
                if matches!(event, PointerEvent::Down(_)) {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            });
        let widget = RowClickable::new(
            NewWidget::new(child),
            false,
            &Theme::default(),
            None,
            propagates,
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (120, 24),
        );

        harness.mouse_move(Point::new(60.0, 12.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));

        (
            child_downs.load(Ordering::Relaxed),
            matches!(
                harness.pop_action::<RowInteraction>().map(|(a, _)| a),
                Some(RowInteraction::Select(_)),
            ),
        )
    }

    /// With propagation on (an expandable collection), a control nested in row
    /// content receives pointer events — this is what lets the disclosure
    /// chevron become reachable. The press also bubbles up so the row still
    /// selects (no leading zone reserved here).
    #[test]
    fn nested_interactive_child_receives_the_press_when_propagating() {
        let (child_downs, selected) = press_with_propagation(true);
        assert_eq!(
            child_downs, 1,
            "the nested child must receive the press (pointer propagation is on)",
        );
        assert!(
            selected,
            "the row still selects — the press bubbles past the non-capturing child",
        );
    }

    /// With propagation off (a plain grid/list — the collection-level default),
    /// the row is opaque: children never see the pointer, exactly as before the
    /// expandable feature. Row selection is unaffected either way. This locks
    /// the collection-level gate so the flip to `true` can't silently reach
    /// non-expandable collections.
    #[test]
    fn opaque_row_withholds_the_press_from_children() {
        let (child_downs, selected) = press_with_propagation(false);
        assert_eq!(
            child_downs, 0,
            "an opaque row must not forward the press to its children",
        );
        assert!(selected, "the row itself still selects");
    }
}
