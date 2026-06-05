//! Hand-rolled Rust tokenizer — covers the subset of Rust that appears in
//! `with_source!` panels (which is the only consumer today). Recognizes
//! keywords, types, function calls, string/char/raw-string literals,
//! line and nested block comments, numeric literals with `_` and type
//! suffixes, lifetimes, and the punctuation/operator residue.
//!
//! Identifier classification is purely lexical:
//! - first char uppercase → `TokenKind::Type`
//! - immediately followed by `(` → `TokenKind::Function`
//! - otherwise → `TokenKind::Identifier`

use super::highlighter::{Highlighter, TokenKind, TokenSpan};

/// Built-in Rust highlighter. Zero-sized; cheap to clone or share.
#[derive(Clone, Copy, Debug, Default)]
pub struct RustHighlighter;

const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "dyn", "async", "await", "yield", "abstract", "box", "do", "final", "macro",
    "override", "priv", "typeof", "union", "unsized", "virtual",
];

impl Highlighter for RustHighlighter {
    fn highlight(&self, source: &str) -> Vec<TokenSpan> {
        let bytes = source.as_bytes();
        let mut out = Vec::new();
        let mut cursor = 0;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            // line comment
            if byte == b'/' && bytes.get(cursor + 1) == Some(&b'/') {
                let end = scan_line_comment(bytes, cursor);
                out.push(TokenSpan {
                    range: cursor..end,
                    kind: TokenKind::Comment,
                });
                cursor = end;
                continue;
            }
            // block comment (nested)
            if byte == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
                let end = scan_block_comment(bytes, cursor);
                out.push(TokenSpan {
                    range: cursor..end,
                    kind: TokenKind::Comment,
                });
                cursor = end;
                continue;
            }
            // raw string: r"..." or r#"..."#
            if byte == b'r'
                && matches!(bytes.get(cursor + 1), Some(&(b'"' | b'#')))
                && let Some(end) = scan_raw_string(bytes, cursor)
            {
                out.push(TokenSpan {
                    range: cursor..end,
                    kind: TokenKind::String,
                });
                cursor = end;
                continue;
            }
            // string literal
            if byte == b'"' {
                let end = scan_string(bytes, cursor);
                out.push(TokenSpan {
                    range: cursor..end,
                    kind: TokenKind::String,
                });
                cursor = end;
                continue;
            }
            // char literal vs lifetime
            if byte == b'\'' {
                let (end, kind) = scan_char_or_lifetime(bytes, cursor);
                out.push(TokenSpan {
                    range: cursor..end,
                    kind,
                });
                cursor = end;
                continue;
            }
            // number
            if byte.is_ascii_digit() {
                let end = scan_number(bytes, cursor);
                out.push(TokenSpan {
                    range: cursor..end,
                    kind: TokenKind::Number,
                });
                cursor = end;
                continue;
            }
            // identifier / keyword
            if is_ident_start(byte) {
                let end = scan_ident(bytes, cursor);
                let word = &source[cursor..end];
                let kind = classify_ident(word, bytes.get(end));
                out.push(TokenSpan {
                    range: cursor..end,
                    kind,
                });
                cursor = end;
                continue;
            }
            // whitespace — emit no span (renderer falls back to `plain`)
            if byte.is_ascii_whitespace() {
                cursor += 1;
                continue;
            }
            // Non-ASCII: never split a multi-byte char into per-byte spans —
            // a range off a char boundary breaks ranged styling downstream.
            // Emit no span (renderer falls back to `plain`; this also covers
            // Unicode identifiers, which `is_ident_start` doesn't recognize)
            // and advance past the whole char.
            if !byte.is_ascii() {
                let ch_len = source[cursor..].chars().next().map_or(1, char::len_utf8);
                cursor += ch_len;
                continue;
            }
            // operator vs punctuation — single-byte classification
            out.push(TokenSpan {
                range: cursor..cursor + 1,
                kind: classify_symbol(byte),
            });
            cursor += 1;
        }
        out
    }
}

fn scan_line_comment(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && bytes[end] != b'\n' {
        end += 1;
    }
    end
}

fn scan_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut end = start + 2;
    let mut depth = 1_usize;
    while end < bytes.len() && depth > 0 {
        if bytes[end] == b'/' && bytes.get(end + 1) == Some(&b'*') {
            depth += 1;
            end += 2;
        } else if bytes[end] == b'*' && bytes.get(end + 1) == Some(&b'/') {
            depth -= 1;
            end += 2;
        } else {
            end += 1;
        }
    }
    end
}

fn scan_raw_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start + 1;
    let mut hashes = 0;
    while bytes.get(end) == Some(&b'#') {
        hashes += 1;
        end += 1;
    }
    if bytes.get(end) != Some(&b'"') {
        return None;
    }
    end += 1;
    while end < bytes.len() {
        if bytes[end] == b'"' {
            let mut probe = end + 1;
            let mut matched = 0;
            while matched < hashes && bytes.get(probe) == Some(&b'#') {
                probe += 1;
                matched += 1;
            }
            if matched == hashes {
                return Some(probe);
            }
        }
        end += 1;
    }
    Some(end)
}

fn scan_string(bytes: &[u8], start: usize) -> usize {
    let mut end = start + 1;
    while end < bytes.len() && bytes[end] != b'"' {
        if bytes[end] == b'\\' && end + 1 < bytes.len() {
            end += 2;
        } else {
            end += 1;
        }
    }
    if end < bytes.len() {
        end += 1; // closing "
    }
    end
}

fn scan_char_or_lifetime(bytes: &[u8], start: usize) -> (usize, TokenKind) {
    // '\u{XXXXXX}' is the longest valid char escape: tick + backslash + u + { + 6 hex + } + tick = 11 bytes.
    // Use a small margin to keep the probe loop bounded.
    const MAX_CHAR_LITERAL_LEN: usize = 12;
    let is_char_lit = if bytes.get(start + 1) == Some(&b'\\') {
        // '\n' '\\' '\'' '\t' '\0' '\xNN' '\u{...}'
        let mut probe = start + 2;
        while probe < bytes.len() && bytes[probe] != b'\'' && probe - start < MAX_CHAR_LITERAL_LEN {
            probe += 1;
        }
        bytes.get(probe) == Some(&b'\'')
    } else {
        matches!(bytes.get(start + 1), Some(&c) if c != b'\'')
            && bytes.get(start + 2) == Some(&b'\'')
    };
    if is_char_lit {
        let mut end = start + 1;
        while end < bytes.len() {
            if bytes[end] == b'\\' && end + 1 < bytes.len() {
                end += 2;
            } else if bytes[end] == b'\'' {
                end += 1;
                break;
            } else {
                end += 1;
            }
        }
        return (end, TokenKind::String);
    }
    // lifetime: '  + ident chars
    let mut end = start + 1;
    while end < bytes.len() && is_ident_continue(bytes[end]) {
        end += 1;
    }
    (end, TokenKind::Identifier)
}

fn scan_number(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    // hex/oct/bin prefix
    if bytes[start] == b'0'
        && matches!(
            bytes.get(start + 1),
            Some(&(b'x' | b'X' | b'o' | b'O' | b'b' | b'B'))
        )
    {
        end += 2;
        while end < bytes.len() && (bytes[end].is_ascii_hexdigit() || bytes[end] == b'_') {
            end += 1;
        }
    } else {
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
            end += 1;
        }
        // fractional part
        if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
            end += 1;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                end += 1;
            }
        }
        // exponent
        if matches!(bytes.get(end), Some(&(b'e' | b'E'))) {
            end += 1;
            if matches!(bytes.get(end), Some(&(b'+' | b'-'))) {
                end += 1;
            }
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                end += 1;
            }
        }
    }
    // type suffix (e.g. _u8, i32, f64)
    while end < bytes.len() && is_ident_continue(bytes[end]) {
        end += 1;
    }
    end
}

fn scan_ident(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && is_ident_continue(bytes[end]) {
        end += 1;
    }
    end
}

fn classify_ident(word: &str, next: Option<&u8>) -> TokenKind {
    if KEYWORDS.contains(&word) {
        TokenKind::Keyword
    } else if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        TokenKind::Type
    } else if next == Some(&b'(') {
        TokenKind::Function
    } else {
        TokenKind::Identifier
    }
}

fn classify_symbol(byte: u8) -> TokenKind {
    if matches!(
        byte,
        b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b';' | b':' | b'.' | b'#' | b'?' | b'@'
    ) {
        TokenKind::Punctuation
    } else {
        TokenKind::Operator
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_ident_continue(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Range;

    fn spans(source: &str) -> Vec<(Range<usize>, TokenKind)> {
        RustHighlighter
            .highlight(source)
            .into_iter()
            .map(|s| (s.range, s.kind))
            .collect()
    }

    #[test]
    fn non_ascii_never_splits_char_boundaries() {
        // Unicode identifier chars, math symbols, and curly quotes all fall
        // through the ASCII token branches; none may yield a span that ends
        // or starts mid-char.
        let src = "let \u{3c0} = caf\u{e9} \u{2014} \u{201c}x\u{201d};";
        for (range, kind) in spans(src) {
            assert!(
                src.is_char_boundary(range.start) && src.is_char_boundary(range.end),
                "span {range:?} ({kind:?}) is off a char boundary in {src:?}"
            );
        }
    }

    #[test]
    fn keyword_and_identifier() {
        let s = "let x";
        assert_eq!(
            spans(s),
            vec![(0..3, TokenKind::Keyword), (4..5, TokenKind::Identifier)]
        );
    }

    #[test]
    fn type_starts_uppercase() {
        let s = "Vec";
        assert_eq!(spans(s), vec![(0..3, TokenKind::Type)]);
    }

    #[test]
    fn function_followed_by_paren() {
        let s = "foo()";
        assert_eq!(
            spans(s),
            vec![
                (0..3, TokenKind::Function),
                (3..4, TokenKind::Punctuation),
                (4..5, TokenKind::Punctuation),
            ]
        );
    }

    #[test]
    fn line_comment() {
        let s = "// hi\nx";
        assert_eq!(
            spans(s),
            vec![(0..5, TokenKind::Comment), (6..7, TokenKind::Identifier)]
        );
    }

    #[test]
    fn nested_block_comment() {
        let s = "/* a /* b */ c */";
        assert_eq!(spans(s), vec![(0..17, TokenKind::Comment)]);
    }

    #[test]
    fn unterminated_block_comment_runs_to_eof() {
        let s = "/* no end";
        assert_eq!(spans(s), vec![(0..9, TokenKind::Comment)]);
    }

    #[test]
    fn string_literal() {
        let s = "\"hi\"";
        assert_eq!(spans(s), vec![(0..4, TokenKind::String)]);
    }

    #[test]
    fn raw_string_no_hashes() {
        let s = "r\"a\"b";
        assert_eq!(
            spans(s),
            vec![(0..4, TokenKind::String), (4..5, TokenKind::Identifier)]
        );
    }

    #[test]
    fn raw_string_with_hashes() {
        let s = "r#\"a\"#";
        assert_eq!(spans(s), vec![(0..6, TokenKind::String)]);
    }

    #[test]
    fn char_literal() {
        let s = "'a'";
        assert_eq!(spans(s), vec![(0..3, TokenKind::String)]);
    }

    #[test]
    fn escaped_char_literal() {
        let s = "'\\n'";
        assert_eq!(spans(s), vec![(0..4, TokenKind::String)]);
    }

    #[test]
    fn lifetime_is_identifier_not_string() {
        let s = "&'a T";
        assert_eq!(
            spans(s),
            vec![
                (0..1, TokenKind::Operator),
                (1..3, TokenKind::Identifier),
                (4..5, TokenKind::Type),
            ]
        );
    }

    #[test]
    fn integer_literal() {
        assert_eq!(spans("42"), vec![(0..2, TokenKind::Number)]);
    }

    #[test]
    fn float_literal() {
        assert_eq!(spans("3.14"), vec![(0..4, TokenKind::Number)]);
    }

    #[test]
    fn hex_literal_with_underscore_and_suffix() {
        assert_eq!(spans("0xFF_u8"), vec![(0..7, TokenKind::Number)]);
    }

    #[test]
    fn keywords_round_trip() {
        for kw in [
            "fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait", "for",
            "while", "loop", "if", "else", "match", "return", "const", "self", "Self", "as",
            "where", "in", "move", "ref", "static", "type", "unsafe", "extern", "crate", "super",
            "break", "continue", "true", "false", "union",
        ] {
            let got = spans(kw);
            assert_eq!(got.len(), 1, "{kw} expected one span, got {got:?}");
            let kind = got[0].1;
            // `Self` capitalized → TokenKind::Type by our classifier, that's fine.
            assert!(
                matches!(kind, TokenKind::Keyword | TokenKind::Type),
                "{kw} should be Keyword or Type (for `Self`), got {kind:?}"
            );
        }
    }

    #[test]
    fn empty_input_emits_no_spans() {
        assert_eq!(spans(""), vec![]);
    }

    #[test]
    fn float_exponent_literal() {
        assert_eq!(spans("1.5e10"), vec![(0..6, TokenKind::Number)]);
        assert_eq!(spans("2e-3"), vec![(0..4, TokenKind::Number)]);
    }

    #[test]
    fn raw_string_with_mismatched_inner_hashes() {
        // r##"foo"#bar"## — the inner `"#` is one hash short of the `##` opener,
        // so it must NOT terminate the string. Termination requires `"##`.
        let s = "r##\"foo\"#bar\"##";
        assert_eq!(spans(s), vec![(0..15, TokenKind::String)]);
    }

    #[test]
    fn operators_punctuation_passthrough() {
        let s = "a + b == c";
        let got = RustHighlighter.highlight(s);
        // Spans should not overlap.
        let mut last = 0;
        for span in &got {
            assert!(span.range.start >= last, "overlapping spans: {got:?}");
            last = span.range.end;
        }
    }
}
