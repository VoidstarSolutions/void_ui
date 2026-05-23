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

use masonry::properties::AutoHideScrollBar;
use masonry::widgets;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::Theme;

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
    auto_hide: bool,
}

/// Wrap `child` in a scroll container with scrollbars on both axes.
pub fn scroll_container<V>(child: V) -> ScrollContainer<V> {
    ScrollContainer {
        child,
        constrain_horizontal: false,
        constrain_vertical: false,
        fill: false,
        auto_hide: false,
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

    /// When `true`, scrollbars fade out when the pointer is not moving over
    /// the container and reappear on pointer activity or scroll wheel use.
    pub fn auto_hide(mut self, v: bool) -> Self {
        self.auto_hide = v;
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
            auto_hide: self.auto_hide,
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
    auto_hide: bool,
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
        let pod = Pod::new_with_props(widget, AutoHideScrollBar(self.auto_hide));
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
        if self.auto_hide != prev.auto_hide {
            element.insert_prop(AutoHideScrollBar(self.auto_hide));
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
