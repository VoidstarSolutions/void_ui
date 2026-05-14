//! Tooltip demo panel used by the void-ui gallery.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};

use super::tooltip;
use crate::Theme;
use crate::components::button;
use crate::with_source;

/// Renders the Tooltip demo panel.
///
/// Hover any button for ~300 ms to see a themed tooltip surface anchored
/// near the cursor. Tooltips dismiss themselves on the next pointer move,
/// click, or window leave.
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
            tooltip(
                "Reset the chart to defaults",
                button("Reset", |_: &mut S| {}).render(theme),
            )
            .render(theme),
            tooltip(
                "Add annotation marker at the cursor",
                button("Annotate", |_: &mut S| {}).render(theme),
            )
            .render(theme),
            tooltip(
                "Toggle the inspector pane",
                button("Inspector", |_: &mut S| {}).render(theme),
            )
            .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    let custom_delay_example = with_source!(theme, {
        flex_row((
            tooltip(
                "Instant — 0 ms delay",
                button("Fast", |_: &mut S| {}).render(theme),
            )
            .delay_ms(0)
            .render(theme),
            tooltip(
                "Slow — 1500 ms delay",
                button("Slow", |_: &mut S| {}).render(theme),
            )
            .delay_ms(1500)
            .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    flex_col((
        header("Default — 300 ms hover-idle, dismiss on pointer activity"),
        default_example,
        header("Custom delay — .delay_ms(0) or .delay_ms(1500)"),
        custom_delay_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}
