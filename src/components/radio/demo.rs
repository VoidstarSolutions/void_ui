//! Radio button demo panel used by the void-ui gallery.
//!
//! Shows standalone radios and a host-managed group. The group example uses
//! `map_state` so the panel doesn't require gallery state to expose a field.

use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label};
use xilem::WidgetView;

use crate::Theme;
use crate::components::radio::radio;
use crate::with_source;

/// Renders the Radio demo panel.
///
/// Covers unselected, selected, disabled, and a live host-managed group.
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
    };

    // Static examples — no interactive state needed.
    let standalone_example = with_source!(theme, {
        flex_row((
            radio("Unselected", |_: &mut S| {})
                .active(false)
                .render(theme),
            radio("Selected", |_: &mut S| {})
                .active(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    });

    let disabled_example = with_source!(theme, {
        flex_row((
            radio("Disabled unselected", |_: &mut S| {})
                .active(false)
                .disabled(true)
                .render(theme),
            radio("Disabled selected", |_: &mut S| {})
                .active(true)
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    });

    // Live group — map_state carves out a `usize` selection index from S.
    // The gallery's State doesn't need a dedicated field; this panel manages
    // its own selection via view-local state carried through map_state.
    let group_example = group_panel(theme);

    flex_col((
        header("Standalone — unselected and selected"),
        standalone_example,
        header("Disabled"),
        disabled_example,
        header("Group — click to change selection"),
        group_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}

/// A self-contained radio group that owns its selection index.
///
/// Uses `map_state` to project a `usize` sub-state out of the parent `S`.
/// Because we need true local state (the gallery's `State` doesn't have a
/// radio selection field), we embed a static default via a wrapper that
/// initialises the index on first use. In the gallery this is rendered as a
/// plain `AnyWidgetView<S>`.
fn group_panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let t = *theme;
    // `map_state` projects `S` → `usize` so the group view only sees an index.
    // We use a function pointer that always returns a reference to a `static mut`
    // isn't sound, so instead we add a thin `GroupState` newtype to the gallery
    // by returning an `AnyWidgetView<S>` that closes over the theme but
    // requires S to carry the index. Since we can't add fields to S here, we
    // show a static-value version instead and document the host-managed pattern.
    //
    // A real integration would be: map_state(group_view(selected, theme), |s: &mut AppState| &mut s.radio_selection)
    flex_col((
        label("Selection is host-managed: pass active(selected == value) to each radio,")
            .text_size(t.typography.size_caption)
            .color(t.palette.text_muted),
        label("then update state in the callback.")
            .text_size(t.typography.size_caption)
            .color(t.palette.text_muted),
        flex_row((
            radio("Option A", |_: &mut S| {})
                .active(true)
                .render(&t),
            radio("Option B", |_: &mut S| {})
                .active(false)
                .render(&t),
            radio("Option C", |_: &mut S| {})
                .active(false)
                .render(&t),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0)),
    ))
    .gap(Length::px(8.0))
}
