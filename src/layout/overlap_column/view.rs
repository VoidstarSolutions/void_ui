// Copyright 2025 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Xilem `View` wrapper for [`super::widget::OverlapColumn`].
//!
//! Mirrors [`crate::anchored_overlay::AnchoredOverlayView`]'s shape: two
//! distinct, differently-typed children threaded individually (not a
//! `ViewSequence` — there are always exactly two, with distinct visual and
//! paint roles, so an n-ary collection would be the wrong tool).

use std::marker::PhantomData;

use masonry::layout::Length;
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use super::widget;

/// Stack `front` above `back` — full-width, separated by a gap — while
/// `front` (and any overflow from its subtree, e.g. an open
/// [`crate::AnchoredOverlay`]-hosted popover) paints on top of `back`.
///
/// This decouples *visual* order (which child sits where in the layout)
/// from *paint* order (which child's subtree wins when they overlap) —
/// masonry has no z-index, so this is the primitive that fills that gap.
/// See the [`widget::OverlapColumn`] doc for the mechanism.
pub fn overlap_column<State, Action, FrontV, BackV>(
    front: FrontV,
    back: BackV,
) -> OverlapColumn<FrontV, BackV, State, Action>
where
    State: 'static,
    Action: 'static,
    FrontV: WidgetView<State, Action>,
    BackV: WidgetView<State, Action>,
{
    OverlapColumn {
        front,
        back,
        gap: Length::ZERO,
        phantom: PhantomData,
    }
}

/// View returned by [`overlap_column`]. Configure with [`Self::gap`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct OverlapColumn<FrontV, BackV, State, Action = ()> {
    front: FrontV,
    back: BackV,
    gap: Length,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<FrontV, BackV, State, Action> OverlapColumn<FrontV, BackV, State, Action> {
    /// Spacing between `front` and `back`.
    pub fn gap(mut self, gap: Length) -> Self {
        self.gap = gap;
        self
    }
}

impl<FrontV, BackV, State, Action> ViewMarker for OverlapColumn<FrontV, BackV, State, Action> {}

impl<FrontV, BackV, State, Action> View<State, Action, ViewCtx>
    for OverlapColumn<FrontV, BackV, State, Action>
where
    State: 'static,
    Action: 'static,
    FrontV: WidgetView<State, Action>,
    BackV: WidgetView<State, Action>,
{
    type Element = Pod<widget::OverlapColumn>;
    type ViewState = (FrontV::ViewState, BackV::ViewState);

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (front, front_state) = self.front.build(ctx, app_state);
        let (back, back_state) = self.back.build(ctx, app_state);
        let widget = widget::OverlapColumn::new(
            front.new_widget.erased(),
            back.new_widget.erased(),
            self.gap,
        );
        (ctx.create_pod(widget), (front_state, back_state))
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.gap != prev.gap {
            widget::OverlapColumn::set_gap(&mut element, self.gap);
        }
        let mut front = widget::OverlapColumn::front_mut(&mut element);
        self.front.rebuild(
            &prev.front,
            &mut view_state.0,
            ctx,
            front.downcast(),
            app_state,
        );
        drop(front);
        let mut back = widget::OverlapColumn::back_mut(&mut element);
        self.back.rebuild(
            &prev.back,
            &mut view_state.1,
            ctx,
            back.downcast(),
            app_state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut front = widget::OverlapColumn::front_mut(&mut element);
        self.front
            .teardown(&mut view_state.0, ctx, front.downcast());
        drop(front);
        let mut back = widget::OverlapColumn::back_mut(&mut element);
        self.back.teardown(&mut view_state.1, ctx, back.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut front = widget::OverlapColumn::front_mut(&mut element);
        let result =
            self.front
                .message(&mut view_state.0, message, front.downcast(), app_state);
        drop(front);
        match result {
            MessageResult::Nop => {
                let mut back = widget::OverlapColumn::back_mut(&mut element);
                self.back
                    .message(&mut view_state.1, message, back.downcast(), app_state)
            }
            other => other,
        }
    }
}
