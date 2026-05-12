//! Button demo panel used by the void-ui gallery.
//!
//! The panel is intentionally generic over the gallery's `State` and emits
//! no actions — the click callbacks are no-ops. The purpose of the demo
//! is to exercise the *visual* state machine (hover, press, the active
//! selector) live, not to do anything when clicked.
//!
//! Adding a new component to the gallery follows the same shape: drop a
//! `demo.rs` in the component folder, export a [`panel`] function, add a
//! variant to [`crate::components::ComponentKind`].

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};

use super::button;
use crate::Theme;

/// Renders the Button demo: two rows showing the default and active
/// variants. Hovering, pressing, and clicking work — they just don't
/// mutate state.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    let default_row = flex_row((
        button("Reset view", |_: &mut S| {}).render(theme),
        button("Annotate", |_: &mut S| {}).render(theme),
        button("Inspector", |_: &mut S| {}).render(theme),
        button("+ Compare", |_: &mut S| {}).render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0));

    let active_row = flex_row((
        button("Reset view", |_: &mut S| {})
            .active(true)
            .render(theme),
        button("Annotate", |_: &mut S| {})
            .active(true)
            .render(theme),
        button("Inspector", |_: &mut S| {})
            .active(true)
            .render(theme),
        button("+ Compare", |_: &mut S| {})
            .active(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0));

    flex_col((
        header("Default — hover for the surface, press for a darker fill"),
        default_row,
        header("Active (host-controlled toggle)"),
        active_row,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(12.0))
}
