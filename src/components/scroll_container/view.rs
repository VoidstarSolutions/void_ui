//! Scroll container — a clipping viewport with themed scrollbars on both axes.
//!
//! Wraps [`ScrollView`] in a builder so it fits the `.render(&theme)` API
//! convention. The clip rect excludes scrollbar track areas, so content is
//! never rendered behind scrollbars.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # let my_content_view = void_ui::label("content").render::<(), ()>(&theme);
//! use void_ui::components::scroll_container;
//! scroll_container(my_content_view)
//!     .render(&theme)
//! # ;
//! ```

use std::marker::PhantomData;

use masonry::core::FromDynWidget;
use masonry::properties::AutoHideScrollBar;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{ContentClip, ScrollView};
use crate::Theme;

/// Controls when scrollbars are shown in a [`ScrollContainer`].
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollBarVisibility {
    /// Scrollbars are always visible at full opacity.
    #[default]
    AlwaysVisible,
    /// Scrollbars appear on pointer activity and fade out after a short delay.
    OnActivity,
    /// Scrollbars are never shown; content fills the full viewport.
    AlwaysHidden,
}

impl ScrollBarVisibility {
    fn auto_hide(self) -> bool {
        matches!(self, Self::OnActivity)
    }

    fn always_hidden(self) -> bool {
        matches!(self, Self::AlwaysHidden)
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
    pub fn fill_viewport(mut self, v: bool) -> Self {
        self.fill = v;
        self
    }

    /// Controls when scrollbars are shown. Defaults to [`ScrollBarVisibility::AlwaysVisible`].
    pub fn scroll_bar_visibility(mut self, v: ScrollBarVisibility) -> Self {
        self.scroll_bar_visibility = v;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ScrollContainerView<V, State, Action>
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
            theme: *theme,
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
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for ScrollContainerView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for ScrollContainerView<V, State, Action>
where
    V: WidgetView<State, Action>,
    V::Widget: FromDynWidget + Sized,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ScrollView<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let scroll_view = ScrollView::new(child_pod.new_widget, &self.theme)
            .constrain_horizontal(self.constrain_horizontal)
            .constrain_vertical(self.constrain_vertical)
            .content_must_fill(self.fill)
            .always_hide_scrollbars(self.scroll_bar_visibility.always_hidden());
        let pod = Pod::new_with_props(
            scroll_view,
            AutoHideScrollBar(self.scroll_bar_visibility.auto_hide()),
        );
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
            ScrollView::set_theme(&mut element, &self.theme);
        }
        if self.constrain_horizontal != prev.constrain_horizontal {
            ScrollView::set_constrain_horizontal(&mut element, self.constrain_horizontal);
        }
        if self.constrain_vertical != prev.constrain_vertical {
            ScrollView::set_constrain_vertical(&mut element, self.constrain_vertical);
        }
        if self.fill != prev.fill {
            ScrollView::set_content_must_fill(&mut element, self.fill);
        }
        if self.scroll_bar_visibility != prev.scroll_bar_visibility {
            element.insert_prop(AutoHideScrollBar(self.scroll_bar_visibility.auto_hide()));
            ScrollView::set_always_hide_scrollbars(
                &mut element,
                self.scroll_bar_visibility.always_hidden(),
            );
        }
        {
            let mut clip = ScrollView::child_mut(&mut element);
            let mut child = ContentClip::child_mut(&mut clip);
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
        let mut clip = ScrollView::child_mut(&mut element);
        let mut child = ContentClip::child_mut(&mut clip);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut clip = ScrollView::child_mut(&mut element);
        let mut child = ContentClip::child_mut(&mut clip);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}

#[cfg(test)]
mod tests {
    use super::scroll_container;
    use crate::Theme;
    use crate::label;

    #[test]
    fn fill_viewport_is_the_canonical_builder_name() {
        let theme = Theme::default();
        let _ = scroll_container(label("content").render::<(), ()>(&theme))
            .fill_viewport(true)
            .render::<(), ()>(&theme);
    }
}
