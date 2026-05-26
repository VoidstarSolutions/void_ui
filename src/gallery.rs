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

/// Renders a view alongside its original source code.
///
/// Re-exported from [`void_ui_macros`] — see that crate's docs for details.
/// Unlike a `macro_rules!` + `stringify!` implementation, this proc-macro
/// recovers the original source bytes via `proc_macro::Span::source_text`,
/// so newlines and indentation in the body block are preserved when the
/// snippet is rendered.
pub use void_ui_macros::with_source;
