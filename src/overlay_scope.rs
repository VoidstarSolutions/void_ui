//! `OverlayScope` widget + `overlay_scope` xilem view: a container that hosts
//! arbitrary `content` plus a discoverable, always-on-top, always-clipped
//! "overlay slot" that deeply-nested descendants can push popups into.
//!
//! Masonry's paint pass is strict depth-first by `children_ids()`
//! registration order, and `set_clip_path` wraps a widget's own paint *and*
//! all its children's recursive paint. `OverlayScope` exploits both facts:
//! it registers `content` first and the overlay slot last (so the slot
//! always paints on top of `content` and everything inside it — including
//! later in-scope siblings), and clips both to its own border box (so the
//! overlay never escapes the container, unlike a window-level `Layer`).
//!
//! Descendants discover the nearest `OverlayScope` ancestor (if any) via the
//! Xilem `Environment` — [`OverlayScopeHandle`] is published with `provides`
//! and read with [`xilem_masonry::core::Environment::get_slot_for_type`] —
//! and push popup content into it with `ctx.mutate_later(scope_id, ...)` plus
//! [`OverlayScope::set_overlay`]. See [`crate::components::dropdown_button`]
//! for the reference consumer (which falls back to [`crate::AnchoredOverlay`]
//! when no scope ancestor exists).
//!
//! A permanent [`crate::overlay_portal::PortalSlot`] child is registered last
//! so that view-level portal content also paints above everything in the scope.

use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, Resource, View, ViewMarker, provides};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use crate::components::popover::PopoverAnchor;
use crate::overlay_portal::PortalSlot;

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

/// Masonry widget that hosts a footprint-dictating `content` child plus an
/// optional, always-last-painted, always-clipped overlay popup.
///
/// `content` alone determines the container's measured size — pushing or
/// clearing the overlay never reflows surrounding layout (mirrors
/// [`crate::AnchoredOverlay::measure`]). The overlay, when present, is
/// anchored relative to a caller-supplied `placement` rect (in this widget's
/// own local coordinates — see [`Self::set_overlay`]) using the same
/// unclamped `PopoverAnchor::child_offset` math as `AnchoredOverlay`.
pub struct OverlayScope {
    handle: OverlayScopeHandle,
    content: WidgetPod<dyn Widget>,
    overlay: Option<WidgetPod<dyn Widget>>,
    portal_slot: WidgetPod<PortalSlot>,
    placement: Rect,
    anchor: PopoverAnchor,
    /// Overlay's placed rect in local coordinates (set during `layout`,
    /// meaningful only while `overlay.is_some()`).
    placed_overlay_rect: Rect,
}

impl OverlayScope {
    pub(crate) fn new(
        handle: OverlayScopeHandle,
        content: NewWidget<dyn Widget>,
        portal_children: Vec<(u64, NewWidget<dyn Widget>)>,
    ) -> Self {
        Self {
            handle,
            content: content.to_pod(),
            overlay: None,
            portal_slot: NewWidget::new(PortalSlot::new(portal_children)).to_pod(),
            placement: Rect::ZERO,
            anchor: PopoverAnchor::BottomStart,
            placed_overlay_rect: Rect::ZERO,
        }
    }

    /// Replace the overlay popup (or clear it when `content` is `None`).
    ///
    /// `placement` is the trigger's anchor rect in *this scope's own local
    /// (content-box) coordinates* — convert from window space with
    /// `ctx.to_local`, exactly as [`crate::components::dropdown_button`]'s
    /// scope-mode path does. `anchor` controls which side of `placement` the
    /// overlay is positioned relative to (see [`PopoverAnchor`]).
    ///
    /// Triggers a layout pass. Replacing an existing overlay removes the old
    /// one first — the slot holds at most one popup at a time; pushing a new
    /// one implicitly dismisses whatever was there (e.g. opening dropdown B
    /// while A's menu occupies the slot silently clears A).
    pub fn set_overlay(
        this: &mut WidgetMut<'_, Self>,
        content: Option<NewWidget<dyn Widget>>,
        placement: Rect,
        anchor: PopoverAnchor,
    ) {
        if let Some(old) = this.widget.overlay.take() {
            this.ctx.remove_child(old);
        }
        if let Some(content) = content {
            this.widget.overlay = Some(content.to_pod());
        }
        this.widget.placement = placement;
        this.widget.anchor = anchor;
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Update the overlay's anchor placement without replacing its content —
    /// used to re-anchor a still-open popup as its trigger scrolls/moves.
    pub fn set_placement(this: &mut WidgetMut<'_, Self>, placement: Rect, anchor: PopoverAnchor) {
        if this.widget.placement == placement && this.widget.anchor == anchor {
            return;
        }
        this.widget.placement = placement;
        this.widget.anchor = anchor;
        this.ctx.request_layout();
    }

    /// Mutable access to the wrapped content for the [`View`] layer.
    pub fn content_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.content)
    }

    /// Mutable access to the overlay popup, if one is currently present —
    /// used by triggers (e.g. `ThemedDropdownButton`) to push live state
    /// updates (like keyboard-highlight) into their menu while it's open.
    pub fn overlay_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> Option<WidgetMut<'t, dyn Widget>> {
        this.widget.overlay.as_mut().map(|o| this.ctx.get_mut(o))
    }

    /// The overlay's last-placed rect in local coordinates, valid while an
    /// overlay is present.
    #[must_use]
    pub fn placed_overlay_rect(&self) -> Rect {
        self.placed_overlay_rect
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
    /// scope and the trigger).
    pub fn set_portal_visible(
        this: &mut WidgetMut<'_, Self>,
        key: u64,
        visible: bool,
        owner: Option<WidgetId>,
        anchor_rect_window: Rect,
        anchor: PopoverAnchor,
        gap: f64,
    ) {
        let local_origin = this.ctx.to_local(anchor_rect_window.origin());
        let placement = Rect::from_origin_size(local_origin, anchor_rect_window.size());
        let mut slot = Self::portal_slot_mut(this);
        PortalSlot::set_visible(&mut slot, key, visible, owner, placement, anchor, gap);
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

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        // Registration order is paint order: content first, overlay last —
        // the overlay always draws on top of content and everything inside
        // it, including later siblings within the scope. This ordering *is*
        // the entire mechanism; nothing else makes the overlay "win".
        // The portal_slot is registered last so view-level portal content
        // also paints above everything in the scope.
        ctx.register_child(&mut self.content);
        if let Some(overlay) = &mut self.overlay {
            ctx.register_child(overlay);
        }
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
        // Ignore the overlay entirely for sizing — see `AnchoredOverlay::measure`
        // for the rationale (a transparent forward, not `redirect_measurement`,
        // which under-measures and causes flex-sibling overlap). Guarantees
        // pushing/clearing the overlay never reflows the scope's container.
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

        // The clip that confines the overlay to this scope's own bounds —
        // the other half of the mechanism alongside registration order.
        ctx.set_clip_path(size.to_rect());

        if let Some(overlay) = &mut self.overlay {
            // Snug to intrinsic content size — see `AnchoredOverlay::layout`
            // for why `SizeDef::MIN` rather than fit-to-available.
            let overlay_size = ctx.compute_size(overlay, SizeDef::MIN, size.into());
            ctx.run_layout(overlay, overlay_size);
            // No bounds enforcement — the overlay may extend past `placement`
            // or even this scope's own border box; `set_clip_path` above
            // handles confinement regardless. Mirrors `AnchoredOverlay::layout`.
            let offset = self
                .anchor
                .child_offset(self.placement.size(), overlay_size)
                + self.placement.origin().to_vec2();
            ctx.place_child(overlay, offset);
            self.placed_overlay_rect = Rect::from_origin_size(offset, overlay_size);
        } else {
            self.placed_overlay_rect = Rect::ZERO;
        }

        ctx.run_layout(&mut self.portal_slot, size);
        ctx.place_child(&mut self.portal_slot, Point::ORIGIN);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — both children paint themselves.
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
        match &self.overlay {
            Some(overlay) => {
                ChildrenIds::from_slice(&[self.content.id(), overlay.id(), self.portal_slot.id()])
            }
            None => ChildrenIds::from_slice(&[self.content.id(), self.portal_slot.id()]),
        }
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
    provides(
        move |_state: &mut State| resource_handle.clone(),
        OverlayScopeRootView {
            handle,
            content,
            phantom: PhantomData,
        },
    )
}

/// The `View` actually wrapped by `provides` in [`overlay_scope`] — builds
/// the [`OverlayScope`] widget around `content`'s pod, threading
/// `build`/`rebuild`/`teardown`/`message` through [`OverlayScope::content_mut`]
/// (single-child analogue of `AnchoredOverlayView`'s two-child threading).
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
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (content, content_state) = self.content.build(ctx, app_state);
        let widget =
            OverlayScope::new(self.handle.clone(), content.new_widget.erased(), Vec::new());
        (ctx.create_pod(widget), content_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let mut content = OverlayScope::content_mut(&mut element);
        self.content.rebuild(
            &prev.content,
            view_state,
            ctx,
            content.downcast(),
            app_state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut content = OverlayScope::content_mut(&mut element);
        self.content.teardown(view_state, ctx, content.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut content = OverlayScope::content_mut(&mut element);
        self.content
            .message(view_state, message, content.downcast(), app_state)
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::kurbo::Rect;
    use masonry::testing::TestHarness;

    use super::*;

    #[test]
    fn set_portal_visible_converts_window_rect_and_shows_the_slot_child() {
        let key = 3;
        let content = masonry::widgets::Label::new("content").prepare().erased();
        let popover = masonry::widgets::Label::new("popover").prepare().erased();
        let scope = OverlayScope::new(OverlayScopeHandle::new(), content, vec![(key, popover)]);
        let mut harness = TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(scope),
        );
        harness.edit_root_widget(|mut wm| {
            OverlayScope::set_portal_visible(
                &mut wm,
                key,
                true,
                None,
                Rect::new(10.0, 10.0, 110.0, 40.0),
                PopoverAnchor::BottomStart,
                4.0,
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
}
