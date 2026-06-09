//! Proc-macros for the `void-ui` crate.
//!
//! Currently exports a single macro: [`with_source`], a replacement for the
//! old `macro_rules!` version that preserved no original source whitespace.

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// Renders a view alongside its original source code.
///
/// Usage: `with_source!(theme_expr, { /* view body */ })`.
///
/// Expands to
/// `overlay_scope(overlap_column(body_view, code_block(source, theme_expr)).gap(8px))`,
/// where `source` is the **original source text** of the body block —
/// including newlines and indentation — recovered via
/// [`proc_macro::Span::source_text`].
///
/// Two distinct, complementary mechanisms stack in that expansion:
///
/// - `overlap_column` keeps `body_view` visually on top of `code_block`
///   (same stacking as the old `flex_col`) while *painting* `body_view` —
///   and any overflow from its subtree, e.g. an open `Popover` hosted in an
///   in-tree `AnchoredOverlay` — on top of `code_block`, a later visual
///   sibling that would otherwise occlude it once it overflows its own
///   footprint. See [`crate::layout::overlap_column`] for the mechanism.
/// - `overlay_scope` gives overlay-shaped descendants of `body_view` that
///   use the scope-push path (e.g. dropdown menus) a discoverable,
///   always-on-top, always-clipped slot to push their popups into. See
///   [`crate::overlay_scope`] for the mechanism.
///
/// Falls back to a stringified token form if the compiler can't provide
/// source text for the input span (rare; happens with macro-generated
/// input).
///
/// The expansion emits absolute `::void_ui::…` paths — proc-macros have no
/// `$crate` equivalent. Consequence: downstream crates must depend on the
/// crate under its real name (`void-ui`); renaming via
/// `my_ui = { package = "void-ui" }` will not resolve.
///
/// # Panics
///
/// Panics at compile time if the input does not match the expected shape
/// `<theme_expr> , { <body> }` — i.e. there is no trailing brace group, or
/// no comma separating the theme expression from the body.
#[proc_macro]
#[allow(clippy::too_many_lines)] // Hand-rolled token-stream builder; splitting hurts readability.
pub fn with_source(input: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = input.into_iter().collect();

    // The body must be the *final* token tree, and a brace group. (A comma
    // inside the theme expression is invisible here — it would live inside a
    // parenthesized `Group` — so straight indexing is unambiguous.)
    let body_idx = tokens
        .len()
        .checked_sub(1)
        .expect("with_source!: empty input");
    let body_group = match &tokens[body_idx] {
        TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => g.clone(),
        _ => panic!("with_source!: expected a `{{ ... }}` block as the final argument"),
    };

    // The comma must immediately precede the body group.
    let comma_idx = body_idx
        .checked_sub(1)
        .filter(|&i| matches!(&tokens[i], TokenTree::Punct(p) if p.as_char() == ','))
        .expect("with_source!: expected `theme_expr , {{ ... }}`");

    // Theme expression is everything before that comma.
    let theme_tokens: TokenStream = tokens[..comma_idx].iter().cloned().collect();

    // Capture the body's original source via Span::source_text.
    let source_string = body_group.span().source_text().map_or_else(
        || body_group.stream().to_string(),
        |s| {
            let trimmed = s.trim();
            let inner = trimmed
                .strip_prefix('{')
                .unwrap_or(trimmed)
                .strip_suffix('}')
                .unwrap_or(trimmed);
            dedent(inner.trim_end().trim_matches('\n'))
        },
    );

    let source_literal = Literal::string(&source_string);
    let body_stream = body_group.stream();

    let mut block = TokenStream::new();

    // let __vs_view = { <body> };
    push_idents(&mut block, &["let", "__vs_view"]);
    push_punct(&mut block, '=', Spacing::Alone);
    block.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Brace,
        body_stream,
    ))));
    push_punct(&mut block, ';', Spacing::Alone);

    // let __vs_source: &str = "...";
    push_idents(&mut block, &["let", "__vs_source"]);
    push_punct(&mut block, ':', Spacing::Alone);
    push_punct(&mut block, '&', Spacing::Alone);
    push_ident(&mut block, "str");
    push_punct(&mut block, '=', Spacing::Alone);
    block.extend(std::iter::once(TokenTree::Literal(source_literal)));
    push_punct(&mut block, ';', Spacing::Alone);

    // ::void_ui::layout::overlap_column( __vs_view , ::void_ui::gallery::code_block(__vs_source, <theme>) )
    //     .gap(::xilem::masonry::layout::Length::px(8.0))
    //
    // `overlap_column` keeps the *visual* stacking (`__vs_view` on top,
    // `code_block` below — unchanged from the old `flex_col`) while
    // reversing *paint* order: `__vs_view` (and any `AnchoredOverlay`-hosted
    // overflow inside it, e.g. an open popover or dropdown menu) paints on
    // top of `code_block`, a later visual sibling that would otherwise
    // occlude it once it overflows its own footprint. See
    // `crate::layout::overlap_column` for the mechanism.
    let mut overlap_expr = TokenStream::new();
    push_path(
        &mut overlap_expr,
        &["", "void_ui", "layout", "overlap_column"],
    );

    let mut call_args = TokenStream::new();
    push_ident(&mut call_args, "__vs_view");
    push_punct(&mut call_args, ',', Spacing::Alone);
    push_path(&mut call_args, &["", "void_ui", "gallery", "code_block"]);
    let mut cb_args = TokenStream::new();
    push_ident(&mut cb_args, "__vs_source");
    push_punct(&mut cb_args, ',', Spacing::Alone);
    cb_args.extend(theme_tokens);
    call_args.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        cb_args,
    ))));
    overlap_expr.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        call_args,
    ))));

    // .gap(::xilem::masonry::layout::Length::px(8.0))
    push_punct(&mut overlap_expr, '.', Spacing::Alone);
    push_ident(&mut overlap_expr, "gap");
    let mut gap_args = TokenStream::new();
    push_path(
        &mut gap_args,
        &["", "xilem", "masonry", "layout", "Length", "px"],
    );
    let mut px_args = TokenStream::new();
    px_args.extend(std::iter::once(TokenTree::Literal(
        Literal::f64_unsuffixed(8.0),
    )));
    gap_args.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        px_args,
    ))));
    overlap_expr.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        gap_args,
    ))));

    // ::void_ui::overlay_scope( <overlap_expr> )
    //
    // Registers an `OverlayScope` ancestor so that overlay-shaped
    // descendants of `__vs_view` that use the scope-push path (dropdown
    // menus) discover it, paint in a slot that always comes last, and stay
    // clipped to this block's own bounds. This is a *separate* mechanism
    // from `overlap_column` above — `overlap_column` fixes paint order
    // between `__vs_view` and `code_block` (its flex-sibling occlusion
    // problem), while `overlay_scope` gives in-scope descendants a
    // shared always-on-top slot for popups they push explicitly. The two
    // stack here because both are needed by different demos. See
    // `crate::overlay_scope` for the mechanism and `dropdown_button`'s
    // discovery/fallback for what happens when no scope is present.
    push_path(&mut block, &["", "void_ui", "overlay_scope"]);
    block.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        overlap_expr,
    ))));

    let mut out = TokenStream::new();
    out.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Brace,
        block,
    ))));
    out
}

/// Strips the common leading whitespace from every line, so the call site's
/// nesting depth doesn't show up as indentation in the rendered snippet.
/// Whitespace-only lines are ignored when computing the common prefix and
/// emptied in the output.
fn dedent(s: &str) -> String {
    let min_indent = s
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    s.lines()
        .map(|line| line.get(min_indent..).unwrap_or_else(|| line.trim_start()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn push_ident(stream: &mut TokenStream, ident: &str) {
    stream.extend(std::iter::once(TokenTree::Ident(Ident::new(
        ident,
        Span::call_site(),
    ))));
}

fn push_idents(stream: &mut TokenStream, idents: &[&str]) {
    for ident in idents {
        push_ident(stream, ident);
    }
}

fn push_punct(stream: &mut TokenStream, ch: char, spacing: Spacing) {
    stream.extend(std::iter::once(TokenTree::Punct(Punct::new(ch, spacing))));
}

/// Push a path of the form `::a::b::c`. The first segment may be empty to
/// indicate an absolute path (leading `::`).
///
/// The two colons of `::` are emitted as `Joint` + `Alone` so the lexer
/// treats them as one path-separator token rather than two stray `:`s.
fn push_path(stream: &mut TokenStream, segments: &[&str]) {
    let mut first_ident = true;
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            debug_assert!(
                i == 0,
                "empty segment only allowed at index 0 for leading ::"
            );
            push_punct(stream, ':', Spacing::Joint);
            push_punct(stream, ':', Spacing::Alone);
            continue;
        }
        if !first_ident {
            push_punct(stream, ':', Spacing::Joint);
            push_punct(stream, ':', Spacing::Alone);
        }
        push_ident(stream, seg);
        first_ident = false;
    }
}

#[cfg(test)]
mod tests {
    use super::dedent;

    #[test]
    fn strips_common_leading_whitespace() {
        let input = "    fn foo() {\n        bar();\n    }";
        assert_eq!(dedent(input), "fn foo() {\n    bar();\n}");
    }

    #[test]
    fn ignores_blank_lines_when_computing_the_minimum_and_empties_them() {
        let input = "    a\n\n    b";
        assert_eq!(dedent(input), "a\n\nb");
    }

    #[test]
    fn whitespace_only_lines_are_treated_like_blank_lines() {
        let input = "    a\n   \n    b";
        assert_eq!(dedent(input), "a\n\nb");
    }

    #[test]
    fn already_flush_text_is_unchanged() {
        let input = "a\nb\nc";
        assert_eq!(dedent(input), "a\nb\nc");
    }

    #[test]
    fn minimum_indent_is_set_by_the_least_indented_line() {
        // The least-indented line (1 space) sets the cut point — lines that
        // started more indented keep their *relative* extra indentation.
        let input = "  a\n    b\n c";
        assert_eq!(dedent(input), " a\n   b\nc");
    }
}
