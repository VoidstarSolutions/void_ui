//! Internal wrapper around the list's `virtual_scroll` body view.
//!
//! [`ListBodyView`] combines two pieces of behavior in a single pass over
//! the `virtual_scroll` child, mirroring
//! [`data_grid::scroll::ScrollToView`](super::super::data_grid::scroll::ScrollToView)
//! plus a lazy-loading hook:
//!
//! - **Scroll-to-index**: when the host's [`ScrollState`] snapshot's
//!   generation changes, clamp the requested index to `0..item_count` and
//!   re-anchor masonry's `VirtualScroll` so that row's top aligns with the
//!   viewport top.
//! - **Lazy loading**: peeks each `VirtualScrollAction` (via
//!   `MessageCtx::maybe_take_message`, which restores the message if `f`
//!   returns `false`) to see the newly active range. When the active range's
//!   end comes within [`List::load_threshold`](super::view::List::load_threshold)
//!   of `item_count`, the host's `on_load_more` callback is invoked. The
//!   message is left untouched so `virtual_scroll`'s own handling proceeds
//!   normally.
//!
//! [`ScrollState`]: crate::components::data_grid::ScrollState

use std::sync::Arc;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};
use xilem::{Pod, ViewCtx};

use crate::components::data_grid::ScrollState;

/// Boxed lazy-load callback (`Fn(&mut State)`), invoked when the
/// virtualized range nears the end of the data.
pub(super) type LoadMore<State> = Arc<dyn Fn(&mut State) + Send + Sync>;

/// `virtual_scroll` range bound: item count (`u64`) → `i64`. Saturates to
/// `i64::MAX` (matches `data_grid::view::scroll_range_end`).
pub(super) fn scroll_range_end(item_count: u64) -> i64 {
    i64::try_from(item_count).unwrap_or(i64::MAX)
}

/// `virtual_scroll` callback index (`i64`) → slice index (`usize`).
/// Saturates so a stray negative/oversized index reads as past-the-end.
pub(super) fn scroll_idx_to_slice(idx: i64) -> usize {
    usize::try_from(idx).unwrap_or(usize::MAX)
}

/// Slice position (`usize`) → item id (`u64`), used by the
/// position-fallback item id ([`ItemIdSource::Position`]). Saturates to
/// `u64::MAX`.
pub(super) fn position_fallback_id(idx: usize) -> u64 {
    u64::try_from(idx).unwrap_or(u64::MAX)
}

/// Clamps a requested scroll index to `0..item_count`, converted to the
/// `i64` anchor domain masonry's `VirtualScroll` uses. `None` when the list
/// is empty — there is no item to anchor to.
pub(super) fn clamp_scroll_index(index: u64, item_count: u64) -> Option<i64> {
    if item_count == 0 {
        return None;
    }
    let clamped = index.min(item_count - 1);
    Some(i64::try_from(clamped).unwrap_or(i64::MAX))
}

/// How the body derives an item's stable id: either the host's projector,
/// or a fallback to the item's slice position when none was supplied.
pub(super) enum ItemIdSource<Item> {
    /// Host-supplied id projector.
    Explicit(Arc<dyn Fn(&Item) -> u64 + Send + Sync>),
    /// No projector: use the item's current slice position as its id.
    Position,
}

// Hand-written so the bound is on the `Arc` (always `Clone`), not on
// `Item` — a derived `Clone` would wrongly require `Item: Clone`.
impl<Item> Clone for ItemIdSource<Item> {
    fn clone(&self) -> Self {
        match self {
            Self::Explicit(f) => Self::Explicit(Arc::clone(f)),
            Self::Position => Self::Position,
        }
    }
}

impl<Item> ItemIdSource<Item> {
    /// The stable id of `item`, which sits at slice position `pos`.
    pub(super) fn id_of(&self, pos: usize, item: &Item) -> u64 {
        match self {
            Self::Explicit(f) => f(item),
            Self::Position => position_fallback_id(pos),
        }
    }
}

/// Wraps the body's `virtual_scroll` view to apply pending [`ScrollState`]
/// requests and to peek lazy-load triggers. See the module docs.
///
/// The element type is the child's own (`Pod<VirtualScroll>`), so every
/// `View` method delegates straight through; `rebuild` additionally
/// re-anchors on a new scroll generation, and `message` additionally peeks
/// `VirtualScrollAction`s for the lazy-load threshold.
pub(super) struct ListBodyView<V, State> {
    pub(super) child: V,
    pub(super) scroll: ScrollState,
    pub(super) item_count: u64,
    pub(super) on_load_more: Option<LoadMore<State>>,
    pub(super) load_threshold: u64,
}

/// View state for [`ListBodyView`]: the child's state plus the last applied
/// scroll-request generation. Tracked in view state (not via
/// `prev`-comparison) so a request made *before* the view first builds is
/// still applied at the first rebuild.
pub(super) struct ListBodyViewState<S> {
    child_state: S,
    applied_generation: u64,
}

impl<V, State> ViewMarker for ListBodyView<V, State> {}

impl<State, V> View<State, (), ViewCtx> for ListBodyView<V, State>
where
    State: 'static,
    V: View<State, (), ViewCtx, Element = Pod<VirtualScrollWidget>>,
{
    type Element = Pod<VirtualScrollWidget>;
    type ViewState = ListBodyViewState<V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (element, child_state) = self.child.build(ctx, app_state);
        // The underlying widget is built anchored at row 0 (the xilem view
        // hardcodes `VirtualScroll::new(0)`), and re-anchoring needs a
        // `WidgetMut` that only exists once the widget is in the tree.
        // Recording generation 0 here means a request pending at build time
        // (snapshot generation != 0) is applied at the first rebuild.
        (
            element,
            ListBodyViewState {
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
            if let Some(idx) = clamp_scroll_index(self.scroll.index(), self.item_count) {
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
        // `VirtualScrollAction` is an action message addressed to this
        // view's own widget (not routed to a loaded row), so it only
        // arrives with an empty remaining path. Routed messages (e.g. a
        // row's click) must not be probed with `maybe_take_message` — doing
        // so trips its "message has reached its target" debug assertion.
        if message.remaining_path().is_empty()
            && let Some(on_load_more) = self.on_load_more.as_ref()
        {
            let item_count = scroll_range_end(self.item_count);
            let threshold = i64::try_from(self.load_threshold).unwrap_or(i64::MAX);
            message.maybe_take_message::<VirtualScrollAction>(|action| {
                if item_count - action.target.end <= threshold {
                    on_load_more(app_state);
                }
                false
            });
        }
        self.child
            .message(&mut view_state.child_state, message, element, app_state)
    }
}
