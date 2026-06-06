//! Separator demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};

use super::separator;
use crate::components::ScrollBarVisibility;
use crate::with_source;
use crate::{Theme, label, scroll_container};

/// Renders the Separator demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let section_header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let horizontal_solid = with_source!(theme, {
        flex_col((
            label("Content above").render(theme),
            separator().render(theme),
            label("Content below").render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    });

    let horizontal_dashed = with_source!(theme, {
        flex_col((
            label("Content above").render(theme),
            separator().dashed().render(theme),
            label("Content below").render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    });

    let color_override = with_source!(theme, {
        flex_col((
            separator().color(theme.palette.teal).render(theme),
            separator().color(theme.palette.coral).render(theme),
            separator().color(theme.palette.border_strong).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    });

    let labeled = with_source!(theme, {
        flex_col((
            separator().label("Section A").render(theme),
            label("Some content").render(theme),
            separator().label("Section B").dashed().render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    });

    let vertical = with_source!(theme, {
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
    });

    let vertical_labeled = with_source!(theme, {
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
    });

    scroll_container(
        flex_col((
            section_header("Horizontal — solid (default)"),
            horizontal_solid,
            section_header("Horizontal — dashed"),
            horizontal_dashed,
            section_header("Color override"),
            color_override,
            section_header("With label"),
            labeled,
            section_header("Vertical"),
            vertical,
            section_header("Vertical, colorful, labeled"),
            vertical_labeled,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
