//! `AnchoredOverlay` widget + `anchored_overlay` xilem view: hosts a
//! footprint-dictating `primary` child plus an `overlay` child anchored
//! relative to it that's free to overflow the container's own bounds.
//!
//! Unlike [`crate::floating::FloatingOverlay`] — which *clamps* a small
//! floating child inside a pre-sized container (e.g. a chart) — this
//! primitive is shaped for the inverse case: a small trigger that must
//! dictate the container's footprint (no reflow when the overlay shows or
//! hides) while the overlay itself is allowed to be *larger* than the
//! trigger and paint outside the container's box. Masonry doesn't clip
//! children's paint by default — only an ancestor that calls
//! `set_clip_path` does — so an overflowing overlay simply stays visible
//! (and hit-testable) until it reaches a real clipping ancestor (a scroll
//! viewport, a themed card). That's what gives "confined to whatever
//! container actually clips, not the whole window" for free.
//!
//! Use this for menus, popovers, comboboxes, context menus, date-pickers —
//! anything that pops a larger panel out of a small trigger without
//! reflowing surrounding layout or escaping its natural container.

use std::marker::PhantomData;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use crate::components::popover::PopoverAnchor;

/// Masonry widget that hosts a `primary` child (which alone dictates the
/// container's measured size) and an `overlay` child anchored relative to
/// it, shown/hidden via [`Self::set_overlay_visible`].
///
/// The overlay is stashed (skipped for layout/paint/accessibility) while
/// hidden, and — when visible — placed at an offset computed from `anchor`
/// and the two children's measured sizes, with no bounds clamping: it may
/// legally extend outside the container's own border box in any direction.
pub struct AnchoredOverlay {
    primary: WidgetPod<dyn Widget>,
    overlay: WidgetPod<dyn Widget>,
    overlay_visible: bool,
    anchor: PopoverAnchor,
    /// Overlay's placed rect in this widget's local coordinates (set during
    /// `layout`, valid only while `overlay_visible`). Exposed so a host
    /// widget that needs to route pointer events to the overlay (e.g. for
    /// outside-click dismissal) can hit-test against it without duplicating
    /// the anchor-offset math.
    placed_overlay_rect: Rect,
    /// Extra space between the primary's edge and the overlay, in the
    /// direction the overlay opens (down for `Bottom*` anchors, up for
    /// `Top*`). Zero by default; set via [`Self::with_gap`].
    gap: Length,
}

impl AnchoredOverlay {
    /// Create an `AnchoredOverlay` hosting `primary` (dictates the
    /// container's footprint) and `overlay` (anchored relative to it,
    /// initially hidden unless `overlay_visible` is `true`).
    #[must_use]
    pub fn new(
        primary: NewWidget<impl Widget + ?Sized>,
        overlay: NewWidget<impl Widget + ?Sized>,
        overlay_visible: bool,
        anchor: PopoverAnchor,
    ) -> Self {
        Self {
            primary: primary.erased().to_pod(),
            overlay: overlay.erased().to_pod(),
            overlay_visible,
            anchor,
            placed_overlay_rect: Rect::ZERO,
            gap: Length::ZERO,
        }
    }

    /// Set the gap between the primary's edge and the overlay, in the
    /// direction the overlay opens.
    #[must_use]
    pub fn with_gap(mut self, gap: Length) -> Self {
        self.gap = gap;
        self
    }

    /// Show or hide the overlay. Triggers a layout pass when changed.
    pub fn set_overlay_visible(this: &mut WidgetMut<'_, Self>, visible: bool) {
        if this.widget.overlay_visible == visible {
            return;
        }
        this.widget.overlay_visible = visible;
        this.ctx.request_layout();
    }

    /// Change where the overlay is anchored relative to the primary.
    /// Triggers a layout pass when changed.
    pub fn set_anchor(this: &mut WidgetMut<'_, Self>, anchor: PopoverAnchor) {
        if this.widget.anchor == anchor {
            return;
        }
        this.widget.anchor = anchor;
        this.ctx.request_layout();
    }

    /// Change the gap between the primary's edge and the overlay. Triggers a
    /// layout pass when changed.
    pub fn set_gap(this: &mut WidgetMut<'_, Self>, gap: Length) {
        if this.widget.gap == gap {
            return;
        }
        this.widget.gap = gap;
        this.ctx.request_layout();
    }

    /// Mutable access to the primary child for the [`View`] layer.
    pub fn primary_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.primary)
    }

    /// Mutable access to the overlay child for the [`View`] layer.
    pub fn overlay_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.overlay)
    }

    /// The overlay's last-placed rect in local coordinates, valid while the
    /// overlay is visible. Used by hosts that need to hit-test against it
    /// directly (e.g. to detect outside clicks for dismissal).
    #[must_use]
    pub fn placed_overlay_rect(&self) -> Rect {
        self.placed_overlay_rect
    }
}

impl Widget for AnchoredOverlay {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        // Registration order is paint order: primary first, overlay last —
        // the overlay always draws on top of the primary and anything else
        // within the same container.
        ctx.register_child(&mut self.primary);
        ctx.register_child(&mut self.overlay);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        // Ignore the overlay entirely for sizing — the container's footprint
        // is the primary's footprint, full stop. This is what guarantees
        // showing/hiding the overlay never reflows surrounding layout.
        //
        // A transparent forward of the received `len_req`/`cross_length` —
        // not `redirect_measurement` (which substitutes this measurement
        // pass's own `auto_length`/`context_size` and disables caching,
        // and was observed to produce a smaller measured height than the
        // primary's actual laid-out height, causing flex siblings to
        // overlap the button).
        ctx.compute_length(
            &mut self.primary,
            len_req.into(),
            LayoutSize::maybe(axis.cross(), cross_length),
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.primary, size);
        ctx.place_child(&mut self.primary, Point::ORIGIN);
        ctx.derive_baselines(&self.primary);

        if self.overlay_visible {
            ctx.set_stashed(&mut self.overlay, false);
            // Snug to intrinsic content size, not fit-to-available — a
            // flex/label-list child measured with `SizeDef::fit(size)`
            // would stretch to fill the *container's* size (the primary's
            // footprint), not its own content. See `FloatingOverlay::layout`
            // for the same rationale.
            let overlay_size = ctx.compute_size(&mut self.overlay, SizeDef::MIN, size.into());
            ctx.run_layout(&mut self.overlay, overlay_size);
            // No bounds enforcement on `place_child` — negative offsets
            // (Top* anchors) and offsets whose extent exceeds `size` are
            // legal. The overlay paints/hit-tests outside our border box
            // and is clipped only by a real `set_clip_path` ancestor.
            let offset = self.anchor.child_offset(size, overlay_size);
            let offset = match self.anchor {
                PopoverAnchor::BottomStart
                | PopoverAnchor::BottomCenter
                | PopoverAnchor::BottomEnd => Point::new(offset.x, offset.y + self.gap.get()),
                PopoverAnchor::TopStart | PopoverAnchor::TopCenter | PopoverAnchor::TopEnd => {
                    Point::new(offset.x, offset.y - self.gap.get())
                }
                // `AnchoredOverlay` is never used with `ViewportQuarter` —
                // that variant is only placed via `PortalSlot::layout`.
                PopoverAnchor::ViewportQuarter => offset,
            };
            ctx.place_child(&mut self.overlay, offset);
            self.placed_overlay_rect = Rect::from_origin_size(offset, overlay_size);
        } else {
            // Stashed children are skipped by layout/paint/accessibility;
            // the parent must not call `run_layout`/`place_child` on them.
            ctx.set_stashed(&mut self.overlay, true);
            self.placed_overlay_rect = Rect::ZERO;
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
        // Purely structural — both children paint themselves.
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
        ChildrenIds::from_slice(&[self.primary.id(), self.overlay.id()])
    }
}

/// Construct an [`AnchoredOverlayView`] hosting `primary` (dictates the
/// container's footprint) and `overlay` (shown when `visible`, anchored
/// relative to `primary` per `anchor`, free to overflow the container).
pub fn anchored_overlay<State, Action, P, O>(
    primary: P,
    overlay: O,
    visible: bool,
    anchor: PopoverAnchor,
) -> AnchoredOverlayView<P, O, State, Action>
where
    State: 'static,
    Action: 'static,
    P: WidgetView<State, Action>,
    O: WidgetView<State, Action>,
{
    AnchoredOverlayView {
        primary,
        overlay,
        visible,
        anchor,
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct AnchoredOverlayView<P, O, State, Action> {
    primary: P,
    overlay: O,
    visible: bool,
    anchor: PopoverAnchor,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<P, O, State, Action> ViewMarker for AnchoredOverlayView<P, O, State, Action> {}

impl<P, O, State, Action> View<State, Action, ViewCtx> for AnchoredOverlayView<P, O, State, Action>
where
    State: 'static,
    Action: 'static,
    P: WidgetView<State, Action>,
    O: WidgetView<State, Action>,
{
    type Element = Pod<AnchoredOverlay>;
    type ViewState = (P::ViewState, O::ViewState);

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (primary, primary_state) = self.primary.build(ctx, app_state);
        let (overlay, overlay_state) = self.overlay.build(ctx, app_state);
        let widget = AnchoredOverlay::new(
            primary.new_widget.erased(),
            overlay.new_widget.erased(),
            self.visible,
            self.anchor,
        );
        (ctx.create_pod(widget), (primary_state, overlay_state))
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.visible != prev.visible {
            AnchoredOverlay::set_overlay_visible(&mut element, self.visible);
        }
        if self.anchor != prev.anchor {
            AnchoredOverlay::set_anchor(&mut element, self.anchor);
        }
        let mut primary = AnchoredOverlay::primary_mut(&mut element);
        self.primary.rebuild(
            &prev.primary,
            &mut view_state.0,
            ctx,
            primary.downcast(),
            app_state,
        );
        drop(primary);
        let mut overlay = AnchoredOverlay::overlay_mut(&mut element);
        self.overlay.rebuild(
            &prev.overlay,
            &mut view_state.1,
            ctx,
            overlay.downcast(),
            app_state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut primary = AnchoredOverlay::primary_mut(&mut element);
        self.primary
            .teardown(&mut view_state.0, ctx, primary.downcast());
        drop(primary);
        let mut overlay = AnchoredOverlay::overlay_mut(&mut element);
        self.overlay
            .teardown(&mut view_state.1, ctx, overlay.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut primary = AnchoredOverlay::primary_mut(&mut element);
        let result =
            self.primary
                .message(&mut view_state.0, message, primary.downcast(), app_state);
        drop(primary);
        match result {
            MessageResult::Nop => {
                let mut overlay = AnchoredOverlay::overlay_mut(&mut element);
                self.overlay
                    .message(&mut view_state.1, message, overlay.downcast(), app_state)
            }
            other => other,
        }
    }
}
