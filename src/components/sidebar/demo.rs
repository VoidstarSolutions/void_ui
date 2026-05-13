//! Sidebar navigation item demo panel used by the void-ui gallery.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, label};

use super::sidebar_item;
use crate::Theme;
use crate::with_source;

/// Renders the `SidebarItem` demo panel.
///
/// Shows the active and default states. Callbacks are no-ops — the panel
/// exercises visual states, not application logic.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let active_example = with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut S| {})
                .active(true)
                .render(theme),
            sidebar_item("Data Grid", |_: &mut S| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut S| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    });

    let default_example = with_source!(theme, {
        flex_col((
            sidebar_item("Button", |_: &mut S| {}).render(theme),
            sidebar_item("Data Grid", |_: &mut S| {}).render(theme),
            sidebar_item("Sidebar", |_: &mut S| {}).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(2.0))
    });

    flex_col((
        header("Active — teal accent bar on the selected nav item"),
        active_example,
        header("Default — hover shows fill, label muted when inactive"),
        default_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}
