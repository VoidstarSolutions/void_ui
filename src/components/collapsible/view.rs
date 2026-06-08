//! Animated collapsible section — xilem view wrapper.
//!
//! Wraps [`CollapsibleWidget`] so it fits the `.render(&theme)` API convention.
//! Provide the section title as a string, the body content as a child
//! `WidgetView`, and a callback invoked when the user clicks the header.
//!
//! ```ignore
//! collapsible(
//!     "Advanced options",
//!     flex_col((
//!         checkbox("Enable debug mode", |s: &mut State| s.debug = !s.debug)
//!             .checked(s.debug)
//!             .render(&theme),
//!     )),
//!     |s: &mut State| s.advanced_open = !s.advanced_open,
//! )
//! .open(state.advanced_open)
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::{ArcStr, FromDynWidget};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{CollapsibleTogglePressed, CollapsibleWidget};
use crate::Theme;
use crate::animated_clip::AnimatedClip;

/// Builder for an animated collapsible section.
///
/// Created with [`collapsible`]. Returns a xilem `WidgetView` via
/// [`Self::render`].
#[must_use = "Collapsible does nothing until rendered with .render(&theme)"]
pub struct Collapsible<V, F> {
    title: ArcStr,
    child: V,
    open: bool,
    on_toggle: F,
}

/// Wrap `child` in an animated collapsible section.
///
/// `on_toggle` is called when the user clicks the header row. Use `.open(bool)`
/// to drive the current state. Defaults to open.
pub fn collapsible<V, F>(title: impl Into<ArcStr>, child: V, on_toggle: F) -> Collapsible<V, F> {
    Collapsible {
        title: title.into(),
        child,
        open: true,
        on_toggle,
    }
}

impl<V, F> Collapsible<V, F> {
    /// Set whether the section is expanded. Defaults to `true` (open).
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> CollapsibleView<V, F, State, Action>
    where
        State: 'static,
        Action: 'static,
        V: WidgetView<State, Action>,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        CollapsibleView {
            title: self.title,
            child: self.child,
            open: self.open,
            on_toggle: self.on_toggle,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Collapsible`].
///
/// Built only through [`Collapsible::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct CollapsibleView<V, F, State, Action> {
    title: ArcStr,
    child: V,
    open: bool,
    on_toggle: F,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, F, State, Action> ViewMarker for CollapsibleView<V, F, State, Action> {}

impl<V, F, State, Action> View<State, Action, ViewCtx> for CollapsibleView<V, F, State, Action>
where
    V: WidgetView<State, Action>,
    V::Widget: FromDynWidget + Sized,
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<CollapsibleWidget<V::Widget>>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) =
            ctx.with_id(ViewId::new(0), |ctx| self.child.build(ctx, app_state));
        let widget = CollapsibleWidget::new(
            self.title.clone(),
            child_pod.new_widget,
            &self.theme,
            self.open,
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
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
        if self.title != prev.title {
            CollapsibleWidget::set_title(&mut element, self.title.clone());
        }
        if self.theme != prev.theme {
            CollapsibleWidget::set_theme(&mut element, &self.theme);
        }
        if self.open != prev.open {
            CollapsibleWidget::set_open(&mut element, self.open);
        }
        ctx.with_id(ViewId::new(0), |ctx| {
            let mut body = CollapsibleWidget::body_mut(&mut element);
            let mut child = AnimatedClip::child_mut(&mut body);
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
            let mut body = CollapsibleWidget::body_mut(&mut element);
            let mut child = AnimatedClip::child_mut(&mut body);
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
        if message.remaining_path().is_empty() {
            if message.take_message::<CollapsibleTogglePressed>().is_some() {
                return MessageResult::Action((self.on_toggle)(app_state));
            }
            return MessageResult::Stale;
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        if id.routing_id() != 0 {
            return MessageResult::Stale;
        }
        let mut body = CollapsibleWidget::body_mut(&mut element);
        let mut child = AnimatedClip::child_mut(&mut body);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}
