//! Programmatic vertical scrolling for the data grid.
//!
//! [`ScrollState`] is a host-owned request token: the host keeps one in
//! its app state, calls [`ScrollState::scroll_to_index`] from any
//! callback, and passes a snapshot to the grid via
//! [`DataGrid::scroll_to`](super::view::DataGrid::scroll_to). The grid's
//! body wrapper (`ScrollToView`) compares the snapshot's generation
//! against the last one it applied and, when they differ, re-anchors
//! masonry's `VirtualScroll` so the requested row's top aligns with the
//! top of the viewport.
//!
//! The index is a **display position** (slice position in the host's
//! ordered view, the same domain as `row_count`) — not a stable row id.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;
use xilem::{Pod, ViewCtx};

use crate::collection::{ScrollState, clamp_scroll_index};

// --- MARK: ScrollToView --------------------------------------------------

/// Internal wrapper around the body's `virtual_scroll` view that applies
/// pending [`ScrollState`] requests.
///
/// The element type is the child's own (`Pod<VirtualScroll>`), so every
/// View method delegates straight through; the only added behavior is in
/// `rebuild`: when the snapshot's generation differs from the last
/// applied one, clamp the requested index to the row range and call
/// [`VirtualScroll::overwrite_anchor`](VirtualScrollWidget::overwrite_anchor),
/// which aligns the row's top with the viewport top and requests layout.
/// The widget then emits its normal `VirtualScrollAction`, which loads
/// the right rows through the existing rebuild path.
pub(super) struct ScrollToView<V> {
    pub(super) child: V,
    pub(super) scroll: ScrollState,
    pub(super) row_count: u64,
}

/// View state for [`ScrollToView`]: the child's state plus the last
/// applied request generation. Tracked in view state (not via
/// `prev`-comparison) so a request made *before* the view first builds
/// is still applied at the first rebuild.
pub(super) struct ScrollToViewState<S> {
    child_state: S,
    applied_generation: u64,
}

impl<V> ViewMarker for ScrollToView<V> {}

impl<State, V> View<State, (), ViewCtx> for ScrollToView<V>
where
    State: 'static,
    V: View<State, (), ViewCtx, Element = Pod<VirtualScrollWidget>>,
{
    type Element = Pod<VirtualScrollWidget>;
    type ViewState = ScrollToViewState<V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (element, child_state) = self.child.build(ctx, app_state);
        // The underlying widget is built anchored at row 0 (the xilem
        // view hardcodes `VirtualScroll::new(0)`), and re-anchoring
        // needs a WidgetMut that only exists once the widget is in the
        // tree. Recording generation 0 here means a request pending at
        // build time (snapshot generation != 0) is applied at the first
        // rebuild. A remounted grid (torn down and rebuilt by the host)
        // therefore re-applies the host's last request on its first
        // rebuild — acceptable, since scroll position is lost on remount
        // anyway.
        (
            element,
            ScrollToViewState {
                child_state,
                applied_generation: 0,
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
        if self.scroll.generation() != view_state.applied_generation {
            view_state.applied_generation = self.scroll.generation();
            if let Some(idx) = clamp_scroll_index(self.scroll.index(), self.row_count) {
                VirtualScrollWidget::overwrite_anchor(&mut element, idx);
            }
        }
        self.child.rebuild(
            &prev.child,
            &mut view_state.child_state,
            ctx,
            element,
            app_state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        self.child
            .teardown(&mut view_state.child_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        self.child
            .message(&mut view_state.child_state, message, element, app_state)
    }
}
