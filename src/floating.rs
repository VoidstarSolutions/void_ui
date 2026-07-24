//! `FloatingOverlay` widget + `floating` xilem view: pointer-event-
//! transparent overlay that positions a single child at a configurable
//! `Option<Point>` within its own bounds, stashing the child when the
//! position is `None`.
//!
//! Used for cursor-anchored inspector pills, snap-popovers, and similar
//! contextual chrome whose position is driven by interactive state
//! rather than parent layout. Like [`crate::pointer_inert::PointerInert`],
//! the overlay returns `false` from both pointer-interaction hooks so
//! it never absorbs clicks meant for the chart underneath in a `ZStack`.

use std::marker::PhantomData;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

/// Masonry widget that hosts a child and positions it at a configurable
/// point within its own bounds. The widget is pointer-inert (same
/// rationale as [`crate::pointer_inert::PointerInert`]) so it never
/// absorbs clicks meant for the chart underneath in a `ZStack`.
///
/// When `position` is `None` the child is *stashed* — Masonry skips
/// painting it and omits it from the accessibility tree. When `position`
/// is `Some(p)`, the child is placed at `p` clamped so it stays fully
/// inside the overlay's content box. Use this for cursor-anchored
/// inspector pills, snap-popovers, and similar contextual chrome whose
/// position is driven by interactive state rather than parent layout.
pub struct FloatingOverlay {
    inner: WidgetPod<dyn Widget>,
    /// Anchor point in this overlay's local coordinates. `None` hides
    /// the child by stashing it.
    position: Option<Point>,
    /// When `true`, the overlay propagates pointer events into the
    /// child so wrapped buttons / inputs receive clicks; the overlay
    /// itself still doesn't accept hits, so the area *outside* the
    /// positioned child stays transparent to underlying siblings.
    /// Default `false` — purely-visual decorations (e.g. inspector
    /// pills) opt out so they never intercept anything.
    interactive: bool,
}

impl FloatingOverlay {
    /// Create a `FloatingOverlay` wrapping `child` at the given `position`
    /// (or hidden when `position` is `None`). Pointer-transparent by
    /// default; use [`Self::new_interactive`] / pass `interactive: true`
    /// through the view constructor when wrapping an interactive child.
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, position: Option<Point>) -> Self {
        Self {
            inner: child.erased().to_pod(),
            position,
            interactive: false,
        }
    }

    /// Construct an interactive floating overlay — pointer events that
    /// land on the child propagate through, while the rest of the
    /// overlay's bounds stay transparent.
    #[must_use]
    pub fn new_interactive(
        child: NewWidget<impl Widget + ?Sized>,
        position: Option<Point>,
    ) -> Self {
        Self {
            inner: child.erased().to_pod(),
            position,
            interactive: true,
        }
    }

    /// Update the anchor position of the floating child. Triggers a
    /// layout pass when the value changes.
    pub fn set_position(this: &mut WidgetMut<'_, Self>, position: Option<Point>) {
        if this.widget.position == position {
            return;
        }
        this.widget.position = position;
        this.ctx.request_layout();
    }

    /// Mutable access to the wrapped child for the [`View`] layer.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}

fn clamp_inside(pos: Point, child: Size, container: Size) -> Point {
    let max_x = (container.width - child.width).max(0.0);
    let max_y = (container.height - child.height).max(0.0);
    Point::new(pos.x.clamp(0.0, max_x), pos.y.clamp(0.0, max_y))
}

impl Widget for FloatingOverlay {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        // The overlay itself never absorbs hits — the positioned child
        // does (when in interactive mode). Hits outside the child's
        // bounds fall through to siblings below in the ZStack.
        false
    }

    fn propagates_pointer_interaction(&self) -> bool {
        // Interactive overlays forward pointer events to their child
        // so wrapped buttons / inputs receive clicks. Decorative
        // overlays (e.g. inspector pills) skip propagation so the
        // child can't intercept hits aimed at siblings below.
        self.interactive
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // The overlay's footprint is dictated by its parent (typically
        // `Dimensions::STRETCH` inside a ZStack), not its child — we
        // want to fill the chart so any anchor point inside the chart
        // is addressable. Report the constrained extent for either
        // axis without consulting the child.
        match len_req {
            LenReq::FitContent(space) => space,
            LenReq::MinContent => Length::ZERO,
            // `Length` can no longer represent infinity (always finite,
            // per masonry's invariant); pick the largest finite value
            // to preserve the old "take whatever the parent gives me"
            // semantics.
            LenReq::MaxContent => Length::px(f64::MAX),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if let Some(pos) = self.position {
            ctx.set_stashed(&mut self.inner, false);
            // Resolve the child's preferred *content* size, not its
            // fit-to-available size. `SizeDef::fit(size)` would cause
            // flex containers to use up all offered space (per their
            // measure implementation: `FitContent(space)` becomes
            // `MinContent` with `min_result = space`), which would
            // make a `SizedBox` with `Background` cover the entire
            // overlay. `SizeDef::MIN` keeps the child snug to its
            // intrinsic content extent so the pill draws as a pill.
            let child_size =
                ctx.compute_size(&mut self.inner, SizeDef::MIN, LayoutSize::from(size));
            ctx.run_layout(&mut self.inner, child_size);
            let placed = clamp_inside(pos, child_size, size);
            ctx.place_child(&mut self.inner, placed);
        } else {
            // Stashed children are not painted, not surfaced to
            // accessibility, and skipped by the layout pass. Per
            // Masonry's layout contract the parent must NOT call
            // `run_layout` or `place_child` on a stashed child — the
            // pass handles them via `recurse_on_children` once they
            // are flagged as `is_explicitly_stashed`.
            ctx.set_stashed(&mut self.inner, true);
        }
    }

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
        ChildrenIds::from_slice(&[self.inner.id()])
    }
}

/// Construct a [`FloatingOverlayView`] that anchors `content` at
/// `position` (in the overlay's local coordinates). When `position` is
/// `None`, the content is hidden.
///
/// The overlay's bounds are dictated by its parent — typically the
/// `ZStack` it lives in — so its coordinate space is shared with sibling
/// stack children. That means a position computed in another sibling's
/// coordinates (e.g., a chart's cell centre) anchors here without any
/// extra translation, as long as both children are stretched to fill
/// the stack.
pub fn floating<State, Action, V>(
    content: V,
    position: Option<Point>,
) -> FloatingOverlayView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    FloatingOverlayView {
        content,
        position,
        interactive: false,
        phantom: PhantomData,
    }
}

/// Same as [`floating`] but the overlay forwards pointer events to
/// the wrapped child so interactive widgets (buttons, text inputs)
/// inside the overlay receive clicks. Area outside the child's hit
/// box still falls through to siblings below in the `ZStack`.
pub fn interactive_floating<State, Action, V>(
    content: V,
    position: Option<Point>,
) -> FloatingOverlayView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    FloatingOverlayView {
        content,
        position,
        interactive: true,
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct FloatingOverlayView<V, State, Action> {
    content: V,
    position: Option<Point>,
    interactive: bool,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for FloatingOverlayView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for FloatingOverlayView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<FloatingOverlay>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.content.build(ctx, app_state);
        let widget = if self.interactive {
            FloatingOverlay::new_interactive(child.new_widget.erased(), self.position)
        } else {
            FloatingOverlay::new(child.new_widget.erased(), self.position)
        };
        (ctx.create_pod(widget), child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.position != prev.position {
            FloatingOverlay::set_position(&mut element, self.position);
        }
        let mut child = FloatingOverlay::child_mut(&mut element);
        self.content
            .rebuild(&prev.content, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = FloatingOverlay::child_mut(&mut element);
        self.content.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut child = FloatingOverlay::child_mut(&mut element);
        self.content
            .message(view_state, message, child.downcast(), app_state)
    }
}
