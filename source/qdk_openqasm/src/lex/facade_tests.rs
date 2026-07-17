// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{RawTokenKind, tokenize};

#[test]
fn raw_tokens_reconstruct_unicode_source_with_contiguous_utf8_spans() {
    let source = "OPENQASM 3.0;\n// µs\nqubit q; §";
    let tokens = tokenize(source);

    assert_eq!(
        tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<String>(),
        source
    );
    assert_eq!(tokens.first().map(|token| token.span.lo), Some(0));
    assert_eq!(
        tokens.last().map(|token| token.span.hi),
        Some(u32::try_from(source.len()).expect("test source length should fit"))
    );
    assert!(
        tokens
            .windows(2)
            .all(|pair| pair[0].span.hi == pair[1].span.lo)
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::Unknown)
    );
}

#[test]
fn raw_tokens_preserve_trivia_details_and_incomplete_literals() {
    let source = "  // line\r\n/* block */ 0b10 0o7 12 0xF 1.5 10ns + \"01";
    let tokens = tokenize(source);

    assert!(tokens.iter().any(|token| {
        token.kind == RawTokenKind::Comment && token.detail == Some("line") && token.is_trivia
    }));
    assert!(tokens.iter().any(|token| {
        token.kind == RawTokenKind::Comment && token.detail == Some("block") && token.is_trivia
    }));
    for detail in ["binary", "octal", "decimal", "hex", "float"] {
        assert!(tokens.iter().any(|token| token.detail == Some(detail)));
    }
    assert!(tokens.iter().any(|token| {
        token.kind == RawTokenKind::LiteralFragment && token.detail == Some("ns")
    }));
    assert!(
        tokens
            .iter()
            .any(|token| { token.kind == RawTokenKind::Punctuation && token.detail == Some("+") })
    );
    assert!(tokens.iter().any(|token| {
        token.kind == RawTokenKind::Bitstring && !token.is_complete && token.text == "\"01"
    }));
}

#[test]
fn unterminated_block_comment_remains_visible_as_trivia() {
    let source = "/* unterminated µ";
    let tokens = tokenize(source);

    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, RawTokenKind::Comment);
    assert_eq!(tokens[0].detail, Some("block"));
    assert_eq!(tokens[0].text, source);
    assert!(tokens[0].is_trivia);
    assert!(tokens[0].is_complete);
    assert_eq!(
        tokens[0].span.hi,
        u32::try_from(source.len()).expect("test source length should fit")
    );
}

#[test]
fn empty_source_has_no_raw_tokens() {
    assert!(tokenize("").is_empty());
}
