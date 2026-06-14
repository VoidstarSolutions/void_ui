//! View-level overlay portal: the typed registry resource that lets
//! `popover` (and future overlay components) mount arbitrary stateful
//! content views into the nearest [`crate::overlay_scope`]'s always-on-top
//! slot, with full xilem rebuild/message semantics.
//!
//! # The full flow
//!
//! 1. **Publish.** [`crate::overlay_scope::overlay_scope`] constructs an
//!    [`OverlayPortal<State, Action>`] inside its `provides` closure and
//!    publishes it into the xilem `Environment`. `provides` build-once
//!    semantics give the registry stable identity for the scope's lifetime.
//! 2. **Register.** `popover()`'s view ([`crate::components::popover`]) finds
//!    the portal via [`portal_from_env`] at `View::build`, registers its
//!    `Arc`-erased content view ([`PortalContentView`]) and gets back a key;
//!    on rebuild it refreshes the entry, on teardown it deregisters.
//! 3. **Mount.** The scope's own view (`OverlayScopeRootView` in
//!    `overlay_scope.rs`) diffs the registry on every build/rebuild and
//!    mounts each entry — wrapped in `PopoverSurface` chrome — as a real view
//!    child of the scope inside [`PortalSlot`]. The diff iterates to a
//!    fixpoint because building/rebuilding/tearing-down an entry can itself
//!    register or deregister *nested* popovers mid-diff. Content is a genuine
//!    view child (element path `…scope… / ViewId(key)`), so rebuilds, theme
//!    swaps, and button callbacks inside popover content all work.
//! 4. **Show/hide/place.** Open state never flows through the registry:
//!    `PopoverHost` pushes visibility and anchor placement to the slot as
//!    *plain data* via `ctx.mutate_later(scope_id, …)`
//!    ([`crate::overlay_scope::OverlayScope::set_portal_visible`] /
//!    `set_portal_placement`), re-anchoring from `compose` while the trigger
//!    scrolls.
//! 5. **Dismiss.** Light dismiss with pass-through, no backdrop. The scope
//!    is an ancestor of everything inside it, so every pointer-down inside
//!    the scope bubbles through `OverlayScope::on_pointer_event`, which asks
//!    the slot (`PortalSlot::dismiss_outside`) to hide all visible children
//!    unless the press landed inside one's content or its trigger rect. The
//!    press is not consumed — it also acts on whatever was under it, and
//!    because nothing occludes the scope while a popover is open, scroll and
//!    hover keep working underneath (the open popover re-anchors via
//!    `PopoverHost::compose`). Owners are notified via `mutate_later` →
//!    `PopoverHost::mark_closed` (safely skipped by masonry if the owner was
//!    removed in the interim).
//!
//! # Known v1 limitations (intentional)
//!
//! - **Outside-scope clicks don't dismiss.** Dismissal observes pointer-downs
//!   bubbling through the scope; clicks beyond its bounds never pass through
//!   it. Escape on the focused trigger and clicks anywhere inside the scope
//!   do dismiss.
//! - **A descendant consuming pointer-downs blocks dismissal.** A widget that
//!   `set_handled`s the *down* half of a press stops it bubbling to the
//!   scope, leaving the popover open for that press. No `void_ui` or common
//!   masonry widget does (they consume ups and scrolls).
//! - **A11y placement.** Portal content appears in the accessibility tree
//!   under the scope's slot, not under its trigger.
//! - **Environment context loss.** Portal content builds/rebuilds as a view
//!   child of the *scope*, so a `provides` published *between* the scope and
//!   the popover call site is not visible inside portal-mounted content
//!   (`with_context` there panics or binds to a value above the scope).
//!   React portals preserve context; this design structurally cannot.
//!
//! Masonry's window-level `Layer` system (`LayerStack`, `create_layer`,
//! `Layer::capture_pointer_event`) solves the first three at the platform
//! level (layers see every pointer event before hit-testing) but has no
//! xilem integration yet; this portal is the userspace stand-in until it
//! does. See `docs/notes/2026-06-10-xilem-overlay-learnings.md`.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use masonry::widgets::Passthrough;
use xilem_masonry::core::{AnyView, Resource, View, ViewPathTracker};
use xilem_masonry::{Pod, ViewCtx};

use crate::Theme;
use crate::components::popover::widget::SurfaceStyle;
use crate::overlay_scope::OverlayScopeHandle;

/// Erased popover-content view stored in the portal registry. Equivalent to
/// [`xilem_masonry::AnyWidgetView`].
///
/// The `+ Send + Sync` bounds are about view *values*, not threading of the
/// portal itself: `WidgetView` declares `Send + Sync` as a supertrait on view
/// values, and the upcoming `PopoverView` carries the erased content as a
/// struct field while itself implementing `WidgetView` — so the erased type
/// must be `Send + Sync` for `PopoverView` to satisfy its own supertrait.
/// This costs users nothing: every concrete `WidgetView` already satisfies
/// the bounds by supertrait.
///
/// The *registry*, by contrast ([`OverlayPortal`]'s `Rc<RefCell<…>>`), stays
/// deliberately non-`Send`: Environment resources and `ViewState` carry no
/// such bounds, and the registry lives its whole life on the UI thread. That
/// split — `Send + Sync` view values, single-threaded resources — is exactly
/// why [`crate::overlay_scope::overlay_scope`] constructs the portal *inside*
/// its `provides` closure rather than capturing one.
pub type PortalContentView<State, Action> =
    dyn AnyView<State, Action, ViewCtx, Pod<Passthrough>> + Send + Sync;

/// View state produced by building an [`Arc`]-wrapped [`PortalContentView`].
/// Named via projection so we don't depend on `xilem_core` internals.
pub(crate) type PortalContentViewState<State, Action> =
    <Arc<PortalContentView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// Where a portal entry's content is positioned within the slot.
///
/// [`crate::components::popover`] registers [`Self::Trigger`] entries: hidden
/// until [`PortalSlot::set_visible`] shows them anchored to a trigger rect
/// (in scope-local coordinates) via [`PopoverAnchor`], wrapped in
/// [`PopoverSurface`] chrome to match in-tree popovers.
///
/// [`crate::components::notification`] registers [`Self::Corner`] entries:
/// always visible, aligned to one of the scope's corners/edges via
/// [`masonry::layout::UnitPoint`] and sized to their own intrinsic content
/// (`SizeDef::MIN`, exactly like [`Self::Trigger`]'s sizing — see
/// [`PortalSlot::layout`]), with no added chrome since toast cards already
/// carry their own surface.
///
/// [`crate::components::dropdown_button`] registers [`Self::BareTrigger`]
/// entries: anchored and shown/hidden exactly like [`Self::Trigger`], but
/// mounted without [`crate::components::popover::widget::PopoverSurface`]
/// chrome — `MenuContent` already paints its own background/border, and
/// wrapping it again would double up padding and chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PortalPlacement {
    Trigger,
    BareTrigger,
    Corner(masonry::layout::UnitPoint),
}

/// One registered popover's content, as the scope's view sees it.
pub(crate) struct PortalEntry<State, Action> {
    pub(crate) key: u64,
    pub(crate) content: Arc<PortalContentView<State, Action>>,
    pub(crate) theme: Theme,
    pub(crate) placement: PortalPlacement,
    pub(crate) style: SurfaceStyle,
}

impl<State, Action> Clone for PortalEntry<State, Action> {
    fn clone(&self) -> Self {
        Self {
            key: self.key,
            content: self.content.clone(),
            theme: self.theme,
            placement: self.placement,
            style: self.style,
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
    pub(crate) fn scope(&self) -> &OverlayScopeHandle {
        &self.scope
    }

    /// Register a popover's content view; returns its portal key.
    pub(crate) fn register(
        &self,
        content: Arc<PortalContentView<State, Action>>,
        theme: &Theme,
        placement: PortalPlacement,
        style: SurfaceStyle,
    ) -> u64 {
        let mut reg = self.inner.borrow_mut();
        let key = reg.next_key;
        reg.next_key += 1;
        reg.entries.push(PortalEntry {
            key,
            content,
            theme: *theme,
            placement,
            style,
        });
        key
    }

    /// Replace the content/theme/placement for an existing key (no-op if unknown).
    pub(crate) fn update(
        &self,
        key: u64,
        content: Arc<PortalContentView<State, Action>>,
        theme: &Theme,
        placement: PortalPlacement,
        style: SurfaceStyle,
    ) {
        let mut reg = self.inner.borrow_mut();
        if let Some(entry) = reg.entries.iter_mut().find(|e| e.key == key) {
            entry.content = content;
            entry.theme = *theme;
            entry.placement = placement;
            entry.style = style;
        }
    }

    /// Remove the entry for `key` (no-op if unknown).
    pub(crate) fn deregister(&self, key: u64) {
        self.inner.borrow_mut().entries.retain(|e| e.key != key);
    }

    /// Snapshot of all entries, in registration order.
    #[must_use]
    pub(crate) fn snapshot(&self) -> Vec<PortalEntry<State, Action>> {
        self.inner.borrow().entries.clone()
    }

    /// The current entry for `key`, if registered. Unlike [`Self::snapshot`]
    /// this reads the live registry — used by the scope's diff to pick up
    /// updates that landed mid-pass (an owner rebuilding before its nested
    /// entry is processed).
    #[must_use]
    pub(crate) fn entry(&self, key: u64) -> Option<PortalEntry<State, Action>> {
        self.inner
            .borrow()
            .entries
            .iter()
            .find(|e| e.key == key)
            .cloned()
    }
}

/// Read the nearest scope's portal from the xilem Environment, tolerating
/// "no scope ancestor" (returns `None`). Mirrors `dropdown_button`'s
/// `OverlayScopeHandle` lookup — `with_context` panics when absent, so we
/// read the slot directly.
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
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef, UnitPoint};

use crate::components::dropdown_button::widget::ThemedDropdownButton;
use crate::components::popover::PopoverAnchor;
use crate::components::popover::widget::PopoverHost;

/// What kind of widget owns a [`PortalChild`], and therefore how
/// [`PortalSlot::dismiss_outside`] notifies it of an outside-press dismissal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnerKind {
    /// A `PopoverHost`, notified via [`PopoverHost::mark_closed`].
    Popover,
    /// A `DialogHost`, notified by submitting `DialogDismissed`.
    Dialog,
    /// A `ThemedDropdownButton`, notified via
    /// [`crate::components::dropdown_button::widget::ThemedDropdownButton::mark_closed`].
    DropdownButton,
}

/// Visibility placement for a portal child: who owns it (for outside-press
/// notification), where it's anchored, and how far to offset it. Grouped
/// into one struct so [`PortalSlot::set_visible`] /
/// [`crate::overlay_scope::OverlayScope::set_portal_visible`] stay under
/// clippy's `too_many_arguments`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalPlacement {
    /// Owner to notify when an outside press dismisses this child. `None` in
    /// tests / ownerless pushes.
    pub owner: Option<WidgetId>,
    pub owner_kind: OwnerKind,
    /// Trigger's anchor rect. In window coordinates for
    /// [`crate::overlay_scope::OverlayScope::set_portal_visible`], converted
    /// to the scope's local coordinates before reaching
    /// [`PortalSlot::set_visible`]. Ignored for
    /// [`PopoverAnchor::ViewportQuarter`] — pass [`Rect::ZERO`].
    pub rect: Rect,
    pub anchor: PopoverAnchor,
    pub gap: f64,
}

/// One permanently-mounted popover surface inside the slot.
struct PortalChild {
    key: u64,
    widget: WidgetPod<dyn Widget>,
    /// Owner to notify (via `owner_kind`) when an outside press dismisses
    /// this child. `None` in tests / ownerless pushes, and unused for
    /// [`PortalPlacement::Corner`] children.
    owner: Option<WidgetId>,
    owner_kind: OwnerKind,
    /// [`PortalPlacement::Trigger`] children start hidden and are shown via
    /// [`Self::set_visible`]; [`PortalPlacement::Corner`] children are always
    /// visible.
    visible: bool,
    /// Trigger's anchor rect in the *scope's* local coordinates. Unused for
    /// [`PortalPlacement::Corner`] children.
    placement: Rect,
    anchor: PopoverAnchor,
    /// Gap between trigger edge and surface, px, in the open direction.
    /// Unused for [`PortalPlacement::Corner`] children.
    gap: f64,
    /// Where layout last placed this child (local coords); valid while visible.
    placed: Rect,
    mode: PortalPlacement,
}

impl PortalChild {
    fn new(key: u64, widget: WidgetPod<dyn Widget>, mode: PortalPlacement) -> Self {
        Self {
            key,
            widget,
            owner: None,
            owner_kind: OwnerKind::Popover,
            visible: matches!(mode, PortalPlacement::Corner(_)),
            placement: Rect::ZERO,
            anchor: PopoverAnchor::BottomStart,
            gap: 0.0,
            placed: Rect::ZERO,
            mode,
        }
    }
}

/// Always-last-painted child of [`crate::overlay_scope::OverlayScope`] that
/// hosts portal-mounted popover content. Children are inserted/removed by
/// the scope's *view* (so xilem rebuilds reach them); visibility and
/// placement are plain-data widget mutations pushed by `PopoverHost` via
/// `mutate_later`.
///
/// The slot never occludes anything: only the visible popover children are
/// hit targets, so scroll, hover, and clicks everywhere else reach the
/// content beneath as if no popover were open. Outside-press dismissal is
/// driven by `OverlayScope` — an ancestor of *everything* in the scope, so
/// every pointer-down inside the scope bubbles through it — which calls
/// [`Self::dismiss_outside`] with the press position (light dismiss with
/// pass-through: the press also acts on whatever was under it).
pub struct PortalSlot {
    children: Vec<PortalChild>,
}

impl PortalSlot {
    #[must_use]
    pub(crate) fn new(children: Vec<(u64, NewWidget<dyn Widget>, PortalPlacement)>) -> Self {
        Self {
            children: children
                .into_iter()
                .map(|(key, widget, mode)| PortalChild::new(key, widget.to_pod(), mode))
                .collect(),
        }
    }

    /// Mount a new child for `key`. Called from the scope view's rebuild when
    /// a popover, dialog, or toast layer registers after initial build.
    pub(crate) fn insert(
        this: &mut WidgetMut<'_, Self>,
        key: u64,
        widget: NewWidget<dyn Widget>,
        mode: PortalPlacement,
    ) {
        this.widget
            .children
            .push(PortalChild::new(key, widget.to_pod(), mode));
        this.ctx.children_changed();
    }

    /// Update a [`PortalPlacement::Corner`] child's corner/edge alignment.
    /// No-op if the key is unknown or unchanged.
    pub(crate) fn set_corner(this: &mut WidgetMut<'_, Self>, key: u64, unit_point: UnitPoint) {
        let Some(child) = this.widget.children.iter_mut().find(|c| c.key == key) else {
            return;
        };
        if child.mode == PortalPlacement::Corner(unit_point) {
            return;
        }
        child.mode = PortalPlacement::Corner(unit_point);
        this.ctx.request_layout();
    }

    /// Unmount the child for `key` (no-op if unknown).
    pub(crate) fn remove_by_key(this: &mut WidgetMut<'_, Self>, key: u64) {
        if let Some(idx) = this.widget.children.iter().position(|c| c.key == key) {
            let child = this.widget.children.remove(idx);
            this.ctx.remove_child(child.widget);
            this.ctx.children_changed();
        }
    }

    /// Mutable access to the child for `key`, for the scope view's rebuild
    /// threading.
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
    pub(crate) fn set_visible(
        this: &mut WidgetMut<'_, Self>,
        key: u64,
        visible: bool,
        placement: PortalPlacement,
    ) {
        let Some(child) = this.widget.children.iter_mut().find(|c| c.key == key) else {
            return;
        };
        if child.visible == visible
            && child.owner == placement.owner
            && child.owner_kind == placement.owner_kind
            && child.placement == placement.rect
            && child.anchor == placement.anchor
            && (child.gap - placement.gap).abs() < f64::EPSILON
        {
            return;
        }
        child.visible = visible;
        child.owner = placement.owner;
        child.owner_kind = placement.owner_kind;
        child.placement = placement.rect;
        child.anchor = placement.anchor;
        child.gap = placement.gap;
        this.ctx.request_layout();
    }

    /// The placed rect for `key`'s child (local coords), valid while visible.
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "test-only accessor for placement assertions")
    )]
    pub(crate) fn placed_rect(&self, key: u64) -> Option<Rect> {
        self.children
            .iter()
            .find(|c| c.key == key && c.visible)
            .map(|c| c.placed)
    }

    /// Re-anchor a visible child as its trigger moves (scrolling). No-op if
    /// the key is unknown or hidden.
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

    /// Dismiss every visible child unless `pos` (slot-local — identical to
    /// scope-local, the slot sits at the scope's origin) is inside a visible
    /// child's content rect or its trigger's anchor rect. Called by
    /// `OverlayScope` for every pointer-down that bubbles up from inside the
    /// scope (light dismiss). The press is *not* consumed — it also acts on
    /// whatever was under it.
    ///
    /// Presses inside a visible child's content are the content's business;
    /// presses inside its trigger rect are the trigger's (its own click
    /// handler toggles on Up — dismissing on the Down half too would make
    /// the toggle re-open a popover the dismissal just closed). Owners of
    /// dismissed children are notified via `mutate_later` →
    /// [`PopoverHost::mark_closed`] (safely skipped by masonry if the owner
    /// was removed in the interim).
    pub(crate) fn dismiss_outside(this: &mut WidgetMut<'_, Self>, pos: Point) {
        if this
            .widget
            .children
            .iter()
            .any(|c| c.visible && (c.placed.contains(pos) || c.placement.contains(pos)))
        {
            return;
        }
        let mut dismissed = false;
        for child in &mut this.widget.children {
            if !child.visible || matches!(child.mode, PortalPlacement::Corner(_)) {
                continue;
            }
            child.visible = false;
            dismissed = true;
            if let Some(owner) = child.owner {
                match child.owner_kind {
                    OwnerKind::Popover => this.ctx.mutate_later(owner, |mut w| {
                        let mut host = w.downcast::<PopoverHost>();
                        PopoverHost::mark_closed(&mut host);
                    }),
                    OwnerKind::Dialog => this.ctx.mutate_later(owner, |mut w| {
                        w.ctx
                            .submit_action::<crate::components::dialog::widget::DialogDismissed>(
                                crate::components::dialog::widget::DialogDismissed,
                            );
                    }),
                    OwnerKind::DropdownButton => this.ctx.mutate_later(owner, |mut w| {
                        let mut dropdown = w.downcast::<ThemedDropdownButton>();
                        ThemedDropdownButton::mark_closed(&mut dropdown);
                    }),
                }
            }
        }
        if dismissed {
            this.ctx.request_layout();
        }
    }
}

impl Widget for PortalSlot {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        // The slot is a pure container laid out to cover the whole scope; it
        // must never be a hit target itself or it would swallow every pointer
        // event meant for the content beneath. Only its visible children
        // (the open popovers) participate in hit-testing.
        false
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
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
        for child in &mut self.children {
            match child.mode {
                PortalPlacement::Corner(unit_point) => {
                    ctx.set_stashed(&mut child.widget, false);
                    // Snug to intrinsic content size — see `AnchoredOverlay::layout`.
                    let child_size =
                        ctx.compute_size(&mut child.widget, SizeDef::MIN, LayoutSize::from(size));
                    ctx.run_layout(&mut child.widget, child_size);
                    let extra = Rect::new(
                        0.0,
                        0.0,
                        size.width - child_size.width,
                        size.height - child_size.height,
                    );
                    let offset = unit_point.resolve(extra);
                    ctx.place_child(&mut child.widget, offset);
                    child.placed = Rect::from_origin_size(offset, child_size);
                }
                PortalPlacement::Trigger | PortalPlacement::BareTrigger if child.visible => {
                    ctx.set_stashed(&mut child.widget, false);
                    // Snug to intrinsic content size — see `AnchoredOverlay::layout`.
                    let child_size =
                        ctx.compute_size(&mut child.widget, SizeDef::MIN, LayoutSize::from(size));
                    ctx.run_layout(&mut child.widget, child_size);
                    // `ViewportQuarter` has no trigger to anchor to — center it
                    // in the slot's own size (the scope's content box) instead
                    // of `child.placement`.
                    let container = match child.anchor {
                        PopoverAnchor::ViewportQuarter => {
                            Rect::from_origin_size(Point::ORIGIN, size)
                        }
                        _ => child.placement,
                    };
                    let offset = child.anchor.child_offset(container.size(), child_size)
                        + container.origin().to_vec2();
                    let offset = match child.anchor {
                        PopoverAnchor::BottomStart
                        | PopoverAnchor::BottomCenter
                        | PopoverAnchor::BottomEnd => Point::new(offset.x, offset.y + child.gap),
                        PopoverAnchor::TopStart
                        | PopoverAnchor::TopCenter
                        | PopoverAnchor::TopEnd => Point::new(offset.x, offset.y - child.gap),
                        PopoverAnchor::ViewportQuarter => offset,
                    };
                    ctx.place_child(&mut child.widget, offset);
                    child.placed = Rect::from_origin_size(offset, child_size);
                }
                PortalPlacement::Trigger | PortalPlacement::BareTrigger => {
                    ctx.set_stashed(&mut child.widget, true);
                    child.placed = Rect::ZERO;
                }
            }
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural; children paint themselves.
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
        let ids: Vec<_> = self.children.iter().map(|c| c.widget.id()).collect();
        ChildrenIds::from_slice(&ids)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::Theme;
    use crate::overlay_scope::OverlayScopeHandle;

    fn content() -> Arc<PortalContentView<(), ()>> {
        let theme = Theme::default();
        Arc::new(crate::label("portal content").render(&theme))
    }

    #[test]
    fn register_allocates_distinct_keys_starting_at_one() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let a = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        let b = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
    }

    #[test]
    fn snapshot_returns_entries_in_registration_order() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let a = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        let b = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        let keys: Vec<u64> = portal.snapshot().iter().map(|e| e.key).collect();
        assert_eq!(keys, vec![a, b]);
    }

    #[test]
    fn update_replaces_content_for_an_existing_key() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let key = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        let replacement = content();
        portal.update(
            key,
            replacement.clone(),
            &theme,
            PortalPlacement::Trigger,
            SurfaceStyle::Popover,
        );
        let snap = portal.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(Arc::ptr_eq(&snap[0].content, &replacement));
    }

    #[test]
    fn deregister_removes_the_entry_and_tolerates_unknown_keys() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let key = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        portal.deregister(key);
        assert!(portal.snapshot().is_empty());
        portal.deregister(999); // must not panic
    }

    #[test]
    fn clones_share_the_same_registry() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let clone = portal.clone();
        let theme = Theme::default();
        clone.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        assert_eq!(portal.snapshot().len(), 1);
    }

    #[test]
    fn keys_are_never_reused_after_deregister() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let first = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        assert_eq!(first, 1);
        portal.deregister(first);
        let second = portal.register(content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        assert_eq!(second, 2, "key must not be recycled after deregister");
    }

    #[test]
    fn update_with_unknown_key_is_a_noop() {
        let portal = OverlayPortal::<(), ()>::new(OverlayScopeHandle::new());
        let theme = Theme::default();
        let original = content();
        portal.register(original.clone(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        // update with a key that was never registered — must not panic
        portal.update(999, content(), &theme, PortalPlacement::Trigger, SurfaceStyle::Popover);
        let snap = portal.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(
            Arc::ptr_eq(&snap[0].content, &original),
            "existing entry must be unchanged after update with unknown key"
        );
    }

    // --- PortalSlot tests ---

    use masonry::core::{EventCtx, PointerButton, PointerEvent, PropertiesMut};
    use masonry::kurbo::{Point, Rect, Size};
    use masonry::testing::TestHarness;

    use crate::components::popover::PopoverAnchor;

    fn test_child() -> NewWidget<dyn Widget> {
        masonry::widgets::Label::new("popover body")
            .prepare()
            .erased()
    }

    fn slot_with_one_child() -> (TestHarness<PortalSlot>, u64) {
        let key = 7;
        let slot = PortalSlot::new(vec![(key, test_child(), PortalPlacement::Trigger)]);
        let harness = TestHarness::create(
            masonry::theme::default_property_set(),
            masonry::core::NewWidget::new(slot),
        );
        (harness, key)
    }

    /// Minimal interactive leaf recording primary presses — stands in for
    /// real popover content (buttons, inputs) so the click-through test can
    /// assert the press was *delivered to the child*, not merely tolerated
    /// by the slot.
    struct ClickProbe {
        downs: usize,
    }

    impl Widget for ClickProbe {
        type Action = NoAction;

        fn on_pointer_event(
            &mut self,
            _ctx: &mut EventCtx<'_>,
            _props: &mut PropertiesMut<'_>,
            event: &PointerEvent,
        ) {
            if matches!(event, PointerEvent::Down(_)) {
                self.downs += 1;
            }
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
            Length::px(40.0)
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

    #[test]
    fn slot_children_start_hidden() {
        let (mut harness, _key) = slot_with_one_child();
        harness.edit_root_widget(|wm| {
            assert!(!wm.widget.children[0].visible);
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
                PortalPlacement {
                    owner: None,
                    owner_kind: OwnerKind::Popover,
                    rect: placement,
                    anchor: PopoverAnchor::BottomStart,
                    gap: 4.0,
                },
            );
        });
        harness.edit_root_widget(|wm| {
            let placed = wm.widget.children[0].placed;
            // BottomStart: x flush with placement left, y = placement bottom + gap.
            assert!((placed.x0 - 10.0).abs() < 1e-9);
            assert!((placed.y0 - 44.0).abs() < 1e-9);
        });
    }

    #[test]
    fn set_visible_centers_a_viewport_quarter_child_in_the_slots_own_size() {
        let (mut harness, key) = slot_with_one_child();
        harness.edit_root_widget(|mut wm| {
            PortalSlot::set_visible(
                &mut wm,
                key,
                true,
                PortalPlacement {
                    owner: None,
                    owner_kind: OwnerKind::Dialog,
                    rect: Rect::ZERO,
                    anchor: PopoverAnchor::ViewportQuarter,
                    gap: 0.0,
                },
            );
        });
        harness.edit_root_widget(|wm| {
            let placed = wm.widget.children[0].placed;
            let window = Size::new(400.0, 400.0);
            // Centered horizontally and sitting a quarter of the way down the
            // slot's own size (== the scope's content box), not anchored to
            // `placement` (which is `Rect::ZERO` here).
            assert!((placed.x0 - (window.width - placed.width()) / 2.0).abs() < 1e-9);
            assert!((placed.y0 - (window.height - placed.height()) * 0.25).abs() < 1e-9);
        });
    }

    #[test]
    fn clicks_pass_through_when_no_popover_is_open() {
        // With every child hidden (stashed) and the slot itself refusing
        // pointer interaction, nothing in the slot is a hit target — pointer
        // events fall through to whatever is beneath and nothing panics.
        let (mut harness, _key) = slot_with_one_child();
        harness.mouse_move(Point::new(390.0, 390.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|wm| {
            assert!(!wm.widget.children[0].visible, "child must stay hidden");
        });
    }

    #[test]
    fn dismiss_outside_hides_visible_children() {
        let (mut harness, key) = slot_with_one_child();
        let placement = Rect::new(10.0, 10.0, 110.0, 40.0);
        harness.edit_root_widget(|mut wm| {
            PortalSlot::set_visible(
                &mut wm,
                key,
                true,
                PortalPlacement {
                    owner: None,
                    owner_kind: OwnerKind::Popover,
                    rect: placement,
                    anchor: PopoverAnchor::BottomStart,
                    gap: 0.0,
                },
            );
        });
        // A press far away from both the placed content and the trigger rect.
        harness.edit_root_widget(|mut wm| {
            PortalSlot::dismiss_outside(&mut wm, Point::new(390.0, 390.0));
        });
        harness.edit_root_widget(|wm| {
            assert!(!wm.widget.children[0].visible);
        });
    }

    #[test]
    fn dismiss_outside_keeps_children_for_presses_on_content_or_trigger() {
        let (mut harness, key) = slot_with_one_child();
        let placement = Rect::new(10.0, 10.0, 110.0, 40.0);
        harness.edit_root_widget(|mut wm| {
            PortalSlot::set_visible(
                &mut wm,
                key,
                true,
                PortalPlacement {
                    owner: None,
                    owner_kind: OwnerKind::Popover,
                    rect: placement,
                    anchor: PopoverAnchor::BottomStart,
                    gap: 0.0,
                },
            );
        });
        let inside_content = harness.edit_root_widget(|wm| wm.widget.children[0].placed.center());
        harness.edit_root_widget(|mut wm| {
            PortalSlot::dismiss_outside(&mut wm, inside_content);
            assert!(
                wm.widget.children[0].visible,
                "a press inside the popover content must not dismiss it"
            );
            // Inside the trigger's anchor rect: the trigger's own click
            // handler owns the toggle; dismissing here too would re-open.
            PortalSlot::dismiss_outside(&mut wm, placement.center());
            assert!(
                wm.widget.children[0].visible,
                "a press on the trigger rect must not dismiss (the trigger toggles on Up)"
            );
        });
    }

    #[test]
    fn pointer_down_inside_a_visible_child_does_not_dismiss() {
        let key = 7;
        let probe = NewWidget::new(ClickProbe { downs: 0 }).erased();
        let slot = PortalSlot::new(vec![(key, probe, PortalPlacement::Trigger)]);
        let mut harness = TestHarness::create(
            masonry::theme::default_property_set(),
            masonry::core::NewWidget::new(slot),
        );
        let placement = Rect::new(10.0, 10.0, 110.0, 40.0);
        harness.edit_root_widget(|mut wm| {
            PortalSlot::set_visible(
                &mut wm,
                key,
                true,
                PortalPlacement {
                    owner: None,
                    owner_kind: OwnerKind::Popover,
                    rect: placement,
                    anchor: PopoverAnchor::BottomStart,
                    gap: 0.0,
                },
            );
        });
        let inside = harness.edit_root_widget(|wm| wm.widget.children[0].placed.center());
        harness.mouse_move(inside);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|mut wm| {
            assert!(wm.widget.children[0].visible);
            // The press must have been *delivered to the child* — i.e. the
            // visible child wins the hit-test inside its placed rect.
            let mut child = PortalSlot::child_mut(&mut wm, key).expect("child exists");
            let probe = child.downcast::<ClickProbe>();
            assert_eq!(
                probe.widget.downs, 1,
                "the press inside the child's rect must reach the child"
            );
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
