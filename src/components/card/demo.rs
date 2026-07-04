//! Card demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};

use super::card;
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

fn kv_row<S: 'static>(
    key: &'static str,
    value: &'static str,
    theme: &Theme,
) -> impl WidgetView<S> + use<S> {
    flex_row((
        label(key).color(theme.palette.text_faint).render(theme),
        label(value).render(theme),
    ))
    .main_axis_alignment(MainAxisAlignment::SpaceBetween)
    .gap(Length::px(16.0))
}

fn default_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        card(
            flex_col((
                kv_row("Open", "142.30", theme),
                kv_row("High", "144.85", theme),
                kv_row("Low", "141.05", theme),
                kv_row("Close", "143.60", theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(4.0)),
        )
        .render(theme)
    })
}

fn bordered_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        card(
            label("A card with a border and a custom background.")
                .multiline(true)
                .render(theme),
        )
        .border()
        .background(theme.palette.surface_2)
        .render(theme)
    })
}

/// Renders the Card demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Card")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Themed rounded, padded surface for floating panels — a lighter-weight sibling of Group Box with no title bar.")
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
            section_header("Default", theme),
            default_section(theme),
            section_header("Bordered, custom background", theme),
            bordered_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
