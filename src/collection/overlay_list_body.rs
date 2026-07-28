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
) -> impl WidgetView<State, Action, Widget: Sized>
where
    State: 'static,
    Action: 'static,
{
    let theme = *theme;
    let len = items.len();
    virtual_scroll(len, move |_state: &mut State, pos: usize| {
        let text = items[pos].clone();
        overlay_list_item(
            text,
            highlighted == Some(pos),
            &theme,
            item_role,
            Arc::clone(&on_select),
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
        let on_select = Arc::new(|_s: &mut S, _text: ArcStr| ());
        let view = overlay_list_body(
            items,
            None,
            &Theme::default(),
            Role::ListBoxOption,
            on_select,
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
    use xilem::core::{DynMessage, Environment, MessageCtx, MessageResult, ViewId};
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
        let on_select = Arc::new(|state: &mut S, text: ArcStr| {
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
}
