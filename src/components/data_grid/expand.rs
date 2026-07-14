//! Expandable / accordion rows for [`super::data_grid`] — the host-owned
//! [`ExpansionState`] plus the small [`DisclosureToggle`] control that a
//! tree-column cell wraps around its chevron.
//!
//! ## Host owns the flattened tree
//!
//! Expansion follows the grid's "host owns the data" split, exactly like
//! sorting and filtering. The grid never grows or hides child rows itself:
//! the host keeps an [`ExpansionState`] (a set of expanded row ids), and
//! when it changes, **the host splices the now-visible descendants into the
//! flat `&[R]` slice it serves and bumps `row_count`**. Children become
//! ordinary flat rows, so they inherit virtualization, the fixed row
//! height, scroll, and selection for free — no special row model.
//!
//! The grid's only jobs are visual + intent: render a disclosure chevron in
//! the tree column (indented by the row's depth), and report a toggle
//! request (the row's stable id) back through
//! [`DataGrid::expandable`](super::view::DataGrid::expandable)'s `on_toggle`
//! so the host can flip its `ExpansionState` and re-derive the slice.
//!
//! The toggle keys by the **stable row id** (the `getRowId` contract shared
//! with selection), so an expanded row stays expanded across host-side
//! sort/filter reordering.

use std::collections::BTreeSet;

use crate::theme::Density;

/// The set of **expanded row ids**, held in the host's app state.
///
/// Mirrors [`SelectionState`](super::SelectionState) /
/// [`FilterState`](super::FilterState): a small, cloneable state object the
/// host owns and mutates, keyed by the stable row id. The grid reads it
/// only indirectly — through the `is_expanded` closure the host passes to
/// [`DataGrid::expandable`](super::view::DataGrid::expandable) — because the
/// grid renders a host-flattened slice and never derives row order itself.
///
/// Backed by a [`BTreeSet`] so iteration is deterministic (id order), which
/// keeps any host-side re-flattening stable frame to frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpansionState {
    expanded: BTreeSet<u64>,
}

impl ExpansionState {
    /// An empty expansion (every expandable row collapsed).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the row with stable id `id` is currently expanded.
    #[must_use]
    pub fn is_expanded(&self, id: u64) -> bool {
        self.expanded.contains(&id)
    }

    /// Flips `id` between expanded and collapsed, returning the new state.
    /// The natural response to a chevron toggle.
    pub fn toggle(&mut self, id: u64) -> bool {
        if self.expanded.remove(&id) {
            false
        } else {
            self.expanded.insert(id);
            true
        }
    }

    /// Marks `id` expanded. No-op if already expanded.
    pub fn expand(&mut self, id: u64) {
        self.expanded.insert(id);
    }

    /// Marks `id` collapsed. No-op if already collapsed.
    pub fn collapse(&mut self, id: u64) {
        self.expanded.remove(&id);
    }

    /// Expands `id` when `expanded`, collapses it otherwise.
    pub fn set(&mut self, id: u64, expanded: bool) {
        if expanded {
            self.expand(id);
        } else {
            self.collapse(id);
        }
    }

    /// Collapses every row.
    pub fn clear(&mut self) {
        self.expanded.clear();
    }

    /// `true` when no row is expanded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.expanded.is_empty()
    }

    /// How many rows are expanded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.expanded.len()
    }

    /// Iterates the expanded ids in ascending order — the host uses this to
    /// re-derive its flattened, ordered slice.
    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.expanded.iter().copied()
    }
}

// --- MARK: DENSITY-DERIVED TREE METRICS --------------------------------

/// Horizontal indent applied per depth level in the tree column
/// (`gap_lg` — 15 px at balanced density). A row at depth `d` shifts its
/// chevron + content right by `d * tree_indent_step`.
pub(crate) fn tree_indent_step(density: &Density) -> f64 {
    f64::from(density.gap_lg)
}

/// Width of the disclosure chevron's square-ish click target
/// (`ui_font_size + pad_h + gap` — 25 px at balanced; the `gap` term over
/// plain `ui_font_size + pad_h` gives the small glyph a more comfortable
/// pointer/touch target without widening the tree indent it sits in). A
/// leaf row reserves the same width as an inert spacer so its cell text
/// lines up under a parent's text. This is also the chevron slice of a
/// parent row's leading defer-to-toggle hit zone (see the tree-cell builder
/// in `view.rs`).
pub(crate) fn disclosure_hit_width(density: &Density) -> f64 {
    f64::from(density.ui_font_size) + f64::from(density.pad_h) + f64::from(density.gap)
}

// --- MARK: DISCLOSURE TOGGLE WIDGET ------------------------------------

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};

use crate::Theme;
use crate::collection::single_child;
use crate::components::click::{self, ClickPhase};
use crate::components::interaction::{
    disclosure_accessibility, is_access_click, keyboard_activate,
};
use crate::focus_ring::paint_focus_ring_inset;

/// Action emitted by [`DisclosureToggle`] on a primary-button click — the
/// user asked to expand/collapse this row. It carries no payload: the
/// [`disclosure_toggle`] view already closes over the row's id, so it just
/// runs the host's `on_toggle` for that row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisclosureToggleRequested;

/// Single-child wrapper that emits [`DisclosureToggleRequested`] on a
/// primary-button release inside its bounds — the interactive chevron on an
/// expandable row's tree cell.
///
/// The expandable row reserves this control's width as a leading
/// defer-to-child zone (see
/// [`ClickableRow::leading_hit`](crate::collection::row_click::ClickableRow::leading_hit)),
/// so a press here toggles the row instead of selecting it. Modeled on
/// [`HeaderClickable`](super::header_click::HeaderClickable) for the pointer
/// path and on `RowClickable` for the keyboard/focus a11y: a
/// [`Role::Button`] disclosure control that reports `aria-expanded` +
/// `aria-level`, is reachable by Tab, activates on Enter/Space and on an
/// assistive-tech click, and paints a focus ring while focused.
pub struct DisclosureToggle {
    child: WidgetPod<dyn Widget>,
    /// Reported to assistive tech via `aria-expanded`.
    expanded: bool,
    /// 1-based tree depth, reported via `aria-level` so a screen reader can
    /// announce how deep the row sits.
    level: usize,
    /// Colors the focus ring painted while the toggle has keyboard focus.
    theme: Theme,
}

// --- MARK: BUILDERS
impl DisclosureToggle {
    #[must_use]
    pub fn new(
        child: NewWidget<impl Widget + ?Sized>,
        expanded: bool,
        level: usize,
        theme: &Theme,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            expanded,
            level,
            theme: *theme,
        }
    }
}

// --- MARK: WIDGETMUT
impl DisclosureToggle {
    /// Returns a mutable reference to the wrapped chevron child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Updates the reported expanded state, requesting an accessibility
    /// update on change so screen readers re-announce `aria-expanded`.
    pub fn set_expanded(this: &mut WidgetMut<'_, Self>, expanded: bool) {
        if this.widget.expanded != expanded {
            this.widget.expanded = expanded;
            this.ctx.request_accessibility_update();
        }
    }

    /// Updates the reported `aria-level` (1-based tree depth).
    pub fn set_level(this: &mut WidgetMut<'_, Self>, level: usize) {
        if this.widget.level != level {
            this.widget.level = level;
            this.ctx.request_accessibility_update();
        }
    }

    /// Replaces the theme used to color the focus ring (e.g. on a theme
    /// swap). Repaints only when the value changes.
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for DisclosureToggle {
    type Action = DisclosureToggleRequested;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Shared Down→capture / Up-iff-active-and-hovered recognizer. The
        // enclosing row defers its leading zone to this control, so the
        // press reaches us rather than selecting the row.
        if let Some(ClickPhase::Up {
            completed: true, ..
        }) = click::primary_click(ctx, event)
        {
            ctx.submit_action::<Self::Action>(DisclosureToggleRequested);
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        // Enter/Space on the focused toggle activates it (keyboard parity
        // with a pointer click). Tab is deliberately not handled so masonry's
        // built-in focus traversal keeps moving between controls.
        if ctx.is_focus_target() && keyboard_activate(event, true) {
            ctx.submit_action::<Self::Action>(DisclosureToggleRequested);
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        // An assistive-tech "click" (e.g. a screen-reader activation) toggles
        // the row, same as a pointer or keyboard activation.
        if is_access_click(event) {
            ctx.submit_action::<Self::Action>(DisclosureToggleRequested);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // The focus ring drawn in `paint` depends on `is_focus_target`;
        // without this, gaining/losing focus wouldn't repaint the ring.
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
        // The chevron child paints the glyph; the toggle adds a focus ring
        // while focused, inset like every other focusable control.
        if ctx.is_focus_target() {
            paint_focus_ring_inset(painter, ctx.border_box_size(), &self.theme);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        disclosure_accessibility(node, self.expanded, true);
        node.set_level(self.level);
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
        false
    }
}

// --- MARK: XILEM VIEW WRAPPER ------------------------------------------

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Wraps a chevron view in [`DisclosureToggle`] and runs `on_toggle`
/// against the host's app state on each primary-button release.
///
/// `on_toggle` closes over the row's stable id, so the host flips its
/// [`ExpansionState`] for that row (and re-derives its flattened slice).
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct DisclosureToggleView<V, State, Action, F> {
    child: V,
    expanded: bool,
    level: usize,
    theme: Theme,
    on_toggle: F,
    phantom: PhantomData<fn() -> (State, Action)>,
}

/// Constructor for [`DisclosureToggleView`]. `expanded` drives the chevron
/// direction (reported via `aria-expanded`); `level` is the 1-based tree
/// depth (`aria-level`); `theme` colors the focus ring; `on_toggle` runs on
/// activation (click / Enter / Space / assistive-tech click).
pub fn disclosure_toggle<V, State, Action, F>(
    child: V,
    expanded: bool,
    level: usize,
    theme: &Theme,
    on_toggle: F,
) -> DisclosureToggleView<V, State, Action, F>
where
    V: WidgetView<State, Action>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
    State: 'static,
    Action: 'static,
{
    DisclosureToggleView {
        child,
        expanded,
        level,
        theme: *theme,
        on_toggle,
        phantom: PhantomData,
    }
}

impl<V, State, Action, F> ViewMarker for DisclosureToggleView<V, State, Action, F> {}

impl<V, State, Action, F> View<State, Action, ViewCtx> for DisclosureToggleView<V, State, Action, F>
where
    V: WidgetView<State, Action>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<DisclosureToggle>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget =
            DisclosureToggle::new(child_pod.new_widget, self.expanded, self.level, &self.theme);
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
        if self.expanded != prev.expanded {
            DisclosureToggle::set_expanded(&mut element, self.expanded);
        }
        if self.level != prev.level {
            DisclosureToggle::set_level(&mut element, self.level);
        }
        if self.theme != prev.theme {
            DisclosureToggle::set_theme(&mut element, &self.theme);
        }
        let mut child = DisclosureToggle::child_mut(&mut element);
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
            let mut child = DisclosureToggle::child_mut(&mut element);
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
        if message
            .take_message::<DisclosureToggleRequested>()
            .is_some()
        {
            MessageResult::Action((self.on_toggle)(app_state))
        } else {
            let mut child = DisclosureToggle::child_mut(&mut element);
            self.child
                .message(view_state, message, child.downcast(), app_state)
        }
    }
}

#[cfg(test)]
mod tests {
    use masonry::accesskit::{Action, ActionRequest, TreeId};
    use masonry::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
    use masonry::core::{NewWidget, PointerButton, TextEvent};
    use masonry::kurbo::Point;
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{DisclosureToggle, DisclosureToggleRequested, ExpansionState};
    use crate::Theme;

    fn key_down(key: Key) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key,
            modifiers: Modifiers::empty(),
            ..Default::default()
        })
    }

    fn key_up(key: Key) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Up,
            key,
            modifiers: Modifiers::empty(),
            ..Default::default()
        })
    }

    #[test]
    fn toggle_flips_membership_and_reports_new_state() {
        let mut exp = ExpansionState::new();
        assert!(!exp.is_expanded(7));
        assert!(exp.toggle(7), "first toggle expands");
        assert!(exp.is_expanded(7));
        assert_eq!(exp.len(), 1);
        assert!(!exp.toggle(7), "second toggle collapses");
        assert!(!exp.is_expanded(7));
        assert!(exp.is_empty());
    }

    #[test]
    fn expand_collapse_set_are_idempotent() {
        let mut exp = ExpansionState::new();
        exp.expand(1);
        exp.expand(1);
        assert_eq!(exp.len(), 1);
        exp.collapse(1);
        exp.collapse(1);
        assert!(exp.is_empty());
        exp.set(2, true);
        exp.set(3, true);
        exp.set(2, false);
        assert!(exp.is_expanded(3));
        assert!(!exp.is_expanded(2));
    }

    #[test]
    fn iter_yields_expanded_ids_in_ascending_order() {
        let mut exp = ExpansionState::new();
        exp.expand(30);
        exp.expand(10);
        exp.expand(20);
        assert_eq!(exp.iter().collect::<Vec<_>>(), vec![10, 20, 30]);
        exp.clear();
        assert!(exp.is_empty());
    }

    fn toggle_harness() -> TestHarness<DisclosureToggle> {
        let chevron = NewWidget::new(Label::new(">"));
        let widget = DisclosureToggle::new(chevron, false, 1, &Theme::default());
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (20, 24))
    }

    #[test]
    fn primary_click_emits_a_toggle_request() {
        let mut harness = toggle_harness();
        harness.mouse_move(Point::new(10.0, 12.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(
            harness.pop_action::<DisclosureToggleRequested>().is_some(),
            "a primary click on the chevron must request a toggle",
        );
    }

    /// Enter and Space activate the focused toggle — keyboard parity with a
    /// click, so the chevron is operable without a pointer. Activation fires
    /// on key-up (matching every other press widget).
    #[test]
    fn enter_and_space_activate_the_focused_toggle() {
        for key in [Key::Named(NamedKey::Enter), Key::Character(" ".into())] {
            let mut harness = toggle_harness();
            let root = harness.root_widget().id();
            harness.focus_on(Some(root));

            harness.process_text_event(key_down(key.clone()));
            // Key-down doesn't activate; only the key-up does.
            assert!(harness.pop_action::<DisclosureToggleRequested>().is_none());

            let handled = harness.process_text_event(key_up(key));
            assert!(handled.is_handled());
            assert!(
                harness.pop_action::<DisclosureToggleRequested>().is_some(),
                "Enter/Space key-up must toggle the focused chevron",
            );
        }
    }

    /// A key press on an unfocused toggle does nothing (keyboard activation
    /// is gated on focus), so it doesn't hijack keys meant for elsewhere.
    #[test]
    fn keys_do_nothing_without_focus() {
        let mut harness = toggle_harness();
        harness.process_text_event(key_up(Key::Character(" ".into())));
        assert!(harness.pop_action::<DisclosureToggleRequested>().is_none());
    }

    /// An assistive-tech Click activates the toggle (screen-reader parity).
    #[test]
    fn access_click_emits_a_toggle_request() {
        let mut harness = toggle_harness();
        let root = harness.root_widget().id();
        // `process_access_event` (unlike `accessibility_click_on`) runs the
        // rewrite passes afterward, so the submitted action reaches the queue.
        harness.process_access_event(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: root.to_raw().into(),
            data: None,
        });
        assert!(
            harness.pop_action::<DisclosureToggleRequested>().is_some(),
            "an assistive-tech click must request a toggle",
        );
    }
}
