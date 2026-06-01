//! Animated sidebar panel — xilem view wrapper.
//!
//! Wraps [`ThemedSidebarPanel`] in a builder so it fits the `.render(&theme)`
//! API convention. Pass any child `WidgetView` (typically a `flex_col` of
//! [`super::sidebar_item`]s with a [`super::sidebar_collapse_button`] at the
//! top) and a host-controlled `collapsed` flag.
//!
//! ```ignore
//! sidebar_panel(
//!     flex_col((
//!         sidebar_collapse_button(|s: &mut State| s.sidebar_collapsed = true)
//!             .render(&theme),
//!         sidebar_item("Dashboard", |s: &mut State| s.nav = Nav::Dashboard)
//!             .active(s.nav == Nav::Dashboard)
//!             .render(&theme),
//!     ))
//!     .cross_axis_alignment(CrossAxisAlignment::Stretch)
//!     .gap(Length::px(2.0)),
//!     state.sidebar_collapsed,
//! )
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::FromDynWidget;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::panel_widget::ThemedSidebarPanel;
use crate::Theme;

/// Builder for an animated sidebar panel.
///
/// Created with [`sidebar_panel`]. Returns a xilem `WidgetView` via
/// [`Self::render`].
#[must_use = "SidebarPanel does nothing until rendered with .render(&theme)"]
pub struct SidebarPanel<V> {
    child: V,
    collapsed: bool,
}

/// Wrap `child` in an animated sidebar panel.
///
/// `collapsed` is the host-controlled target state. When it toggles the panel
/// slides its width to 0 (hidden) or back to the child's natural width over
/// ~250 ms.
pub fn sidebar_panel<V>(child: V, collapsed: bool) -> SidebarPanel<V> {
    SidebarPanel { child, collapsed }
}

impl<V> SidebarPanel<V> {
    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> SidebarPanelView<V, State, Action>
    where
        State: 'static,
        Action: 'static,
        V: WidgetView<State, Action>,
    {
        SidebarPanelView {
            child: self.child,
            collapsed: self.collapsed,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`SidebarPanel`].
///
/// Built only through [`SidebarPanel::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SidebarPanelView<V, State, Action> {
    child: V,
    collapsed: bool,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for SidebarPanelView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for SidebarPanelView<V, State, Action>
where
    V: WidgetView<State, Action>,
    V::Widget: FromDynWidget + Sized,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ThemedSidebarPanel<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let panel = ThemedSidebarPanel::new(child_pod.new_widget, &self.theme)
            .with_collapsed(self.collapsed);
        let pod = Pod::new(panel);
        (pod, child_state)
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
            ThemedSidebarPanel::set_theme(&mut element, &self.theme);
        }
        if self.collapsed != prev.collapsed {
            ThemedSidebarPanel::set_collapsed(&mut element, self.collapsed);
        }
        {
            let mut child = ThemedSidebarPanel::child_mut(&mut element);
            self.child
                .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = ThemedSidebarPanel::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut child = ThemedSidebarPanel::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}
