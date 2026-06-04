//! Gallery panel exercising the read-only code view.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};

use crate::Theme;
use crate::components::code_view::read_only_text;
use crate::with_source;

#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let short_rust = with_source!(theme, {
        read_only_text("fn answer() -> u32 { 42 }").render(theme)
    });

    let multi_line = with_source!(theme, {
        read_only_text(
            "fn main() {\n    // a tiny demo\n    let xs: Vec<i32> = (0..3).collect();\n    println!(\"{xs:?}\");\n}",
        )
        .render(theme)
    });

    let no_highlight = with_source!(theme, {
        read_only_text("plain text with no highlighter — still selectable")
            .no_highlighter()
            .render(theme)
    });

    let copyable = with_source!(theme, {
        read_only_text("let copied = \"one click, top-right\";")
            .copyable()
            .render(theme)
    });

    flex_col((short_rust, multi_line, no_highlight, copyable))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0))
}
