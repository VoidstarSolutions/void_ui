//! Notification demo panel used by the void-ui gallery.
//!
//! Toasts are meant to float over the entire app, not just this panel, so
//! the toast list and corner position live in the host application's state
//! rather than being owned locally (contrast with most other demo panels —
//! see `project_xilem_local_state` in memory). The host's state must
//! implement [`AsMut<NotificationDemoState>`]; `examples/gallery.rs` stores
//! a [`NotificationDemoState`] and renders [`overlay`] in a `zstack` around
//! its whole window. This panel renders only the trigger buttons and
//! position picker.

use std::time::Duration;

use masonry::layout::Length;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{
    DEFAULT_TIMEOUT, NotificationPosition, notification, notification_layer, notification_stack,
};
use crate::components::ScrollBarVisibility;
use crate::layout::flex_wrap;
use crate::{AlertVariant, ButtonVariant, Theme, button, label, scroll_container};
use crate::{separator, with_source};

/// Map a toast's [`AlertVariant`] to the [`ButtonVariant`] of the button that triggers it.
fn button_variant_for(variant: AlertVariant) -> ButtonVariant {
    match variant {
        AlertVariant::Default => ButtonVariant::Default,
        AlertVariant::Info => ButtonVariant::Info,
        AlertVariant::Success => ButtonVariant::Success,
        AlertVariant::Warning => ButtonVariant::Warning,
        AlertVariant::Error => ButtonVariant::Danger,
    }
}

const POSITIONS: [(NotificationPosition, &str); 6] = [
    (NotificationPosition::TopLeft, "Top Left"),
    (NotificationPosition::TopCenter, "Top Center"),
    (NotificationPosition::TopRight, "Top Right"),
    (NotificationPosition::BottomLeft, "Bottom Left"),
    (NotificationPosition::BottomCenter, "Bottom Center"),
    (NotificationPosition::BottomRight, "Bottom Right"),
];

/// A single active toast, identified by `id` so its `on_close` can remove it.
#[derive(Debug, Clone)]
pub struct ToastEntry {
    pub id: u64,
    pub message: String,
    pub variant: AlertVariant,
    pub timeout: Option<Duration>,
}

/// Toast list + corner position for the notification demo.
///
/// Owned by the host application's state (e.g. `examples/gallery.rs`'s
/// `State`) so [`overlay`] can render toasts over the whole window.
#[derive(Debug, Clone, Default)]
pub struct NotificationDemoState {
    pub toasts: Vec<ToastEntry>,
    pub next_id: u64,
    pub position: NotificationPosition,
}

impl NotificationDemoState {
    /// Push a new toast onto the stack.
    pub fn push_toast(
        &mut self,
        message: &'static str,
        variant: AlertVariant,
        timeout: Option<Duration>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        self.toasts.push(ToastEntry {
            id,
            message: message.to_string(),
            variant,
            timeout,
        });
    }
}

/// Implemented by host state types that embed a [`NotificationDemoState`].
pub trait NotificationDemoHost: AsMut<NotificationDemoState> + 'static {}

impl<S: AsMut<NotificationDemoState> + 'static> NotificationDemoHost for S {}

fn section_header<S: 'static>(
    text: &'static str,
    theme: &Theme,
) -> impl WidgetView<S, ()> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn position_row<S: NotificationDemoHost>(
    theme: &Theme,
    demo: &NotificationDemoState,
) -> impl WidgetView<S, ()> + use<S> {
    let buttons: Vec<Box<AnyWidgetView<S, ()>>> = POSITIONS
        .iter()
        .map(|&(position, name)| -> Box<AnyWidgetView<S, ()>> {
            Box::new(
                button(move |s: &mut S| s.as_mut().position = position)
                    .label(name)
                    .active(demo.position == position)
                    .render(theme),
            )
        })
        .collect();
    flex_row(buttons)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(6.0))
}

fn trigger_button<S: NotificationDemoHost>(
    theme: &Theme,
    label_text: &'static str,
    message: &'static str,
    variant: AlertVariant,
    timeout: Option<Duration>,
) -> impl WidgetView<S, ()> + use<S> {
    button(move |s: &mut S| s.as_mut().push_toast(message, variant, timeout))
        .label(label_text)
        .variant(button_variant_for(variant))
        .render(theme)
}

fn triggers_row<S: NotificationDemoHost>(theme: &Theme) -> impl WidgetView<S, ()> + use<S> {
    let dismiss_all = button(|s: &mut S| s.as_mut().toasts.clear())
        .label("Dismiss All")
        .render(theme);

    flex_wrap((
        trigger_button::<S>(
            theme,
            "Show Notification",
            "This is a notification.",
            AlertVariant::Default,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button::<S>(
            theme,
            "Info",
            "You have been saved successfully.",
            AlertVariant::Info,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button::<S>(
            theme,
            "Success",
            "Your changes have been saved.",
            AlertVariant::Success,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button::<S>(
            theme,
            "Warning",
            "This action cannot be undone.",
            AlertVariant::Warning,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button::<S>(
            theme,
            "Error",
            "There was a problem with your request.",
            AlertVariant::Error,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button::<S>(
            theme,
            "Custom timeout (2s)",
            "This toast auto-dismisses after 2 seconds.",
            AlertVariant::Info,
            Some(Duration::from_secs(2)),
        ),
        trigger_button::<S>(
            theme,
            "No timeout",
            "This toast stays until dismissed manually.",
            AlertVariant::Warning,
            None,
        ),
        dismiss_all,
    ))
    .gap(8.0)
}

fn live_demo_section<S: NotificationDemoHost>(
    theme: &Theme,
    demo: &NotificationDemoState,
) -> impl WidgetView<S, ()> + use<S> {
    with_source!(theme, {
        flex_col((position_row::<S>(theme, demo), triggers_row::<S>(theme)))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(8.0))
    })
}

fn build_inner<S: NotificationDemoHost>(
    theme: &Theme,
    demo: &NotificationDemoState,
) -> impl WidgetView<S, ()> + use<S> {
    let title_block = flex_col((
        label("Notification")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Toast card built on Alert, with a close button and an optional auto-dismiss \
             timeout. Buttons below trigger toasts that float over the whole window, \
             anchored to the chosen corner.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            section_header::<S>("Live demo", theme),
            live_demo_section::<S>(theme, demo),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

/// Render the active toast stack as a corner-anchored overlay layer.
///
/// Registers the stack with the nearest ancestor [`crate::overlay_scope`]'s
/// portal, anchored to [`NotificationDemoState::position`] (see
/// [`notification_layer`]). Place the result anywhere inside an
/// `overlay_scope`-wrapped tree (see `examples/gallery.rs`) so toasts float
/// over the whole window, not just the notification demo panel.
#[must_use]
pub fn overlay<S: NotificationDemoHost>(
    theme: &Theme,
    demo: &NotificationDemoState,
) -> Box<AnyWidgetView<S, ()>> {
    let items: Vec<Box<AnyWidgetView<S, ()>>> = demo
        .toasts
        .iter()
        .map(|toast| -> Box<AnyWidgetView<S, ()>> {
            let id = toast.id;
            let mut n = notification(toast.message.clone()).variant(toast.variant);
            n = match toast.timeout {
                Some(timeout) => n.timeout(timeout),
                None => n.no_timeout(),
            };
            Box::new(
                n.on_close(move |s: &mut S| s.as_mut().toasts.retain(|t| t.id != id))
                    .render(theme),
            )
        })
        .collect();

    let stack = sized_box(notification_stack(theme, items))
        .fixed_width(Length::px(280.0))
        .padding(Length::px(8.0));

    Box::new(notification_layer(stack, theme, demo.position))
}

type InnerView<S> = Box<AnyWidgetView<S, ()>>;
type InnerViewState<S> = <InnerView<S> as View<S, (), ViewCtx>>::ViewState;

/// Opaque state owned by the notification demo panel.
pub struct NotificationDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct NotificationDemoPanelState<S: NotificationDemoHost> {
    inner_view: InnerView<S>,
    inner_state: InnerViewState<S>,
}

/// Renders the Notification demo panel (trigger buttons + position picker).
///
/// `S` must implement [`AsMut<NotificationDemoState>`]; pair this with
/// [`overlay`] rendered around the host application's whole window.
#[must_use]
pub fn panel(theme: &Theme) -> NotificationDemoPanel {
    NotificationDemoPanel { theme: *theme }
}

impl ViewMarker for NotificationDemoPanel {}

impl<S: NotificationDemoHost> View<S, (), ViewCtx> for NotificationDemoPanel {
    type ViewState = NotificationDemoPanelState<S>;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut S) -> (Self::Element, Self::ViewState) {
        let demo = app_state.as_mut().clone();
        let inner_view: InnerView<S> = Box::new(build_inner::<S>(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, app_state);
        (
            element,
            NotificationDemoPanelState {
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut NotificationDemoPanelState<S>,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        app_state: &mut S,
    ) {
        let demo = app_state.as_mut().clone();
        let new_inner: InnerView<S> = Box::new(build_inner::<S>(&self.theme, &demo));
        new_inner.rebuild(&vs.inner_view, &mut vs.inner_state, ctx, element, app_state);
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut NotificationDemoPanelState<S>,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut NotificationDemoPanelState<S>,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        app_state: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, app_state)
    }
}

#[cfg(test)]
mod tests {
    use super::{AlertVariant, DEFAULT_TIMEOUT, NotificationDemoState, NotificationPosition};

    #[test]
    fn default_state_is_empty_with_top_right_position() {
        let state = NotificationDemoState::default();
        assert!(state.toasts.is_empty());
        assert_eq!(state.next_id, 0);
        assert_eq!(state.position, NotificationPosition::TopRight);
    }

    #[test]
    fn push_toast_appends_with_incrementing_ids() {
        let mut state = NotificationDemoState::default();

        state.push_toast("first", AlertVariant::Info, Some(DEFAULT_TIMEOUT));
        state.push_toast("second", AlertVariant::Error, None);

        assert_eq!(state.toasts.len(), 2);

        assert_eq!(state.toasts[0].id, 0);
        assert_eq!(state.toasts[0].message, "first");
        assert_eq!(state.toasts[0].variant, AlertVariant::Info);
        assert_eq!(state.toasts[0].timeout, Some(DEFAULT_TIMEOUT));

        assert_eq!(state.toasts[1].id, 1);
        assert_eq!(state.toasts[1].message, "second");
        assert_eq!(state.toasts[1].variant, AlertVariant::Error);
        assert_eq!(state.toasts[1].timeout, None);

        assert_eq!(state.next_id, 2);
    }

    #[test]
    fn dismiss_all_clears_toasts_without_resetting_id_counter() {
        let mut state = NotificationDemoState::default();
        state.push_toast("first", AlertVariant::Info, Some(DEFAULT_TIMEOUT));
        state.push_toast("second", AlertVariant::Info, Some(DEFAULT_TIMEOUT));

        state.toasts.clear();

        assert!(state.toasts.is_empty());
        assert_eq!(state.next_id, 2);

        state.push_toast("third", AlertVariant::Info, Some(DEFAULT_TIMEOUT));
        assert_eq!(state.toasts[0].id, 2);
    }
}
