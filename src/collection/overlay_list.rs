//! `overlay_list` — the xilem `View` wrapping `CollectionListWidget`,
//! structurally parallel to `CollectionBodyView` wrapping
//! `CollectionBodyWidget` (`body_view.rs`). Owns nothing itself beyond
//! forwarding to `overlay_list_body`'s own `virtual_scroll` child and
//! keeping `CollectionListWidget`'s `item_count`/`active_start` in sync.
//!
//! ## `active_start`
//!
//! `CollectionListWidget` (Task 4) has no way to derive its own materialized
//! window's starting offset itself — see its module doc
//! (`imperative_list.rs`) — so it exposes `set_active_start` for this View
//! to call on every `rebuild`. The value comes from peeking (non-consuming)
//! at the child's `VirtualScrollAction` in `message()`, mirroring
//! `body_view.rs`'s `CollectionBodyView` tracking `active_range` via
//! `maybe_take_message::<VirtualScrollAction>`: only the `Fetch` variant
//! carries a materialized-range change, and materialization for that new
//! range lands in the *next* `rebuild` (once the child's own `rebuild` has
//! actually added/removed rows), so `message()` only remembers
//! `fetch.target().start` in `ViewState` for that next `rebuild` to apply.

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::accesskit::Role;
use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};
use xilem::{Pod, ViewCtx, WidgetView};

use super::imperative_list::CollectionListWidget;
use super::item_row::OnActivated;
use super::item_row_view::OnSelect;
use super::overlay_list_body::overlay_list_body;
use crate::Theme;

// `accepts_focus` pushed this to 8 args, one over clippy's
// `too_many_arguments` threshold — allowed rather than bundled into a
// config struct, mirroring the precedent at
// `components::date_picker::widget`/`components::slider::widget` (params
// here are heterogeneous enough — an `Arc`, two `Role`s, two callback types,
// a `bool` — that a bundling struct wouldn't read any more clearly than the
// flat argument list).
#[allow(clippy::too_many_arguments)]
pub(crate) fn overlay_list<State, Action>(
    items: Arc<Vec<ArcStr>>,
    highlighted: Option<usize>,
    theme: &Theme,
    container_role: Role,
    item_role: Role,
    on_select: OnSelect<State, Action>,
    on_activated: Option<OnActivated>,
    accepts_focus: bool,
) -> impl WidgetView<State, Action, Widget: Sized>
where
    State: 'static,
    Action: 'static,
{
    let item_count = items.len();
    OverlayListView {
        child: overlay_list_body(
            items,
            highlighted,
            theme,
            item_role,
            on_select,
            on_activated,
        ),
        item_count,
        container_role,
        accepts_focus,
        phantom: PhantomData,
    }
}

struct OverlayListView<V, State, Action> {
    child: V,
    item_count: usize,
    container_role: Role,
    /// Forwarded verbatim into `CollectionListWidget::new` — see that
    /// widget's `accepts_focus` field doc for why this must be per-caller
    /// rather than hardcoded. Not read anywhere else in this View: it never
    /// changes across `rebuild`s for a given caller (autocomplete always
    /// passes `true`, `dropdown_button` always passes `false`), so there is no
    /// `set_accepts_focus`-style setter to call from `rebuild` the way
    /// `container_role` doesn't have one either.
    accepts_focus: bool,
    phantom: PhantomData<fn(State) -> Action>,
}

struct OverlayListViewState<S> {
    child_state: S,
    /// Slice-position offset of the first materialized row, captured from
    /// the child's `VirtualScrollAction::Fetch` in `message()` and applied
    /// to `CollectionListWidget::set_active_start` on the *next* `rebuild`
    /// (once the child has actually materialized that range) — see the
    /// module doc.
    active_start: usize,
}

impl<V, State, Action> ViewMarker for OverlayListView<V, State, Action> {}

impl<State, Action, V> View<State, Action, ViewCtx> for OverlayListView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: View<State, Action, ViewCtx, Element = Pod<VirtualScrollWidget>>,
{
    type Element = Pod<CollectionListWidget>;
    type ViewState = OverlayListViewState<V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_element, child_state) = self.child.build(ctx, app_state);
        (
            Pod::new(CollectionListWidget::new(
                child_element.new_widget,
                self.item_count,
                self.container_role,
                self.accepts_focus,
            )),
            OverlayListViewState {
                child_state,
                active_start: 0,
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
        {
            let vs = CollectionListWidget::virtual_scroll_mut(&mut element);
            self.child
                .rebuild(&prev.child, &mut view_state.child_state, ctx, vs, app_state);
        }
        if self.item_count != prev.item_count {
            CollectionListWidget::set_item_count(&mut element, self.item_count);
        }
        // Materialization for `view_state.active_start` (captured by the
        // most recent `message()` peek, if any) has now caught up — the
        // child's own `rebuild` just ran above — so push it down. Every
        // rebuild, not just when it changed: `CollectionListWidget` has no
        // other way to learn this (see its module doc), and the push is
        // cheap (no repaint/relayout — see `set_active_start`'s doc).
        CollectionListWidget::set_active_start(&mut element, view_state.active_start);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let vs = CollectionListWidget::virtual_scroll_mut(&mut element);
        self.child.teardown(&mut view_state.child_state, ctx, vs);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // Only peek once the message has stopped routing to a child:
        // maybe_take_message debug-asserts the path is empty, so probing
        // mid-route would panic. Mirrors `CollectionBodyView::message`
        // (`body_view.rs:378-405`).
        let mut active_start_update: Option<usize> = None;
        if message.remaining_path().is_empty() {
            // Peek without consuming (`false`): the `VirtualScrollAction`
            // still routes onward to the child so virtualization handles it.
            message.maybe_take_message::<VirtualScrollAction>(|action| {
                if let VirtualScrollAction::Fetch(fetch) = action {
                    active_start_update = Some(fetch.target().start);
                }
                false
            });
        }
        let vs = CollectionListWidget::virtual_scroll_mut(&mut element);
        let result = self
            .child
            .message(&mut view_state.child_state, message, vs, app_state);
        // Materialization for `active_start_update` is deferred to the next
        // rebuild (see the module doc), so just remember it here.
        if let Some(active_start) = active_start_update {
            view_state.active_start = active_start;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use masonry::accesskit::Role;
    use masonry::core::ArcStr;
    use xilem::WidgetView;

    use super::overlay_list;
    use crate::Theme;

    struct S;

    fn assert_widget_view<V: WidgetView<S, ()>>(_: &V) {}

    #[test]
    fn overlay_list_builds_a_widget_view() {
        let items: Arc<Vec<ArcStr>> = Arc::new(vec!["Apple".into(), "Banana".into()]);
        let on_select = Arc::new(|_s: &mut S, _pos: usize, _text: ArcStr| ());
        let view = overlay_list(
            items,
            None,
            &Theme::default(),
            Role::ListBox,
            Role::ListBoxOption,
            on_select,
            None,
            true,
        );
        assert_widget_view(&view);
    }
}

/// Integration test proving the `active_start` wiring documented at the top
/// of this module actually works: drives a real `Fetch` cycle through
/// `OverlayListView`'s own `message`/`rebuild` (not
/// `CollectionListWidget::set_active_start` called directly, as
/// `imperative_list.rs`'s own widget-level tests do) to re-anchor the
/// materialized window far from its initial position, then calls
/// `CollectionListWidget::set_highlight` on an index inside that new window
/// and checks its highlighted state pushes immediately. That push only
/// happens when `active_start` correctly reflects the real post-reanchor
/// offset (see `set_highlight`'s materialized-window bounds check in
/// `imperative_list.rs`) — a stuck-at-0 `active_start` would make the check
/// see the far-away target as unmaterialized and silently skip the push.
#[cfg(test)]
mod integration_tests {
    use std::marker::PhantomData;
    use std::sync::Arc;

    use masonry::accesskit::Role;
    use masonry::core::{ArcStr, Widget as _, WidgetId};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use xilem::core::{DynMessage, Environment, MessageCtx, MessageResult, View};
    use xilem::masonry::widgets::{VirtualScroll as VirtualScrollWidget, VirtualScrollAction};
    use xilem::{Pod, ViewCtx};

    use super::OverlayListView;
    use crate::Theme;
    use crate::collection::imperative_list::CollectionListWidget;
    use crate::collection::overlay_list_body::overlay_list_body;
    use crate::test_support;

    struct S;

    /// Mirrors `overlay_list_body.rs`'s own `drive_to_fixpoint`: pumps real
    /// `View::message`/`View::rebuild` calls until `VirtualScroll` stops
    /// requesting a `Fetch` — the exact path `OverlayListView::message`'s
    /// `active_start` peek and `OverlayListView::rebuild`'s
    /// `set_active_start` push run on.
    ///
    /// Generic over `View<S, (), ViewCtx, Element = Pod<CollectionListWidget>>`
    /// directly (rather than `WidgetView<S, ()>` with a `Widget: Sized`
    /// bound, as `overlay_list_body.rs`'s own version does) because the
    /// public `overlay_list()` wrapper's return type is pinned by the task
    /// spec to plain `impl WidgetView<State, Action>` with no `Widget`
    /// bound — callers (including this test) can't name or assume
    /// `Sized`-ness of its opaque `Widget`. Building `OverlayListView`
    /// directly instead sidesteps that opacity while exercising the exact
    /// same `build`/`message`/`rebuild` this task added.
    fn drive_to_fixpoint<V>(
        view: &V,
        view_state: &mut V::ViewState,
        ctx: &mut ViewCtx,
        harness: &mut TestHarness<CollectionListWidget>,
        state: &mut S,
    ) where
        V: View<S, (), ViewCtx, Element = Pod<CollectionListWidget>>,
    {
        let mut iteration = 0;
        loop {
            iteration += 1;
            assert!(iteration <= 1000, "Took too long to reach fixpoint");
            let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() else {
                break;
            };
            if !matches!(action, VirtualScrollAction::Fetch(_)) {
                // A `Scroll` action doesn't change the materialized window.
                continue;
            }
            harness.edit_root_widget(|element| {
                let mut message =
                    MessageCtx::new(Environment::new(), vec![], DynMessage::new(action));
                let result = view.message(view_state, &mut message, element, state);
                assert!(
                    matches!(result, MessageResult::RequestRebuild),
                    "a Fetch should ask the View to request a rebuild"
                );
            });
            harness.edit_root_widget(|element| {
                view.rebuild(view, view_state, ctx, element, state);
            });
        }
    }

    #[test]
    fn active_start_tracked_through_message_and_rebuild_lets_set_highlight_push_after_reanchor() {
        const ITEM_COUNT: usize = 1000;
        const TARGET: usize = 900;

        let items: Arc<Vec<ArcStr>> = Arc::new(
            (0..ITEM_COUNT)
                .map(|i| ArcStr::from(format!("item {i}")))
                .collect(),
        );
        let theme = Theme::default();
        let on_select = Arc::new(|_s: &mut S, _pos: usize, _text: ArcStr| ());

        // Built directly rather than via `overlay_list()` — see
        // `drive_to_fixpoint`'s doc comment for why — but otherwise
        // identical to what that public builder constructs.
        let view = OverlayListView {
            child: overlay_list_body(
                Arc::clone(&items),
                None,
                &theme,
                Role::ListBoxOption,
                on_select,
                None,
            ),
            item_count: items.len(),
            container_role: Role::ListBox,
            accepts_focus: true,
            phantom: PhantomData,
        };

        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = S;
        let (pod, mut view_state) = view.build(&mut ctx, &mut state);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), pod.new_widget, (200, 400));

        drive_to_fixpoint(&view, &mut view_state, &mut ctx, &mut harness, &mut state);

        // Re-anchor far from the initial (near-0) window, exactly as
        // `CollectionListWidget::set_highlight`'s own scroll_to branch
        // would for an unmaterialized target, then drive the resulting
        // Fetch through the *real* View message/rebuild cycle so
        // `OverlayListView` has to track and apply the new `active_start`
        // (unlike `imperative_list.rs`'s widget-level tests, which set it
        // directly).
        harness.edit_root_widget(|mut element| {
            let mut vs = CollectionListWidget::virtual_scroll_mut(&mut element);
            VirtualScrollWidget::scroll_to(&mut vs, TARGET);
        });
        drive_to_fixpoint(&view, &mut view_state, &mut ctx, &mut harness, &mut state);

        // Find TARGET's materialized WidgetId by its accessible label —
        // `overlay_list_item` sets it to the row's text ("item 900").
        let materialized_ids: Vec<WidgetId> = harness.edit_root_widget(|mut element| {
            let vs = CollectionListWidget::virtual_scroll_mut(&mut element);
            vs.widget.children_ids().iter().copied().collect()
        });
        harness.redraw();
        let target_label = format!("item {TARGET}");
        let target_id = *materialized_ids
            .iter()
            .find(|&&id| {
                harness
                    .access_node(id)
                    .is_some_and(|node| node.label().as_deref() == Some(target_label.as_str()))
            })
            .unwrap_or_else(|| {
                panic!(
                    "expected item {TARGET} to be materialized after re-anchoring, got {} rows",
                    materialized_ids.len()
                )
            });

        // The real assertion: set_highlight on TARGET must push straight
        // onto this already-materialized row (no extra scroll_to/Fetch
        // needed) — which only happens if `active_start` correctly
        // reflects the post-reanchor offset the View just tracked via
        // `message()`/`rebuild()`, rather than sitting stuck at its
        // build-time 0.
        harness.edit_root_widget(|mut element| {
            CollectionListWidget::set_highlight(&mut element, Some(TARGET));
        });
        harness.redraw();
        assert_eq!(
            harness
                .access_node(target_id)
                .expect("row exists")
                .is_selected(),
            Some(true),
            "set_highlight(TARGET) should push highlighted=true immediately onto \
             the already-materialized row at TARGET, proving active_start was \
             correctly tracked through message()/rebuild() rather than left at \
             its build-time 0"
        );
    }
}
