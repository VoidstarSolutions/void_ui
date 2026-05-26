//! Gallery utilities: the `code_block` view and the `with_source!` macro.
//!
//! Demo panels in each component's `demo.rs` use these to display the source
//! snippet that produced a live example directly below it.

use xilem::WidgetView;

use crate::Theme;
use crate::components::text_field::read_only_text;

/// Renders `source` in a styled monospace code panel with Rust syntax
/// highlighting.
///
/// Typically called via the [`with_source`](crate::with_source) macro rather
/// than directly.
#[must_use]
pub fn code_block<S: 'static>(source: &str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    read_only_text(source).render(theme)
}

/// Wraps a view expression and appends its stringified source below it.
///
/// The source is captured at compile time via `stringify!` and rendered by
/// [`code_block`]. The live output and the code panel are stacked in a
/// `flex_col` with `CrossAxisAlignment::Stretch` so the code panel fills
/// the same width as the demo content.
///
/// ```ignore
/// with_source!(theme, {
///     flex_row((
///         button("Reset view", |_: &mut S| {}).render(theme),
///         button("Annotate",   |_: &mut S| {}).render(theme),
///     ))
///     .gap(Length::px(8.0))
/// })
/// ```
#[macro_export]
macro_rules! with_source {
    ($theme:expr, { $($body:tt)* }) => {{
        let view   = { $($body)* };
        let source = stringify!($($body)*);
        ::xilem::view::flex_col((view, $crate::gallery::code_block(source, $theme)))
            .cross_axis_alignment(::xilem::view::CrossAxisAlignment::Stretch)
            .gap(::xilem::masonry::layout::Length::px(8.0))
    }};
}
