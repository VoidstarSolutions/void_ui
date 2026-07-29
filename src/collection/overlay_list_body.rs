//! Wraps `xilem::view::virtual_scroll` directly for the overlay-list
//! substrate (autocomplete suggestions, dropdown menu items) — the same
//! mechanism `src/collection/body_view.rs`'s `collection_body` already uses
//! for `list`/`data_grid`, drastically simplified: no selection lens, tree
//! metadata, lazy-load, or click-routing helpers, since none of that serves
//! a single-highlighted-index, single-focus-target list. See the design
//! spec's "Key insight" for why `on_action`-based Fetch handling doesn't
//! work here and this View-based mechanism is required instead.

use std::sync::Arc;

use masonry::accesskit::Role;
use masonry::core::ArcStr;
use xilem::WidgetView;
use xilem::view::virtual_scroll;

use super::item_row::OnActivated;
use super::item_row_view::{OnSelect, overlay_list_item};
use crate::Theme;

/// Builds the virtualized row content. `items`/`highlighted` are captured
/// directly by the per-index closure below, not read from `State` — see the
/// design spec's "Key insight" for why item content must NOT be derived from
/// `State` here (autocomplete's filtered suggestions are computed at
/// `AutocompleteView`'s own build/rebuild time and passed down as a plain
/// argument, not through a `State` accessor).
pub(crate) fn overlay_list_body<State, Action>(
    items: Arc<Vec<ArcStr>>,
    highlighted: Option<usize>,
    theme: &Theme,
    item_role: Role,
    on_select: OnSelect<State, Action>,
    on_activated: Option<OnActivated>,
) -> impl WidgetView<State, Action, Widget = xilem::masonry::widgets::VirtualScroll>
where
    State: 'static,
    Action: 'static,
{
    let theme = *theme;
    let len = items.len();
    virtual_scroll(len, move |_state: &mut State, pos: usize| {
        // `pos` can momentarily exceed `items.len()` during a rebuild that
        // shrinks the list (e.g. a click-selection that collapses
        // autocomplete's suggestions down to the one exact match):
        // `virtual_scroll`'s own `View::rebuild` refreshes every
        // still-materialized row's content *before* the resulting `Fetch`
        // trims the widget down to the new, shorter range. Indexing
        // unconditionally here panics on that transitional call — mirrors
        // `collection_body`'s identical guard for the same race
        // (`body_view.rs`'s "pos past the end" comment).
        let text = items.get(pos).cloned().unwrap_or_else(|| ArcStr::from(""));
        // Whether this row is the whole list's first/last item right now —
        // see `OverlayListItem::round_top`'s doc comment for why each edge
        // row rounds its own corners instead of the container clipping
        // them. `len.checked_sub(1)` avoids underflowing for an empty list
        // (unreachable in practice — `virtual_scroll` never invokes this
        // closure with `len == 0` — but cheap to make impossible outright).
        let round_top = pos == 0;
        let round_bottom = len.checked_sub(1) == Some(pos);
        overlay_list_item(
            text,
            pos,
            round_top,
            round_bottom,
            highlighted == Some(pos),
            &theme,
            item_role,
            Arc::clone(&on_select),
            on_activated.clone(),
        )
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use masonry::accesskit::Role;
    use masonry::core::ArcStr;
    use xilem::WidgetView;

    use super::overlay_list_body;
    use crate::Theme;

    struct S;

    fn assert_widget_view<V: WidgetView<S, ()>>(_: &V) {}

    #[test]
    fn overlay_list_body_builds_a_widget_view() {
        let items: Arc<Vec<ArcStr>> = Arc::new(vec!["Apple".into(), "Banana".into()]);
        let on_select = Arc::new(|_s: &mut S, _pos: usize, _text: ArcStr| ());
        let view = overlay_list_body(
            items,
            None,
            &Theme::default(),
            Role::ListBoxOption,
            on_select,
            None,
        );
        assert_widget_view(&view);
    }
}

/// Integration test proving `overlay_list_body` actually virtualizes and
/// routes row selection through a *real* xilem View message/rebuild cycle —
/// not just a `WidgetView` type-check, and not just `masonry::testing::
/// TestHarness` driving the masonry widget layer directly (which is how
/// `body_view.rs`'s own `scroll_to_moves_the_materialized_window_to_the_target`
/// test works, but that test manually calls `VirtualScroll::add_child`/
/// `remove_child` at the widget level, bypassing exactly the View-level
/// `Fetch`-answering machinery this task depends on).
///
/// Precedent for driving `View::build`/`View::message` directly against a
/// real `ViewCtx` (rather than only exercising the masonry widget layer) is
/// `crate::components::input::view`'s test module
/// (`input_cleared_emits_empty_string_to_callback`,
/// `changed_action_passes_new_text_to_callback`): it builds a `ViewCtx` via
/// `crate::test_support`, builds the view, puts the resulting widget in a
/// `TestHarness`, and calls `View::message` directly inside
/// `harness.edit_root_widget`. This test follows that same pattern and adds
/// `View::rebuild` — needed because `virtual_scroll`'s own `message()` only
/// stashes the `Fetch` action (`view_state.pending_action = Some(action)`)
/// and returns `MessageResult::RequestRebuild`; the actual add/remove of
/// child widgets happens in the *next* `rebuild`, per its upstream source
/// (`xilem_masonry::view::virtual_scroll`).
#[cfg(test)]
mod integration_tests {
    use std::sync::Arc;

    use masonry::accesskit::Role;
    use masonry::core::{ArcStr, Widget as _};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use xilem::core::{DynMessage, Environment, MessageCtx, MessageResult, View, ViewId};
    use xilem::masonry::widgets::VirtualScrollAction;
    use xilem::{ViewCtx, WidgetView};

    use super::overlay_list_body;
    use crate::Theme;
    use crate::test_support;

    struct S {
        selected: Option<ArcStr>,
    }

    /// Pumps real `View::message`/`View::rebuild` calls until `VirtualScroll`
    /// stops requesting a `Fetch`. Unlike `body_view.rs`'s `drive_to_fixpoint`
    /// (which manually drives `VirtualScroll::add_child`/`remove_child` at
    /// the widget level), this routes the popped `VirtualScrollAction`
    /// through the actual `virtual_scroll` View's `message`/`rebuild` — the
    /// exact mechanism `overlay_list_body` depends on, since a plain
    /// `Widget::on_action` handler cannot answer `Fetch`
    /// (`VirtualScrollFetchAction` isn't `Clone` and `on_action` only lends a
    /// borrowed reference — confirmed against the pinned masonry source).
    fn drive_to_fixpoint<V>(
        view: &V,
        view_state: &mut V::ViewState,
        ctx: &mut ViewCtx,
        harness: &mut TestHarness<V::Widget>,
        state: &mut S,
    ) where
        V: WidgetView<S, ()>,
        V::Widget: Sized,
    {
        let mut iteration = 0;
        loop {
            iteration += 1;
            assert!(iteration <= 1000, "Took too long to reach fixpoint");
            let Some((action, _id)) = harness.pop_action::<VirtualScrollAction>() else {
                break;
            };
            if !matches!(action, VirtualScrollAction::Fetch(_)) {
                // A `Scroll` action doesn't change the materialized window
                // (it fires when the in-viewport range shifts); nothing to
                // drive for this test.
                continue;
            }
            harness.edit_root_widget(|element| {
                let mut message =
                    MessageCtx::new(Environment::new(), vec![], DynMessage::new(action));
                let result = view.message(view_state, &mut message, element, state);
                assert!(
                    matches!(result, MessageResult::RequestRebuild),
                    "a Fetch should ask virtual_scroll's View to request a rebuild"
                );
            });
            harness.edit_root_widget(|element| {
                view.rebuild(view, view_state, ctx, element, state);
            });
        }
    }

    #[test]
    fn overlay_list_body_virtualizes_and_routes_selection_through_real_view_messages() {
        const ITEM_COUNT: usize = 1000;
        let items: Arc<Vec<ArcStr>> = Arc::new(
            (0..ITEM_COUNT)
                .map(|i| ArcStr::from(format!("item {i}")))
                .collect(),
        );
        let theme = Theme::default();
        let on_select = Arc::new(|state: &mut S, _pos: usize, text: ArcStr| {
            state.selected = Some(text);
        });

        // `highlighted = Some(0)` is captured directly by the per-index
        // closure (see this module's doc comment / the design spec's "Key
        // insight") — `S` below carries no items/highlighted field at all,
        // so there is no `State` path this could have come from.
        let view = overlay_list_body(
            Arc::clone(&items),
            Some(0),
            &theme,
            Role::ListBoxOption,
            on_select,
            None,
        );

        run_body(&view, &items);
    }

    /// Generic over the view's concrete (but otherwise opaque) `Widget`
    /// type, with an explicit `Sized` bound: `overlay_list_body`'s return
    /// type is `impl WidgetView<State, Action>`, and `WidgetView::Widget:
    /// ?Sized` (to allow erased `AnyWidgetView` content elsewhere in the
    /// crate) — so at the `#[test]`'s call site the compiler can't assume
    /// `TestHarness<V::Widget>` is constructible without this bound proving
    /// it explicitly, even though the concrete type
    /// (`masonry::widgets::VirtualScroll`) obviously is.
    fn run_body<V>(view: &V, items: &Arc<Vec<ArcStr>>)
    where
        V: WidgetView<S, ()>,
        V::Widget: Sized,
    {
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = S { selected: None };
        let (pod, mut view_state) = view.build(&mut ctx, &mut state);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), pod.new_widget, (200, 400));

        drive_to_fixpoint(view, &mut view_state, &mut ctx, &mut harness, &mut state);

        // Virtualization: only a small window around the anchor is
        // materialized, not all `ITEM_COUNT` rows. Counted via the generic
        // `Widget::children_ids()` trait method (not `VirtualScroll::len()`,
        // a widget-specific inherent method unavailable through the opaque,
        // generic `V::Widget`).
        let materialized_ids: Vec<_> = harness
            .root_widget()
            .children_ids()
            .iter()
            .copied()
            .collect();
        assert!(
            !materialized_ids.is_empty() && materialized_ids.len() < 100,
            "expected a small materialized window, got {} of {}",
            materialized_ids.len(),
            items.len()
        );

        // Content: index 0 is always in the initial window (the widget
        // anchors at 0), and its text/highlighted state came straight from
        // the closure's captured `items`/`highlighted` arguments.
        let first_id = *materialized_ids
            .first()
            .expect("row 0 should be materialized");
        harness.redraw();
        let node = harness
            .access_node(first_id)
            .expect("materialized row has an accessibility node");
        assert_eq!(node.label(), Some("item 0".to_string()));
        assert_eq!(
            node.is_selected(),
            Some(true),
            "row 0 should reflect highlighted = Some(0)"
        );

        // Selection: simulate row 0's own click action (`OverlayListItem`
        // submits its displayed text as its `ArcStr` widget action) bubbling
        // up to `virtual_scroll`'s `message()`, which strips the leading
        // per-index `ViewId` it assigned when building that child and routes
        // the rest to `OverlayListItemView::message`, which calls
        // `on_select`.
        harness.edit_root_widget(|element| {
            let mut message = MessageCtx::new(
                Environment::new(),
                vec![ViewId::new(0)],
                DynMessage::new(items[0].clone()),
            );
            let result = view.message(&mut view_state, &mut message, element, &mut state);
            assert!(matches!(result, MessageResult::Action(())));
        });
        assert_eq!(state.selected, Some(items[0].clone()));
    }

    /// Regression test for a real crash: `overlay_list_body`'s per-index
    /// closure used to index `items[pos]` unconditionally, which panics
    /// (`index out of bounds`) when `pos` momentarily exceeds the new,
    /// shrunk `items.len()`. This is exactly what happens live in
    /// autocomplete: clicking a suggestion collapses `compute_filtered`
    /// down to the one exact match, and `xilem_masonry::view::virtual_scroll`'s
    /// own `View::rebuild` refreshes every *already-materialized* row's
    /// content against the new (shorter) `items` — via this closure —
    /// before the resulting `Fetch` trims the widget down to match. So a
    /// row materialized at, say, index 8 against a 1000-item list gets
    /// asked to rebuild against a 1-item list in the very same `rebuild`
    /// call, well before any `Fetch`/`add_child`/`remove_child` cleanup
    /// runs. Reproduced directly (bypassing `drive_to_fixpoint`, which
    /// would just re-fetch and hide the bug) by handing the *already
    /// materialized-against-1000-items* widget a freshly built view over
    /// only 1 item and calling `View::rebuild` once.
    #[test]
    fn rebuild_survives_the_item_list_shrinking_out_from_under_materialized_rows() {
        const ITEM_COUNT: usize = 1000;
        let items: Arc<Vec<ArcStr>> = Arc::new(
            (0..ITEM_COUNT)
                .map(|i| ArcStr::from(format!("item {i}")))
                .collect(),
        );
        let theme = Theme::default();
        let on_select: super::OnSelect<S, ()> = Arc::new(|_s: &mut S, _pos: usize, _t: ArcStr| ());

        let view = overlay_list_body(
            Arc::clone(&items),
            None,
            &theme,
            Role::ListBoxOption,
            Arc::clone(&on_select),
            None,
        );

        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = S { selected: None };
        let (pod, mut view_state) = view.build(&mut ctx, &mut state);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), pod.new_widget, (200, 400));
        drive_to_fixpoint(&view, &mut view_state, &mut ctx, &mut harness, &mut state);

        let materialized_before = harness.root_widget().children_ids().len();
        assert!(
            materialized_before > 1,
            "need more than 1 materialized row for this test to actually exercise the \
             out-of-range case; got {materialized_before}"
        );

        // The shrunk list, and the new view built over it — mirrors
        // `AutocompleteView::rebuild` building a fresh `overlay_list_body`
        // from a freshly recomputed `filtered` every rebuild.
        let shrunk_items: Arc<Vec<ArcStr>> = Arc::new(vec![items[0].clone()]);
        let shrunk_view = overlay_list_body(
            Arc::clone(&shrunk_items),
            None,
            &theme,
            Role::ListBoxOption,
            on_select,
            None,
        );

        // This is the actual regression check: pre-fix, this panicked
        // ("index out of bounds") the moment `virtual_scroll`'s `rebuild`
        // reached any materialized row past index 0. Reaching the
        // assertions below at all proves the fix.
        harness.edit_root_widget(|element| {
            shrunk_view.rebuild(&view, &mut view_state, &mut ctx, element, &mut state);
        });

        // Drive the resulting Fetch to completion and confirm the widget
        // ends up in a sane, correctly-trimmed state, not just "didn't
        // panic".
        drive_to_fixpoint(
            &shrunk_view,
            &mut view_state,
            &mut ctx,
            &mut harness,
            &mut state,
        );
        let materialized_ids: Vec<_> = harness
            .root_widget()
            .children_ids()
            .iter()
            .copied()
            .collect();
        assert_eq!(
            materialized_ids.len(),
            1,
            "after the shrink settles, exactly the 1 remaining item should be materialized"
        );
        harness.redraw();
        let node = harness
            .access_node(materialized_ids[0])
            .expect("materialized row has an accessibility node");
        assert_eq!(node.label(), Some("item 0".to_string()));
    }
}
