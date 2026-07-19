//! Meter demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use super::meter;
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

fn solid_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            meter(0.25).width(220.0).render(theme),
            meter(0.6).width(220.0).render(theme),
            meter(1.0).width(220.0).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(10.0))
    })
}

/// Same gradient at three fractions, side by side — demonstrates that a
/// given x-coordinate along the track is always the same color regardless
/// of how much is filled (spec Decision 1).
fn gradient_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            meter(0.2)
                .fill_gradient(theme.palette.green, theme.palette.coral)
                .width(220.0)
                .render(theme),
            meter(0.55)
                .fill_gradient(theme.palette.green, theme.palette.coral)
                .width(220.0)
                .render(theme),
            meter(0.9)
                .fill_gradient(theme.palette.green, theme.palette.coral)
                .width(220.0)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(10.0))
    })
}

fn label_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((meter(0.72)
            .fill_gradient(theme.palette.green, theme.palette.coral)
            .label("72%")
            .width(220.0)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
    })
}

/// Renders the Meter demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Meter")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Track + fill primitive for a 0.0..=1.0 fraction \u{2014} a solid \
             or two-stop gradient fill, with an optional centered label.",
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
            section_header("Solid fill", theme),
            solid_section(theme),
            section_header("Heat-tinted gradient fill", theme),
            gradient_section(theme),
            section_header("With a centered label", theme),
            label_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
