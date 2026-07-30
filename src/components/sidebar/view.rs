//! Sidebar navigation item — interactive, theme-driven.
//!
//! Wraps [`super::widget::ThemedSidebarItem`] in a xilem [`View`]. Pointer
//! state (hover, press) is tracked by the masonry widget; the `selected`
//! flag is the host-controlled selected-row state. An optional trailing
//! action (added with [`SidebarItem::action`]) is revealed by the widget on
//! row hover or keyboard focus-within — see `super::widget` and
//! `super::reveal` for the mechanism.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # #[derive(PartialEq)]
//! # enum Section { Charts }
//! # struct State { focused: Section }
//! # let state = State { focused: Section::Charts };
//! use void_ui::components::sidebar_item;
//! sidebar_item("Charts", |s: &mut State| s.focused = Section::Charts)
//!     .selected(state.focused == Section::Charts)
//!     .render(&theme)
//! # ;
//! ```
//!
//! Whether a row has an action is fixed for the row's lifetime, the same
//! contract as the label text (see the comment at the bottom of
//! [`SidebarItemView::rebuild`]): [`SidebarItemView::rebuild`] still
//! *handles* a presence change correctly (it replaces the row's content
//! wholesale via [`ThemedSidebarItem::set_content`]), but that is the rare,
//! slow path — key a fresh `View` instead if a row's action needs to come
//! and go across rebuilds.

use std::marker::PhantomData;

use masonry::core::{ArcStr, CollectionWidget, NewWidget, StyleProperty, Widget as _};
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Flex, Label};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::reveal::RevealBox;
use super::widget::{ACTIONS_INDEX, LABEL_INDEX, ThemedSidebarItem};
use crate::Theme;

/// Routing id for the trailing-action child view — the row has a single
/// managed child view, so a fixed id suffices.
const ACTION_ID: ViewId = ViewId::new(0);

/// The `View::ViewState` of the erased `Box<AnyWidgetView<State, Action>>`
/// used for the trailing action.
type ActionViewState<State, Action> =
    <Box<AnyWidgetView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// Builder for a themed sidebar navigation item.
///
/// Created with [`sidebar_item`]. Returns a xilem `WidgetView` via
/// [`Self::render`].
#[must_use = "SidebarItem does nothing until rendered with .render(&theme)"]
pub struct SidebarItem<F, State, Action> {
    label: ArcStr,
    selected: bool,
    disabled: bool,
    callback: F,
    action: Option<Box<AnyWidgetView<State, Action>>>,
}

/// Create a new sidebar navigation item with the given label and selection
/// callback.
///
/// The callback is invoked on primary-pointer release inside the widget and on
/// Space / Enter while the widget is focused.
pub fn sidebar_item<State, Action, F>(
    label: impl Into<ArcStr>,
    callback: F,
) -> SidebarItem<F, State, Action>
where
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    SidebarItem {
        label: label.into(),
        selected: false,
        disabled: false,
        callback,
        action: None,
    }
}

impl<F, State, Action> SidebarItem<F, State, Action> {
    /// Mark this item as the currently-selected nav entry.
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Attach a trailing control (e.g. a settings gear), revealed on row
    /// hover or keyboard focus-within. Replaces any action set by an
    /// earlier call. Clicking it fires its own callback and does not
    /// select the row.
    pub fn action(mut self, view: impl WidgetView<State, Action>) -> Self
    where
        State: 'static,
        Action: 'static,
    {
        self.action = Some(Box::new(view));
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> SidebarItemView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        SidebarItemView {
            label: self.label,
            selected: self.selected,
            disabled: self.disabled,
            theme: *theme,
            callback: self.callback,
            action: self.action,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`SidebarItem`].
///
/// Built only through [`SidebarItem::render`]; not constructed directly by
/// callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SidebarItemView<F, State, Action> {
    label: ArcStr,
    selected: bool,
    disabled: bool,
    theme: Theme,
    callback: F,
    action: Option<Box<AnyWidgetView<State, Action>>>,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> SidebarItemView<F, State, Action> {
    /// The label text color for the current selected/disabled state.
    fn text_color(&self) -> masonry::peniko::Color {
        if self.disabled {
            self.theme.palette.text_faint
        } else if self.selected {
            self.theme.palette.text
        } else {
            self.theme.palette.text_muted
        }
    }

    /// Builds a fresh label widget matching this view's current
    /// text/theme/selected/disabled state.
    fn build_label(&self) -> NewWidget<Label> {
        let mut label = Label::new(self.label.clone())
            .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
            .prepare();
        label
            .properties
            .insert(ContentColor::new(self.text_color()));
        label
    }
}

impl<F, State, Action> ViewMarker for SidebarItemView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for SidebarItemView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedSidebarItem>;
    type ViewState = Option<ActionViewState<State, Action>>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let label = self.build_label();
        let (widget, action_state) = match &self.action {
            None => (
                ThemedSidebarItem::new(label, &self.theme)
                    .with_selected(self.selected)
                    .with_disabled(self.disabled),
                None,
            ),
            Some(action) => {
                let (pod, state) = ctx.with_id(ACTION_ID, |ctx| action.build(ctx, app_state));
                (
                    ThemedSidebarItem::new_with_actions(label, pod.new_widget, &self.theme)
                        .with_selected(self.selected)
                        .with_disabled(self.disabled),
                    Some(state),
                )
            }
        };
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, action_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            ThemedSidebarItem::set_theme(&mut element, &self.theme);
        }
        if self.selected != prev.selected {
            ThemedSidebarItem::set_selected(&mut element, self.selected);
        }
        if self.disabled != prev.disabled {
            ThemedSidebarItem::set_disabled(&mut element, self.disabled);
        }
        let paint_props_changed = self.theme != prev.theme
            || self.selected != prev.selected
            || self.disabled != prev.disabled;

        match (&prev.action, &self.action) {
            (None, None) => {
                // Steady state, no action: cheap targeted recolor — the
                // exact same path this view used before it could have
                // actions at all.
                if paint_props_changed {
                    let mut child = ThemedSidebarItem::child_mut(&mut element);
                    child.insert_prop(ContentColor::new(self.text_color()));
                    if self.theme != prev.theme {
                        let mut lbl = child.downcast::<Label>();
                        Label::insert_style(
                            &mut lbl,
                            StyleProperty::FontSize(self.theme.density.ui_font_size),
                        );
                    }
                }
            }
            (Some(prev_action), Some(action)) => {
                // Steady state, action unchanged presence: recolor the
                // label through the row, and diff the action view in place.
                if paint_props_changed {
                    let mut row = ThemedSidebarItem::row_mut(&mut element);
                    let mut label_widget = Flex::get_mut(&mut row, LABEL_INDEX);
                    let mut lbl = label_widget.downcast::<Label>();
                    lbl.insert_prop(ContentColor::new(self.text_color()));
                    if self.theme != prev.theme {
                        Label::insert_style(
                            &mut lbl,
                            StyleProperty::FontSize(self.theme.density.ui_font_size),
                        );
                    }
                }
                ctx.with_id(ACTION_ID, |ctx| {
                    let mut row = ThemedSidebarItem::row_mut(&mut element);
                    let mut action_widget = Flex::get_mut(&mut row, ACTIONS_INDEX);
                    let mut reveal_box = action_widget.downcast::<RevealBox>();
                    let mut child = RevealBox::child_mut(&mut reveal_box);
                    let state = view_state.as_mut().expect("action state present");
                    action.rebuild(prev_action, state, ctx, child.downcast(), app_state);
                });
            }
            (None, Some(action)) => {
                // Rare transition: an action just got attached. See the
                // module docs — a row's action presence is meant to be
                // fixed at construction, so this replaces the content
                // wholesale rather than splicing a `Flex` row in after the
                // fact (which masonry's API does not support: an
                // already-inserted `WidgetPod` cannot be turned back into a
                // `NewWidget`).
                let label = self.build_label();
                let (pod, state) = ctx.with_id(ACTION_ID, |ctx| action.build(ctx, app_state));
                ThemedSidebarItem::set_content(&mut element, label, Some(pod.new_widget));
                *view_state = Some(state);
            }
            (Some(prev_action), None) => {
                // Rare transition: the action was removed. Tear down the
                // outgoing action view before the widget drops its subtree.
                if let Some(mut state) = view_state.take() {
                    ctx.with_id(ACTION_ID, |ctx| {
                        let mut row = ThemedSidebarItem::row_mut(&mut element);
                        let mut action_widget = Flex::get_mut(&mut row, ACTIONS_INDEX);
                        let mut reveal_box = action_widget.downcast::<RevealBox>();
                        let mut child = RevealBox::child_mut(&mut reveal_box);
                        prev_action.teardown(&mut state, ctx, child.downcast());
                    });
                }
                let label = self.build_label();
                ThemedSidebarItem::set_content(&mut element, label, None::<NewWidget<Label>>);
            }
        }
        // Label text is not re-applied here; masonry's Label doesn't expose
        // stable post-construction text mutation. If the label string
        // changes the parent view should replace the whole item.
        let _ = &prev.label;
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        if let (Some(action), Some(state)) = (&self.action, view_state.as_mut()) {
            ctx.with_id(ACTION_ID, |ctx| {
                let mut row = ThemedSidebarItem::row_mut(&mut element);
                let mut action_widget = Flex::get_mut(&mut row, ACTIONS_INDEX);
                let mut reveal_box = action_widget.downcast::<RevealBox>();
                let mut child = RevealBox::child_mut(&mut reveal_box);
                action.teardown(state, ctx, child.downcast());
            });
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if message.remaining_path().is_empty() {
            return match message.take_message::<ButtonPress>() {
                Some(_press) => MessageResult::Action((self.callback)(app_state)),
                None => MessageResult::Stale,
            };
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        if id.routing_id() != ACTION_ID.routing_id() {
            return MessageResult::Stale;
        }
        match (&self.action, view_state.as_mut()) {
            (Some(action), Some(state)) => {
                let mut row = ThemedSidebarItem::row_mut(&mut element);
                let mut action_widget = Flex::get_mut(&mut row, ACTIONS_INDEX);
                let mut reveal_box = action_widget.downcast::<RevealBox>();
                let mut child = RevealBox::child_mut(&mut reveal_box);
                action.message(state, message, child.downcast(), app_state)
            }
            _ => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sidebar_item;
    use crate::Theme;

    #[test]
    fn selected_is_the_canonical_builder_name() {
        let theme = Theme::default();
        // `State`/`Action` are inferred from the closure's `&mut u8`
        // annotation and its `()`-returning body — no turbofish needed on
        // `render` now that `SidebarItem`/`SidebarItemView` carry `State`/
        // `Action` as struct type parameters (required so `.action(view)`
        // can be typed).
        let _ = sidebar_item("Charts", |_: &mut u8| {})
            .selected(true)
            .render(&theme);
    }

    #[test]
    fn action_attaches_a_trailing_control() {
        let theme = Theme::default();
        let _ = sidebar_item("Charts", |_: &mut u8| {})
            .action(crate::button(|_: &mut u8| {}).label("Edit").render(&theme))
            .render(&theme);
    }
}
