//! Button demo panel used by the void-ui gallery.
//!
//! Each example block is wrapped in [`crate::with_source!`] so the code
//! snippet that produced the live output is shown directly below it. Adding
//! a new variant is: add a `header`, add an example block, wrap in
//! `with_source!`.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};

use super::button;
use crate::Theme;
use crate::components::ButtonVariant;
use crate::with_source;

/// Renders the Button demo panel.
///
/// Covers the default, danger, disabled, and icon variants. Callbacks are
/// no-ops — the panel exercises visual states, not application logic.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let default_example = with_source!(theme, {
        flex_row((
            button("Reset view", |_: &mut S| {}).render(theme),
            button("Annotate", |_: &mut S| {}).render(theme),
            button("Inspector", |_: &mut S| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let active_example = with_source!(theme, {
        flex_row((
            button("Reset view", |_: &mut S| {})
                .active(true)
                .render(theme),
            button("Annotate", |_: &mut S| {})
                .active(true)
                .render(theme),
            button("Inspector", |_: &mut S| {})
                .active(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let danger_example = with_source!(theme, {
        flex_row((
            button("Delete", |_: &mut S| {})
                .variant(ButtonVariant::Danger)
                .render(theme),
            button("Remove all", |_: &mut S| {})
                .variant(ButtonVariant::Danger)
                .render(theme),
            button("Clear history", |_: &mut S| {})
                .variant(ButtonVariant::Danger)
                .active(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let disabled_example = with_source!(theme, {
        flex_row((
            button("Reset view", |_: &mut S| {})
                .disabled(true)
                .render(theme),
            button("Annotate", |_: &mut S| {})
                .disabled(true)
                .render(theme),
            button("Delete", |_: &mut S| {})
                .variant(ButtonVariant::Danger)
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let primary_example = with_source!(theme, {
        flex_row((
            button("Confirm", |_: &mut S| {})
                .variant(ButtonVariant::Primary)
                .render(theme),
            button("Submit", |_: &mut S| {})
                .variant(ButtonVariant::Primary)
                .active(true)
                .render(theme),
            button("Apply", |_: &mut S| {})
                .variant(ButtonVariant::Primary)
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let ghost_example = with_source!(theme, {
        flex_row((
            button("Settings", |_: &mut S| {})
                .variant(ButtonVariant::Ghost)
                .render(theme),
            button("Filters", |_: &mut S| {})
                .variant(ButtonVariant::Ghost)
                .active(true)
                .render(theme),
            button("Export", |_: &mut S| {})
                .variant(ButtonVariant::Ghost)
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let warning_example = with_source!(theme, {
        flex_row((
            button("Archive", |_: &mut S| {})
                .variant(ButtonVariant::Warning)
                .render(theme),
            button("Overwrite", |_: &mut S| {})
                .variant(ButtonVariant::Warning)
                .active(true)
                .render(theme),
            button("Reset", |_: &mut S| {})
                .variant(ButtonVariant::Warning)
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    flex_col((
        header("Default — hover for surface fill, press for darker fill"),
        default_example,
        header("Active (host-controlled toggle)"),
        active_example,
        header("Primary variant — teal fill, always-visible background"),
        primary_example,
        header("Ghost variant — always-visible border, no fill at rest"),
        ghost_example,
        header("Warning variant — amber tones on hover / active"),
        warning_example,
        header("Danger variant — coral tones on hover / active"),
        danger_example,
        header("Disabled — muted text, no interaction"),
        disabled_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}
