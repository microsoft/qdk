// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Lexer;
use crate::lex::raw::{Single, Token, TokenKind};
use expect_test::{Expect, expect};

fn check(input: &str, expect: &Expect) {
    let actual: Vec<_> = Lexer::new(input).collect();
    expect.assert_debug_eq(&actual);
}

#[test]
fn singles() {
    for single in enum_iterator::all::<Single>() {
        let actual: Vec<_> = Lexer::new(&single.to_string()).collect();
        let kind = TokenKind::Single(single);
        assert_eq!(actual, vec![Token { kind, offset: 0 }]);
    }
}

#[test]
fn braces() {
    check(
        "{}",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Open(
                            Brace,
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Single(
                        Close(
                            Brace,
                        ),
                    ),
                    offset: 1,
                },
            ]
        "#]],
    );
}

#[test]
fn negate() {
    check(
        "-x",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Minus,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Ident,
                    offset: 1,
                },
            ]
        "#]],
    );
}

#[test]
fn whitespace() {
    check(
        "-   x",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Minus,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 1,
                },
                Token {
                    kind: Ident,
                    offset: 4,
                },
            ]
        "#]],
    );
}

#[test]
fn comment() {
    check(
        "//comment\nx",
        &expect![[r#"
            [
                Token {
                    kind: Comment(
                        Normal,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 9,
                },
                Token {
                    kind: Ident,
                    offset: 10,
                },
            ]
        "#]],
    );
}

#[test]
fn doc_comment() {
    check(
        "///comment\nx",
        &expect![[r#"
            [
                Token {
                    kind: Comment(
                        Doc,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 10,
                },
                Token {
                    kind: Ident,
                    offset: 11,
                },
            ]
        "#]],
    );
}

#[test]
fn comment_four_slashes() {
    check(
        "////comment\nx",
        &expect![[r#"
            [
                Token {
                    kind: Comment(
                        Normal,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 11,
                },
                Token {
                    kind: Ident,
                    offset: 12,
                },
            ]
        "#]],
    );
}

#[test]
fn string() {
    check(
        r#""string""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Normal {
                            terminated: true,
                        },
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn string_escape_quote() {
    check(
        r#""str\"ing""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Normal {
                            terminated: true,
                        },
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn string_missing_ending() {
    check(
        r#""string"#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Normal {
                            terminated: false,
                        },
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_missing_ending() {
    check(
        r#"$"string"#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            None,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string() {
    check(
        r#"$"string""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_braced() {
    check(
        r#"$"{x}""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Ident,
                    offset: 3,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 4,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_block() {
    check(
        r#"$"{ {x} }""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 3,
                },
                Token {
                    kind: Single(
                        Open(
                            Brace,
                        ),
                    ),
                    offset: 4,
                },
                Token {
                    kind: Ident,
                    offset: 5,
                },
                Token {
                    kind: Single(
                        Close(
                            Brace,
                        ),
                    ),
                    offset: 6,
                },
                Token {
                    kind: Whitespace,
                    offset: 7,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 8,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_nested_block() {
    check(
        r#"$"{ { {x} } }""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 3,
                },
                Token {
                    kind: Single(
                        Open(
                            Brace,
                        ),
                    ),
                    offset: 4,
                },
                Token {
                    kind: Whitespace,
                    offset: 5,
                },
                Token {
                    kind: Single(
                        Open(
                            Brace,
                        ),
                    ),
                    offset: 6,
                },
                Token {
                    kind: Ident,
                    offset: 7,
                },
                Token {
                    kind: Single(
                        Close(
                            Brace,
                        ),
                    ),
                    offset: 8,
                },
                Token {
                    kind: Whitespace,
                    offset: 9,
                },
                Token {
                    kind: Single(
                        Close(
                            Brace,
                        ),
                    ),
                    offset: 10,
                },
                Token {
                    kind: Whitespace,
                    offset: 11,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 12,
                },
            ]
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn interpolated_string_for_loop() {
    check(
        r#"$"{for i in 1..3 {}}""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Ident,
                    offset: 3,
                },
                Token {
                    kind: Whitespace,
                    offset: 6,
                },
                Token {
                    kind: Ident,
                    offset: 7,
                },
                Token {
                    kind: Whitespace,
                    offset: 8,
                },
                Token {
                    kind: Ident,
                    offset: 9,
                },
                Token {
                    kind: Whitespace,
                    offset: 11,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 12,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 13,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 14,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 15,
                },
                Token {
                    kind: Whitespace,
                    offset: 16,
                },
                Token {
                    kind: Single(
                        Open(
                            Brace,
                        ),
                    ),
                    offset: 17,
                },
                Token {
                    kind: Single(
                        Close(
                            Brace,
                        ),
                    ),
                    offset: 18,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 19,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_unclosed_block() {
    check(
        r#"$"{ {x }""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 3,
                },
                Token {
                    kind: Single(
                        Open(
                            Brace,
                        ),
                    ),
                    offset: 4,
                },
                Token {
                    kind: Ident,
                    offset: 5,
                },
                Token {
                    kind: Whitespace,
                    offset: 6,
                },
                Token {
                    kind: Single(
                        Close(
                            Brace,
                        ),
                    ),
                    offset: 7,
                },
                Token {
                    kind: String(
                        Normal {
                            terminated: false,
                        },
                    ),
                    offset: 8,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_escape_brace() {
    check(
        r#"$"\{""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_unclosed_brace() {
    check(
        r#"$"{"#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_unclosed_brace_quote() {
    check(
        r#"$"{""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: String(
                        Normal {
                            terminated: false,
                        },
                    ),
                    offset: 3,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_unopened_brace() {
    check(
        r#"$"}"#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            None,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_unopened_brace_quote() {
    check(
        r#"$"}""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_braced_index() {
    check(
        r#"$"{xs[0]}""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Ident,
                    offset: 3,
                },
                Token {
                    kind: Single(
                        Open(
                            Bracket,
                        ),
                    ),
                    offset: 5,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 6,
                },
                Token {
                    kind: Single(
                        Close(
                            Bracket,
                        ),
                    ),
                    offset: 7,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 8,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_two_braced() {
    check(
        r#"$"{x} {y}""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Ident,
                    offset: 3,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 4,
                },
                Token {
                    kind: Ident,
                    offset: 7,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 8,
                },
            ]
        "#]],
    );
}

#[test]
fn interpolated_string_braced_normal_string() {
    check(
        r#"$"{"{}"}""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: String(
                        Normal {
                            terminated: true,
                        },
                    ),
                    offset: 3,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 7,
                },
            ]
        "#]],
    );
}

#[test]
fn nested_interpolated_string() {
    check(
        r#"$"{$"{x}"}""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 3,
                },
                Token {
                    kind: Ident,
                    offset: 6,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 7,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 9,
                },
            ]
        "#]],
    );
}

#[test]
fn nested_interpolated_string_with_exprs() {
    check(
        r#"$"foo {x + $"bar {y}"} baz""#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Ident,
                    offset: 7,
                },
                Token {
                    kind: Whitespace,
                    offset: 8,
                },
                Token {
                    kind: Single(
                        Plus,
                    ),
                    offset: 9,
                },
                Token {
                    kind: Whitespace,
                    offset: 10,
                },
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 11,
                },
                Token {
                    kind: Ident,
                    offset: 18,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 19,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 21,
                },
            ]
        "#]],
    );
}

#[test]
fn nested_interpolated_string_followed_by_braces() {
    check(
        r#"$"{$"{x}"}" {y}"#,
        &expect![[r#"
            [
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: String(
                        Interpolated(
                            DollarQuote,
                            Some(
                                LBrace,
                            ),
                        ),
                    ),
                    offset: 3,
                },
                Token {
                    kind: Ident,
                    offset: 6,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 7,
                },
                Token {
                    kind: String(
                        Interpolated(
                            RBrace,
                            Some(
                                Quote,
                            ),
                        ),
                    ),
                    offset: 9,
                },
                Token {
                    kind: Whitespace,
                    offset: 11,
                },
                Token {
                    kind: Single(
                        Open(
                            Brace,
                        ),
                    ),
                    offset: 12,
                },
                Token {
                    kind: Ident,
                    offset: 13,
                },
                Token {
                    kind: Single(
                        Close(
                            Brace,
                        ),
                    ),
                    offset: 14,
                },
            ]
        "#]],
    );
}

#[test]
fn binary() {
    check(
        "0b10110",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Binary,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
    check(
        "0B10110",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Binary,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn octal() {
    check(
        "0o70351",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Octal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
    check(
        "0O70351",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Octal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn decimal() {
    check(
        "123",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn number_seps() {
    check(
        "123_456",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn number_underscore_prefix() {
    check(
        "_123_456",
        &expect![[r#"
            [
                Token {
                    kind: Ident,
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn int_dot_dot() {
    check(
        "0..",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 1,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 2,
                },
            ]
        "#]],
    );
}

#[test]
fn dot_dot_int() {
    check(
        "..0",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 1,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 2,
                },
            ]
        "#]],
    );
}

#[test]
fn dot_dot_dot_int() {
    check(
        "...0",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 1,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 2,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 3,
                },
            ]
        "#]],
    );
}

#[test]
fn hexadecimal() {
    check(
        "0x123abc",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Hexadecimal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
    check(
        "0X123abc",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Hexadecimal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn bigint() {
    check(
        "123L",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        BigInt(
                            Decimal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn negative() {
    check(
        "-4",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Minus,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 1,
                },
            ]
        "#]],
    );
}

#[test]
fn positive() {
    check(
        "+4",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Plus,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 1,
                },
            ]
        "#]],
    );
}

#[test]
fn bigint_hexadecimal() {
    check(
        "0x123abcL",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        BigInt(
                            Hexadecimal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn float() {
    check(
        "1.23",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_zero() {
    check(
        "0123",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_point() {
    check(
        ".123",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 1,
                },
            ]
        "#]],
    );
}

#[test]
fn trailing_point() {
    check(
        "123.",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn exp() {
    check(
        "1e23",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
    check(
        "1E23",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn exp_plus() {
    check(
        "1e+23",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn exp_minus() {
    check(
        "1e-23",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_point_exp() {
    check(
        ".25e2",
        &expect![[r#"
            [
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 1,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_zero_point() {
    check(
        "0.25",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_zero_zero_point() {
    check(
        "00.25",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_zero_exp() {
    check(
        "0.25e2",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn unknown() {
    check(
        "##",
        &expect![[r#"
            [
                Token {
                    kind: Unknown,
                    offset: 0,
                },
                Token {
                    kind: Unknown,
                    offset: 1,
                },
            ]
        "#]],
    );
}

#[test]
fn float_hexadecimal() {
    check(
        "0x123.45",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Int(
                            Hexadecimal,
                        ),
                    ),
                    offset: 0,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 5,
                },
                Token {
                    kind: Number(
                        Int(
                            Decimal,
                        ),
                    ),
                    offset: 6,
                },
            ]
        "#]],
    );
}
