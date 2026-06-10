//! Group box demo panel used by the void-ui gallery.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};

use super::group_box;
use crate::components::ScrollBarVisibility;
use crate::with_source;
use crate::{Theme, label, scroll_container, separator};

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn body<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    flex_col((
        label("Some grouped content goes here.").render(theme),
        label("It can be any widget view.")
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(4.0))
}

fn normal_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        group_box(body(theme)).title("Normal").render(theme)
    })
}

fn fill_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        group_box(body(theme)).title("Fill").fill().render(theme)
    })
}

fn outline_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        group_box(body(theme))
            .title("Outline")
            .outline()
            .render(theme)
    })
}

fn untitled_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, { group_box(body(theme)).outline().render(theme) })
}

/// Renders the `GroupBox` demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Group Box")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Titled container that visually groups related content.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            section_header("Normal (default)", theme),
            normal_section(theme),
            section_header("Fill", theme),
            fill_section(theme),
            section_header("Outline", theme),
            outline_section(theme),
            section_header("Without a title", theme),
            untitled_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
