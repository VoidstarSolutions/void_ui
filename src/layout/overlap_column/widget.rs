// Copyright 2025 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Masonry widget backing [`super::view::overlap_column`].
//!
//! Lays out exactly two children in a vertical stack — `front` above
//! `back`, separated by `gap` — but registers (and therefore paints) them
//! in the *opposite* order. Masonry's paint pass is strictly depth-first by
//! [`Widget::children_ids`] registration order (see the module doc atop
//! [`crate::overlay_scope`]), so this is the masonry-level equivalent of a
//! CSS `z-index`: `front` keeps its visual position at the top while still
//! painting — and hit-testing — on top of everything in `back`'s subtree,
//! including overflow that spills downward past `front`'s own footprint
//! (e.g. an open [`crate::AnchoredOverlay`]-hosted popover).

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};

/// Stacks `front` above `back` (vertically, full-width, with a gap) while
/// painting `front` — and any overflow from its subtree — on top of `back`.
///
/// `back` is registered first (painted/hit-tested underneath); `front` is
/// registered last (painted/hit-tested on top). Both children are stretched
/// to the container's width and measured at their natural height, mirroring
/// a two-item `flex_col` with `CrossAxisAlignment::Stretch`.
pub struct OverlapColumn {
    front: WidgetPod<dyn Widget>,
    back: WidgetPod<dyn Widget>,
    gap: Length,
}

impl OverlapColumn {
    pub(crate) fn new(
        front: NewWidget<impl Widget + ?Sized>,
        back: NewWidget<impl Widget + ?Sized>,
        gap: Length,
    ) -> Self {
        Self {
            front: front.erased().to_pod(),
            back: back.erased().to_pod(),
            gap,
        }
    }

    /// Change the spacing between `front` and `back`. Triggers a layout
    /// pass when changed.
    pub fn set_gap(this: &mut WidgetMut<'_, Self>, gap: Length) {
        if this.widget.gap == gap {
            return;
        }
        this.widget.gap = gap;
        this.ctx.request_layout();
    }

    /// Mutable access to the front (visually-top, paints-on-top) child for
    /// the [`View`](xilem_masonry::core::View) layer.
    pub fn front_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.front)
    }

    /// Mutable access to the back (visually-bottom, paints-underneath) child
    /// for the [`View`](xilem_masonry::core::View) layer.
    pub fn back_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.back)
    }
}

impl Widget for OverlapColumn {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        // Registration order is paint order — `back` first (underneath),
        // `front` last (on top). This ordering, alone, is the entire
        // mechanism; layout below places them in the opposite (visual) order.
        ctx.register_child(&mut self.back);
        ctx.register_child(&mut self.front);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let auto_length: LenDef = len_req.into();
        match axis {
            // Cross axis: both children are stretched to the same width, so
            // the container's width is whichever child wants to be wider.
            Axis::Horizontal => {
                let front_w = ctx.compute_length(
                    &mut self.front,
                    auto_length,
                    LayoutSize::maybe(Axis::Vertical, cross_length),
                    axis,
                    cross_length,
                );
                let back_w = ctx.compute_length(
                    &mut self.back,
                    auto_length,
                    LayoutSize::maybe(Axis::Vertical, cross_length),
                    axis,
                    cross_length,
                );
                front_w.max(back_w)
            }
            // Main axis: a vertical stack — combined height is both
            // children's natural heights (at the given width) plus the gap.
            Axis::Vertical => {
                let front_h = ctx.compute_length(
                    &mut self.front,
                    auto_length,
                    LayoutSize::maybe(Axis::Horizontal, cross_length),
                    axis,
                    cross_length,
                );
                let back_h = ctx.compute_length(
                    &mut self.back,
                    auto_length,
                    LayoutSize::maybe(Axis::Horizontal, cross_length),
                    axis,
                    cross_length,
                );
                Length::px(front_h.get() + self.gap.get() + back_h.get())
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // Both children stretch to the container's width and take their
        // natural height — the same shape as a two-item `flex_col` with
        // `CrossAxisAlignment::Stretch`.
        let stretch_width = SizeDef::new(LenDef::Fixed(Length::px(size.width)), LenDef::MaxContent);

        let front_size = ctx.compute_size(&mut self.front, stretch_width, size.into());
        ctx.run_layout(&mut self.front, front_size);
        ctx.place_child(&mut self.front, Point::ORIGIN);
        ctx.derive_baselines(&self.front);

        let back_size = ctx.compute_size(&mut self.back, stretch_width, size.into());
        ctx.run_layout(&mut self.back, back_size);
        ctx.place_child(
            &mut self.back,
            Point::new(0.0, front_size.height + self.gap.get()),
        );
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
        ChildrenIds::from_slice(&[self.back.id(), self.front.id()])
    }
}
