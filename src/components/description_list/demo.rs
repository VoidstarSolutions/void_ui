//! Description list demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};

use super::description_list;
use crate::components::ScrollBarVisibility;
use crate::with_source;
use crate::{Theme, badge, label, scroll_container, status_dot};

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn horizontal_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        description_list::<S, ()>()
            .item("Name", label("Ada Lovelace").render(theme))
            .item("Status", status_dot(theme.palette.green).render(theme))
            .item("Role", badge("Admin").render(theme))
            .render(theme)
    })
}

fn stacked_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        description_list::<S, ()>()
            .item("Address", label("12 Analytical Way").render(theme))
            .item("Notes", label("Enjoys long division.").render(theme))
            .stacked()
            .render(theme)
    })
}

/// Renders the Description List demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Description List")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Ordered label/value pairs — a themed `<dl>` analog, horizontal or stacked.")
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
            section_header("Horizontal (default)", theme),
            horizontal_section(theme),
            section_header("Stacked", theme),
            stacked_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
