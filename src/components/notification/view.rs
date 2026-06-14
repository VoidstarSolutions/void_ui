//! Xilem view for the notification (toast) component.
//!
//! A themed message card — built on [`crate::components::alert::Alert`] —
//! that can auto-dismiss after a configurable timeout in addition to its
//! close (X) button. There is no positioning or stacking host here: place
//! the rendered card(s) yourself, e.g. via [`notification_stack`], then
//! register the stack as a corner-anchored overlay with [`notification_layer`].
//!
//! ```ignore
//! use std::time::Duration;
//! use void_ui::components::notification::notification;
//! use void_ui::AlertVariant;
//!
//! notification("Saved successfully.")
//!     .variant(AlertVariant::Success)
//!     .with_timeout(Duration::from_secs(3))
//!     .on_close(|s: &mut State| s.dismiss_toast(id))
//!     .render(&theme)
//! ```
//!
//! If this card's host widget can be reused for a *different* toast across
//! rebuilds (e.g. a `flex_col` stack where dismissing one toast shifts the
//! rest into its slot), store each toast's own creation time and pass it via
//! [`Notification::created_at`] — see that method's docs.

use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};

use masonry::accesskit::Live;
use masonry::core::ArcStr;
use masonry::layout::UnitPoint;
use masonry::widgets::SizedBox;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::widget::{NotificationHost, NotificationTimeout};
use crate::components::alert::{CloseCallback, alert};
use crate::overlay_portal::{PortalContentView, PortalPlacement, portal_from_env};
use crate::{AlertVariant, IconName, Theme};

/// Default auto-dismiss delay, matching gpui-component's notification default.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default fixed width for a [`notification_overlay`]'s toast stack, in px.
pub const DEFAULT_NOTIFICATION_WIDTH: f64 = 280.0;

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
    timeout_explicit: bool,
    created_at: Instant,
    on_close: C,
}

/// Create a notification with the given message.
///
/// Defaults to [`AlertVariant::Default`] (no accent color, no icon) and a
/// [`DEFAULT_TIMEOUT`] auto-dismiss countdown (only takes effect once
/// [`Notification::on_close`] is set — see [`Self::with_timeout`]).
/// `created_at` defaults to [`Instant::now`] at the point this builder is
/// constructed — override with [`Self::created_at`] if this notification's
/// host widget may be reused for a different toast across rebuilds (see that
/// method's docs).
pub fn notification(message: impl Into<ArcStr>) -> Notification {
    Notification {
        message: message.into(),
        title: None,
        variant: AlertVariant::Default,
        icon: None,
        show_icon: true,
        timeout: Some(DEFAULT_TIMEOUT),
        timeout_explicit: false,
        created_at: Instant::now(),
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
    /// The countdown starts from [`Self::created_at`] (which defaults to
    /// [`Instant::now`] at the time [`notification`] was called) —
    /// override it explicitly if this card's host widget may be reused for
    /// a different toast across rebuilds, e.g. in a `flex_col` stack.
    ///
    /// Requires [`Self::on_close`] — with no callback there is nothing to
    /// notify when the timeout elapses, so [`Self::render`] panics if this
    /// is called without one.
    pub fn with_timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self.timeout_explicit = true;
        self
    }

    /// Disable auto-dismiss; the card persists until the user clicks X.
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self.timeout_explicit = false;
        self
    }

    /// Override the instant this toast was created (became visible), used as
    /// the starting point for its auto-dismiss countdown.
    ///
    /// Defaults to [`Instant::now`] at the time [`notification`] was called,
    /// which is correct for a toast that's only ever built once. If
    /// `flex_col` positional diffing can reuse this card's host widget for a
    /// *different* toast (e.g. a notification stack where an earlier toast
    /// is dismissed and the list shifts), the default is recomputed on every
    /// rebuild and the countdown never advances — store this toast's own
    /// creation time on its entry (e.g. `Instant::now()` when it's pushed)
    /// and pass the same value here on every render.
    pub fn created_at(mut self, created_at: Instant) -> Self {
        self.created_at = created_at;
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
            timeout_explicit: self.timeout_explicit,
            created_at: self.created_at,
            on_close,
        }
    }

    /// Materialize the xilem view at the supplied theme.
    ///
    /// # Panics
    ///
    /// Panics if [`Self::with_timeout`] was called without [`Self::on_close`]
    /// — such a timeout can never fire, since there is no callback to notify.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<C, State, Action>
    where
        State: 'static,
        Action: 'static,
        // `Clone` because the close callback is needed in two places — the
        // close button (via the inner Alert) and the auto-dismiss timer. `()`
        // is `Clone`, and closures with no captures (or `Clone` captures,
        // e.g. a `Copy` toast id) are `Clone` automatically, so this is
        // rarely a real constraint.
        C: CloseCallback<State, Action> + Clone + Send + Sync + 'static,
    {
        assert!(
            !self.timeout_explicit || C::enabled(),
            "Notification::with_timeout() has no effect without Notification::on_close()"
        );
        let timeout = if C::enabled() { self.timeout } else { None };
        let armed_at = timeout.map(|_| self.created_at);
        // Errors interrupt assistive technology immediately (`Role::Alert` +
        // `Live::Assertive`); other variants announce politely once the user
        // is idle (`Role::Status` + `Live::Polite`) so routine toasts don't
        // talk over whatever the user is currently doing.
        let live = match self.variant {
            AlertVariant::Error => Live::Assertive,
            _ => Live::Polite,
        };
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
            armed_at,
            live,
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
    armed_at: Option<Instant>,
    live: Live,
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
        let widget = NotificationHost::new(
            child.new_widget.erased(),
            self.timeout,
            self.armed_at,
            self.live,
        );
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
        NotificationHost::set_timeout(&mut element, self.timeout, self.armed_at);
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
/// Pass the result to [`notification_layer`] to anchor it to one of the 6
/// [`NotificationPosition`] corners as a portal overlay — the active list of
/// toasts, dismissal, and sizing are all the host application's
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

/// Register `content` (typically a [`notification_stack`]) as a corner-anchored
/// layer in the nearest ancestor [`crate::overlay_scope`]'s portal.
///
/// The scope's view mounts the content in its always-on-top `PortalSlot`,
/// sized to its own intrinsic content and aligned to `position`'s
/// [`UnitPoint`] corner (see [`NotificationPosition`]'s conversion). The
/// returned view itself renders nothing in-tree — wrap your app's root (or
/// the region toasts should float over) in [`crate::overlay_scope`] and place
/// this view anywhere inside it.
///
/// # Panics
///
/// Panics at `build` if there is no ancestor [`crate::overlay_scope`].
pub fn notification_layer<State, Action, V>(
    content: V,
    theme: &Theme,
    position: NotificationPosition,
) -> NotificationLayerView<State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    let content: Arc<PortalContentView<State, Action>> = Arc::new(content);
    NotificationLayerView {
        content,
        theme: *theme,
        position,
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct NotificationLayerView<State, Action> {
    content: Arc<PortalContentView<State, Action>>,
    theme: Theme,
    position: NotificationPosition,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action> ViewMarker for NotificationLayerView<State, Action> {}

impl<State, Action> View<State, Action, ViewCtx> for NotificationLayerView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<SizedBox>;
    type ViewState = (crate::overlay_portal::OverlayPortal<State, Action>, u64);

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx)
            .expect("notification_layer requires an ancestor overlay_scope");
        let key = portal.register(
            self.content.clone(),
            &self.theme,
            PortalPlacement::Corner(self.position.into()),
        );
        let element = ctx.create_pod(SizedBox::empty());
        (element, (portal, key))
    }

    fn rebuild(
        &self,
        _prev: &Self,
        (portal, key): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        portal.update(
            *key,
            self.content.clone(),
            &self.theme,
            PortalPlacement::Corner(self.position.into()),
        );
    }

    fn teardown(
        &self,
        (portal, key): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        portal.deregister(*key);
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

/// Build a corner-anchored toast overlay from rendered notification views in
/// one call: stacks `items` with [`notification_stack`], wraps the stack in
/// a fixed-width, padded [`sized_box`], and registers it via
/// [`notification_layer`] anchored to `position`.
///
/// This is the one-call entry point for the common case. Consumers needing a
/// different width/padding/wrapper should compose [`notification_stack`] and
/// [`notification_layer`] directly instead.
///
/// # Panics
///
/// Panics at `build` if there is no ancestor [`crate::overlay_scope`] (see
/// [`notification_layer`]).
pub fn notification_overlay<State, Action>(
    theme: &Theme,
    position: NotificationPosition,
    items: Vec<Box<AnyWidgetView<State, Action>>>,
) -> NotificationLayerView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    let stack = sized_box(notification_stack(theme, items))
        .fixed_width(Length::px(DEFAULT_NOTIFICATION_WIDTH))
        .padding(Length::px(f64::from(theme.density.pad)));
    notification_layer(stack, theme, position)
}

#[cfg(test)]
mod tests {
    use super::{NotificationPosition, UnitPoint, notification};
    use crate::Theme;

    #[test]
    #[should_panic(
        expected = "Notification::with_timeout() has no effect without Notification::on_close()"
    )]
    fn render_panics_on_explicit_timeout_without_on_close() {
        let theme = Theme::default();
        let _ = notification("hi")
            .with_timeout(std::time::Duration::from_secs(2))
            .render::<(), ()>(&theme);
    }

    #[test]
    fn render_allows_explicit_timeout_with_on_close() {
        let theme = Theme::default();
        let _ = notification("hi")
            .with_timeout(std::time::Duration::from_secs(2))
            .on_close(|(): &mut ()| {})
            .render::<(), ()>(&theme);
    }

    #[test]
    fn render_allows_default_timeout_without_on_close() {
        let theme = Theme::default();
        // No explicit .with_timeout() call — the default DEFAULT_TIMEOUT must not panic.
        let _ = notification("hi").render::<(), ()>(&theme);
    }

    #[test]
    fn no_timeout_after_with_timeout_does_not_panic_without_on_close() {
        let theme = Theme::default();
        // .no_timeout() after .with_timeout() clears the explicit-timeout
        // flag too, so this must not trigger the "with_timeout() has no
        // effect without on_close()" panic.
        let _ = notification("hi")
            .with_timeout(std::time::Duration::from_secs(2))
            .no_timeout()
            .render::<(), ()>(&theme);
    }

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
