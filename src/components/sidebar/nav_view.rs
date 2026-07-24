//! Xilem view for a roving-tabindex sidebar nav list.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { selected_section: usize }
//! # let state = State { selected_section: 0 };
//! use void_ui::components::sidebar::{SidebarNavItem, sidebar_nav};
//!
//! sidebar_nav(
//!     vec![
//!         SidebarNavItem::new("Dashboard"),
//!         SidebarNavItem::new("Charts"),
//!         SidebarNavItem::new("Settings"),
//!     ],
//!     state.selected_section,
//!     |s: &mut State, i| s.selected_section = i,
//! )
//! .render(&theme)
//! # ;
//! ```
//!
//! Selection is host-managed, mirroring [`crate::components::tabs::tabs`]:
//! the host passes `active` and an `on_select` callback that updates its own
//! state. The whole list is a single Tab stop; Up/Down move a roving focus
//! indicator among items, and Enter/Space activate the focused item.

use std::marker::PhantomData;

use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{AnyWidgetView, Pod, ViewCtx};

use super::nav_widget::{SidebarNavItemNode, SidebarNavSelected, ThemedSidebarNav};
use crate::Theme;
use crate::label;

// --- MARK: NAV ITEM

/// One item in a [`sidebar_nav`] list — a label with an optional accessible
/// name override.
#[derive(Clone, Debug)]
pub struct SidebarNavItem {
    label: ArcStr,
    aria_label: Option<ArcStr>,
}

impl SidebarNavItem {
    /// Create a new nav item with the given label.
    #[must_use]
    pub fn new(label: impl Into<ArcStr>) -> Self {
        Self {
            label: label.into(),
            aria_label: None,
        }
    }

    /// Sets the accessible name announced by screen readers, overriding the
    /// visible label.
    #[must_use]
    pub fn aria_label(mut self, label: impl Into<ArcStr>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    /// The accessible name for this item: [`Self::aria_label`] if set,
    /// otherwise the visible label.
    fn accessible_name(&self) -> Option<ArcStr> {
        self.aria_label.clone().or_else(|| Some(self.label.clone()))
    }
}

/// Builds the label content for one item, colored for its active state.
fn item_content<State: 'static, Action: 'static>(
    item: &SidebarNavItem,
    active: bool,
    theme: &Theme,
) -> Box<AnyWidgetView<State, Action>> {
    let color = if active {
        theme.palette.text
    } else {
        theme.palette.text_muted
    };
    Box::new(label(item.label.clone()).color(color).render(theme))
}

// --- MARK: BUILDER

/// View state for one item's content — the same erased state xilem uses for
/// `Box<AnyWidgetView<State, Action>>`.
type ItemViewState<State, Action> =
    <Box<AnyWidgetView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// Builder for a sidebar nav list. Created with [`sidebar_nav`]. Returns a
/// xilem `WidgetView` via [`Self::render`].
#[must_use = "SidebarNav does nothing until rendered with .render(&theme)"]
pub struct SidebarNav<F, State, Action> {
    items: Vec<SidebarNavItem>,
    active: usize,
    on_select: F,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Create a sidebar nav list. `active` is the currently-selected item's
/// index; `on_select` is called with the activated item's index when the
/// user picks a different one (by click, or Enter/Space on the
/// roving-focused item).
pub fn sidebar_nav<State, Action, F>(
    items: Vec<SidebarNavItem>,
    active: usize,
    on_select: F,
) -> SidebarNav<F, State, Action>
where
    F: Fn(&mut State, usize) -> Action + Send + Sync + 'static,
{
    SidebarNav {
        items,
        active,
        on_select,
        phantom: PhantomData,
    }
}

impl<F, State, Action> SidebarNav<F, State, Action> {
    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> SidebarNavView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, usize) -> Action + Send + Sync + 'static,
    {
        SidebarNavView {
            items: self.items,
            active: self.active,
            on_select: self.on_select,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

// --- MARK: VIEW

/// The materialized [`View`] backing a [`SidebarNav`].
///
/// Built only through [`SidebarNav::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SidebarNavView<F, State, Action> {
    items: Vec<SidebarNavItem>,
    active: usize,
    on_select: F,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for SidebarNavView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for SidebarNavView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, usize) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedSidebarNav>;
    type ViewState = Vec<ItemViewState<State, Action>>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut new_widgets = Vec::with_capacity(self.items.len());
        let mut states = Vec::with_capacity(self.items.len());
        for (i, item) in self.items.iter().enumerate() {
            let view = item_content::<State, Action>(item, i == self.active, &self.theme);
            #[allow(clippy::cast_possible_truncation)]
            let (pod, state) = ctx.with_id(ViewId::new(i as u64), |ctx| view.build(ctx, app_state));
            new_widgets.push(pod.new_widget.erased());
            states.push(state);
        }
        let names = self
            .items
            .iter()
            .map(SidebarNavItem::accessible_name)
            .collect();
        let widget = ThemedSidebarNav::new(new_widgets, names, self.active, &self.theme);
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
            ThemedSidebarNav::set_theme(&mut element, &self.theme);
        }
        if self.active != prev.active {
            ThemedSidebarNav::set_active(&mut element, self.active);
        }

        if self.items.len() == prev.items.len() {
            for (i, (item, prev_item)) in self.items.iter().zip(&prev.items).enumerate() {
                let view = item_content::<State, Action>(item, i == self.active, &self.theme);
                let prev_view =
                    item_content::<State, Action>(prev_item, i == prev.active, &prev.theme);
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let mut node = ThemedSidebarNav::item_mut(&mut element, i);
                    if item.accessible_name() != prev_item.accessible_name() {
                        SidebarNavItemNode::set_name(&mut node, item.accessible_name());
                    }
                    let mut child = SidebarNavItemNode::child_mut(&mut node);
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
                    item_content::<State, Action>(prev_item, i == prev.active, &prev.theme);
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let mut node = ThemedSidebarNav::item_mut(&mut element, i);
                    let mut child = SidebarNavItemNode::child_mut(&mut node);
                    prev_view.teardown(&mut view_state[i], ctx, child.downcast());
                });
            }
            let mut new_widgets = Vec::with_capacity(self.items.len());
            let mut new_states = Vec::with_capacity(self.items.len());
            for (i, item) in self.items.iter().enumerate() {
                let view = item_content::<State, Action>(item, i == self.active, &self.theme);
                #[allow(clippy::cast_possible_truncation)]
                let (pod, state) =
                    ctx.with_id(ViewId::new(i as u64), |ctx| view.build(ctx, app_state));
                new_widgets.push(pod.new_widget.erased());
                new_states.push(state);
            }
            let names = self
                .items
                .iter()
                .map(SidebarNavItem::accessible_name)
                .collect();
            ThemedSidebarNav::set_items(&mut element, new_widgets, names, self.active);
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
            let view = item_content::<State, Action>(item, i == self.active, &self.theme);
            #[allow(clippy::cast_possible_truncation)]
            ctx.with_id(ViewId::new(i as u64), |ctx| {
                let mut node = ThemedSidebarNav::item_mut(&mut element, i);
                let mut child = SidebarNavItemNode::child_mut(&mut node);
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
            if let Some(msg) = message.take_message::<SidebarNavSelected>() {
                return MessageResult::Action((self.on_select)(app_state, msg.0));
            }
            return MessageResult::Stale;
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        let index = usize::try_from(id.routing_id()).unwrap_or(usize::MAX);
        if let Some(item) = self.items.get(index) {
            let view = item_content::<State, Action>(item, index == self.active, &self.theme);
            let mut node = ThemedSidebarNav::item_mut(&mut element, index);
            let mut child = SidebarNavItemNode::child_mut(&mut node);
            view.message(&mut view_state[index], message, child.downcast(), app_state)
        } else {
            MessageResult::Stale
        }
    }
}
