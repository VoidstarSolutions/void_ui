//! Action row demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};

use super::action_row;
use crate::components::ScrollBarVisibility;
use crate::components::alert::AlertVariant;
use crate::components::button::ButtonVariant;
use crate::with_source;
use crate::{Theme, button, label, scroll_container};

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

/// A subtle text-variant action button (the demo's callbacks are inert).
fn action_button<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S, ()> + use<S> {
    button(|_: &mut S| {})
        .label(text)
        .variant(ButtonVariant::Text)
        .render(theme)
}

fn basic_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        action_row("EURUSD")
            .leading_dot(theme.palette.success)
            .action(action_button("Edit", theme))
            .render(theme)
    })
}

fn summary_and_badge_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        action_row("GBPUSD")
            .leading_dot(theme.palette.warning)
            .secondary("cable")
            .badge("STALE", AlertVariant::Warning)
            .action(action_button("Refresh", theme))
            .render(theme)
    })
}

fn multiple_actions_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        action_row("USDJPY")
            .leading_dot(theme.palette.danger)
            .secondary("disconnected")
            .action(action_button("Reconnect", theme))
            .action(action_button("Remove", theme))
            .render(theme)
    })
}

/// A realistic stack of rows — the citadel "symbol list" shape this component
/// was filed for: a status dot, a flexed label, and trailing actions per row.
fn row_list_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            action_row("Box 0.25")
                .leading_dot(theme.palette.success)
                .badge("LIVE", AlertVariant::Success)
                .action(action_button("Settings", theme))
                .render(theme),
            action_row("Box 0.50")
                .leading_dot(theme.palette.success)
                .badge("LIVE", AlertVariant::Success)
                .action(action_button("Settings", theme))
                .render(theme),
            action_row("Box 1.00")
                .leading_dot(theme.palette.text_faint)
                .secondary("paused")
                .action(action_button("Settings", theme))
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
    })
}

/// Renders the Action Row demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Action Row")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "A themed list row: leading status dot, a flexed primary label (with optional inline \
             summary), an optional trailing badge, and trailing action controls.",
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
            crate::separator().render(theme),
            section_header("Dot + label + action", theme),
            basic_section(theme),
            section_header("Inline summary + status badge", theme),
            summary_and_badge_section(theme),
            section_header("Multiple trailing actions", theme),
            multiple_actions_section(theme),
            section_header("A row list (symbol-list shape)", theme),
            row_list_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
