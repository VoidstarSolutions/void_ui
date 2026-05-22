//! Checkbox demo panel used by the void-ui gallery.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};

use super::checkbox;
use crate::Theme;
use crate::with_source;

/// Renders the Checkbox demo panel.
///
/// Shows unchecked / checked / disabled states, with and without labels.
/// Callbacks are no-ops — the panel exercises visual states only.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    // Box-only rows (no label)
    let bare = with_source!(theme, {
        flex_row((
            checkbox(false, |_: &mut S| {}).render(theme),
            checkbox(true, |_: &mut S| {}).render(theme),
            checkbox(false, |_: &mut S| {}).disabled(true).render(theme),
            checkbox(true, |_: &mut S| {}).disabled(true).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
    });
    // Rows with labels
    let labeled = flex_col((
        flex_row((
            checkbox(false, |_: &mut S| {})
                .label("Unchecked")
                .render(theme),
            checkbox(true, |_: &mut S| {})
                .label("Checked")
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0)),
        flex_row((
            checkbox(false, |_: &mut S| {})
                .label("Disabled off")
                .disabled(true)
                .render(theme),
            checkbox(true, |_: &mut S| {})
                .label("Disabled on")
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(8.0));

    flex_col((
        header("Box only — unchecked / checked / disabled off / disabled on"),
        bare,
        header("With label"),
        labeled,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}
