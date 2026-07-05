//! `OverlayScope` widget + `overlay_scope` xilem view: a container that hosts
//! arbitrary `content` plus a discoverable, always-on-top, always-clipped
//! overlay slot that deeply-nested descendants can put popups into: the
//! portal slot ([`crate::overlay_portal::PortalSlot`]), a permanent child
//! whose content is mounted by the scope's *own view* from the
//! [`crate::overlay_portal::OverlayPortal`] Environment resource that
//! `overlay_scope` publishes. Descendants (e.g. `popover`, `dropdown_button`,
//! `autocomplete`, `dialog`, `notification_layer`) register erased content
//! *views* into the portal; the scope's view builds them as real view
//! children, so arbitrary stateful content keeps full xilem semantics
//! (rebuilds, theme swaps, button callbacks). Open/close/placement are plain
//! data pushed from descendants via `ctx.mutate_later(scope_id, ...)`
//! ([`OverlayScope::set_portal_visible`] /
//! [`OverlayScope::set_portal_placement`]). See [`crate::overlay_portal`] for
//! the full flow.
//!
//! Masonry's paint pass is strict depth-first by `children_ids()` registration
//! order, and `set_clip_path` wraps a widget's own paint *and* all its
//! children's recursive paint. `OverlayScope` exploits both facts: children
//! register in the order `content`, portal slot — so portal content always
//! paints on top of `content` and everything inside it (including later
//! in-scope siblings) — and everything is clipped to the scope's own border
//! box (so overlays never escape the container, unlike a window-level masonry
//! `Layer`). That registration order *is* the entire z-mechanism; nothing
//! else makes the overlays "win".
//!
//! Descendants discover the nearest `OverlayScope` ancestor (if any) via the
//! Xilem `Environment` — [`OverlayScopeHandle`] (for `mutate_later` targeting)
//! and [`crate::overlay_portal::OverlayPortal`] (the portal registry) are both
//! published with `provides` and read with
//! [`xilem_masonry::core::Environment::get_slot_for_type`]. Consumers fall
//! back to [`crate::AnchoredOverlay`] when no scope ancestor exists.

use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::properties::Padding;
use xilem_masonry::core::{
    MessageCtx, MessageResult, Mut, Resource, View, ViewId, ViewMarker, ViewPathTracker, provides,
};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use crate::Theme;
use crate::components::popover::widget::{PopoverSurface, SurfaceStyle};
use crate::overlay_portal::{
    OverlayPortal, PortalContentView, PortalContentViewState, PortalEntry, PortalPlacement,
    PortalSlot, PortalVisibility, portal_from_env,
};

/// Resource published into the Xilem [`Environment`](xilem_masonry::core::Environment)
/// by [`overlay_scope`], letting arbitrary descendants discover the scope's
/// `WidgetId` and push content into it via `ctx.mutate_later`.
///
/// `Environment` values are written at `View::build` time, before the scope
/// widget (and thus its `WidgetId`) exists — so this wraps a lazily-filled
/// cell rather than a raw ID. The `Arc<OnceLock<_>>` is constructed up front,
/// cloned into both the published resource and the widget itself, and filled
/// exactly once, by the widget, on its first [`Update::WidgetAdded`].
#[derive(Clone, Debug)]
pub struct OverlayScopeHandle(Arc<OnceLock<WidgetId>>);

impl Resource for OverlayScopeHandle {}

impl OverlayScopeHandle {
    pub(crate) fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    fn set(&self, id: WidgetId) {
        let _ = self.0.set(id);
    }

    /// The scope widget's `WidgetId`, once it has been mounted.
    ///
    /// `None` only in the brief window between this handle being published
    /// (at `View::build`) and the scope widget's first `Update::WidgetAdded`
    /// — both of which happen before any event can reach a descendant, so in
    /// practice this is always `Some` by the time a consumer needs it.
    #[must_use]
    pub fn widget_id(&self) -> Option<WidgetId> {
        self.0.get().copied()
    }
}

thread_local! {
    /// The [`OverlayPortal`] of the outermost `overlay_scope` currently being
    /// built or rebuilt, valid for the duration of that scope's (and its
    /// descendants') `build`/`rebuild` call — `None` outside of that window.
    ///
    /// `dialog` reads this via [`root_portal`] to always target the root
    /// scope, unlike `popover`, which uses the *nearest* scope's portal
    /// published through the Environment. A nested
    /// `overlay_scope` leaves this untouched, so descendants — including
    /// dialogs registered by a nested scope's content — see the outermost
    /// scope's portal.
    static ROOT_PORTAL: RefCell<Option<Box<dyn Any>>> = RefCell::new(None);
}

/// If no scope has yet claimed the root slot for this build/rebuild pass,
/// claim it for `portal` and return `true` — the caller must call
/// [`release_root_portal`] once its content subtree has finished
/// building/rebuilding. Otherwise leave the existing claim untouched and
/// return `false`.
fn claim_root_portal<State: 'static, Action: 'static>(
    portal: &OverlayPortal<State, Action>,
) -> bool {
    ROOT_PORTAL.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_some() {
            return false;
        }
        *cell = Some(Box::new(portal.clone()));
        true
    })
}

/// Release a claim made by [`claim_root_portal`].
fn release_root_portal() {
    ROOT_PORTAL.with(|cell| *cell.borrow_mut() = None);
}

/// RAII wrapper around [`claim_root_portal`]/[`release_root_portal`]: holds
/// the claim (if this call won it) until dropped. `OverlayScopeRootView`
/// keeps one of these alive for its whole `build`/`rebuild` — including the
/// portal-entry fixpoint loop — so that portal-mounted content (e.g. a
/// `dialog` nested inside a `popover`'s content) built *during* that loop can
/// still see the root portal via [`root_portal`].
struct RootPortalGuard {
    is_root: bool,
}

impl RootPortalGuard {
    fn claim<State: 'static, Action: 'static>(portal: &OverlayPortal<State, Action>) -> Self {
        Self {
            is_root: claim_root_portal(portal),
        }
    }
}

impl Drop for RootPortalGuard {
    fn drop(&mut self) {
        if self.is_root {
            release_root_portal();
        }
    }
}

/// The [`OverlayPortal`] of the outermost `overlay_scope` ancestor currently
/// being built or rebuilt, if any.
///
/// Returns `None` both when there is no `overlay_scope` ancestor at all and
/// when the root scope was published for a different `State`/`Action` pair
/// (e.g. a sub-tree rendered with different generic parameters than its
/// enclosing scope) — callers should treat both cases the same way.
pub(crate) fn root_portal<State: 'static, Action: 'static>() -> Option<OverlayPortal<State, Action>>
{
    ROOT_PORTAL.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|boxed| boxed.downcast_ref::<OverlayPortal<State, Action>>())
            .cloned()
    })
}

/// Masonry widget that hosts a footprint-dictating `content` child plus a
/// permanent, always-clipped [`PortalSlot`] (registered last, so it paints
/// above `content` and everything inside it).
///
/// `content` alone determines the container's measured size — showing or
/// hiding portal children never reflows surrounding layout (mirrors
/// [`crate::AnchoredOverlay::measure`]). Portal children carry per-key
/// placements (see [`Self::set_portal_visible`]).
pub struct OverlayScope {
    handle: OverlayScopeHandle,
    content: WidgetPod<dyn Widget>,
    portal_slot: WidgetPod<PortalSlot>,
}

impl OverlayScope {
    pub(crate) fn new(
        handle: OverlayScopeHandle,
        content: NewWidget<dyn Widget>,
        portal_children: Vec<(u64, NewWidget<dyn Widget>, PortalPlacement)>,
    ) -> Self {
        Self {
            handle,
            content: content.to_pod(),
            portal_slot: NewWidget::new(PortalSlot::new(portal_children)).to_pod(),
        }
    }

    /// Mutable access to the wrapped content for the [`View`] layer.
    pub fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.content)
    }

    /// Mutable access to the portal slot for the scope view and tests.
    pub(crate) fn portal_slot_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, PortalSlot> {
        this.ctx.get_mut(&mut this.widget.portal_slot)
    }

    /// Show/hide a portal child. `anchor_rect_window` is the trigger's box in
    /// *window* coordinates; converted here with `to_local` exactly like the
    /// dropdown's scope push (robust to scrolling/transforms between the
    /// scope and the trigger). For
    /// [`crate::components::popover::PopoverAnchor::ViewportQuarter`],
    /// `anchor_rect_window` is ignored — pass [`Rect::ZERO`] — since
    /// `PortalSlot::layout` centers that variant in its own size rather than
    /// any placement rect.
    pub(crate) fn set_portal_visible(
        this: &mut WidgetMut<'_, Self>,
        key: u64,
        visible: bool,
        placement: PortalVisibility,
    ) {
        let local_origin = this.ctx.to_local(placement.rect.origin());
        let rect = Rect::from_origin_size(local_origin, placement.rect.size());
        let mut slot = Self::portal_slot_mut(this);
        PortalSlot::set_visible(
            &mut slot,
            key,
            visible,
            PortalVisibility { rect, ..placement },
        );
    }

    /// Re-anchor a visible portal child as its trigger moves.
    pub fn set_portal_placement(
        this: &mut WidgetMut<'_, Self>,
        key: u64,
        anchor_rect_window: Rect,
    ) {
        let local_origin = this.ctx.to_local(anchor_rect_window.origin());
        let placement = Rect::from_origin_size(local_origin, anchor_rect_window.size());
        let mut slot = Self::portal_slot_mut(this);
        PortalSlot::set_placement(&mut slot, key, placement);
    }
}

impl Widget for OverlayScope {
    type Action = NoAction;

    /// Outside-press dismissal for portal popovers (light dismiss with
    /// pass-through). The scope is an ancestor of everything inside it —
    /// content, triggers, and the portal slot — so every pointer-down inside
    /// the scope bubbles through here, no occluding backdrop required:
    /// scroll, hover, and clicks all reach the content beneath an open
    /// popover normally (the open popover re-anchors via
    /// `PopoverHost::compose`, driven by its own anim-frame loop while open —
    /// see that method). The press is not consumed; whether it dismisses is
    /// the slot's call (`PortalSlot::dismiss_outside` — deferred via
    /// `mutate_child_later` because the per-child visibility/placement state
    /// lives in the slot, out of reach of an `EventCtx`).
    ///
    /// Known leak: a descendant that `set_handled`s the *down* half of a
    /// press stops it bubbling here, leaving the popover open for that
    /// press. No `void_ui` or common masonry widget does (they consume Ups
    /// and Scrolls), and the failure mode is benign.
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent { state, .. }) = event {
            // Slot-local == scope-local: the slot is placed at the scope origin.
            let pos = ctx.local_position(state.position);
            ctx.mutate_child_later(&mut self.portal_slot, move |mut slot| {
                PortalSlot::dismiss_outside(&mut slot, pos);
            });
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        // Registration order is paint order: content first, the portal slot
        // last — portal content always draws on top of content and everything
        // inside it (including later siblings within the scope). This
        // ordering *is* the entire mechanism; nothing else makes the overlay
        // "win".
        ctx.register_child(&mut self.content);
        ctx.register_child(&mut self.portal_slot);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        // Ignore the portal slot entirely for sizing — see
        // `AnchoredOverlay::measure` for the rationale (a transparent forward,
        // not `redirect_measurement`, which under-measures and causes
        // flex-sibling overlap). Guarantees showing/hiding portal children
        // never reflows the scope's container.
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

        // The clip that confines portal content to this scope's own bounds —
        // the other half of the mechanism alongside registration order.
        ctx.set_clip_path(size.to_rect());

        ctx.run_layout(&mut self.portal_slot, size);
        ctx.place_child(&mut self.portal_slot, Point::ORIGIN);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — the two children paint themselves.
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut masonry::core::PropertiesMut<'_>,
        event: &Update,
    ) {
        if let Update::WidgetAdded = event {
            // The moment we exist, publish our ID so descendants that cloned
            // this handle at `View::build` time can resolve it lazily.
            self.handle.set(ctx.widget_id());
        }
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
        ChildrenIds::from_slice(&[self.content.id(), self.portal_slot.id()])
    }
}

/// Wrap `content` in an [`OverlayScope`], publishing its `WidgetId` into the
/// Xilem `Environment` so that any descendant (e.g. a `dropdown_button`) can
/// discover it and push popups into it — popups that paint on top of
/// everything inside `content`, clipped to `content`'s own bounds.
///
/// ```ignore
/// use void_ui::{overlay_scope, components::scroll_container::scroll_container};
/// overlay_scope(scroll_container(my_content)).render(&theme)
/// ```
pub fn overlay_scope<State, Action, V>(content: V) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    let handle = OverlayScopeHandle::new();
    let resource_handle = handle.clone();
    let portal_handle = handle.clone();
    // `provides` build-once semantics: each app pass recreates these closures
    // when the view value is rebuilt, but xilem only ever invokes them at the
    // provider's *build* — the value produced then is the one published for
    // the provider's whole lifetime; later closure results are ignored. The
    // `OverlayPortal` is therefore constructed *inside* the closure (it holds
    // an `Rc` registry, and `WidgetView` requires the view value — including
    // captures — to be `Send + Sync`): exactly one portal is ever created,
    // with stable identity. The root view must not rely on captures after
    // build either; it reads the published resource back out of the
    // Environment in `build` (the inner `provides` has already pushed it by
    // then) and keeps that clone in its ViewState.
    provides(
        move |_state: &mut State| resource_handle.clone(),
        provides(
            move |_state: &mut State| OverlayPortal::<State, Action>::new(portal_handle.clone()),
            OverlayScopeRootView {
                handle,
                content,
                phantom: PhantomData,
            },
        ),
    )
}

/// One portal entry currently mounted in the slot, with the view-state xilem
/// needs to rebuild/teardown it.
struct MountedEntry<State: 'static, Action: 'static> {
    key: u64,
    view: Arc<PortalContentView<State, Action>>,
    view_state: PortalContentViewState<State, Action>,
    theme: Theme,
    placement: PortalPlacement,
}

#[doc(hidden)]
pub struct OverlayScopeViewState<State: 'static, Action: 'static, ContentVS> {
    content_state: ContentVS,
    portal: OverlayPortal<State, Action>,
    mounted: Vec<MountedEntry<State, Action>>,
}

/// `ViewId` for the scope's `content` child; portal entries use their key
/// (keys start at 1, so 0 is never a portal key).
const CONTENT_VIEW_ID: ViewId = ViewId::new(0);

/// Wrap freshly-built portal content for mounting in the [`PortalSlot`].
///
/// [`PortalPlacement::Trigger`] entries (popovers) get the popover chrome:
/// density padding on the content, [`PopoverSurface`] for background/border.
/// Mirrors `PopoverHost::new`'s in-tree wrapping so portal and fallback
/// popovers look identical.
///
/// [`PortalPlacement::BareTrigger`] entries (dropdown menus) and
/// [`PortalPlacement::Corner`] entries (toast layers) are mounted as-is — the
/// registered content already carries its own surface/background, and adding
/// popover chrome would double it up.
fn wrap_portal_content(
    pod: Pod<masonry::widgets::Passthrough>,
    theme: &Theme,
    placement: PortalPlacement,
    style: SurfaceStyle,
) -> NewWidget<dyn Widget> {
    match placement {
        PortalPlacement::Trigger => {
            let mut content = pod.new_widget.erased();
            content
                .properties
                .insert(Padding::all(Length::px(f64::from(theme.density.pad))));
            NewWidget::new(PopoverSurface::new(content, theme, style)).erased()
        }
        PortalPlacement::BareTrigger | PortalPlacement::Corner(_) => pod.new_widget.erased(),
    }
}

/// Access a mounted portal entry's content widget, i.e. the
/// `Pod<Passthrough>` that `entry.content` (an `Arc<dyn AnyView<..,
/// Pod<Passthrough>>>`) builds/rebuilds/tears down directly.
///
/// [`PortalPlacement::Trigger`] slot children are that `Passthrough` wrapped
/// in [`PopoverSurface`] chrome, so unwrap one layer first.
/// [`PortalPlacement::BareTrigger`] and [`PortalPlacement::Corner`] slot
/// children mount the `Passthrough` bare (see [`wrap_portal_content`]), so
/// `child` (itself type-erased to `dyn Widget`) already *is* it — the
/// caller's subsequent `.downcast()` resolves it to `Pod<Passthrough>`.
fn with_portal_content<R>(
    mut child: WidgetMut<'_, dyn Widget>,
    placement: PortalPlacement,
    f: impl FnOnce(WidgetMut<'_, dyn Widget>) -> R,
) -> R {
    match placement {
        PortalPlacement::Trigger => {
            let mut surface = child.downcast::<PopoverSurface>();
            f(PopoverSurface::content_mut(&mut surface))
        }
        PortalPlacement::BareTrigger | PortalPlacement::Corner(_) => f(child),
    }
}

/// Unmount entries in `mounted` whose portal registration is gone from
/// `entries` (including nested deregistrations discovered on later
/// iterations of the caller's fixpoint loop — without this, a nested popover
/// removed while open would linger painted until the next app-state change).
/// Returns whether anything was unmounted, since teardown can itself
/// deregister nested entries (after `entries` was snapshotted), so unmounting
/// counts as progress in the caller's loop — otherwise a cascade of removals
/// could stall before fully unmounting.
fn unmount_stale_entries<State: 'static, Action: 'static>(
    ctx: &mut ViewCtx,
    element: &mut Mut<'_, Pod<OverlayScope>>,
    mounted: &mut Vec<MountedEntry<State, Action>>,
    entries: &[PortalEntry<State, Action>],
) -> bool {
    let mut progressed = false;
    let mut idx = 0;
    while idx < mounted.len() {
        let key = mounted[idx].key;
        if entries.iter().any(|e| e.key == key) {
            idx += 1;
            continue;
        }
        let mut gone = mounted.remove(idx);
        progressed = true;
        {
            let mut slot = OverlayScope::portal_slot_mut(element);
            let child = PortalSlot::child_mut(&mut slot, key)
                .expect("mounted entry must have a slot child");
            with_portal_content(child, gone.placement, |mut content| {
                ctx.with_id(ViewId::new(key), |ctx| {
                    gone.view
                        .teardown(&mut gone.view_state, ctx, content.downcast());
                });
            });
        }
        let mut slot = OverlayScope::portal_slot_mut(element);
        PortalSlot::remove_by_key(&mut slot, key);
    }
    progressed
}

/// The `View` actually wrapped by the nested `provides` in [`overlay_scope`].
/// Two-part job: it builds the [`OverlayScope`] widget around `content`'s pod
/// (threading `build`/`rebuild`/`teardown`/`message` through
/// [`OverlayScope::content_mut`]), and it mounts/diffs the content views
/// registered in the scope's [`OverlayPortal`] as [`PortalSlot`] children —
/// each under element path `…scope path… / ViewId(key)` so xilem messages
/// from inside portal-mounted popovers route back correctly.
#[must_use = "View values do nothing unless provided to Xilem."]
struct OverlayScopeRootView<V, State, Action> {
    handle: OverlayScopeHandle,
    content: V,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for OverlayScopeRootView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for OverlayScopeRootView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<OverlayScope>;
    type ViewState = OverlayScopeViewState<State, Action, V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx)
            .expect("overlay_scope provides OverlayPortal for its own subtree");

        // Claim the root-portal slot for the rest of this `build` — both
        // `content` (which may contain nested `overlay_scope`s and
        // `dialog`s) and the portal-entry fixpoint loop below (which builds
        // popover/dialog content that may itself contain nested `dialog`s)
        // need `root_portal` to resolve. See `root_portal` and
        // `RootPortalGuard`.
        let _root_portal_guard = RootPortalGuard::claim(&portal);
        let (content, content_state) =
            ctx.with_id(CONTENT_VIEW_ID, |ctx| self.content.build(ctx, app_state));

        // Mount registered entries, iterating to a fixpoint: building an
        // entry can itself register nested popovers (a popover inside another
        // popover's content), which mutate the registry mid-loop, so each
        // iteration re-snapshots and processes only keys not yet seen this
        // pass. Keys are monotonically allocated, so the loop terminates.
        // Removal can't happen during build (nothing is mounted yet), so no
        // removal arm is needed here — unlike `rebuild`.
        let mut mounted = Vec::new();
        let mut slot_children = Vec::new();
        let mut processed: Vec<u64> = Vec::new();
        loop {
            let entries = portal.snapshot();
            let mut progressed = false;
            for entry in &entries {
                if processed.contains(&entry.key) {
                    continue;
                }
                processed.push(entry.key);
                progressed = true;
                // Re-fetch at processing time: an earlier entry's build this
                // pass may have `update()`d this key after the snapshot was
                // taken.
                let Some(entry) = portal.entry(entry.key) else {
                    // Registered and deregistered within this build pass —
                    // nothing to mount.
                    continue;
                };
                let (pod, view_state) = ctx.with_id(ViewId::new(entry.key), |ctx| {
                    entry.content.build(ctx, app_state)
                });
                let wrapped = wrap_portal_content(pod, &entry.theme, entry.placement, entry.style);
                slot_children.push((entry.key, wrapped, entry.placement));
                mounted.push(MountedEntry {
                    key: entry.key,
                    view: entry.content.clone(),
                    view_state,
                    theme: entry.theme,
                    placement: entry.placement,
                });
            }
            if !progressed {
                break;
            }
        }

        let widget = OverlayScope::new(
            self.handle.clone(),
            content.new_widget.erased(),
            slot_children,
        );
        (
            ctx.create_pod(widget),
            OverlayScopeViewState {
                content_state,
                portal,
                mounted,
            },
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        // 1. Rebuild content first — descendant popovers refresh the registry
        //    during this call; the fixpoint diff below then converges on
        //    whatever the registry says, including registrations that happen
        //    mid-diff while entries themselves rebuild.
        //
        //    Claim the root-portal slot for the rest of this `rebuild` (see
        //    `root_portal` and `RootPortalGuard`) — both `content` (which may
        //    contain nested `overlay_scope`s and `dialog`s) and the
        //    portal-entry fixpoint loop below (which builds/rebuilds
        //    popover/dialog content that may itself contain nested
        //    `dialog`s) need `root_portal` to resolve.
        let _root_portal_guard = RootPortalGuard::claim(&view_state.portal);
        {
            let mut content = OverlayScope::content_mut(&mut element);
            ctx.with_id(CONTENT_VIEW_ID, |ctx| {
                self.content.rebuild(
                    &prev.content,
                    &mut view_state.content_state,
                    ctx,
                    content.downcast(),
                    app_state,
                );
            });
        }

        // 2./3. Diff the registry against mounted entries, iterating to a
        // fixpoint: building/rebuilding/tearing-down an entry can itself
        // register or remove nested popovers (a popover inside another
        // popover's content), which mutate the registry mid-diff. Both arms
        // count as progress, and both are bounded — removals strictly shrink
        // `mounted`, while the kept/new arm processes each key at most once
        // per rebuild and keys are monotonically allocated — so the loop
        // terminates. Entries are processed in registration (= key) order and
        // re-fetched from the live registry at processing time, which together
        // guarantee an owner's `update()` this pass is observed when its
        // (later-keyed) entry is processed — the pass-start snapshot alone
        // would be stale.
        let mut processed: Vec<u64> = Vec::new();
        loop {
            let entries = view_state.portal.snapshot();
            let mut progressed = false;

            // Unmount entries whose popovers deregistered (including nested
            // deregistrations discovered on later iterations — without this, a
            // nested popover removed while open would linger painted until the
            // next app-state change). Teardown can itself deregister nested
            // entries (after this pass's snapshot), so unmounting counts as
            // progress — otherwise a cascade of removals could stall before
            // fully unmounting.
            progressed |=
                unmount_stale_entries(ctx, &mut element, &mut view_state.mounted, &entries);

            // Rebuild kept entries, mount new ones — only keys not yet
            // processed this pass. The snapshot supplies only the ORDER and
            // the key set; each entry's content/theme is re-fetched live.
            for entry in &entries {
                if processed.contains(&entry.key) {
                    continue;
                }
                processed.push(entry.key);
                progressed = true;
                // Re-fetch at processing time: an earlier entry's rebuild this
                // pass may have `update()`d this key after the snapshot was
                // taken.
                let Some(entry) = view_state.portal.entry(entry.key) else {
                    // Deregistered since the snapshot — the removal arm of the
                    // next iteration unmounts it (`progressed` is already true,
                    // so that iteration runs).
                    continue;
                };
                if let Some(m) = view_state.mounted.iter_mut().find(|m| m.key == entry.key) {
                    let mut slot = OverlayScope::portal_slot_mut(&mut element);
                    match entry.placement {
                        PortalPlacement::Trigger => {
                            if m.theme != entry.theme {
                                let mut child = PortalSlot::child_mut(&mut slot, entry.key)
                                    .expect("mounted entry must have a slot child");
                                let mut surface = child.downcast::<PopoverSurface>();
                                PopoverSurface::set_theme(&mut surface, &entry.theme);
                            }
                        }
                        PortalPlacement::BareTrigger => {}
                        PortalPlacement::Corner(unit_point) => {
                            PortalSlot::set_corner(&mut slot, entry.key, unit_point);
                        }
                    }
                    let child = PortalSlot::child_mut(&mut slot, entry.key)
                        .expect("mounted entry must have a slot child");
                    with_portal_content(child, entry.placement, |mut content| {
                        ctx.with_id(ViewId::new(entry.key), |ctx| {
                            entry.content.rebuild(
                                &m.view,
                                &mut m.view_state,
                                ctx,
                                content.downcast(),
                                app_state,
                            );
                        });
                    });
                    m.theme = entry.theme;
                    m.placement = entry.placement;
                    m.view = entry.content.clone();
                } else {
                    let (pod, vs_new) = ctx.with_id(ViewId::new(entry.key), |ctx| {
                        entry.content.build(ctx, app_state)
                    });
                    let wrapped =
                        wrap_portal_content(pod, &entry.theme, entry.placement, entry.style);
                    let mut slot = OverlayScope::portal_slot_mut(&mut element);
                    PortalSlot::insert(&mut slot, entry.key, wrapped, entry.placement);
                    view_state.mounted.push(MountedEntry {
                        key: entry.key,
                        view: entry.content.clone(),
                        view_state: vs_new,
                        theme: entry.theme,
                        placement: entry.placement,
                    });
                }
            }

            if !progressed {
                break;
            }
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut content = OverlayScope::content_mut(&mut element);
            ctx.with_id(CONTENT_VIEW_ID, |ctx| {
                self.content
                    .teardown(&mut view_state.content_state, ctx, content.downcast());
            });
        }
        for m in &mut view_state.mounted {
            let mut slot = OverlayScope::portal_slot_mut(&mut element);
            // Tolerant `if let` (not `expect`): the whole widget tree is being
            // dropped wholesale here, so a missing slot child is not an
            // invariant violation worth panicking over.
            if let Some(child) = PortalSlot::child_mut(&mut slot, m.key) {
                with_portal_content(child, m.placement, |mut content| {
                    ctx.with_id(ViewId::new(m.key), |ctx| {
                        m.view.teardown(&mut m.view_state, ctx, content.downcast());
                    });
                });
            }
        }
        view_state.mounted.clear();
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let first = message
            .take_first()
            .expect("OverlayScopeRootView received a message with an empty path");
        if first == CONTENT_VIEW_ID {
            let mut content = OverlayScope::content_mut(&mut element);
            return self.content.message(
                &mut view_state.content_state,
                message,
                content.downcast(),
                app_state,
            );
        }
        let Some(m) = view_state
            .mounted
            .iter_mut()
            .find(|m| ViewId::new(m.key) == first)
        else {
            return MessageResult::Stale;
        };
        let mut slot = OverlayScope::portal_slot_mut(&mut element);
        let Some(child) = PortalSlot::child_mut(&mut slot, m.key) else {
            return MessageResult::Stale;
        };
        with_portal_content(child, m.placement, |mut content| {
            m.view
                .message(&mut m.view_state, message, content.downcast(), app_state)
        })
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::{EventCtx, NewWidget, PointerButton, PointerEvent, PropertiesMut};
    use masonry::kurbo::{Rect, Vec2};
    use masonry::testing::TestHarness;

    use super::*;
    use crate::components::popover::PopoverAnchor;
    use crate::overlay_portal::OwnerKind;

    /// Scope content standing in for "the app under the popover": records
    /// every pointer Down and Scroll delivered to it, so tests can assert
    /// that input reaches the content beneath an open popover instead of
    /// being swallowed by the overlay machinery.
    #[derive(Default)]
    struct EventProbe {
        downs: usize,
        scrolls: usize,
    }

    impl Widget for EventProbe {
        type Action = NoAction;

        fn on_pointer_event(
            &mut self,
            _ctx: &mut EventCtx<'_>,
            _props: &mut PropertiesMut<'_>,
            event: &PointerEvent,
        ) {
            match event {
                PointerEvent::Down(_) => self.downs += 1,
                PointerEvent::Scroll(_) => self.scrolls += 1,
                _ => {}
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
            Length::px(400.0)
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

    /// A scope whose content is an [`EventProbe`] and whose portal slot holds
    /// one child, already made visible with its trigger anchored at
    /// (10,10)-(110,40). The popover content itself ends up placed just below
    /// that rect; everything else in the 400x400 window is "outside".
    fn probe_scope_with_open_popover() -> (TestHarness<OverlayScope>, u64) {
        let key = 3;
        let content = NewWidget::new(EventProbe::default()).erased();
        let popover = masonry::widgets::Label::new("popover").prepare().erased();
        let scope = OverlayScope::new(
            OverlayScopeHandle::new(),
            content,
            vec![(key, popover, PortalPlacement::Trigger)],
        );
        let mut harness = TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(scope),
        );
        harness.edit_root_widget(|mut wm| {
            OverlayScope::set_portal_visible(
                &mut wm,
                key,
                true,
                PortalVisibility {
                    owner: None,
                    owner_kind: OwnerKind::Popover,
                    rect: Rect::new(10.0, 10.0, 110.0, 40.0),
                    anchor: PopoverAnchor::BottomStart,
                    gap: 4.0,
                },
            );
        });
        (harness, key)
    }

    #[test]
    fn scroll_outside_an_open_popover_reaches_content_beneath() {
        let (mut harness, key) = probe_scope_with_open_popover();
        harness.mouse_move(masonry::kurbo::Point::new(390.0, 390.0));
        harness.mouse_wheel(Vec2::new(0.0, -10.0));
        harness.edit_root_widget(|mut wm| {
            let mut content = OverlayScope::content_mut(&mut wm);
            let probe = content.downcast::<EventProbe>();
            assert_eq!(
                probe.widget.scrolls, 1,
                "a scroll outside the open popover must reach the content beneath"
            );
        });
        harness.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "scrolling must not dismiss the popover (it re-anchors via compose instead)"
            );
        });
    }

    #[test]
    fn pointer_down_outside_an_open_popover_dismisses_and_passes_through() {
        let (mut harness, key) = probe_scope_with_open_popover();
        harness.mouse_move(masonry::kurbo::Point::new(390.0, 390.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_none(),
                "a pointer down outside the popover must dismiss it"
            );
        });
        harness.edit_root_widget(|mut wm| {
            let mut content = OverlayScope::content_mut(&mut wm);
            let probe = content.downcast::<EventProbe>();
            assert_eq!(
                probe.widget.downs, 1,
                "the dismissing pointer down must also reach the content beneath"
            );
        });
    }

    #[test]
    fn pointer_down_on_the_trigger_rect_does_not_dismiss() {
        // The trigger's own click handler toggles the popover on Up; if the
        // scope also dismissed on the Down half, the toggle would re-open it
        // (close-then-reopen). Downs inside the trigger's anchor rect are
        // therefore the trigger's business, not the scope's.
        let (mut harness, key) = probe_scope_with_open_popover();
        harness.mouse_move(masonry::kurbo::Point::new(50.0, 20.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            assert!(
                slot.widget.placed_rect(key).is_some(),
                "a down on the trigger rect is the trigger's to handle, not an outside-press dismiss"
            );
        });
    }

    #[test]
    fn set_portal_visible_converts_window_rect_and_shows_the_slot_child() {
        let key = 3;
        let content = masonry::widgets::Label::new("content").prepare().erased();
        let popover = masonry::widgets::Label::new("popover").prepare().erased();
        let scope = OverlayScope::new(
            OverlayScopeHandle::new(),
            content,
            vec![(key, popover, PortalPlacement::Trigger)],
        );
        let mut harness = TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(scope),
        );
        harness.edit_root_widget(|mut wm| {
            OverlayScope::set_portal_visible(
                &mut wm,
                key,
                true,
                PortalVisibility {
                    owner: None,
                    owner_kind: OwnerKind::Popover,
                    rect: Rect::new(10.0, 10.0, 110.0, 40.0),
                    anchor: PopoverAnchor::BottomStart,
                    gap: 4.0,
                },
            );
        });
        // The scope sits at the window origin, so window == local coords and
        // the slot child must be placed at (10, 44).
        harness.mouse_move(masonry::kurbo::Point::ZERO);
        harness.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            let placed = slot.widget.placed_rect(key).expect("child placed");
            assert!((placed.x0 - 10.0).abs() < 1e-9);
            assert!((placed.y0 - 44.0).abs() < 1e-9);
        });
    }

    #[test]
    fn set_portal_visible_centers_a_viewport_quarter_child_in_the_scope() {
        let key = 3;
        let content = masonry::widgets::Label::new("content").prepare().erased();
        let popover = masonry::widgets::Label::new("popover").prepare().erased();
        let scope = OverlayScope::new(
            OverlayScopeHandle::new(),
            content,
            vec![(key, popover, PortalPlacement::Trigger)],
        );
        let mut harness = TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(scope),
        );
        harness.edit_root_widget(|mut wm| {
            // `anchor_rect_window` is ignored for `ViewportQuarter` — pass `Rect::ZERO`.
            OverlayScope::set_portal_visible(
                &mut wm,
                key,
                true,
                PortalVisibility {
                    owner: None,
                    owner_kind: OwnerKind::Dialog,
                    rect: Rect::ZERO,
                    anchor: PopoverAnchor::ViewportQuarter,
                    gap: 0.0,
                },
            );
        });
        harness.mouse_move(masonry::kurbo::Point::ZERO);
        harness.edit_root_widget(|mut wm| {
            let slot = OverlayScope::portal_slot_mut(&mut wm);
            let placed = slot.widget.placed_rect(key).expect("child placed");
            let window = Size::new(400.0, 400.0);
            assert!((placed.x0 - (window.width - placed.width()) / 2.0).abs() < 1e-9);
            assert!((placed.y0 - (window.height - placed.height()) * 0.25).abs() < 1e-9);
        });
    }

    /// [`root_portal`] surfaces the outermost claimed scope's portal — a
    /// nested scope's `claim_root_portal` is a no-op while an outer claim is
    /// active, so descendants of both (including a `dialog` registered from
    /// the nested scope's content) see the outer scope's portal.
    #[test]
    fn root_portal_is_the_outermost_claimed_scope() {
        let outer_handle = OverlayScopeHandle::new();
        let inner_handle = OverlayScopeHandle::new();
        outer_handle.set(WidgetPod::new(masonry::widgets::Label::new("outer")).id());
        inner_handle.set(WidgetPod::new(masonry::widgets::Label::new("inner")).id());
        let outer_portal = OverlayPortal::<(), ()>::new(outer_handle.clone());
        let inner_portal = OverlayPortal::<(), ()>::new(inner_handle.clone());

        assert!(root_portal::<(), ()>().is_none());

        assert!(claim_root_portal(&outer_portal));
        // A nested scope's claim is a no-op while the outer claim is active.
        assert!(!claim_root_portal(&inner_portal));

        let seen = root_portal::<(), ()>().expect("root portal claimed");
        assert_eq!(seen.scope().widget_id(), outer_handle.widget_id());

        release_root_portal();
        assert!(root_portal::<(), ()>().is_none());
    }
}
