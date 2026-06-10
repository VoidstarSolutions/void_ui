//! Xilem view for [`crate::components::tabs`].
//!
//! ```ignore
//! use void_ui::components::tabs::{tabs, TabItem, TabsVariant};
//!
//! tabs(
//!     vec![TabItem::label("Account"), TabItem::label("Profile")],
//!     state.selected_tab,
//!     |s: &mut State, i| s.selected_tab = i,
//! )
//! .variant(TabsVariant::Underline)
//! .render(&theme)
//! ```
//!
//! Selection is host-managed, mirroring [`crate::components::radio`]: the
//! host passes `selected` and an `on_select` callback that updates its own
//! state.

use std::marker::PhantomData;

use masonry::core::ArcStr;
use masonry::peniko::Color;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx};

use super::TabsVariant;
use super::widget::{TabSelected, TabsWidget};
use crate::Theme;
use crate::components::icon::{IconName, icon};
use crate::label;

// --- MARK: TAB ITEM

/// One item in a [`Tabs`] row — a label, an icon, or both.
#[derive(Clone, Debug)]
pub struct TabItem {
    label: Option<ArcStr>,
    icon: Option<IconName>,
}

impl TabItem {
    /// A text-only item.
    #[must_use]
    pub fn label(label: impl Into<ArcStr>) -> Self {
        Self {
            label: Some(label.into()),
            icon: None,
        }
    }

    /// An icon-only item.
    #[must_use]
    pub fn icon(icon: IconName) -> Self {
        Self {
            label: None,
            icon: Some(icon),
        }
    }

    /// Add a leading icon to a labeled item.
    #[must_use]
    pub fn with_icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// Builds the icon/label content for one item, colored for its
/// selected/unselected state.
fn item_content<State: 'static, Action: 'static>(
    item: &TabItem,
    color: Color,
    theme: &Theme,
) -> Box<AnyWidgetView<State, Action>> {
    let mut children: Vec<Box<AnyWidgetView<State, Action>>> = Vec::new();
    if let Some(name) = item.icon {
        children.push(Box::new(icon(name).color(color).render(theme)));
    }
    if let Some(text) = &item.label {
        children.push(Box::new(label(text.clone()).color(color).render(theme)));
    }
    Box::new(
        flex_row(children)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.col) / 2.0)),
    )
}

// --- MARK: BUILDER

/// View state for one item's content — the same erased state xilem uses for
/// `Box<AnyWidgetView<State, Action>>`.
type ItemViewState<State, Action> =
    <Box<AnyWidgetView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// Builder for a tabs row. Created with [`tabs`]. Returns a xilem
/// `WidgetView` via [`Self::render`].
#[must_use = "Tabs does nothing until rendered with .render(&theme)"]
pub struct Tabs<F, State, Action> {
    items: Vec<TabItem>,
    selected: usize,
    on_select: F,
    variant: TabsVariant,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Create a tabs row. `selected` is the active item's index; `on_select` is
/// called with the clicked item's index when the user picks a different tab.
pub fn tabs<State, Action, F>(
    items: Vec<TabItem>,
    selected: usize,
    on_select: F,
) -> Tabs<F, State, Action>
where
    F: Fn(&mut State, usize) -> Action + Send + Sync + 'static,
{
    Tabs {
        items,
        selected,
        on_select,
        variant: TabsVariant::default(),
        phantom: PhantomData,
    }
}

impl<F, State, Action> Tabs<F, State, Action> {
    /// Set the visual style. Defaults to [`TabsVariant::Default`].
    pub fn variant(mut self, variant: TabsVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> TabsView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, usize) -> Action + Send + Sync + 'static,
    {
        TabsView {
            items: self.items,
            selected: self.selected,
            on_select: self.on_select,
            variant: self.variant,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

// --- MARK: VIEW

/// The materialized [`View`] backing a [`Tabs`].
///
/// Built only through [`Tabs::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct TabsView<F, State, Action> {
    items: Vec<TabItem>,
    selected: usize,
    on_select: F,
    variant: TabsVariant,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> TabsView<F, State, Action> {
    fn item_color(&self, index: usize) -> Color {
        if index == self.selected {
            self.theme.palette.text
        } else {
            self.theme.palette.text_muted
        }
    }
}

impl<F, State, Action> ViewMarker for TabsView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for TabsView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, usize) -> Action + Send + Sync + 'static,
{
    type Element = Pod<TabsWidget>;
    type ViewState = Vec<ItemViewState<State, Action>>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut new_widgets = Vec::with_capacity(self.items.len());
        let mut states = Vec::with_capacity(self.items.len());
        for (i, item) in self.items.iter().enumerate() {
            let view = item_content::<State, Action>(item, self.item_color(i), &self.theme);
            #[allow(clippy::cast_possible_truncation)]
            let (pod, state) = ctx.with_id(ViewId::new(i as u64), |ctx| view.build(ctx, app_state));
            new_widgets.push(pod.new_widget.erased());
            states.push(state);
        }
        let widget = TabsWidget::new(new_widgets, self.variant, self.selected, &self.theme);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, states)
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
            TabsWidget::set_theme(&mut element, &self.theme);
        }
        if self.variant != prev.variant {
            TabsWidget::set_variant(&mut element, self.variant);
        }
        if self.selected != prev.selected {
            TabsWidget::set_selected(&mut element, self.selected);
        }

        if self.items.len() == prev.items.len() {
            for (i, (item, prev_item)) in self.items.iter().zip(&prev.items).enumerate() {
                let view = item_content::<State, Action>(item, self.item_color(i), &self.theme);
                let prev_view =
                    item_content::<State, Action>(prev_item, prev.item_color(i), &prev.theme);
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let mut child = TabsWidget::item_mut(&mut element, i);
                    view.rebuild(
                        &prev_view,
                        &mut view_state[i],
                        ctx,
                        child.downcast(),
                        app_state,
                    );
                });
            }
        } else {
            for (i, prev_item) in prev.items.iter().enumerate() {
                let prev_view =
                    item_content::<State, Action>(prev_item, prev.item_color(i), &prev.theme);
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let mut child = TabsWidget::item_mut(&mut element, i);
                    prev_view.teardown(&mut view_state[i], ctx, child.downcast());
                });
            }
            let mut new_widgets = Vec::with_capacity(self.items.len());
            let mut new_states = Vec::with_capacity(self.items.len());
            for (i, item) in self.items.iter().enumerate() {
                let view = item_content::<State, Action>(item, self.item_color(i), &self.theme);
                #[allow(clippy::cast_possible_truncation)]
                let (pod, state) =
                    ctx.with_id(ViewId::new(i as u64), |ctx| view.build(ctx, app_state));
                new_widgets.push(pod.new_widget.erased());
                new_states.push(state);
            }
            TabsWidget::set_items(&mut element, new_widgets);
            *view_state = new_states;
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        for (i, item) in self.items.iter().enumerate() {
            let view = item_content::<State, Action>(item, self.item_color(i), &self.theme);
            #[allow(clippy::cast_possible_truncation)]
            ctx.with_id(ViewId::new(i as u64), |ctx| {
                let mut child = TabsWidget::item_mut(&mut element, i);
                view.teardown(&mut view_state[i], ctx, child.downcast());
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
            if let Some(msg) = message.take_message::<TabSelected>() {
                return MessageResult::Action((self.on_select)(app_state, msg.0));
            }
            return MessageResult::Stale;
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        let index = usize::try_from(id.routing_id()).unwrap_or(usize::MAX);
        if let Some(item) = self.items.get(index) {
            let view = item_content::<State, Action>(item, self.item_color(index), &self.theme);
            let mut child = TabsWidget::item_mut(&mut element, index);
            view.message(&mut view_state[index], message, child.downcast(), app_state)
        } else {
            MessageResult::Stale
        }
    }
}
