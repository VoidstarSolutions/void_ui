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

/// One labeled entry in the Apple-symbol legend: the glyph chip above its
/// name. The glyph is passed as the literal key so it renders on every host,
/// not only macOS (the platform-mapped `Modifier` path is shown separately in
/// the combo section).
fn legend_entry<S: 'static>(
    glyph: &'static str,
    name: &'static str,
    theme: &Theme,
) -> impl WidgetView<S> + use<S> {
    flex_col((
        kbd(glyph).render(theme),
        label(name)
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_faint)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(4.0))
}

// The full macOS keyboard glyph vocabulary, grouped by function. Rows are kept
// to <=5 entries so none overruns the horizontally-constrained panel.
fn apple_symbols_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            flex_row((
                legend_entry("⌘", "Command", theme),
                legend_entry("⌃", "Control", theme),
                legend_entry("⌥", "Option", theme),
                legend_entry("⇧", "Shift", theme),
                legend_entry("⇪", "Caps Lock", theme),
            ))
            .gap(Length::px(14.0)),
            flex_row((
                legend_entry("⎋", "Esc", theme),
                legend_entry("⇥", "Tab", theme),
                legend_entry("↩", "Return", theme),
                legend_entry("⌤", "Enter", theme),
                legend_entry("⏏", "Eject", theme),
            ))
            .gap(Length::px(14.0)),
            flex_row((
                legend_entry("⌫", "Delete", theme),
                legend_entry("⌦", "Fwd Delete", theme),
            ))
            .gap(Length::px(14.0)),
            flex_row((
                legend_entry("↑", "Up", theme),
                legend_entry("↓", "Down", theme),
                legend_entry("←", "Left", theme),
                legend_entry("→", "Right", theme),
            ))
            .gap(Length::px(14.0)),
            flex_row((
                legend_entry("⇞", "Page Up", theme),
                legend_entry("⇟", "Page Down", theme),
                legend_entry("↖", "Home", theme),
                legend_entry("↘", "End", theme),
            ))
            .gap(Length::px(14.0)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(14.0))
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
            section_header("Apple keyboard symbols", theme),
            apple_symbols_section(theme),
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
