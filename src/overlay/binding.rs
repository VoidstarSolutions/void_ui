//! `PortalBinding` — the one implementation of the portal open/close/reanchor
//! push that every portal-hosted component (`popover`, `dropdown_button`,
//! `autocomplete`, `dialog`) previously hand-copied: resolve the scope's
//! widget id → compute the trigger's window-space anchor rect → stash it for
//! `compose`-time re-anchoring → `mutate_later` into the scope →
//! `OverlayScope::set_portal_visible` → (on open) arm the per-frame
//! `compose` loop that tracks ancestor scrolling.
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
use masonry::kurbo::{Point, Rect};

use crate::overlay::OverlayAnchor;
use crate::overlay_portal::{DismissHook, PortalOwner, PortalVisibility};
use crate::overlay_scope::{OverlayScope, OverlayScopeHandle};

/// The subset of masonry context capabilities the portal push needs, shimmed
/// over the ctx types' inherent methods (they share no trait upstream).
pub(crate) trait PortalCtx {
    /// The current widget's id (`ctx.widget_id()`).
    fn host_widget_id(&self) -> WidgetId;
    /// The current widget's border box in window coordinates —
    /// `Rect::from_origin_size(ctx.to_window(Point::ORIGIN), ctx.border_box_size())`.
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
                Rect::from_origin_size(self.to_window(Point::ORIGIN), self.border_box_size())
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
/// handle, the portal key, the dismiss hook to register on open, and the
/// last pushed window-space anchor rect (so `reanchor` is a cheap no-op
/// while nothing moved).
pub(crate) struct PortalBinding {
    scope: OverlayScopeHandle,
    key: u64,
    on_dismiss: DismissHook,
    last_anchor_rect_window: Option<Rect>,
}

impl PortalBinding {
    #[must_use]
    pub(crate) fn new(scope: OverlayScopeHandle, key: u64, on_dismiss: DismissHook) -> Self {
        Self {
            scope,
            key,
            on_dismiss,
            last_anchor_rect_window: None,
        }
    }

    /// The portal key, for component-specific slot-child pushes (menu
    /// highlight, suggestion items, theme) that go beyond visibility.
    #[must_use]
    pub(crate) fn key(&self) -> u64 {
        self.key
    }

    /// Whether the scope widget is mounted (its handle filled). All methods
    /// silently no-op when it isn't; callers that must not flip their own
    /// `open` flag in that state check this first.
    ///
    /// Unused until the next consumer (`autocomplete`/`dialog`) is ported
    /// onto `PortalBinding`.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn is_ready(&self) -> bool {
        self.scope.widget_id().is_some()
    }

    /// The scope's widget id, for component-specific `mutate_later` pushes.
    #[must_use]
    pub(crate) fn scope_widget_id(&self) -> Option<WidgetId> {
        self.scope.widget_id()
    }

    /// Push "visible" with the host's current window-space anchor rect, and
    /// arm the per-frame re-anchor loop. [`OverlayAnchor::ViewportQuarter`]
    /// (dialogs) has no trigger: the rect is [`Rect::ZERO`] (ignored by
    /// `PortalSlot::layout` for that variant, and computing real geometry
    /// could run pre-layout from `Update::WidgetAdded`) and no loop is armed
    /// (a centered dialog doesn't track a scrolling trigger).
    pub(crate) fn open(&mut self, ctx: &mut impl PortalOpenCtx, anchor: OverlayAnchor, gap: f64) {
        let Some(scope_id) = self.scope.widget_id() else {
            return;
        };
        let rect = if anchor == OverlayAnchor::ViewportQuarter {
            Rect::ZERO
        } else {
            ctx.host_anchor_rect_window()
        };
        self.last_anchor_rect_window = Some(rect);
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
        if anchor != OverlayAnchor::ViewportQuarter {
            ctx.arm_reanchor_loop();
        }
    }

    /// Push "hidden" (the canonical close sentinel — owner cleared, rect
    /// zeroed; those fields are unread while hidden and every `open`
    /// re-pushes them).
    pub(crate) fn close(&mut self, ctx: &mut impl PortalCtx) {
        let Some(scope_id) = self.scope.widget_id() else {
            return;
        };
        self.last_anchor_rect_window = None;
        let key = self.key;
        ctx.queue_mutate(scope_id, move |mut w| {
            let mut scope = w.downcast::<OverlayScope>();
            OverlayScope::set_portal_visible(
                &mut scope,
                key,
                false,
                PortalVisibility {
                    owner: None,
                    rect: Rect::ZERO,
                    anchor: OverlayAnchor::BottomStart,
                    gap: 0.0,
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
