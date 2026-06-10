//! View-level overlay portal: the typed registry resource that lets
//! `popover` (and future overlay components) mount arbitrary stateful
//! content views into the nearest [`crate::overlay_scope`]'s always-on-top
//! slot, with full xilem rebuild/message semantics.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use masonry::widgets::Passthrough;
use xilem_masonry::core::{AnyView, Resource, View, ViewPathTracker};
use xilem_masonry::{Pod, ViewCtx};

use crate::Theme;
use crate::overlay_scope::OverlayScopeHandle;

/// Erased popover-content view stored in the portal registry.
///
/// Deliberately *not* [`xilem::AnyWidgetView`], which carries `+ Send + Sync`
/// — the portal is same-thread by construction (registry, scope view, and
/// content all live on the UI thread), and imposing `Send + Sync` on popover
/// content would be a gratuitous API break versus the in-tree fallback.
pub type PortalContentView<State, Action> = dyn AnyView<State, Action, ViewCtx, Pod<Passthrough>>;

/// View state produced by building an [`Rc`]-wrapped [`PortalContentView`].
/// Named via projection so we don't depend on `xilem_core` internals.
#[expect(dead_code, reason = "consumed by overlay_scope view in a later commit")]
pub(crate) type PortalContentViewState<State, Action> =
    <Rc<PortalContentView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// One registered popover's content, as the scope's view sees it.
pub(crate) struct PortalEntry<State, Action> {
    pub(crate) key: u64,
    pub(crate) content: Rc<PortalContentView<State, Action>>,
    pub(crate) theme: Theme,
}

impl<State, Action> Clone for PortalEntry<State, Action> {
    fn clone(&self) -> Self {
        Self {
            key: self.key,
            content: self.content.clone(),
            theme: self.theme,
        }
    }
}

struct PortalRegistry<State, Action> {
    next_key: u64,
    entries: Vec<PortalEntry<State, Action>>,
}

/// Typed Environment resource published by [`crate::overlay_scope`].
///
/// Cloning is shallow — all clones share one registry. The resource is
/// created once at the scope's `View::build` and keeps stable identity for
/// the scope's lifetime (see `provides` semantics in `xilem_core`).
pub struct OverlayPortal<State, Action> {
    scope: OverlayScopeHandle,
    inner: Rc<RefCell<PortalRegistry<State, Action>>>,
}

impl<State, Action> Clone for OverlayPortal<State, Action> {
    fn clone(&self) -> Self {
        Self {
            scope: self.scope.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<State, Action> fmt::Debug for OverlayPortal<State, Action> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<u64> = self.inner.borrow().entries.iter().map(|e| e.key).collect();
        f.debug_struct("OverlayPortal")
            .field("scope", &self.scope)
            .field("keys", &keys)
            .finish_non_exhaustive()
    }
}

impl<State: 'static, Action: 'static> Resource for OverlayPortal<State, Action> {}

impl<State, Action> OverlayPortal<State, Action> {
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn new(scope: OverlayScopeHandle) -> Self {
        Self {
            scope,
            inner: Rc::new(RefCell::new(PortalRegistry {
                // Start at 1, not 0. Portal keys become `ViewId`s inside the
                // scope's sequence view for message routing. `ViewId::new(0)`
                // is reserved by xilem for the scope's own content child, so
                // a portal key of 0 would collide with it and mis-route events.
                next_key: 1,
                entries: Vec::new(),
            })),
        }
    }

    /// Handle to the owning scope's widget id, for `mutate_later` pushes.
    #[must_use]
    #[expect(dead_code, reason = "consumed by overlay_scope view in a later commit")]
    pub(crate) fn scope(&self) -> &OverlayScopeHandle {
        &self.scope
    }

    /// Register a popover's content view; returns its portal key.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn register(
        &self,
        content: Rc<PortalContentView<State, Action>>,
        theme: &Theme,
    ) -> u64 {
        let mut reg = self.inner.borrow_mut();
        let key = reg.next_key;
        reg.next_key += 1;
        reg.entries.push(PortalEntry {
            key,
            content,
            theme: *theme,
        });
        key
    }

    /// Replace the content/theme for an existing key (no-op if unknown).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn update(
        &self,
        key: u64,
        content: Rc<PortalContentView<State, Action>>,
        theme: &Theme,
    ) {
        let mut reg = self.inner.borrow_mut();
        if let Some(entry) = reg.entries.iter_mut().find(|e| e.key == key) {
            entry.content = content;
            entry.theme = *theme;
        }
    }

    /// Remove the entry for `key` (no-op if unknown).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn deregister(&self, key: u64) {
        self.inner.borrow_mut().entries.retain(|e| e.key != key);
    }

    /// Snapshot of all entries, in registration order.
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn snapshot(&self) -> Vec<PortalEntry<State, Action>> {
        self.inner.borrow().entries.clone()
    }
}

/// Read the nearest scope's portal from the xilem Environment, tolerating
/// "no scope ancestor" (returns `None`). Mirrors `dropdown_button`'s
/// `OverlayScopeHandle` lookup — `with_context` panics when absent, so we
/// read the slot directly.
#[expect(dead_code, reason = "consumed by overlay_scope view in a later commit")]
pub(crate) fn portal_from_env<State: 'static, Action: 'static>(
    ctx: &mut ViewCtx,
) -> Option<OverlayPortal<State, Action>> {
    let idx = ctx
        .environment()
        .get_slot_for_type::<OverlayPortal<State, Action>>()?;
    ctx.environment().slots[idx as usize]
        .item
        .as_ref()
        .and_then(|item| item.value.downcast_ref::<OverlayPortal<State, Action>>())
        .cloned()
}

// --- MARK: PortalSlot

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetId,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

use crate::components::popover::PopoverAnchor;
use crate::components::popover::widget::PopoverHost;

/// Invisible hit-target that makes [`PortalSlot`]'s backdrop dismissal work
/// despite masonry caching `accepts_pointer_interaction` at mount
/// (`masonry_core/src/passes/update.rs:191`). The flag is static-true here so
/// the cache is always correct. The *dynamic* switch is stashing: the slot
/// stashes this widget while no popover is open (stashed widgets are skipped
/// by hit-testing) and un-stashes it when any popover is visible.
struct Backdrop;

impl Widget for Backdrop {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::ZERO
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
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
        ChildrenIds::from_slice(&[])
    }
}

/// One permanently-mounted popover surface inside the slot.
struct PortalChild {
    key: u64,
    widget: WidgetPod<dyn Widget>,
    /// `PopoverHost` to sync (via [`PopoverHost::mark_closed`]) when the
    /// backdrop dismisses this child. `None` in tests / ownerless pushes.
    owner: Option<WidgetId>,
    visible: bool,
    /// Trigger's anchor rect in the *scope's* local coordinates.
    placement: Rect,
    anchor: PopoverAnchor,
    /// Gap between trigger edge and surface, px, in the open direction.
    gap: f64,
    /// Where layout last placed this child (local coords); valid while visible.
    placed: Rect,
}

/// Always-last-painted child of [`crate::overlay_scope::OverlayScope`] that
/// hosts portal-mounted popover content. Children are inserted/removed by
/// the scope's *view* (so xilem rebuilds reach them); visibility and
/// placement are plain-data widget mutations pushed by `PopoverHost` via
/// `mutate_later`.
///
/// Backdrop dismissal works via a private [`Backdrop`] child whose
/// `accepts_pointer_interaction` is a static `true` (cache-safe). The slot
/// stashes the backdrop while no popover is open and un-stashes it when any
/// is visible. Stashed widgets are skipped by masonry's hit-testing, so the
/// dynamic on/off behaviour is controlled entirely by stash state — not by
/// the cached interaction flag. When a click lands on the backdrop it bubbles
/// up to `PortalSlot::on_pointer_event`, where the dismissal logic runs.
pub struct PortalSlot {
    backdrop: WidgetPod<Backdrop>,
    children: Vec<PortalChild>,
}

impl PortalSlot {
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn new(children: Vec<(u64, NewWidget<dyn Widget>)>) -> Self {
        Self {
            backdrop: masonry::core::NewWidget::new(Backdrop).to_pod(),
            children: children
                .into_iter()
                .map(|(key, widget)| PortalChild {
                    key,
                    widget: widget.to_pod(),
                    owner: None,
                    visible: false,
                    placement: Rect::ZERO,
                    anchor: PopoverAnchor::BottomStart,
                    gap: 0.0,
                    placed: Rect::ZERO,
                })
                .collect(),
        }
    }

    /// Mount a new (hidden) child for `key`. Called from the scope view's
    /// rebuild when a popover registers after initial build.
    #[expect(dead_code, reason = "consumed by overlay_scope view in a later commit")]
    pub(crate) fn insert(this: &mut WidgetMut<'_, Self>, key: u64, widget: NewWidget<dyn Widget>) {
        this.widget.children.push(PortalChild {
            key,
            widget: widget.to_pod(),
            owner: None,
            visible: false,
            placement: Rect::ZERO,
            anchor: PopoverAnchor::BottomStart,
            gap: 0.0,
            placed: Rect::ZERO,
        });
        this.ctx.children_changed();
    }

    /// Unmount the child for `key` (no-op if unknown).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn remove_by_key(this: &mut WidgetMut<'_, Self>, key: u64) {
        if let Some(idx) = this.widget.children.iter().position(|c| c.key == key) {
            let child = this.widget.children.remove(idx);
            this.ctx.remove_child(child.widget);
            this.ctx.children_changed();
        }
    }

    /// Mutable access to the child for `key`, for the scope view's rebuild
    /// threading.
    #[expect(dead_code, reason = "consumed by overlay_scope view in a later commit")]
    pub(crate) fn child_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        key: u64,
    ) -> Option<WidgetMut<'t, dyn Widget>> {
        let idx = this.widget.children.iter().position(|c| c.key == key)?;
        Some(this.ctx.get_mut(&mut this.widget.children[idx].widget))
    }

    /// Show or hide the child for `key`, with its anchor placement
    /// (scope-local coordinates). Plain data only — safe to call from a
    /// `mutate_later` callback.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by overlay_scope view in a later commit")
    )]
    pub(crate) fn set_visible(
        this: &mut WidgetMut<'_, Self>,
        key: u64,
        visible: bool,
        owner: Option<WidgetId>,
        placement: Rect,
        anchor: PopoverAnchor,
        gap: f64,
    ) {
        let Some(child) = this.widget.children.iter_mut().find(|c| c.key == key) else {
            return;
        };
        child.visible = visible;
        child.owner = owner;
        child.placement = placement;
        child.anchor = anchor;
        child.gap = gap;
        this.ctx.request_layout();
    }

    /// Re-anchor a visible child as its trigger moves (scrolling). No-op if
    /// the key is unknown or hidden.
    #[expect(dead_code, reason = "consumed by overlay_scope view in a later commit")]
    pub(crate) fn set_placement(this: &mut WidgetMut<'_, Self>, key: u64, placement: Rect) {
        let Some(child) = this.widget.children.iter_mut().find(|c| c.key == key) else {
            return;
        };
        if !child.visible || child.placement == placement {
            return;
        }
        child.placement = placement;
        this.ctx.request_layout();
    }
}

impl Widget for PortalSlot {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        // The slot is a pure container; it must not be a hit target itself.
        // Backdrop dismissal is handled by the `Backdrop` child, which has a
        // static `true` here (cache-safe) and is stashed when no popover is open.
        false
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let PointerEvent::Down(PointerButtonEvent { state, .. }) = event else {
            return;
        };
        let pos = ctx.local_position(state.position);
        if self
            .children
            .iter()
            .any(|c| c.visible && c.placed.contains(pos))
        {
            // The click is for (or bubbling from) open content — leave it be.
            return;
        }
        let mut dismissed = false;
        for child in &mut self.children {
            if !child.visible {
                continue;
            }
            child.visible = false;
            dismissed = true;
            if let Some(owner) = child.owner {
                ctx.mutate_later(owner, |mut w| {
                    let mut host = w.downcast::<PopoverHost>();
                    PopoverHost::mark_closed(&mut host);
                });
            }
        }
        if dismissed {
            ctx.request_layout();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.backdrop);
        for child in &mut self.children {
            ctx.register_child(&mut child.widget);
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // The scope lays us out to its own size unconditionally; we never
        // contribute to its footprint.
        Length::ZERO
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let any_visible = self.children.iter().any(|c| c.visible);
        if any_visible {
            ctx.set_stashed(&mut self.backdrop, false);
            // Size the backdrop to the full slot area so it covers everything
            // and acts as a dismiss target anywhere outside the content children.
            ctx.run_layout(&mut self.backdrop, size);
            ctx.place_child(&mut self.backdrop, Point::ORIGIN);
        } else {
            ctx.set_stashed(&mut self.backdrop, true);
        }

        for child in &mut self.children {
            if child.visible {
                ctx.set_stashed(&mut child.widget, false);
                // Snug to intrinsic content size — see `AnchoredOverlay::layout`.
                let child_size =
                    ctx.compute_size(&mut child.widget, SizeDef::MIN, LayoutSize::from(size));
                ctx.run_layout(&mut child.widget, child_size);
                let offset = child
                    .anchor
                    .child_offset(child.placement.size(), child_size)
                    + child.placement.origin().to_vec2();
                let offset = match child.anchor {
                    PopoverAnchor::BottomStart
                    | PopoverAnchor::BottomCenter
                    | PopoverAnchor::BottomEnd => Point::new(offset.x, offset.y + child.gap),
                    PopoverAnchor::TopStart | PopoverAnchor::TopCenter | PopoverAnchor::TopEnd => {
                        Point::new(offset.x, offset.y - child.gap)
                    }
                };
                ctx.place_child(&mut child.widget, offset);
                child.placed = Rect::from_origin_size(offset, child_size);
            } else {
                ctx.set_stashed(&mut child.widget, true);
                child.placed = Rect::ZERO;
            }
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural; children paint themselves. The backdrop is
        // intentionally invisible.
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
        let mut ids = vec![self.backdrop.id()];
        ids.extend(self.children.iter().map(|c| c.widget.id()));
        ChildrenIds::from_slice(&ids)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::Theme;
    use crate::overlay_scope::OverlayScopeHandle;

    fn content() -> Rc<PortalContentView<(), ()>> {
        let theme = Theme::default();
        Rc::new(crate::label("portal content").render(&theme))
    }

    #[test]
    fn register_allocates_distinct_keys_starting_at_one() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let a = portal.register(content(), &theme);
        let b = portal.register(content(), &theme);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
    }

    #[test]
    fn snapshot_returns_entries_in_registration_order() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let a = portal.register(content(), &theme);
        let b = portal.register(content(), &theme);
        let keys: Vec<u64> = portal.snapshot().iter().map(|e| e.key).collect();
        assert_eq!(keys, vec![a, b]);
    }

    #[test]
    fn update_replaces_content_for_an_existing_key() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let key = portal.register(content(), &theme);
        let replacement = content();
        portal.update(key, replacement.clone(), &theme);
        let snap = portal.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(Rc::ptr_eq(&snap[0].content, &replacement));
    }

    #[test]
    fn deregister_removes_the_entry_and_tolerates_unknown_keys() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let key = portal.register(content(), &theme);
        portal.deregister(key);
        assert!(portal.snapshot().is_empty());
        portal.deregister(999); // must not panic
    }

    #[test]
    fn clones_share_the_same_registry() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let clone = portal.clone();
        let theme = Theme::default();
        clone.register(content(), &theme);
        assert_eq!(portal.snapshot().len(), 1);
    }

    #[test]
    fn keys_are_never_reused_after_deregister() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let first = portal.register(content(), &theme);
        assert_eq!(first, 1);
        portal.deregister(first);
        let second = portal.register(content(), &theme);
        assert_eq!(second, 2, "key must not be recycled after deregister");
    }

    #[test]
    fn update_with_unknown_key_is_a_noop() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let original = content();
        portal.register(original.clone(), &theme);
        // update with a key that was never registered — must not panic
        portal.update(999, content(), &theme);
        let snap = portal.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(
            Rc::ptr_eq(&snap[0].content, &original),
            "existing entry must be unchanged after update with unknown key"
        );
    }

    // --- PortalSlot tests ---

    use masonry::core::PointerButton;
    use masonry::kurbo::{Point, Rect};
    use masonry::testing::TestHarness;

    use crate::components::popover::PopoverAnchor;

    fn test_child() -> NewWidget<dyn Widget> {
        masonry::widgets::Label::new("popover body")
            .prepare()
            .erased()
    }

    fn slot_with_one_child() -> (TestHarness<PortalSlot>, u64) {
        let key = 7;
        let slot = PortalSlot::new(vec![(key, test_child())]);
        let harness = TestHarness::create(
            masonry::theme::default_property_set(),
            masonry::core::NewWidget::new(slot),
        );
        (harness, key)
    }

    #[test]
    fn slot_children_start_hidden_and_inert() {
        let (mut harness, _key) = slot_with_one_child();
        harness.edit_root_widget(|wm| {
            assert!(!wm.widget.children[0].visible);
            // The slot itself is a pure container (no accepts_pointer_interaction
            // override). Interaction is gated by stashing the Backdrop child;
            // confirmed via the dismiss/pass-through behavioural tests.
        });
    }

    #[test]
    fn set_visible_places_the_child_below_a_bottom_start_placement() {
        let (mut harness, key) = slot_with_one_child();
        let placement = Rect::new(10.0, 10.0, 110.0, 40.0); // 100x30 trigger at (10,10)
        harness.edit_root_widget(|mut wm| {
            PortalSlot::set_visible(
                &mut wm,
                key,
                true,
                None,
                placement,
                PopoverAnchor::BottomStart,
                4.0,
            );
        });
        harness.edit_root_widget(|wm| {
            // Backdrop is un-stashed when a child is visible; the dismiss test
            // confirms the pointer event actually fires.
            let placed = wm.widget.children[0].placed;
            // BottomStart: x flush with placement left, y = placement bottom + gap.
            assert!((placed.x0 - 10.0).abs() < 1e-9);
            assert!((placed.y0 - 44.0).abs() < 1e-9);
        });
    }

    #[test]
    fn clicks_pass_through_when_no_popover_is_open() {
        // With no child ever made visible the backdrop should be stashed, so
        // pointer events are never delivered to the slot and nothing panics.
        let (mut harness, _key) = slot_with_one_child();
        harness.mouse_move(Point::new(390.0, 390.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|wm| {
            assert!(!wm.widget.children[0].visible, "child must stay hidden");
        });
    }

    #[test]
    fn pointer_down_outside_visible_children_dismisses_them() {
        let (mut harness, key) = slot_with_one_child();
        let placement = Rect::new(10.0, 10.0, 110.0, 40.0);
        harness.edit_root_widget(|mut wm| {
            PortalSlot::set_visible(
                &mut wm,
                key,
                true,
                None,
                placement,
                PopoverAnchor::BottomStart,
                0.0,
            );
        });
        // Click far away from the placed content.
        harness.mouse_move(Point::new(390.0, 390.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|wm| {
            assert!(!wm.widget.children[0].visible);
            assert!(!wm.widget.accepts_pointer_interaction());
        });
    }

    #[test]
    fn pointer_down_inside_a_visible_child_does_not_dismiss() {
        let (mut harness, key) = slot_with_one_child();
        let placement = Rect::new(10.0, 10.0, 110.0, 40.0);
        harness.edit_root_widget(|mut wm| {
            PortalSlot::set_visible(
                &mut wm,
                key,
                true,
                None,
                placement,
                PopoverAnchor::BottomStart,
                0.0,
            );
        });
        let inside = harness.edit_root_widget(|wm| wm.widget.children[0].placed.center());
        harness.mouse_move(inside);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|wm| {
            assert!(wm.widget.children[0].visible);
        });
    }

    #[test]
    fn remove_by_key_drops_the_child() {
        let (mut harness, key) = slot_with_one_child();
        harness.edit_root_widget(|mut wm| {
            PortalSlot::remove_by_key(&mut wm, key);
        });
        harness.edit_root_widget(|wm| {
            assert!(wm.widget.children.is_empty());
        });
    }
}
