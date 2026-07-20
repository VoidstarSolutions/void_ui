//! Status dot demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use super::status_dot;
use crate::components::ScrollBarVisibility;
use crate::with_source;
use crate::{Theme, label, scroll_container};

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn row<S: 'static, D: WidgetView<S>>(
    text: &'static str,
    dot: D,
    theme: &Theme,
) -> impl WidgetView<S> + use<S, D> {
    flex_row((dot, label(text).render(theme)))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
}

fn default_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            row(
                "Online",
                status_dot(theme.palette.green).render(theme),
                theme,
            ),
            row("Away", status_dot(theme.palette.amber).render(theme), theme),
            row(
                "Offline",
                status_dot(theme.palette.text_faint).render(theme),
                theme,
            ),
            row(
                "Error",
                status_dot(theme.palette.coral).render(theme),
                theme,
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    })
}

fn size_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            status_dot(theme.palette.teal).size(6.0).render(theme),
            status_dot(theme.palette.teal).render(theme),
            status_dot(theme.palette.teal).size(12.0).render(theme),
            status_dot(theme.palette.teal).size(18.0).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

/// Renders the Status Dot demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Status Dot")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Small filled circle for per-row/per-item status (connection state, online/offline).",
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
            section_header("Default (density-driven)", theme),
            default_section(theme),
            section_header("Custom sizes", theme),
            size_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
