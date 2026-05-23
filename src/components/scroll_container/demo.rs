//! Scroll container demo panel used by the void-ui gallery.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{AnyFlexChild, CrossAxisAlignment, FlexExt as _, flex_col, flex_row, label, sized_box};

use super::scroll_container;
use crate::Theme;
use crate::with_source;

/// Renders the Scroll Container demo panel.
///
/// Displays a fixed-size viewport containing content that overflows on both
/// axes. Drag the scrollbars or use the scroll wheel to navigate.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let both_axes = with_source!(theme, {
        sized_box(
            scroll_container(content_grid::<S>(theme, 12, 8)).render(theme),
        )
        .fixed_width(Length::px(320.0))
        .fixed_height(Length::px(200.0))
    });

    let vertical_only = with_source!(theme, {
        sized_box(
            scroll_container(content_grid::<S>(theme, 4, 20))
                .constrain_horizontal(true)
                .render(theme),
        )
        .fixed_width(Length::px(320.0))
        .fixed_height(Length::px(160.0))
    });

    flex_col((
        header("Both axes — 12 × 8 grid in a 320 × 200 viewport"),
        both_axes,
        header("Vertical only — constrain_horizontal(true)"),
        vertical_only,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}

fn content_grid<S: 'static>(theme: &Theme, cols: u32, rows: u32) -> impl WidgetView<S> + use<S> {
    let cell_w = 80.0_f64;
    let cell_h = 32.0_f64;
    let bg_a = theme.palette.surface;
    let bg_b = theme.palette.surface_2;
    let caption = theme.typography.size_caption;
    let text_muted = theme.palette.text_muted;

    let row_views: Vec<AnyFlexChild<S, ()>> = (0..rows)
        .map(|r| {
            let cells: Vec<AnyFlexChild<S, ()>> = (0..cols)
                .map(|c| {
                    let bg = if (r + c) % 2 == 0 { bg_a } else { bg_b };
                    sized_box(label(format!("{r},{c}")).text_size(caption).color(text_muted))
                        .fixed_width(Length::px(cell_w))
                        .fixed_height(Length::px(cell_h))
                        .background_color(bg)
                        .into_any_flex()
                })
                .collect();
            flex_row(cells)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .into_any_flex()
        })
        .collect();

    flex_col(row_views).cross_axis_alignment(CrossAxisAlignment::Start)
}
