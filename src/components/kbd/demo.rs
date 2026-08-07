//! Gallery demo for the `kbd` component.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use super::{Modifier, kbd};
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

fn keys_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            kbd("F5").render(theme),
            kbd("Enter").render(theme),
            kbd("→").render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

fn combo_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            kbd("S").mods([Modifier::Cmd]).render(theme),
            kbd("K")
                .mods([Modifier::Cmd, Modifier::Shift])
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

// A menu-row-style pairing: command text on the left, its shortcut chip
// trailing on the right — the primary intended use of `kbd`.
fn menu_row_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            label("Save").render(theme),
            kbd("S").mods([Modifier::Cmd]).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(24.0))
    })
}

/// Renders the Kbd demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Kbd")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Inline keycap chip for a keyboard shortcut, with platform-aware symbol mapping.")
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
            section_header("Bare keys", theme),
            keys_section(theme),
            section_header("Modified combos", theme),
            combo_section(theme),
            section_header("Menu row pairing", theme),
            menu_row_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
