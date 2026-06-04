//! Icon demo panel used by the void-ui gallery.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use super::{IconName, icon};
use crate::components::ScrollBarVisibility;
use crate::label;
use crate::with_source;
use crate::{Theme, scroll_container};

/// Renders the Icon demo panel.
///
/// Shows the Lucide icon set — color and size overrides, all via the
/// `.color()` and `.size()` builder methods. The host must have registered
/// [`crate::LUCIDE_FONT_BYTES`] or icons will render as missing-glyph boxes.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let named_example = with_source!(theme, {
        flex_row((
            icon(IconName::ChevronLeft).render(theme),
            icon(IconName::ChevronRight).render(theme),
            icon(IconName::ChevronUp).render(theme),
            icon(IconName::ChevronDown).render(theme),
            icon(IconName::Plus).render(theme),
            icon(IconName::Minus).render(theme),
            icon(IconName::X).render(theme),
            icon(IconName::Check).render(theme),
            icon(IconName::Copy).render(theme),
            icon(IconName::Ellipsis).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    });

    let color_example = with_source!(theme, {
        flex_row((
            icon(IconName::Check).render(theme),
            icon(IconName::Check)
                .color(theme.palette.teal)
                .render(theme),
            icon(IconName::Check)
                .color(theme.palette.coral)
                .render(theme),
            icon(IconName::Check)
                .color(theme.palette.amber)
                .render(theme),
            icon(IconName::Check)
                .color(theme.palette.text_muted)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    });

    let size_example = with_source!(theme, {
        flex_row((
            icon(IconName::ChevronRight).size(10.0).render(theme),
            icon(IconName::ChevronRight).size(16.0).render(theme),
            icon(IconName::ChevronRight).size(24.0).render(theme),
            icon(IconName::ChevronRight).size(40.0).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    });

    scroll_container(
        flex_col((
            header("Named icons — Lucide 1.17.0, all at default size and color"),
            named_example,
            header("Color override — .color(palette.*)"),
            color_example,
            header("Size override — .size(px)"),
            size_example,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
