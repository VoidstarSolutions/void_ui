//! Alert demo panel used by the void-ui gallery.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};

use super::{AlertVariant, alert};
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

fn variants_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        flex_col((
            alert("This is a notification.").render(theme),
            alert("You have been saved successfully.")
                .variant(AlertVariant::Info)
                .render(theme),
            alert("Your changes have been saved.")
                .variant(AlertVariant::Success)
                .render(theme),
            alert("This action cannot be undone.")
                .variant(AlertVariant::Warning)
                .render(theme),
            alert("There was a problem with your request.")
                .variant(AlertVariant::Error)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0))
    })
}

fn titled_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        alert("There was a problem with your request.")
            .variant(AlertVariant::Error)
            .title("Uh oh! Something went wrong.")
            .render(theme)
    })
}

fn closable_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        alert("You have been saved successfully.")
            .variant(AlertVariant::Info)
            .on_close(|_: &mut S| {})
            .render(theme)
    })
}

fn banner_section<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    with_source!(theme, {
        alert("This is a banner alert.")
            .variant(AlertVariant::Warning)
            .banner()
            .render(theme)
    })
}

/// Renders the Alert demo panel.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let title_block = flex_col((
        label("Alert")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Themed message card with an optional title, leading icon, and close button.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            section_header("Variants", theme),
            variants_section(theme),
            section_header("With title", theme),
            titled_section(theme),
            section_header("With close button", theme),
            closable_section(theme),
            section_header("Banner", theme),
            banner_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
