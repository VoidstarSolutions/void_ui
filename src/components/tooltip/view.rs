//! Xilem view that wraps any child view with hover-driven tooltip behavior.
//!
//! ```ignore
//! use void_ui::components::{button, tooltip};
//! tooltip(
//!     "Reset the chart to defaults",
//!     button(|_: &mut State| {}).label("Reset").render(&theme),
//! )
//! .render(&theme)
//! ```

use std::marker::PhantomData;
use std::time::Duration;

use masonry::core::ArcStr;
use masonry::peniko::Color;
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use super::widget::TooltipHost;
use crate::Theme;

/// Default hover-idle delay before the tooltip appears.
pub const DEFAULT_DELAY_MS: u64 = 300;

/// One row of a [`tooltip_rows`] tooltip: a small filled dot in `color`
/// followed by `label`, e.g. one entry of a status-colour legend ("Ready"
/// beside a green dot).
#[derive(Clone, PartialEq)]
pub struct TooltipRow {
    pub(super) color: Color,
    pub(super) label: ArcStr,
}

impl TooltipRow {
    /// Creates a legend row: `color` renders as a small filled dot, `label`
    /// as text beside it.
    pub fn new(color: Color, label: impl Into<ArcStr>) -> Self {
        Self {
            color,
            label: label.into(),
        }
    }
}

/// What a [`Tooltip`] shows on its popped-up surface: either a single line
/// of text, or a vertical list of colour-coded [`TooltipRow`]s (e.g. a
/// status legend). `void_ui`'s tooltip layer can't host arbitrary child
/// views (see [`super::widget`]'s module docs), so rows are the one
/// richer shape it renders directly.
#[derive(Clone, PartialEq)]
pub(crate) enum TooltipContent {
    Text(ArcStr),
    Rows(Vec<TooltipRow>),
}

/// Builder for a hover-driven tooltip wrapping an inner view.
///
/// Created with [`tooltip`] or [`tooltip_rows`]. Returns a xilem `WidgetView`
/// via [`Self::render`].
#[must_use = "Tooltip does nothing until rendered with .render(&theme)"]
pub struct Tooltip<V> {
    content: TooltipContent,
    child: V,
    delay: Duration,
}

/// Wraps `child` with hover-driven tooltip behavior showing a single line
/// of `text`.
///
/// The tooltip surface appears after the pointer has been idle over the
/// child for the configured delay (default 300 ms). It dismisses itself on
/// the next pointer activity.
pub fn tooltip<V>(text: impl Into<ArcStr>, child: V) -> Tooltip<V> {
    Tooltip {
        content: TooltipContent::Text(text.into()),
        child,
        delay: Duration::from_millis(DEFAULT_DELAY_MS),
    }
}

/// Wraps `child` with hover-driven tooltip behavior showing a vertical list
/// of colour-coded [`TooltipRow`]s (e.g. a status-colour legend) instead of
/// a single line of text. Otherwise identical to [`tooltip`].
pub fn tooltip_rows<V>(rows: Vec<TooltipRow>, child: V) -> Tooltip<V> {
    Tooltip {
        content: TooltipContent::Rows(rows),
        child,
        delay: Duration::from_millis(DEFAULT_DELAY_MS),
    }
}

impl<V> Tooltip<V> {
    /// Sets the hover-idle delay before the tooltip appears.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> TooltipView<V, State, Action>
    where
        State: 'static,
        Action: 'static,
        V: WidgetView<State, Action>,
    {
        TooltipView {
            content: self.content,
            child: self.child,
            delay: self.delay,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Tooltip`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct TooltipView<V, State, Action> {
    content: TooltipContent,
    child: V,
    delay: Duration,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for TooltipView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for TooltipView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<TooltipHost>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = TooltipHost::new(
            child_pod.new_widget.erased(),
            self.content.clone(),
            &self.theme,
            self.delay,
        );
        (ctx.create_pod(widget), child_state)
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
            TooltipHost::set_theme(&mut element, &self.theme);
        }
        if self.content != prev.content {
            TooltipHost::set_content(&mut element, self.content.clone());
        }
        if self.delay != prev.delay {
            TooltipHost::set_delay(&mut element, self.delay);
        }
        let mut child = TooltipHost::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = TooltipHost::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut child = TooltipHost::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::tooltip;
    use crate::Theme;
    use crate::label;

    #[test]
    fn delay_is_the_canonical_builder_name() {
        let theme = Theme::default();
        let _ = tooltip("hint", label("child").render::<(), ()>(&theme))
            .delay(Duration::from_millis(500))
            .render::<(), ()>(&theme);
    }
}
