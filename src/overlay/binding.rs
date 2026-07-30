//! `PortalBinding` — the one implementation of the portal open/close/reanchor
//! push that every portal-hosted component (`popover`, `dropdown_button`,
//! `autocomplete`, `dialog`) previously hand-copied: resolve the scope's
//! widget id → compute the trigger's window-space anchor rect → stash it for
//! `compose`-time re-anchoring → `mutate_later` into the scope →
//! `OverlayScope::set_portal_visible` → (on open) arm the per-frame
//! `compose` loop that tracks ancestor scrolling.
//!
//! [`compose_reanchor`] and [`arm_reanchor_on_anim_frame`] consolidate the
//! `Widget::compose`/`on_anim_frame` bodies that drive that loop — every
//! portal-capable widget's `compose`/`on_anim_frame` is a one-line call into
//! these; only the `Hosting` match to extract `Option<&mut PortalBinding>`
//! stays per-component, since each widget's `Hosting` enum carries different
//! non-portal fields.
//!
//! Masonry's context types (`EventCtx`, `ActionCtx`, `UpdateCtx`,
//! `MutateCtx`, `ComposeCtx`) expose the needed methods as *separate
//! inherent impls*, not a shared trait — which is why each component grew a
//! per-ctx macro. [`PortalCtx`]/[`PortalOpenCtx`] solve that once: tiny shim
//! traits implemented for exactly the ctx types that have the underlying
//! methods. `ComposeCtx` implements only [`PortalCtx`] because masonry does
//! not provide `request_compose`/`request_anim_frame` on it (see the
//! `impl_context_method!` groups in `masonry_core/src/core/contexts.rs`) —
//! and `compose` never opens, only re-anchors, so it doesn't need them.
//!
//! Error handling is uniform: an unfilled [`OverlayScopeHandle`] (the scope
//! widget not yet mounted) makes every method a silent no-op. Callers that
//! must not flip their own `open` flag in that state check
//! [`PortalBinding::is_ready`] first (see `AutocompleteWidget::open_on_focus`).

use masonry::core::{
    ActionCtx, ComposeCtx, EventCtx, MutateCtx, UpdateCtx, Widget, WidgetId, WidgetMut,
};
use masonry::kurbo::{Point, Rect, Size};

use crate::overlay::OverlayAnchor;
use crate::overlay_portal::{DismissHook, PortalOwner, PortalVisibility};
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// The subset of masonry context capabilities the portal push needs, shimmed
/// over the ctx types' inherent methods (they share no trait upstream).
pub(crate) trait PortalCtx {
    /// The current widget's id (`ctx.widget_id()`).
    fn host_widget_id(&self) -> WidgetId;
    /// The current widget's border box in window coordinates —
    /// `Rect::from_origin_size(ctx.to_window(Point::ORIGIN), ctx.border_box().size())`.
    fn host_anchor_rect_window(&self) -> Rect;
    /// `ctx.mutate_later(target, f)`.
    fn queue_mutate(
        &mut self,
        target: WidgetId,
        f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static,
    );
}

/// Contexts that can also arm the open-popup re-anchor loop
/// (`request_compose` + `request_anim_frame`). `ComposeCtx` deliberately
/// does not implement this — masonry omits those methods on it.
pub(crate) trait PortalOpenCtx: PortalCtx {
    /// `ctx.request_compose(); ctx.request_anim_frame();`
    fn arm_reanchor_loop(&mut self);
}

macro_rules! impl_portal_ctx {
    ($($ctx:ty),+ $(,)?) => {$(
        impl PortalCtx for $ctx {
            fn host_widget_id(&self) -> WidgetId {
                self.widget_id()
            }

            fn host_anchor_rect_window(&self) -> Rect {
                Rect::from_origin_size(self.to_window(Point::ORIGIN), self.border_box().size())
            }

            fn queue_mutate(
                &mut self,
                target: WidgetId,
                f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static,
            ) {
                self.mutate_later(target, f);
            }
        }
    )+};
}

impl_portal_ctx!(
    EventCtx<'_>,
    ActionCtx<'_>,
    UpdateCtx<'_>,
    MutateCtx<'_>,
    ComposeCtx<'_>,
);

macro_rules! impl_portal_open_ctx {
    ($($ctx:ty),+ $(,)?) => {$(
        impl PortalOpenCtx for $ctx {
            fn arm_reanchor_loop(&mut self) {
                self.request_compose();
                self.request_anim_frame();
            }
        }
    )+};
}

impl_portal_open_ctx!(EventCtx<'_>, ActionCtx<'_>, UpdateCtx<'_>, MutateCtx<'_>);

/// One component's live connection to its portal-mounted content: the scope
/// handle, the portal key, the dismiss hook to register on open, the last
/// pushed window-space anchor rect (so `reanchor` is a cheap no-op while
/// nothing moved), and the last pushed anchor/gap (so `close` re-pushes the
/// real values instead of inventing a default — see [`Self::close`]).
pub(crate) struct PortalBinding {
    scope: OverlayScopeHandle,
    key: u64,
    on_dismiss: DismissHook,
    last_anchor_rect_window: Option<Rect>,
    last_anchor: OverlayAnchor,
    last_gap: f64,
}

impl PortalBinding {
    #[must_use]
    pub(crate) fn new(scope: OverlayScopeHandle, key: u64, on_dismiss: DismissHook) -> Self {
        Self {
            scope,
            key,
            on_dismiss,
            last_anchor_rect_window: None,
            last_anchor: OverlayAnchor::default(),
            last_gap: 0.0,
        }
    }

    /// The portal key, for component-specific slot-child pushes (menu
    /// highlight, suggestion items, theme) that go beyond visibility.
    #[must_use]
    pub(crate) fn key(&self) -> u64 {
        self.key
    }

    /// Repoint this binding at a different registered entry — needed when a
    /// caller deregisters and re-registers under a fresh key instead of
    /// updating the existing entry in place (autocomplete does this on
    /// reopen; see `AutocompleteView::rebuild`'s doc comment). Without this,
    /// `open`/`close`/`reanchor` would keep pushing visibility for the old,
    /// now-deregistered key — a silent no-op (`PortalSlot::set_visible`
    /// early-returns on an unknown key), so the dropdown would never
    /// actually show. Callers that switch keys must re-push visibility
    /// afterward (e.g. call `open` again) since this alone doesn't.
    pub(crate) fn set_key(&mut self, key: u64) {
        self.key = key;
    }

    /// Whether the scope widget is mounted (its handle filled). All methods
    /// silently no-op when it isn't; callers that must not flip their own
    /// `open` flag in that state check this first.
    #[must_use]
    pub(crate) fn is_ready(&self) -> bool {
        self.scope.widget_id().is_some()
    }

    /// The scope's widget id, for component-specific `mutate_later` pushes.
    #[must_use]
    pub(crate) fn scope_widget_id(&self) -> Option<WidgetId> {
        self.scope.widget_id()
    }

    /// Shared body of [`Self::open`], [`Self::refresh`], and
    /// [`Self::open_at_point`]: queue the "visible" push with an explicit
    /// window-space anchor rect. Returns whether it was queued (`false` when
    /// the scope isn't mounted yet), so `open`/`open_at_point` know whether
    /// to also arm the re-anchor loop.
    fn push_visible_rect(
        &mut self,
        ctx: &mut impl PortalCtx,
        rect: Rect,
        anchor: OverlayAnchor,
        gap: f64,
    ) -> bool {
        let Some(scope_id) = self.scope.widget_id() else {
            return false;
        };
        self.last_anchor_rect_window = Some(rect);
        self.last_anchor = anchor;
        self.last_gap = gap;
        let key = self.key;
        let owner = PortalOwner {
            id: ctx.host_widget_id(),
            on_dismiss: self.on_dismiss,
        };
        ctx.queue_mutate(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_visible(
                &mut scope,
                key,
                true,
                PortalVisibility {
                    owner: Some(owner),
                    rect,
                    anchor,
                    gap,
                },
            );
        });
        true
    }

    /// Push "visible" with the host's current window-space anchor rect —
    /// derives the rect from `ctx` (the trigger widget's own border box);
    /// see [`Self::push_visible_rect`] for the cursor-anchored equivalent
    /// [`Self::open_at_point`] uses instead.
    fn push_visible(&mut self, ctx: &mut impl PortalCtx, anchor: OverlayAnchor, gap: f64) -> bool {
        let rect = if anchor.has_trigger() {
            ctx.host_anchor_rect_window()
        } else {
            Rect::ZERO
        };
        self.push_visible_rect(ctx, rect, anchor, gap)
    }

    /// Push "visible" with the host's current window-space anchor rect, and
    /// arm the per-frame re-anchor loop. Anchors without a trigger (see
    /// [`OverlayAnchor::has_trigger`]; currently just `ViewportQuarter`,
    /// i.e. dialogs) get [`Rect::ZERO`] instead (ignored by
    /// `PortalSlot::layout` for that variant, and computing real geometry
    /// could run pre-layout from `Update::WidgetAdded`) and no loop is armed
    /// (a centered dialog doesn't track a scrolling trigger).
    pub(crate) fn open(&mut self, ctx: &mut impl PortalOpenCtx, anchor: OverlayAnchor, gap: f64) {
        if self.push_visible(ctx, anchor, gap) && anchor.has_trigger() {
            ctx.arm_reanchor_loop();
        }
    }

    /// Open at a fixed window-space point (e.g. a right-click's cursor
    /// location) instead of tracking a host widget's box. Never arms the
    /// re-anchor loop: the point is captured once and — unlike a trigger
    /// widget's box, which can move as an ancestor scrolls — never needs
    /// re-deriving (see
    /// `docs/superpowers/specs/2026-07-17-context-menu-portal-zorder-design.md`,
    /// Decision 2). `OverlayAnchor::BottomStart` on a zero-size rect resolves
    /// `child_offset` to exactly `window_point` before `PortalSlot::layout`'s
    /// viewport clamp shifts it back on-screen if needed.
    pub(crate) fn open_at_point(&mut self, ctx: &mut impl PortalCtx, window_point: Point) {
        self.push_visible_rect(
            ctx,
            Rect::from_origin_size(window_point, Size::ZERO),
            OverlayAnchor::BottomStart,
            0.0,
        );
    }

    /// Re-push the visibility payload for an already-open child whose
    /// anchor/gap changed (e.g. a theme swap) — same as `open` but without
    /// arming the re-anchor loop, since a caller using this is by
    /// definition already open and already keeping that loop running
    /// (see popover's `on_anim_frame` self-perpetuation).
    pub(crate) fn refresh(&mut self, ctx: &mut impl PortalCtx, anchor: OverlayAnchor, gap: f64) {
        self.push_visible(ctx, anchor, gap);
    }

    /// Push "hidden" (the canonical close sentinel — owner cleared, rect
    /// zeroed; those fields are unread while hidden and every `open`
    /// re-pushes them). Anchor/gap carry the last real values pushed by
    /// `open`/`refresh` rather than an invented default — today nothing
    /// reads them while hidden, but a hardcoded `BottomStart` would be
    /// silently wrong for dialogs (`ViewportQuarter`) the moment that
    /// changes.
    pub(crate) fn close(&mut self, ctx: &mut impl PortalCtx) {
        let Some(scope_id) = self.scope.widget_id() else {
            return;
        };
        self.last_anchor_rect_window = None;
        let key = self.key;
        let anchor = self.last_anchor;
        let gap = self.last_gap;
        ctx.queue_mutate(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_visible(
                &mut scope,
                key,
                false,
                PortalVisibility {
                    owner: None,
                    rect: Rect::ZERO,
                    anchor,
                    gap,
                },
            );
        });
    }

    /// Re-anchor a visible child as the host moves in window space (called
    /// from `compose`, driven by the loop `open` armed). No-op while the
    /// rect is unchanged from the last push.
    pub(crate) fn reanchor(&mut self, ctx: &mut impl PortalCtx) {
        let Some(scope_id) = self.scope.widget_id() else {
            return;
        };
        let rect = ctx.host_anchor_rect_window();
        if self.last_anchor_rect_window == Some(rect) {
            return;
        }
        self.last_anchor_rect_window = Some(rect);
        let key = self.key;
        ctx.queue_mutate(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_placement(&mut scope, key, rect);
        });
    }
}

/// Shared `Widget::compose` body for portal-capable widgets (`popover`,
/// `dropdown_button`, `autocomplete`): re-anchor a still-open portal child.
/// `binding` is `Some` only when the widget is in portal-hosting mode; each
/// caller extracts it from its own `Hosting` enum (which also carries
/// component-specific fields `PortalBinding` doesn't know about).
pub(crate) fn compose_reanchor(
    ctx: &mut ComposeCtx<'_>,
    open: bool,
    binding: Option<&mut PortalBinding>,
) {
    if !open {
        return;
    }
    if let Some(binding) = binding {
        binding.reanchor(ctx);
    }
}

/// Shared `Widget::on_anim_frame` body for portal-capable widgets: keeps
/// `compose` running every frame while a portal child is open, so it
/// re-anchors regardless of pointer position or which ancestor scrolled.
///
/// This is a deliberate busy-poll, not an oversight: masonry's compose pass
/// only calls a widget's `compose` if that widget already requested it, and
/// there's no `Update` variant or timer API in the pinned masonry version
/// that notifies an arbitrary descendant when an unrelated ancestor
/// scrolls. Without that upstream hook, per-frame polling while open is the
/// only way to catch "some ancestor scrolled" regardless of which one.
/// Revisit this once masonry exposes a scroll-changed notification or a
/// timer primitive that isn't tied to the display's refresh rate.
pub(crate) fn arm_reanchor_on_anim_frame(ctx: &mut UpdateCtx<'_>, open: bool, is_portal: bool) {
    if !open || !is_portal {
        return;
    }
    ctx.request_compose();
    ctx.request_anim_frame();
}
