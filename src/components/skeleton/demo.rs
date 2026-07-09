//! Skeleton demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};

use super::skeleton;
use crate::components::ScrollBarVisibility;
use crate::with_source;
use crate::{Theme, card, label, scroll_container};

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn caption<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_muted)
        .render(theme)
}

fn text_lines_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            skeleton().render(theme),
            skeleton().render(theme),
            // A short trailing line, as a paragraph's last line reads.
            skeleton().width(180.0).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(10.0))
    })
}

fn shapes_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            // Avatar circle.
            skeleton().circle(48.0).render(theme),
            // Square thumbnail with a custom radius.
            skeleton()
                .rectangle()
                .size(48.0)
                .rounded(12.0)
                .render(theme),
            // Image block.
            skeleton()
                .rectangle()
                .width(120.0)
                .height(48.0)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
    })
}

/// Pulse, wave, and none side by side on the same shape, so the sweep vs.
/// fade vs. static difference is directly comparable.
fn animations_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_row((
            flex_col((
                caption("pulse", theme),
                skeleton()
                    .rectangle()
                    .width(140.0)
                    .height(48.0)
                    .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(6.0)),
            flex_col((
                caption("wave", theme),
                skeleton()
                    .rectangle()
                    .width(140.0)
                    .height(48.0)
                    .wave()
                    .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(6.0)),
            flex_col((
                caption("none", theme),
                skeleton()
                    .rectangle()
                    .width(140.0)
                    .height(48.0)
                    .animated(false)
                    .render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(6.0)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(20.0))
    })
}

fn secondary_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            skeleton().render(theme),
            skeleton().secondary().render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(10.0))
    })
}

/// A realistic media-card placeholder: an avatar beside two heading lines,
/// then a block of body text.
fn card_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        card(
            flex_col((
                flex_row((
                    skeleton().circle(40.0).render(theme),
                    sized_box(
                        flex_col((
                            skeleton().height(14.0).render(theme),
                            skeleton().height(12.0).width(120.0).render(theme),
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .gap(Length::px(8.0)),
                    )
                    .width(Length::px(200.0)),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .gap(Length::px(12.0)),
                skeleton()
                    .rectangle()
                    .width(260.0)
                    .height(96.0)
                    .rounded(8.0)
                    .render(theme),
                skeleton().render(theme),
                skeleton().width(180.0).render(theme),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(12.0)),
        )
        .render(theme)
    })
}

/// A form placeholder: a label line above each field's rectangular block.
fn form_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    fn field<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
        flex_col((
            skeleton().height(11.0).width(80.0).render(theme),
            skeleton().rectangle().height(32.0).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(6.0))
    }

    with_source!(theme, {
        card(
            flex_col((field(theme), field(theme), field(theme)))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(Length::px(16.0)),
        )
        .render(theme)
    })
}

/// A handful of table rows: a narrow leading column and a wide trailing one.
fn table_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    fn row<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
        flex_row((
            skeleton()
                .rectangle()
                .width(60.0)
                .height(14.0)
                .render(theme),
            skeleton().height(14.0).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    }

    with_source!(theme, {
        card(
            flex_col((row(theme), row(theme), row(theme)))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(Length::px(12.0)),
        )
        .render(theme)
    })
}

/// Renders the Skeleton demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Skeleton")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Loading placeholder — an animated shape sized to stand in for content \
             while it loads.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            section_header("Text lines", theme),
            text_lines_section(theme),
            section_header("Shapes — circle, square, block", theme),
            shapes_section(theme),
            section_header("Animation — pulse, wave, none", theme),
            animations_section(theme),
            section_header("Secondary tone", theme),
            secondary_section(theme),
            section_header("Composed — loading card", theme),
            card_section(theme),
            section_header("Composed — form", theme),
            form_section(theme),
            section_header("Composed — table rows", theme),
            table_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
