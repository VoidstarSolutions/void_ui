//! Checkbox demo panel used by the void-ui gallery.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};

use super::checkbox;
use crate::Theme;
use crate::with_source;

/// Interactive state for the checkbox demo panel.
///
/// Add this as a field on your gallery app-state and pass it (plus lens
/// closures) to [`panel`].
#[derive(Debug, Clone, Default)]
pub struct CheckboxDemo {
    pub bare_a: bool,
    pub bare_b: bool,
    pub labeled_a: bool,
    pub labeled_b: bool,
}

/// Renders the Checkbox demo panel with interactive (toggleable) checkboxes.
///
/// `checked` is a snapshot of the current demo state; the four `on_*`
/// callbacks mutate the host's state when the user toggles each checkbox.
#[must_use]
pub fn panel<S, FA, FB, FC, FD>(
    theme: &Theme,
    checked: &CheckboxDemo,
    on_bare_a: FA,
    on_bare_b: FB,
    on_labeled_a: FC,
    on_labeled_b: FD,
) -> impl WidgetView<S> + use<S, FA, FB, FC, FD>
where
    S: 'static,
    FA: Fn(&mut S) + Send + Sync + 'static,
    FB: Fn(&mut S) + Send + Sync + 'static,
    FC: Fn(&mut S) + Send + Sync + 'static,
    FD: Fn(&mut S) + Send + Sync + 'static,
{
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let (bare_a, bare_b) = (checked.bare_a, checked.bare_b);
    let (labeled_a, labeled_b) = (checked.labeled_a, checked.labeled_b);

    // Box-only rows (no label)
    let bare = with_source!(theme, {
        flex_row((
            checkbox(bare_a, on_bare_a).render(theme),
            checkbox(bare_b, on_bare_b).render(theme),
            checkbox(false, |_: &mut S| {}).disabled(true).render(theme),
            checkbox(true, |_: &mut S| {}).disabled(true).render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
    });
    // Rows with labels
    let labeled = with_source!(theme, {
        flex_row((
            checkbox(labeled_a, on_labeled_a)
                .label("Unchecked")
                .render(theme),
            checkbox(labeled_b, on_labeled_b)
                .label("Checked")
                .render(theme),
            checkbox(false, |_: &mut S| {})
                .label("Disabled off")
                .disabled(true)
                .render(theme),
            checkbox(true, |_: &mut S| {})
                .label("Disabled on")
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    });

    flex_col((header("Box only"), bare, header("With label"), labeled))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0))
}
