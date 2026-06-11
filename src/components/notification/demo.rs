//! Notification demo panel used by the void-ui gallery.

use std::time::Duration;

use masonry::layout::{Length, UnitPoint};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, ZStackExt as _, flex_col, flex_row, sized_box, zstack};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{DEFAULT_TIMEOUT, NotificationPosition, notification, notification_stack};
use crate::components::ScrollBarVisibility;
use crate::layout::flex_wrap;
use crate::with_source;
use crate::{AlertVariant, Theme, button, label, scroll_container};

const POSITIONS: [(NotificationPosition, &str); 6] = [
    (NotificationPosition::TopLeft, "Top Left"),
    (NotificationPosition::TopCenter, "Top Center"),
    (NotificationPosition::TopRight, "Top Right"),
    (NotificationPosition::BottomLeft, "Bottom Left"),
    (NotificationPosition::BottomCenter, "Bottom Center"),
    (NotificationPosition::BottomRight, "Bottom Right"),
];

#[derive(Debug, Clone)]
struct ToastEntry {
    id: u64,
    message: String,
    variant: AlertVariant,
    timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
struct NotificationDemo {
    toasts: Vec<ToastEntry>,
    next_id: u64,
    position: NotificationPosition,
}

impl NotificationDemo {
    fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 0,
            position: NotificationPosition::TopRight,
        }
    }

    fn push_toast(&mut self, message: &'static str, variant: AlertVariant, timeout: Option<Duration>) {
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

fn section_header(text: &'static str, theme: &Theme) -> impl WidgetView<NotificationDemo> + use<> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn position_row(
    theme: &Theme,
    state: &NotificationDemo,
) -> impl WidgetView<NotificationDemo> + use<> {
    let buttons: Vec<Box<AnyWidgetView<NotificationDemo>>> = POSITIONS
        .iter()
        .map(
            |&(position, name)| -> Box<AnyWidgetView<NotificationDemo>> {
                Box::new(
                    button(move |s: &mut NotificationDemo| s.position = position)
                        .label(name)
                        .active(state.position == position)
                        .render(theme),
                )
            },
        )
        .collect();
    flex_row(buttons)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(6.0))
}

fn trigger_button(
    theme: &Theme,
    label_text: &'static str,
    message: &'static str,
    variant: AlertVariant,
    timeout: Option<Duration>,
) -> impl WidgetView<NotificationDemo> + use<> {
    button(move |s: &mut NotificationDemo| s.push_toast(message, variant, timeout))
        .label(label_text)
        .render(theme)
}

fn triggers_row(theme: &Theme) -> impl WidgetView<NotificationDemo> + use<> {
    let dismiss_all = button(|s: &mut NotificationDemo| s.toasts.clear())
        .label("Dismiss All")
        .render(theme);

    flex_wrap((
        trigger_button(
            theme,
            "Show Notification",
            "This is a notification.",
            AlertVariant::Default,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button(
            theme,
            "Info",
            "You have been saved successfully.",
            AlertVariant::Info,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button(
            theme,
            "Success",
            "Your changes have been saved.",
            AlertVariant::Success,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button(
            theme,
            "Warning",
            "This action cannot be undone.",
            AlertVariant::Warning,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button(
            theme,
            "Error",
            "There was a problem with your request.",
            AlertVariant::Error,
            Some(DEFAULT_TIMEOUT),
        ),
        trigger_button(
            theme,
            "Custom timeout (2s)",
            "This toast auto-dismisses after 2 seconds.",
            AlertVariant::Info,
            Some(Duration::from_secs(2)),
        ),
        trigger_button(
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

fn live_demo_section(
    theme: &Theme,
    state: &NotificationDemo,
) -> impl WidgetView<NotificationDemo> + use<> {
    let controls = flex_col((position_row(theme, state), triggers_row(theme)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0));

    let placeholder = sized_box(label("Demo area").render(theme))
        .fixed_height(Length::px(320.0))
        .padding(Length::px(16.0))
        .background_color(theme.palette.surface_2)
        .border(theme.palette.border, Length::px(1.0))
        .corner_radius(Length::px(f64::from(theme.radius.small)));

    let items: Vec<Box<AnyWidgetView<NotificationDemo>>> = state
        .toasts
        .iter()
        .map(|toast| -> Box<AnyWidgetView<NotificationDemo>> {
            let id = toast.id;
            let mut n = notification(toast.message.clone()).variant(toast.variant);
            n = match toast.timeout {
                Some(timeout) => n.timeout(timeout),
                None => n.no_timeout(),
            };
            Box::new(
                n.on_close(move |s: &mut NotificationDemo| s.toasts.retain(|t| t.id != id))
                    .render(theme),
            )
        })
        .collect();

    let overlay = sized_box(notification_stack(theme, items))
        .fixed_width(Length::px(280.0))
        .padding(Length::px(8.0))
        .alignment(UnitPoint::from(state.position));

    let demo_area = zstack((placeholder, overlay));

    with_source!(theme, {
        flex_col((controls, demo_area))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(8.0))
    })
}

fn build_inner(theme: &Theme, state: &NotificationDemo) -> impl WidgetView<NotificationDemo> + use<> {
    let title_block = flex_col((
        label("Notification")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Toast card built on Alert, with a close button and an optional auto-dismiss timeout. \
             Buttons below trigger toasts anchored to the chosen corner.",
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
            section_header("Live demo", theme),
            live_demo_section(theme, state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

type InnerView = Box<AnyWidgetView<NotificationDemo>>;
type InnerViewState = <InnerView as View<NotificationDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the notification demo panel.
pub struct NotificationDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct NotificationDemoPanelState {
    demo: NotificationDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Notification demo panel.
///
/// The returned view owns the live-stack toast list internally; callers do
/// not need to store or lens into any notification state.
#[must_use]
pub fn panel(theme: &Theme) -> NotificationDemoPanel {
    NotificationDemoPanel { theme: *theme }
}

impl ViewMarker for NotificationDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for NotificationDemoPanel {
    type ViewState = NotificationDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = NotificationDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            NotificationDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut NotificationDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.demo));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.demo,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut NotificationDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut NotificationDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}
