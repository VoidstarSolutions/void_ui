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
//!
//! Up/Down also move focus between materialized rows — handled locally
//! here rather than in
//! [`CollectionBodyWidget`](super::body::CollectionBodyWidget), which would
//! be the more natural owner, because masonry's `VirtualScroll` sits
//! between a focused row and `CollectionBodyWidget` in the tree and claims
//! Up/Down via its own built-in arrow-key scrolling before
//! `CollectionBodyWidget`'s `on_text_event` would run (masonry's keyboard
//! dispatch is bubble-only, with no capture phase). `CollectionBodyWidget`
//! precomputes each row's up/down neighbor and pushes it via
//! [`Self::set_nav`] whenever the materialized window settles, since a row
//! doesn't know its siblings on its own.

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, Modifiers, NewWidget,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};

use super::single_child;
use crate::Theme;
use crate::components::click::{self, ClickPhase};
use crate::focus_ring::paint_focus_ring_inset;

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
    /// Right on a collapsed parent, or Left on an expanded parent — *toggle*
    /// the row's expansion (WAI-ARIA treegrid). Emitted only by expandable
    /// rows that carry [`TreeRowMeta`]; the view routes it to the host's
    /// expansion toggle.
    Toggle,
}

/// Which host callback a [`RowInteraction`] resolves to, once the row's
/// activation-handler presence is known. Split out from the view's message
/// dispatch so the select/activate/fallback routing is unit-testable without
/// standing up a live view tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowDispatch {
    /// Run the selection callback with these modifiers — a pointer click,
    /// Space, or an Enter that fell back for lack of an activation handler.
    Select(RowClickAction),
    /// Run the activation callback (only possible when one is wired).
    Activate,
    /// Run the expansion-toggle callback (an expandable row's Right/Left).
    Toggle,
}

/// Resolves a [`RowInteraction`] to the callback that should fire. `Select`
/// always selects; `Activate` activates only when the row has a handler
/// (`has_activate`), otherwise it falls back to selection so keyboard
/// operation never regresses when no handler is wired; `Toggle` routes to the
/// expansion handler.
fn classify_interaction(interaction: RowInteraction, has_activate: bool) -> RowDispatch {
    match interaction {
        RowInteraction::Toggle => RowDispatch::Toggle,
        // A wired Enter activates; everything else (any click/Space, or an
        // unwired Enter) selects — the fallback that keeps keyboard operation
        // intact when no activation handler is present.
        RowInteraction::Activate(_) if has_activate => RowDispatch::Activate,
        RowInteraction::Select(action) | RowInteraction::Activate(action) => {
            RowDispatch::Select(action)
        }
    }
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

/// Per-row tree metadata a collection attaches to an expandable row so the row
/// can answer directional (Right/Left) keyboard navigation locally: whether it
/// is an expandable parent, its expand state, and its tree depth. `None` on a
/// [`RowClickable`] means a plain, non-tree row (Right/Left do nothing here and
/// bubble to the collection body for focus movement).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRowMeta {
    /// 0-based tree depth (used by focus-to-parent navigation).
    pub depth: u16,
    /// Whether this row has children (i.e. is an expandable parent, not a leaf).
    pub has_children: bool,
    /// Whether this parent row is currently expanded.
    pub is_expanded: bool,
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
    /// — whether pointer hover/press reaches the row's children. `body_view`
    /// sets this `true` for every collection: the capture guard in
    /// `on_pointer_event` is what actually keeps a non-capturing child from
    /// stealing the row's selection, so propagation itself is safe
    /// unconditionally, not just for rows with an interactive leading child.
    /// `false` remains available as a widget-level opaque-row mode (see
    /// [`Self::new`]'s doc). It's a **collection-level** constant, not
    /// per-row: masonry caches this at widget creation and virtualization
    /// recycles a row widget across positions (a leaf slot may later render a
    /// different row), so it can't vary per row within one collection.
    propagates_pointer: bool,
    /// Tree metadata for directional keyboard nav, set by an expandable
    /// collection. `Some` on a tree row lets Right/Left act locally (toggle
    /// expansion); `None` on a plain row leaves Right/Left to bubble up. See
    /// [`TreeRowMeta`] and [`Self::on_text_event`].
    tree_meta: Option<TreeRowMeta>,
    /// Precomputed up/down materialized-neighbor targets for Up/Down
    /// row-focus navigation, pushed by
    /// [`CollectionBodyWidget::refresh_row_nav`](super::body::CollectionBodyWidget::refresh_row_nav)
    /// whenever the materialized window settles. `None` when there's no
    /// neighbor in that direction (a materialized-window or data edge) —
    /// see [`Self::on_text_event`].
    nav_up: Option<WidgetId>,
    nav_down: Option<WidgetId>,
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
        tree_meta: Option<TreeRowMeta>,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            selected,
            theme: *theme,
            leading_hit,
            propagates_pointer,
            tree_meta,
            nav_up: None,
            nav_down: None,
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

    /// Updates the row's [`TreeRowMeta`] (expand state / depth), driving what
    /// Right/Left do on the focused row. Cheap: it only affects the next key
    /// press, so no repaint or relayout is requested.
    pub fn set_tree_meta(this: &mut WidgetMut<'_, Self>, meta: Option<TreeRowMeta>) {
        this.widget.tree_meta = meta;
    }

    /// Updates the row's up/down materialized-neighbor targets for Up/Down
    /// navigation (see [`Self::on_text_event`]). Cheap: it only affects the
    /// next key press, so no repaint or relayout is requested.
    pub(crate) fn set_nav(
        this: &mut WidgetMut<'_, Self>,
        up: Option<WidgetId>,
        down: Option<WidgetId>,
    ) {
        this.widget.nav_up = up;
        this.widget.nav_down = down;
    }
}

impl RowClickable {
    /// Whether a directional key should *toggle* this row's expansion — true
    /// only for an expandable parent whose current expand state matches
    /// `expanded`. Right acts on a collapsed parent (`expanded == false`), Left
    /// on an expanded one (`expanded == true`); leaves and plain rows never
    /// toggle, so their Right/Left bubble up for focus movement.
    fn toggles_on(&self, expanded: bool) -> bool {
        self.tree_meta
            .is_some_and(|m| m.has_children && m.is_expanded == expanded)
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
        // capture guard that keeps the row from stealing a press an interactive
        // cell child was aimed at. Two ways the row defers:
        //
        //  1. A descendant that already *captured the pointer* on this Down — a
        //     cell button ("Open"), the disclosure chevron. Pointer events
        //     bubble child→parent and `capture_pointer` is last-writer-wins
        //     (masonry), so without this guard the row would overwrite the
        //     child's capture on the way up and swallow the click. Because the
        //     child ran first, its capture is already visible here as the
        //     pointer-capture target. This is the general "pass the click
        //     through to clickable children" rule — no per-control geometry.
        //  2. The positional leading hit zone, for a leading child that does
        //     *not* capture (an inert alignment spacer still reserves the box).
        //
        // When the guard rejects, there's no `Down` (so no capture, no focus)
        // and the release can't complete (`is_active` stayed false), so the row
        // neither selects nor steals — the child owns the interaction. A pointer
        // click that reaches the row is always a *selection* intent.
        let zone = self.leading_hit;
        match click::primary_click_when(ctx, event, |ctx, state| {
            let descendant_captured = ctx
                .pointer_capture_target_id()
                .is_some_and(|id| id != ctx.widget_id());
            let x = ctx.local_position(state.position).x;
            !descendant_captured && !zone.is_some_and(|z| z.contains(x))
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
        // Up/Down move focus to the precomputed materialized neighbor
        // (pushed by `CollectionBodyWidget::refresh_row_nav`). Always
        // handled once a row has focus — even a no-op at a materialized or
        // data edge — so the event never falls through to `VirtualScroll`'s
        // built-in scroll-by-arrow-key, which would otherwise silently
        // replace row-focus movement with viewport scrolling (masonry's
        // keyboard dispatch is bubble-only, and `VirtualScroll` sits
        // between a focused row and `CollectionBodyWidget` in the tree —
        // see the module docs above).
        match &key.key {
            Key::Named(NamedKey::ArrowDown) => {
                if let Some(target) = self.nav_down {
                    ctx.set_focus(target);
                }
                ctx.set_handled();
                return;
            }
            Key::Named(NamedKey::ArrowUp) => {
                if let Some(target) = self.nav_up {
                    ctx.set_focus(target);
                }
                ctx.set_handled();
                return;
            }
            _ => {}
        }
        let interaction = match &key.key {
            Key::Named(NamedKey::Enter) => {
                RowInteraction::Activate(row_click_action(key.modifiers))
            }
            Key::Character(s) if s == " " => {
                RowInteraction::Select(row_click_action(key.modifiers))
            }
            // WAI-ARIA treegrid: Right on a collapsed parent expands it, Left
            // on an expanded parent collapses it — a *toggle* the row can
            // decide locally from its own tree metadata. The other Right/Left
            // cases (Right on an expanded parent → first child, Left on a
            // child → parent) are focus movement the row can't do (it doesn't
            // know row order), so they're deliberately left unhandled to bubble
            // to `CollectionBodyWidget`.
            Key::Named(NamedKey::ArrowRight) if self.toggles_on(false) => RowInteraction::Toggle,
            Key::Named(NamedKey::ArrowLeft) if self.toggles_on(true) => RowInteraction::Toggle,
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
            paint_focus_ring_inset(painter, ctx.border_box().size(), &self.theme);
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
        // Collection-gated (see the `propagates_pointer` field): `body_view`
        // sets `true` for every collection, plain grid/list included, so
        // interactive cell children receive hover/press. A press over a
        // non-interactive cell still bubbles up here, so row selection is
        // unchanged; the capture/positional guard in `on_pointer_event` keeps
        // a press a child actually claimed from also selecting the row.
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

/// Boxed row-expansion toggle handler, run when Right/Left toggles an
/// expandable row. The row id is baked in by the caller, so it only needs
/// `&mut State`. `None` means the row can't be toggled by keyboard.
type RowToggle<State> = Arc<dyn Fn(&mut State) + Send + Sync>;

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
    /// See [`RowClickable::tree_meta`]. `None` by default (plain row); set via
    /// [`Self::tree_meta`] on an expandable row so Right/Left can toggle it.
    tree_meta: Option<TreeRowMeta>,
    /// Expansion-toggle handler for Right/Left on an expandable row; `None` by
    /// default. Set via [`Self::on_toggle`].
    on_toggle: Option<RowToggle<State>>,
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
        tree_meta: None,
        on_toggle: None,
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
    /// row child — a disclosure chevron, a cell button — can receive them).
    /// `false` (the default) keeps the row an opaque selection target;
    /// `body_view` passes `true` unconditionally, since the capture guard in
    /// `RowClickable::on_pointer_event` makes propagation safe even when no
    /// row child ever defers.
    ///
    /// Pass a **collection-level** value, not a per-row one: it's cached at
    /// widget creation and virtualization recycles a row across positions —
    /// see [`RowClickable::new`].
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

    /// Attaches [`TreeRowMeta`] so Right/Left act on this row as a tree node —
    /// Right expands a collapsed parent, Left collapses an expanded one (via
    /// [`Self::on_toggle`]). `None` (the default) leaves the row plain.
    pub fn tree_meta(mut self, meta: Option<TreeRowMeta>) -> Self {
        self.tree_meta = meta;
        self
    }

    /// Sets the row's expansion-toggle handler, run when Right/Left toggles an
    /// expandable row (see [`Self::tree_meta`]). The row id is the caller's to
    /// bake into `on_toggle`.
    pub fn on_toggle(mut self, on_toggle: impl Fn(&mut State) + Send + Sync + 'static) -> Self
    where
        State: 'static,
    {
        self.on_toggle = Some(Arc::new(on_toggle));
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
            self.tree_meta,
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
        if self.tree_meta != prev.tree_meta {
            RowClickable::set_tree_meta(&mut element, self.tree_meta);
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
                match classify_interaction(*interaction, self.on_activate.is_some()) {
                    RowDispatch::Select(action) => (self.on_click)(app_state, action),
                    // `classify_interaction` only yields `Activate` when a
                    // handler is wired, so this `if let` always binds; an
                    // unwired Enter comes back as `Select` (the fallback).
                    RowDispatch::Activate => {
                        if let Some(on_activate) = self.on_activate.as_ref() {
                            on_activate(app_state);
                        }
                    }
                    RowDispatch::Toggle => {
                        if let Some(on_toggle) = self.on_toggle.as_ref() {
                            on_toggle(app_state);
                        }
                    }
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
    use masonry::core::{NewWidget, PointerButton, PointerEvent, TextEvent, WidgetId};
    use masonry::kurbo::{Axis, Point};
    use masonry::layout::Length;
    use masonry::testing::{ModularWidget, TestHarness};
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::{
        LeadingHitZone, RowClickAction, RowClickable, RowDispatch, RowInteraction, TreeRowMeta,
        classify_interaction,
    };
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
        let widget = RowClickable::new(child, false, &Theme::default(), zone, true, None);
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (120, 24))
    }

    /// A `RowClickable` carrying [`TreeRowMeta`] (an expandable tree row), for
    /// exercising the Right/Left toggle intents.
    fn tree_harness(has_children: bool, is_expanded: bool) -> TestHarness<RowClickable> {
        let child = NewWidget::new(Label::new("row"));
        let meta = TreeRowMeta {
            depth: 0,
            has_children,
            is_expanded,
        };
        let widget = RowClickable::new(child, false, &Theme::default(), None, true, Some(meta));
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(widget),
            (120, 24),
        );
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));
        harness
    }

    fn press_arrow(
        harness: &mut TestHarness<RowClickable>,
        key: NamedKey,
    ) -> Option<RowInteraction> {
        harness.process_text_event(key_down(Key::Named(key), Modifiers::empty()));
        harness.pop_action::<RowInteraction>().map(|(a, _)| a)
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

    /// The view-layer dispatch decision, isolated: a `Select` intent always
    /// selects, regardless of whether an activation handler is wired.
    #[test]
    fn select_intent_always_dispatches_to_selection() {
        let action = RowClickAction {
            shift: true,
            action_mod: false,
        };
        let intent = RowInteraction::Select(action);
        assert_eq!(
            classify_interaction(intent, true),
            RowDispatch::Select(action),
        );
        assert_eq!(
            classify_interaction(intent, false),
            RowDispatch::Select(action),
        );
    }

    /// With an activation handler wired, Enter dispatches to *activation* —
    /// not selection.
    #[test]
    fn activate_intent_dispatches_to_activation_when_handler_wired() {
        let intent = RowInteraction::Activate(RowClickAction::default());
        assert_eq!(
            classify_interaction(intent, true),
            RowDispatch::Activate,
            "a wired handler routes Enter to activation",
        );
    }

    /// The regression guard #108 promises: with *no* activation handler, Enter
    /// falls back to selection (carrying its modifiers) so keyboard operation
    /// never regresses on grids that don't opt into activation. This is the
    /// path the live view takes when `on_activate` is `None`.
    #[test]
    fn activate_intent_falls_back_to_selection_when_no_handler() {
        let action = RowClickAction {
            shift: true,
            action_mod: true,
        };
        let intent = RowInteraction::Activate(action);
        assert_eq!(
            classify_interaction(intent, false),
            RowDispatch::Select(action),
            "an unwired Enter must fall back to selection, modifiers intact",
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
            None,
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

    /// With propagation on — `body_view`'s setting for every collection, not
    /// just an expandable one — a control nested in row content receives
    /// pointer events; this is what lets both the disclosure chevron and a
    /// plain grid's cell button become reachable. The press also bubbles up
    /// so the row still selects (no leading zone reserved, and the child
    /// doesn't capture).
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

    /// With propagation off, the row is opaque: children never see the
    /// pointer, exactly as before the expandable feature. Row selection is
    /// unaffected either way. `body_view` never constructs this combination
    /// today — every collection propagates, and the capture guard is what
    /// keeps that safe — but the flag stays a widget-level mode in its own
    /// right, so this locks its behavior in isolation.
    #[test]
    fn opaque_row_withholds_the_press_from_children() {
        let (child_downs, selected) = press_with_propagation(false);
        assert_eq!(
            child_downs, 0,
            "an opaque row must not forward the press to its children",
        );
        assert!(selected, "the row itself still selects");
    }

    /// A dispatch-level check: a `Toggle` intent routes to the expansion
    /// handler, independent of whether an activation handler is wired.
    #[test]
    fn toggle_intent_dispatches_to_toggle() {
        assert_eq!(
            classify_interaction(RowInteraction::Toggle, true),
            RowDispatch::Toggle,
        );
        assert_eq!(
            classify_interaction(RowInteraction::Toggle, false),
            RowDispatch::Toggle,
        );
    }

    /// WAI-ARIA treegrid: Right on a **collapsed** parent emits `Toggle`
    /// (expand); Left on a collapsed parent does not (it's a focus-to-parent
    /// move the row leaves to the collection body).
    #[test]
    fn right_toggles_a_collapsed_parent_left_does_not() {
        let mut harness = tree_harness(true, false);
        assert_eq!(
            press_arrow(&mut harness, NamedKey::ArrowRight),
            Some(RowInteraction::Toggle),
            "Right on a collapsed parent expands it",
        );
        assert_eq!(
            press_arrow(&mut harness, NamedKey::ArrowLeft),
            None,
            "Left on a collapsed parent is focus movement — not the row's job",
        );
    }

    /// Left on an **expanded** parent emits `Toggle` (collapse); Right on an
    /// expanded parent does not (it's a focus-to-first-child move).
    #[test]
    fn left_toggles_an_expanded_parent_right_does_not() {
        let mut harness = tree_harness(true, true);
        assert_eq!(
            press_arrow(&mut harness, NamedKey::ArrowLeft),
            Some(RowInteraction::Toggle),
            "Left on an expanded parent collapses it",
        );
        assert_eq!(
            press_arrow(&mut harness, NamedKey::ArrowRight),
            None,
            "Right on an expanded parent is focus movement — not the row's job",
        );
    }

    /// A leaf row (`has_children` = false) never toggles: both arrows bubble up
    /// for focus movement.
    #[test]
    fn leaf_row_never_toggles() {
        let mut harness = tree_harness(false, false);
        assert_eq!(press_arrow(&mut harness, NamedKey::ArrowRight), None);
        assert_eq!(press_arrow(&mut harness, NamedKey::ArrowLeft), None);
    }

    /// A plain (non-tree) row with no `TreeRowMeta` ignores Right/Left entirely.
    #[test]
    fn plain_row_ignores_arrows() {
        let mut harness = harness();
        let root = harness.root_widget().id();
        harness.focus_on(Some(root));
        assert_eq!(press_arrow(&mut harness, NamedKey::ArrowRight), None);
        assert_eq!(press_arrow(&mut harness, NamedKey::ArrowLeft), None);
    }

    /// Builds a harness whose root is a `RowClickable` ("outer") wrapping a
    /// second, nested `RowClickable` ("inner") as its content. The nesting is
    /// just a cheap way to get a second real, focusable `WidgetId` inside a
    /// single harness to use as a nav target — in production the target is
    /// always a sibling row (pushed by `CollectionBodyWidget::refresh_row_nav`),
    /// but `RowClickable`'s own `on_text_event` doesn't know or care where the
    /// id came from, so this is a faithful unit test of its local behavior.
    /// Returns `(harness, outer_id, inner_id)` with `outer` already focused.
    fn harness_with_nested_row() -> (TestHarness<RowClickable>, WidgetId, WidgetId) {
        let inner = NewWidget::new(RowClickable::new(
            NewWidget::new(Label::new("inner")),
            false,
            &Theme::default(),
            None,
            false,
            None,
        ));
        let inner_id = inner.id();
        let outer = RowClickable::new(inner, false, &Theme::default(), None, false, None);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), NewWidget::new(outer), (120, 48));
        let outer_id = harness.root_widget().id();
        harness.focus_on(Some(outer_id));
        (harness, outer_id, inner_id)
    }

    /// With a nav target set, `ArrowDown` moves focus to it and is handled.
    #[test]
    fn arrow_down_moves_focus_to_the_nav_target() {
        let (mut harness, _outer_id, inner_id) = harness_with_nested_row();
        harness.edit_root_widget(|mut row| RowClickable::set_nav(&mut row, None, Some(inner_id)));
        let handled = harness.process_text_event(key_down(
            Key::Named(NamedKey::ArrowDown),
            Modifiers::empty(),
        ));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(inner_id));
    }

    /// Same for `ArrowUp` / `nav_up`.
    #[test]
    fn arrow_up_moves_focus_to_the_nav_target() {
        let (mut harness, _outer_id, inner_id) = harness_with_nested_row();
        harness.edit_root_widget(|mut row| RowClickable::set_nav(&mut row, Some(inner_id), None));
        let handled =
            harness.process_text_event(key_down(Key::Named(NamedKey::ArrowUp), Modifiers::empty()));
        assert!(handled.is_handled());
        assert_eq!(harness.focused_widget_id(), Some(inner_id));
    }

    /// With no nav target in that direction (a materialized/data edge), the key
    /// is still handled — never left to fall through to `VirtualScroll`'s
    /// built-in scroll-by-arrow-key — but focus doesn't move.
    #[test]
    fn arrow_down_without_a_target_is_a_handled_no_op() {
        let (mut harness, outer_id, _inner_id) = harness_with_nested_row();
        // nav_down defaults to None; no set_nav call.
        let handled = harness.process_text_event(key_down(
            Key::Named(NamedKey::ArrowDown),
            Modifiers::empty(),
        ));
        assert!(
            handled.is_handled(),
            "must be handled so it never falls through to VirtualScroll's native scroll"
        );
        assert_eq!(harness.focused_widget_id(), Some(outer_id));
    }

    /// Presses the primary button mid-row over a full-width interactive child
    /// that *captures* the pointer on Down — a cell button ("Open"), the
    /// disclosure chevron — and returns `(child_downs, row_selected)`. Builds
    /// `RowClickable` with `(leading_hit: None, propagates_pointer: true)` —
    /// exactly what `body_view::collection_body` derives for *every*
    /// collection (plain grid, plain list, or expandable), not a state
    /// production wiring can't reach.
    fn press_with_capturing_child() -> (usize, bool) {
        let child_downs = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&child_downs);
        let child = ModularWidget::new(())
            .accepts_pointer_interaction(true)
            .measure_fn(|(), _ctx, _props, axis, _len_req, _cross| match axis {
                Axis::Horizontal => Length::px(120.0),
                Axis::Vertical => Length::px(24.0),
            })
            .pointer_event_fn(move |(), ctx, _props, event| {
                // Claim the press exactly as a real cell button does.
                if matches!(event, PointerEvent::Down(_)) {
                    counter.fetch_add(1, Ordering::Relaxed);
                    ctx.capture_pointer();
                }
            });
        let widget = RowClickable::new(
            NewWidget::new(child),
            false,
            &Theme::default(),
            // No leading hit zone: deference here is driven purely by the
            // child's capture, not by any per-control geometry.
            None,
            true,
            None,
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

    /// The new capture-deference guard: an interactive cell child that
    /// *captures* the pointer on Down owns the press. Pointer events bubble
    /// child->parent and `capture_pointer` is last-writer-wins, so without the
    /// guard the row would overwrite the child's capture on the way up and
    /// swallow the click. The child must still receive the press (it passes
    /// through), and the row must neither steal it nor select. Regression for
    /// "the Open button on a grid row does nothing". Contrast with
    /// `nested_interactive_child_receives_the_press_when_propagating`, where a
    /// *non-capturing* child lets the press bubble on and the row selects.
    #[test]
    fn capturing_child_takes_the_click_and_the_row_defers() {
        let (child_downs, selected) = press_with_capturing_child();
        assert_eq!(
            child_downs, 1,
            "the press must reach the capturing child (it owns the click)",
        );
        assert!(
            !selected,
            "the row must defer to the capturing child — no click stolen, no select",
        );
    }
}
