//! Xilem `View` wrapper around `OverlayListItem`, mirroring the
//! `RowClickable`/`ClickableRow` pair in `row_click.rs` at a smaller scale:
//! a masonry `Widget` that submits an action on click, paired with a `View`
//! that catches it via `message()` with real ownership (xilem's View-message
//! system, not masonry's `on_action`/`ErasedAction` bubbling — see the design
//! spec's "Key insight" for why that distinction matters here).

use std::marker::PhantomData;
use std::sync::Arc;

use masonry::accesskit::Role;
use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::item_row::{OnActivated, OverlayListItem};
use crate::Theme;

/// Boxed row-selection handler: `(state, row index, selected text) ->
/// Action`. Unlike `ClickableRow`'s `on_click` (which requires `Action:
/// Default` and always signals via `Action::default()`), this returns the
/// *real* action the host wants — mirroring `AutocompleteView`'s existing
/// `OnChanged` shape (`autocomplete/view.rs:45`), since a row selection is a
/// real, meaningful event, not a "something changed, rerun" marker. The
/// index lets callers resolve the selected row unambiguously even when two
/// rows share the same displayed text (`ArcStr` is kept alongside it since
/// some consumers — autocomplete — have a genuinely text-based selection
/// semantic and want it directly).
pub(crate) type OnSelect<State, Action> =
    Arc<dyn Fn(&mut State, usize, ArcStr) -> Action + Send + Sync>;

struct OverlayListItemView<State, Action> {
    text: ArcStr,
    highlighted: bool,
    theme: Theme,
    role: Role,
    /// This row's position in the item list. Static per row instance (a
    /// given `OverlayListItemView` is always rebuilt against the same
    /// index), so — like `role`/`on_activated` — never diffed in `rebuild`.
    pos: usize,
    /// Whether this row is the first/last in the whole list right now —
    /// see `OverlayListItem::round_top`'s doc comment. Unlike `pos`, this
    /// *is* diffed in `rebuild`: virtualization recycles row instances
    /// across indices, so the same `OverlayListItemView`/element pair can
    /// stop or start being an edge row across rebuilds (e.g. a shrinking
    /// filtered list) without being torn down.
    round_top: bool,
    round_bottom: bool,
    on_select: OnSelect<State, Action>,
    /// See `item_row::OnActivated`'s doc comment — an optional, `EventCtx`-
    /// level side effect run synchronously when a pointer click completes
    /// selection (e.g. autocomplete returning focus to its text field).
    /// Practically static per component instance (not diffed on rebuild,
    /// matching `role`, which also isn't diffed below).
    on_activated: Option<OnActivated>,
    phantom: PhantomData<fn(State) -> Action>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn overlay_list_item<State, Action>(
    text: ArcStr,
    pos: usize,
    round_top: bool,
    round_bottom: bool,
    highlighted: bool,
    theme: &Theme,
    role: Role,
    on_select: OnSelect<State, Action>,
    on_activated: Option<OnActivated>,
) -> impl WidgetView<State, Action> + use<State, Action>
where
    State: 'static,
    Action: 'static,
{
    OverlayListItemView {
        text,
        highlighted,
        theme: *theme,
        role,
        pos,
        round_top,
        round_bottom,
        on_select,
        on_activated,
        phantom: PhantomData,
    }
}

impl<State, Action> ViewMarker for OverlayListItemView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for OverlayListItemView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<OverlayListItem>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = OverlayListItem::new(
            self.text.clone(),
            self.highlighted,
            &self.theme,
            self.role,
            self.on_activated.clone(),
            self.round_top,
            self.round_bottom,
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut (),
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) {
        if self.text != prev.text {
            OverlayListItem::set_text(&mut element, self.text.clone());
        }
        if self.highlighted != prev.highlighted {
            OverlayListItem::set_highlighted(&mut element, self.highlighted);
        }
        if self.theme != prev.theme {
            OverlayListItem::set_theme(&mut element, &self.theme);
        }
        if self.round_top != prev.round_top || self.round_bottom != prev.round_bottom {
            OverlayListItem::set_rounded_corners(&mut element, self.round_top, self.round_bottom);
        }
    }

    fn teardown(&self, _view_state: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _view_state: &mut (),
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if let Some(boxed) = message.take_message::<ArcStr>() {
            return MessageResult::Action((self.on_select)(app_state, self.pos, *boxed));
        }
        tracing::error!(?message, "unexpected message in OverlayListItemView");
        MessageResult::Stale
    }
}

#[cfg(test)]
mod tests {
    use masonry::accesskit::Role;
    use masonry::core::ArcStr;
    use xilem::WidgetView;

    use super::overlay_list_item;
    use crate::Theme;

    struct S;

    fn assert_widget_view<V: WidgetView<S, ()>>(_: &V) {}

    #[test]
    fn overlay_list_item_builds_a_widget_view() {
        let on_select: super::OnSelect<S, ()> =
            std::sync::Arc::new(|_s: &mut S, _pos: usize, _text: ArcStr| ());
        let view = overlay_list_item(
            "Apple".into(),
            0,
            true,
            false,
            false,
            &Theme::default(),
            Role::ListBoxOption,
            on_select,
            None,
        );
        assert_widget_view(&view);
    }
}
