//! Gallery panel exercising the read-only code view.

use masonry::layout::Length;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};

use crate::components::ScrollBarVisibility;
use crate::components::code_view::read_only_text;
use crate::with_source;
use crate::{Theme, label, scroll_container, separator};

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
        read_only_text("let copied = \"one click, right edge\";")
            .copyable()
            .render(theme)
    });
    let title_block = flex_col((
        label("Code View")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("A read-only text view with optional syntax highlighting, line numbers, and copy button.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            short_rust,
            multi_line,
            no_highlight,
            copyable,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}
