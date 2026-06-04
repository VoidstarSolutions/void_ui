//! Proc-macros for the `void-ui` crate.
//!
//! Currently exports a single macro: [`with_source`], a replacement for the
//! old `macro_rules!` version that preserved no original source whitespace.

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// Renders a view alongside its original source code.
///
/// Usage: `with_source!(theme_expr, { /* view body */ })`.
///
/// Expands to a `flex_col((body_view, code_block(source, theme_expr)))`
/// stacked column with stretched cross-axis alignment, where `source` is
/// the **original source text** of the body block — including newlines and
/// indentation — recovered via [`proc_macro::Span::source_text`].
///
/// Falls back to a stringified token form if the compiler can't provide
/// source text for the input span (rare; happens with macro-generated
/// input).
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

    // Find the body: the final TokenTree must be a brace group.
    let body_idx = tokens
        .iter()
        .rposition(|t| matches!(t, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace))
        .expect("with_source!: expected a `{ ... }` block as the last argument");
    let body_group = match &tokens[body_idx] {
        TokenTree::Group(g) => g.clone(),
        _ => unreachable!(),
    };

    // Find the comma immediately preceding the body group.
    let comma_idx = tokens[..body_idx]
        .iter()
        .rposition(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ','))
        .expect("with_source!: expected `theme_expr , { ... }`");

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
            inner.trim_end().trim_matches('\n').to_string()
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

    // ::xilem::view::flex_col(( __vs_view , ::void_ui::gallery::code_block(__vs_source, <theme>) ))
    push_path(&mut block, &["", "xilem", "view", "flex_col"]);

    let mut outer_args = TokenStream::new();
    let mut tuple = TokenStream::new();
    push_ident(&mut tuple, "__vs_view");
    push_punct(&mut tuple, ',', Spacing::Alone);
    push_path(&mut tuple, &["", "void_ui", "gallery", "code_block"]);
    let mut cb_args = TokenStream::new();
    push_ident(&mut cb_args, "__vs_source");
    push_punct(&mut cb_args, ',', Spacing::Alone);
    cb_args.extend(theme_tokens);
    tuple.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        cb_args,
    ))));
    outer_args.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        tuple,
    ))));
    block.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        outer_args,
    ))));

    // .cross_axis_alignment(::xilem::view::CrossAxisAlignment::Stretch)
    push_punct(&mut block, '.', Spacing::Alone);
    push_ident(&mut block, "cross_axis_alignment");
    let mut caa_args = TokenStream::new();
    push_path(
        &mut caa_args,
        &["", "xilem", "view", "CrossAxisAlignment", "Stretch"],
    );
    block.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        caa_args,
    ))));

    // .gap(::xilem::masonry::layout::Length::px(8.0))
    push_punct(&mut block, '.', Spacing::Alone);
    push_ident(&mut block, "gap");
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
    block.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        gap_args,
    ))));

    let mut out = TokenStream::new();
    out.extend(std::iter::once(TokenTree::Group(Group::new(
        Delimiter::Brace,
        block,
    ))));
    out
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
