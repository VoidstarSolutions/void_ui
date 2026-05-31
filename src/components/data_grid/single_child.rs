//! Shared plumbing for the grid's single-child pass-through widgets.
//!
//! [`RowClickable`](super::row_click::RowClickable),
//! [`HeaderClickable`](super::header_click::HeaderClickable), and
//! [`CopyOnShortcut`](super::copy_shortcut::CopyOnShortcut) each wrap one
//! child and exist only to intercept an event (a pointer click, a copy
//! shortcut) — their measure / layout / child-registration are *identical*
//! transparent delegations to that child. Rather than copy those four
//! [`Widget`] method bodies into all three (and into any future wrapper),
//! they live here as free helpers; each widget's trait method becomes a
//! one-line call.
//!
//! These are deliberately free functions, not a base widget or a macro:
//! there is no extra widget in the tree (so `RowClickable` stays flat on
//! the per-row hot path), and every call site reads literally as
//! "delegate to child" with no hidden control flow.
//!
//! Scope is the *single*-child wrappers only.
//! [`ColumnStrip`](super::column_strip::ColumnStrip) is a multi-child
//! widget with authoritative-x layout, so it shares none of this and is
//! intentionally left out.

use masonry::core::{ChildrenIds, LayoutCtx, MeasureCtx, RegisterCtx, Widget, WidgetPod};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};

/// Transparent [`Widget::measure`]: report exactly the child's measurement
/// along `axis` (the wrapper adds no size of its own).
pub(super) fn measure(
    ctx: &mut MeasureCtx<'_>,
    child: &mut WidgetPod<dyn Widget>,
    axis: Axis,
    len_req: LenReq,
    cross_length: Option<Length>,
) -> Length {
    let auto_length = len_req.into();
    ctx.compute_length(
        child,
        auto_length,
        LayoutSize::maybe(axis.cross(), cross_length),
        axis,
        cross_length,
    )
}

/// Transparent [`Widget::layout`]: size the child to fill the wrapper,
/// place it at the origin, and inherit its baselines.
pub(super) fn layout(ctx: &mut LayoutCtx<'_>, child: &mut WidgetPod<dyn Widget>, size: Size) {
    let child_size = ctx.compute_size(child, SizeDef::fixed(size), size.into());
    ctx.run_layout(child, child_size);
    ctx.place_child(child, Point::ORIGIN);
    ctx.derive_baselines(child);
}

/// [`Widget::register_children`] for a lone child.
pub(super) fn register_children(ctx: &mut RegisterCtx<'_>, child: &mut WidgetPod<dyn Widget>) {
    ctx.register_child(child);
}

/// [`Widget::children_ids`] for a lone child.
pub(super) fn children_ids(child: &WidgetPod<dyn Widget>) -> ChildrenIds {
    ChildrenIds::from_slice(&[child.id()])
}
