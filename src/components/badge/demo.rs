//! Badge demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use super::{badge, pill};
use crate::components::ScrollBarVisibility;
use crate::with_source;
use crate::{AlertVariant, Theme, label, scroll_container};

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn rounded_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            badge("Draft").render(theme),
            badge("Info").variant(AlertVariant::Info).render(theme),
            badge("Success")
                .variant(AlertVariant::Success)
                .render(theme),
            badge("Warning")
                .variant(AlertVariant::Warning)
                .render(theme),
            badge("Error").variant(AlertVariant::Error).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

fn pill_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            pill("Draft").render(theme),
            pill("Info").variant(AlertVariant::Info).render(theme),
            pill("Success").variant(AlertVariant::Success).render(theme),
            pill("Warning").variant(AlertVariant::Warning).render(theme),
            pill("Error").variant(AlertVariant::Error).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

/// Renders the Badge demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Badge")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Inline chip with a themed background and optional semantic accent.")
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
            section_header("Rounded (badge)", theme),
            rounded_section(theme),
            section_header("Capsule (pill)", theme),
            pill_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
