//! Separator demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};

use super::separator;
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

fn horizontal_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            label("Content above").render(theme),
            separator().render(theme),
            label("Content below").render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    })
}

fn horizontal_dashed_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            label("Content above").render(theme),
            separator().dashed().render(theme),
            label("Content below").render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    })
}

fn color_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            separator().color(theme.palette.teal).render(theme),
            separator().color(theme.palette.coral).render(theme),
            separator().color(theme.palette.border_strong).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    })
}

fn labeled_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            separator().label("Section A").render(theme),
            label("Some content").render(theme),
            separator().label("Section B").dashed().render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    })
}

fn vertical_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        sized_box(
            flex_row((
                label("A").render(theme),
                separator().vertical().render(theme),
                label("B").render(theme),
                separator().vertical().dashed().render(theme),
                label("C").render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(8.0)),
        )
        .height(Length::px(40.0))
    })
}

fn vertical_labeled_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        sized_box(
            flex_row((
                label("A").render(theme),
                separator().vertical().label("Section A").render(theme),
                label("B").render(theme),
                separator()
                    .vertical()
                    .label("Section B")
                    .dashed()
                    .render(theme),
                label("C").render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(8.0)),
        )
        .height(Length::px(40.0))
    })
}

/// Renders the Separator demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Separator")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Themed divider line — horizontal or vertical, solid or dashed, with optional label.",
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
            section_header("Horizontal — solid (default)", theme),
            horizontal_section(theme),
            section_header("Horizontal — dashed", theme),
            horizontal_dashed_section(theme),
            section_header("Color override", theme),
            color_section(theme),
            section_header("With label", theme),
            labeled_section(theme),
            section_header("Vertical", theme),
            vertical_section(theme),
            section_header("Vertical, labeled", theme),
            vertical_labeled_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
