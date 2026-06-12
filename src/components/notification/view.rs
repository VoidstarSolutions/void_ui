//! Xilem view for the notification (toast) component.
//!
//! A themed message card — built on [`crate::components::alert::Alert`] —
//! that can auto-dismiss after a configurable timeout in addition to its
//! close (X) button. There is no positioning or stacking host here: place
//! the rendered card(s) yourself, e.g. via [`notification_stack`] inside a
//! `zstack` aligned with [`NotificationPosition`].
//!
//! ```ignore
//! use std::time::Duration;
//! use void_ui::components::notification::notification;
//! use void_ui::AlertVariant;
//!
//! notification("Saved successfully.")
//!     .variant(AlertVariant::Success)
//!     .timeout(Duration::from_secs(3))
//!     .on_close(|s: &mut State| s.dismiss_toast(id))
//!     .render(&theme)
//! ```

use std::marker::PhantomData;
use std::time::Duration;

use masonry::core::ArcStr;
use masonry::layout::UnitPoint;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::widget::{NotificationHost, NotificationOverlay, NotificationTimeout};
use crate::components::alert::{CloseCallback, alert};
use crate::{AlertVariant, IconName, Theme};

/// Default auto-dismiss delay, matching gpui-component's notification default.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Marker trait for [`Notification::on_close`] callbacks.
///
/// In addition to [`CloseCallback`] (shared with [`crate::components::alert::Alert`]),
/// a [`Notification`] needs its close callback in two places — the close
/// button (via the inner [`Alert`](crate::components::alert::Alert)) and the
/// auto-dismiss timer — so it must be [`Clone`]. `()` is `Clone`, and
/// closures with no captures (or `Clone` captures, e.g. a `Copy` toast id)
/// are `Clone` automatically, so this is rarely a real constraint.
pub trait DismissCallback<State, Action>:
    CloseCallback<State, Action> + Clone + Send + Sync + 'static
{
}

impl<T, State, Action> DismissCallback<State, Action> for T where
    T: CloseCallback<State, Action> + Clone + Send + Sync + 'static
{
}

/// Builder for a notification (toast) card.
///
/// Created with [`notification`]. Configure with builder methods; materialize
/// as a xilem view via [`Self::render`].
#[must_use = "Notification does nothing until rendered with .render(&theme)"]
pub struct Notification<C = ()> {
    message: ArcStr,
    title: Option<ArcStr>,
    variant: AlertVariant,
    icon: Option<IconName>,
    show_icon: bool,
    timeout: Option<Duration>,
    on_close: C,
}

/// Create a notification with the given message.
///
/// Defaults to [`AlertVariant::Default`] (no accent color, no icon) and a
/// [`DEFAULT_TIMEOUT`] auto-dismiss countdown (only takes effect once
/// [`Notification::on_close`] is set — see [`Self::timeout`]).
pub fn notification(message: impl Into<ArcStr>) -> Notification {
    Notification {
        message: message.into(),
        title: None,
        variant: AlertVariant::Default,
        icon: None,
        show_icon: true,
        timeout: Some(DEFAULT_TIMEOUT),
        on_close: (),
    }
}

impl<C> Notification<C> {
    /// Set the visual style variant. Also selects the default leading icon
    /// for that variant (override with [`Self::icon`] or suppress with
    /// [`Self::no_icon`]).
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set a title shown above the message in the variant's accent color.
    pub fn title(mut self, title: impl Into<ArcStr>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Override the leading icon. Defaults to the variant's icon, if any.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Suppress the leading icon entirely, even if the variant has a default.
    pub fn no_icon(mut self) -> Self {
        self.show_icon = false;
        self
    }

    /// Auto-dismiss after `duration` of being shown.
    ///
    /// Ignored unless [`Self::on_close`] is set — with no callback there is
    /// nothing to notify when the timeout elapses, so the timer never arms.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Disable auto-dismiss; the card persists until the user clicks X.
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Show a close (X) button and arm the auto-dismiss timer (if any);
    /// both invoke `on_close` when triggered.
    pub fn on_close<F>(self, on_close: F) -> Notification<F> {
        Notification {
            message: self.message,
            title: self.title,
            variant: self.variant,
            icon: self.icon,
            show_icon: self.show_icon,
            timeout: self.timeout,
            on_close,
        }
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<C, State, Action>
    where
        State: 'static,
        Action: 'static,
        C: DismissCallback<State, Action>,
    {
        let timeout = if C::enabled() { self.timeout } else { None };
        let on_close = self.on_close.clone();

        let mut a = alert(self.message).variant(self.variant);
        if let Some(title) = self.title {
            a = a.title(title);
        }
        if let Some(icon) = self.icon {
            a = a.icon(icon);
        }
        if !self.show_icon {
            a = a.no_icon();
        }
        // Variant backgrounds are translucent accent tints, designed to sit
        // on a page background. A toast floats over arbitrary content, so
        // back it with an opaque surface to flatten that tint into a solid
        // card instead of letting whatever's underneath show through.
        let content = sized_box(a.on_close(self.on_close).render(theme))
            .background_color(theme.palette.surface)
            .corner_radius(Length::px(f64::from(theme.radius.small)));

        NotificationView {
            content,
            timeout,
            on_close,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Notification`].
///
/// Built only through [`Notification::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct NotificationView<V, C, State, Action> {
    content: V,
    timeout: Option<Duration>,
    on_close: C,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, C, State, Action> ViewMarker for NotificationView<V, C, State, Action> {}

impl<V, C, State, Action> View<State, Action, ViewCtx> for NotificationView<V, C, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
    C: CloseCallback<State, Action>,
{
    type Element = Pod<NotificationHost>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.content.build(ctx, app_state);
        let widget = NotificationHost::new(child.new_widget.erased(), self.timeout);
        // `with_action_widget` registers the widget as an action source so
        // `NotificationTimeout` bubbles up to this view's `message` handler.
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
        let mut child = NotificationHost::child_mut(&mut element);
        self.content
            .rebuild(&prev.content, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut child = NotificationHost::child_mut(&mut element);
            self.content.teardown(view_state, ctx, child.downcast());
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
            match message.take_message::<NotificationTimeout>() {
                Some(_) => MessageResult::Action(self.on_close.call(app_state)),
                None => MessageResult::Stale,
            }
        } else {
            let mut child = NotificationHost::child_mut(&mut element);
            self.content
                .message(view_state, message, child.downcast(), app_state)
        }
    }
}

/// One of the 6 viewport corners a notification stack can be anchored to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationPosition {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl From<NotificationPosition> for UnitPoint {
    fn from(position: NotificationPosition) -> Self {
        match position {
            NotificationPosition::TopLeft => Self::TOP_LEFT,
            NotificationPosition::TopCenter => Self::TOP,
            NotificationPosition::TopRight => Self::TOP_RIGHT,
            NotificationPosition::BottomLeft => Self::BOTTOM_LEFT,
            NotificationPosition::BottomCenter => Self::BOTTOM,
            NotificationPosition::BottomRight => Self::BOTTOM_RIGHT,
        }
    }
}

/// Stack a column of notification cards with consistent spacing.
///
/// Place the result inside a `zstack` and pair it with
/// `sized_box(...).alignment(position.into())` to anchor it to one of the 6
/// [`NotificationPosition`] corners — the active list of toasts, dismissal,
/// width, and the surrounding `zstack` are all the host application's
/// responsibility.
#[must_use]
pub fn notification_stack<State: 'static, Action: 'static>(
    theme: &Theme,
    items: Vec<Box<AnyWidgetView<State, Action>>>,
) -> impl WidgetView<State, Action> + use<State, Action> {
    flex_col(items)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(f64::from(theme.density.pad) * 0.5))
}

/// Wrap `content` (typically a [`notification_stack`]) so it reports its
/// intrinsic content size to a surrounding `zstack`, rather than expanding
/// to fill it.
///
/// Place the result in a `zstack` covering the whole window and chain
/// `.alignment(UnitPoint::from(position))` (see [`NotificationPosition`]'s
/// [`UnitPoint`] conversion) onto it to anchor the stack to one of the 6
/// corners. See [`NotificationOverlay`] for why this wrapper is needed —
/// without it, `ZStack`'s alignment can't tell top from bottom.
pub fn notification_overlay<State, Action, V>(
    content: V,
) -> NotificationOverlayView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    NotificationOverlayView {
        content,
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct NotificationOverlayView<V, State, Action> {
    content: V,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, State, Action> ViewMarker for NotificationOverlayView<V, State, Action> {}

impl<V, State, Action> View<State, Action, ViewCtx> for NotificationOverlayView<V, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<NotificationOverlay>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.content.build(ctx, app_state);
        let widget = NotificationOverlay::new(child.new_widget.erased());
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
        let mut child = NotificationOverlay::child_mut(&mut element);
        self.content
            .rebuild(&prev.content, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = NotificationOverlay::child_mut(&mut element);
        self.content.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut child = NotificationOverlay::child_mut(&mut element);
        self.content
            .message(view_state, message, child.downcast(), app_state)
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationPosition, UnitPoint};

    #[test]
    fn position_maps_to_matching_unit_point_corner() {
        assert_eq!(
            UnitPoint::from(NotificationPosition::TopLeft),
            UnitPoint::TOP_LEFT
        );
        assert_eq!(
            UnitPoint::from(NotificationPosition::TopCenter),
            UnitPoint::TOP
        );
        assert_eq!(
            UnitPoint::from(NotificationPosition::TopRight),
            UnitPoint::TOP_RIGHT
        );
        assert_eq!(
            UnitPoint::from(NotificationPosition::BottomLeft),
            UnitPoint::BOTTOM_LEFT
        );
        assert_eq!(
            UnitPoint::from(NotificationPosition::BottomCenter),
            UnitPoint::BOTTOM
        );
        assert_eq!(
            UnitPoint::from(NotificationPosition::BottomRight),
            UnitPoint::BOTTOM_RIGHT
        );
    }

    #[test]
    fn default_position_is_top_right() {
        assert_eq!(
            NotificationPosition::default(),
            NotificationPosition::TopRight
        );
    }
}
