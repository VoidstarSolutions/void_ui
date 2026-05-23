//! Scroll container — a clipping viewport with themed scrollbars on both axes.
//!
//! Wraps masonry's [`Portal`](masonry::widgets::Portal) widget in a `void_ui`
//! builder so it fits the `.render(&theme)` API convention. The scrollbars
//! are drawn by masonry; theme is stored for future color/width customization
//! via masonry's property system.
//!
//! ```ignore
//! use void_ui::components::scroll_container;
//! scroll_container(my_content_view)
//!     .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::properties::{AutoHideScrollBar, Collapsible};
use masonry::widgets;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::Theme;

/// Controls when scrollbars are shown in a [`ScrollContainer`].
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollBarVisibility {
    /// Scrollbars are always visible at full opacity.
    #[default]
    AlwaysVisible,
    /// Scrollbars appear on pointer activity and fade out after a short delay.
    ///
    /// The first hide cycle requires at least one app-state change (rebuild) to
    /// have occurred after the container was built, so that `Collapsible` is
    /// propagated to the scrollbar children. Any interactive widget in the same
    /// view tree satisfies this; the demo switcher does so automatically.
    OnActivity,
    /// Scrollbars are hidden at rest. Portal will briefly show them (~400 ms)
    /// when the pointer moves over the container — a known masonry limitation
    /// that cannot be suppressed from this layer without an upstream API change.
    AlwaysHidden,
}

impl ScrollBarVisibility {
    fn auto_hide(self) -> bool {
        matches!(self, Self::OnActivity | Self::AlwaysHidden)
    }

    fn collapsible(self) -> bool {
        matches!(self, Self::OnActivity | Self::AlwaysHidden)
    }
}

/// Builder for a scroll container.
///
/// Created with [`scroll_container`]. Returns a xilem `WidgetView` via
/// [`Self::render`].
#[must_use = "ScrollContainer does nothing until rendered with .render(&theme)"]
pub struct ScrollContainer<V> {
    child: V,
    constrain_horizontal: bool,
    constrain_vertical: bool,
    fill: bool,
    scroll_bar_visibility: ScrollBarVisibility,
}

/// Wrap `child` in a scroll container with scrollbars on both axes.
pub fn scroll_container<V>(child: V) -> ScrollContainer<V> {
    ScrollContainer {
        child,
        constrain_horizontal: false,
        constrain_vertical: false,
        fill: false,
        scroll_bar_visibility: ScrollBarVisibility::default(),
    }
}

impl<V> ScrollContainer<V> {
    /// When `true`, the child's width is bounded by the viewport; no horizontal
    /// scrollbar is shown and horizontal wheel input is ignored.
    pub fn constrain_horizontal(mut self, v: bool) -> Self {
        self.constrain_horizontal = v;
        self
    }

    /// When `true`, the child's height is bounded by the viewport; no vertical
    /// scrollbar is shown and vertical wheel input is ignored.
    pub fn constrain_vertical(mut self, v: bool) -> Self {
        self.constrain_vertical = v;
        self
    }

    /// When `true`, the child is guaranteed to be at least as large as the
    /// viewport on each axis.
    pub fn fill(mut self, v: bool) -> Self {
        self.fill = v;
        self
    }

    /// Controls when scrollbars are shown. Defaults to [`ScrollBarVisibility::AlwaysVisible`].
    pub fn scroll_bar_visibility(mut self, v: ScrollBarVisibility) -> Self {
        self.scroll_bar_visibility = v;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, _theme: &Theme) -> ScrollContainerView<V, State, Action>
    where
        State: 'static,
        Action: 'static,
        V: WidgetView<State, Action>,
    {
        ScrollContainerView {
            child: self.child,
            constrain_horizontal: self.constrain_horizontal,
            constrain_vertical: self.constrain_vertical,
            fill: self.fill,
            scroll_bar_visibility: self.scroll_bar_visibility,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`ScrollContainer`].
///
/// Built only through [`ScrollContainer::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ScrollContainerView<V, State, Action> {
    child: V,
    constrain_horizontal: bool,
    constrain_vertical: bool,
    fill: bool,
    scroll_bar_visibility: ScrollBarVisibility,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for ScrollContainerView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for ScrollContainerView<V, State, Action>
where
    V: WidgetView<State, Action>,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<widgets::Portal<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = widgets::Portal::new(child_pod.new_widget)
            .constrain_horizontal(self.constrain_horizontal)
            .constrain_vertical(self.constrain_vertical)
            .content_must_fill(self.fill);
        let pod = Pod::new_with_props(widget, AutoHideScrollBar(self.scroll_bar_visibility.auto_hide()));
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
        if self.constrain_horizontal != prev.constrain_horizontal {
            widgets::Portal::set_constrain_horizontal(&mut element, self.constrain_horizontal);
        }
        if self.constrain_vertical != prev.constrain_vertical {
            widgets::Portal::set_constrain_vertical(&mut element, self.constrain_vertical);
        }
        if self.fill != prev.fill {
            widgets::Portal::set_content_must_fill(&mut element, self.fill);
        }
        if self.scroll_bar_visibility != prev.scroll_bar_visibility {
            element.insert_prop(AutoHideScrollBar(self.scroll_bar_visibility.auto_hide()));
            {
                let mut h = widgets::Portal::horizontal_scrollbar_mut(&mut element);
                h.insert_prop(Collapsible(self.scroll_bar_visibility.collapsible()));
            }
            {
                let mut v = widgets::Portal::vertical_scrollbar_mut(&mut element);
                v.insert_prop(Collapsible(self.scroll_bar_visibility.collapsible()));
            }
        }
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child_element, app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child_element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let child_element = widgets::Portal::child_mut(&mut element);
        self.child
            .message(view_state, message, child_element, app_state)
    }
}
