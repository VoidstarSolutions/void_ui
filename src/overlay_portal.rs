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
}
