//! Animated collapsible sidebar panel — xilem view wrapper.
//!
//! Wraps [`ThemedSidebarPanel`] so it fits the `.render(&theme)` API
//! convention. Provide the sidebar's content as a child `WidgetView` and a
//! callback that is invoked when the user clicks the built-in toggle strip.
//!
//! ```ignore
//! sidebar_panel(
//!     flex_col((
//!         sidebar_item("Dashboard", |s: &mut State| s.nav = Nav::Dashboard)
//!             .selected(s.nav == Nav::Dashboard)
//!             .render(&theme),
//!     ))
//!     .cross_axis_alignment(CrossAxisAlignment::Stretch)
//!     .gap(Length::px(2.0)),
//!     |s: &mut State| s.sidebar_collapsed = !s.sidebar_collapsed,
//! )
//! .collapsed(state.sidebar_collapsed)
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::FromDynWidget;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::panel_widget::{SidebarTogglePressed, ThemedSidebarPanel};
use crate::Theme;
use crate::animated_clip::AnimatedClip;

/// Builder for an animated collapsible sidebar panel.
///
/// Created with [`sidebar_panel`]. Returns a xilem `WidgetView` via
/// [`Self::render`].
#[must_use = "SidebarPanel does nothing until rendered with .render(&theme)"]
pub struct SidebarPanel<V, F> {
    child: V,
    collapsed: bool,
    on_toggle: F,
}

/// Wrap `child` in an animated sidebar panel with a built-in toggle strip.
///
/// `on_toggle` is called when the user clicks the right-edge strip. Use
/// `.collapsed(bool)` to set the initial / current state.
pub fn sidebar_panel<V, F>(child: V, on_toggle: F) -> SidebarPanel<V, F> {
    SidebarPanel {
        child,
        collapsed: false,
        on_toggle,
    }
}

impl<V, F> SidebarPanel<V, F> {
    /// Set the collapsed state. Defaults to `false` (expanded).
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> SidebarPanelView<V, F, State, Action>
    where
        State: 'static,
        Action: 'static,
        V: WidgetView<State, Action>,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        SidebarPanelView {
            child: self.child,
            collapsed: self.collapsed,
            on_toggle: self.on_toggle,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`SidebarPanel`].
///
/// Built only through [`SidebarPanel::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SidebarPanelView<V, F, State, Action> {
    child: V,
    collapsed: bool,
    on_toggle: F,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, F, State, Action> ViewMarker for SidebarPanelView<V, F, State, Action> {}

impl<V, F, State, Action> View<State, Action, ViewCtx> for SidebarPanelView<V, F, State, Action>
where
    V: WidgetView<State, Action>,
    V::Widget: FromDynWidget + Sized,
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedSidebarPanel<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) =
            ctx.with_id(ViewId::new(0), |ctx| self.child.build(ctx, app_state));
        let panel = ThemedSidebarPanel::new(child_pod.new_widget, &self.theme, self.collapsed);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(panel));
        (element, child_state)
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
        ctx.with_id(ViewId::new(0), |ctx| {
            let mut content = ThemedSidebarPanel::content_mut(&mut element);
            let mut child = AnimatedClip::child_mut(&mut content);
            self.child
                .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            let mut content = ThemedSidebarPanel::content_mut(&mut element);
            let mut child = AnimatedClip::child_mut(&mut content);
            self.child.teardown(view_state, ctx, child.downcast());
        });
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // `take_message` asserts that the message has reached its final target
        // (remaining_path is empty). Only call it when we're the target; otherwise
        // the message is destined for a child action widget (e.g. a sidebar item)
        // and must be routed further.
        if message.remaining_path().is_empty() {
            if message.take_message::<SidebarTogglePressed>().is_some() {
                return MessageResult::Action((self.on_toggle)(app_state));
            }
            return MessageResult::Stale;
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        if id.routing_id() != 0 {
            return MessageResult::Stale;
        }
        let mut content = ThemedSidebarPanel::content_mut(&mut element);
        let mut child = AnimatedClip::child_mut(&mut content);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}
