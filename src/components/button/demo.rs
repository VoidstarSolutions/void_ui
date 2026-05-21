//! Button demo panel used by the void-ui gallery.
//!
//! Each example block is wrapped in [`crate::with_source!`] so the code
//! snippet that produced the live output is shown directly below it. Adding
//! a new variant is: add a `header`, add an example block, wrap in
//! `with_source!`.

use masonry::kurbo::BezPath;
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
/// Covers all 10 named variants, plus active / disabled / loading / trailing-icon
/// states. Callbacks are no-ops — the panel exercises visual states only.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    // --- variants (rest / active / disabled) ---

    let default_example = with_source!(theme, {
        flex_row((
            button("Default", |_: &mut S| {}).render(theme),
            button("Primary", |_: &mut S| {})
                .variant(ButtonVariant::Primary)
                .render(theme),
            button("Secondary", |_: &mut S| {})
                .variant(ButtonVariant::Secondary)
                .render(theme),
            button("Danger", |_: &mut S| {})
                .variant(ButtonVariant::Danger)
                .render(theme),
            button("Warning", |_: &mut S| {})
                .variant(ButtonVariant::Warning)
                .render(theme),
            button("Success", |_: &mut S| {})
                .variant(ButtonVariant::Success)
                .render(theme),
            button("Info", |_: &mut S| {})
                .variant(ButtonVariant::Info)
                .render(theme),
            button("Ghost", |_: &mut S| {})
                .variant(ButtonVariant::Ghost)
                .render(theme),
            button("Link", |_: &mut S| {})
                .variant(ButtonVariant::Link)
                .render(theme),
            button("Text", |_: &mut S| {})
                .variant(ButtonVariant::Text)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    // --- loading & trailing icon ---

    let loading_example = with_source!(theme, {
        flex_row((
            button("Saving…", |_: &mut S| {})
                .loading(true)
                .render(theme),
            button("Saving…", |_: &mut S| {})
                .variant(ButtonVariant::Primary)
                .loading(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let caret = {
        let mut p = BezPath::new();
        p.move_to((0.2, 0.35));
        p.line_to((0.5, 0.65));
        p.line_to((0.8, 0.35));
        p
    };

    let trailing_icon_example = with_source!(theme, {
        flex_row((
            button("More options", |_: &mut S| {})
                .trailing_icon(caret.clone())
                .render(theme),
            button("More options", |_: &mut S| {})
                .variant(ButtonVariant::Ghost)
                .trailing_icon(caret.clone())
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    // Split into two nested flex_cols to stay within xilem's tuple-impl limit.
    let top = flex_col((header("Default"), default_example))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0));

    let bottom = flex_col((
        header("Loading — spinner, interaction blocked"),
        loading_example,
        header("Trailing icon"),
        trailing_icon_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0));

    flex_col((top, bottom))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0))
}
