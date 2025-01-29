// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Lexer, Token, TokenKind};
use crate::lex::Delim;
use expect_test::{expect, Expect};
use qsc_data_structures::span::Span;

fn check(input: &str, expect: &Expect) {
    let actual: Vec<_> = Lexer::new(input).collect();
    expect.assert_debug_eq(&actual);
}

fn op_string(kind: TokenKind) -> Option<String> {
    match kind {
        TokenKind::Close(Delim::Brace) => Some("}".to_string()),
        TokenKind::Close(Delim::Bracket) => Some("]".to_string()),
        TokenKind::Close(Delim::Paren) => Some(")".to_string()),
        TokenKind::Colon => Some(":".to_string()),
        TokenKind::Comma => Some(",".to_string()),
        TokenKind::Dot => Some(".".to_string()),
        TokenKind::Eq => Some("=".to_string()),
        TokenKind::Open(Delim::Brace) => Some("{".to_string()),
        TokenKind::Open(Delim::Bracket) => Some("[".to_string()),
        TokenKind::Open(Delim::Paren) => Some("(".to_string()),
        TokenKind::PlusPlus => Some("++".to_string()),
        TokenKind::Keyword(_) => None,
        TokenKind::Type(_) => todo!(),
        TokenKind::GPhase => todo!(),
        TokenKind::Inv => todo!(),
        TokenKind::Pow => todo!(),
        TokenKind::Ctrl => todo!(),
        TokenKind::NegCtrl => todo!(),
        TokenKind::Dim => todo!(),
        TokenKind::DurationOf => todo!(),
        TokenKind::Delay => todo!(),
        TokenKind::Reset => todo!(),
        TokenKind::Measure => todo!(),
        TokenKind::Barrier => todo!(),
        TokenKind::Literal(literal) => todo!(),
        TokenKind::Semicolon => todo!(),
        TokenKind::Arrow => todo!(),
        TokenKind::UnaryOperator(unary_operator) => todo!(),
        TokenKind::BinaryOperator(closed_binary_operator) => todo!(),
        TokenKind::BinaryOperatorEq(closed_binary_operator) => todo!(),
        TokenKind::ComparisonOperator(comparison_operator) => todo!(),
        TokenKind::Identifier => todo!(),
        TokenKind::HardwareQubit => todo!(),
    }
}

#[test]
fn basic_ops() {
    for kind in enum_iterator::all() {
        let Some(input) = op_string(kind) else {
            continue;
        };
        let actual: Vec<_> = Lexer::new(&input).collect();
        let len = input
            .len()
            .try_into()
            .expect("input length should fit into u32");
        assert_eq!(
            actual,
            vec![Ok(Token {
                kind,
                span: Span { lo: 0, hi: len }
            }),]
        );
    }
}

#[test]
fn empty() {
    check(
        "",
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn amp() {
    check(
        "&",
        &expect![[r#"
            [
                Err(
                    IncompleteEof(
                        Single(
                            Amp,
                        ),
                        ClosedBinOp(
                            AmpAmpAmp,
                        ),
                        Span {
                            lo: 1,
                            hi: 1,
                        },
                    ),
                ),
            ]
        "#]],
    );
}

#[test]
fn amp_amp() {
    check(
        "&&",
        &expect![[r#"
            [
                Err(
                    IncompleteEof(
                        Single(
                            Amp,
                        ),
                        ClosedBinOp(
                            AmpAmpAmp,
                        ),
                        Span {
                            lo: 2,
                            hi: 2,
                        },
                    ),
                ),
            ]
        "#]],
    );
}

#[test]
fn amp_plus() {
    check(
        "&+",
        &expect![[r#"
            [
                Err(
                    Incomplete(
                        Single(
                            Amp,
                        ),
                        ClosedBinOp(
                            AmpAmpAmp,
                        ),
                        Single(
                            Plus,
                        ),
                        Span {
                            lo: 1,
                            hi: 2,
                        },
                    ),
                ),
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Plus,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 2,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn amp_multibyte() {
    check(
        "&🦀",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: BinaryOperator(
                            Amp,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Err(
                    Unknown(
                        '🦀',
                        Span {
                            lo: 1,
                            hi: 5,
                        },
                    ),
                ),
            ]
        "#]],
    );
}

#[test]
fn amp_amp_amp_amp() {
    check(
        "&&&&",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            AmpAmpAmp,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            AmpAmpAmp,
                        ),
                        span: Span {
                            lo: 3,
                            hi: 6,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn int() {
    check(
        "123",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn negative_int() {
    check(
        "-123",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Minus,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn positive_int() {
    check(
        "+123",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Plus,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 4,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: BigInt(
                            Decimal,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn negative_bigint() {
    check(
        "-123L",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Minus,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: BigInt(
                            Decimal,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 5,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn positive_bigint() {
    check(
        "+123L",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Plus,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: BigInt(
                            Decimal,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 5,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: Float,
                        span: Span {
                            lo: 0,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn negative_float() {
    check(
        "-1.23",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Minus,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Float,
                        span: Span {
                            lo: 1,
                            hi: 5,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn positive_float() {
    check(
        "+1.23",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Plus,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Float,
                        span: Span {
                            lo: 1,
                            hi: 5,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn leading_point() {
    check(
        ".1",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Dot,
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 2,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn trailing_point() {
    check(
        "1.",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Float,
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn leading_zero_float() {
    check(
        "0.42",
        &expect![[r#"
        [
            Ok(
                Token {
                    kind: Float,
                    span: Span {
                        lo: 0,
                        hi: 4,
                    },
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn dot_dot_int() {
    check(
        "..1",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: DotDot,
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 2,
                            hi: 3,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn dot_dot_dot_int() {
    check(
        "...1",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: DotDotDot,
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 3,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn int_dot_dot() {
    check(
        "1..",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: DotDot,
                        span: Span {
                            lo: 1,
                            hi: 3,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn int_dot_dot_dot() {
    check(
        "1...",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: DotDotDot,
                        span: Span {
                            lo: 1,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn dot_dot_dot_int_dot_dot_dot() {
    check(
        "...1...",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: DotDotDot,
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 3,
                            hi: 4,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: DotDotDot,
                        span: Span {
                            lo: 4,
                            hi: 7,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn two_points_with_leading() {
    check(
        ".1.2",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Dot,
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Float,
                        span: Span {
                            lo: 1,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn leading_point_exp() {
    check(
        ".1e2",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Dot,
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Float,
                        span: Span {
                            lo: 1,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn ident() {
    check(
        "foo",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Normal,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 8,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn string_empty() {
    check(
        r#""""#,
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: String(
                            Normal,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn string_missing_ending() {
    check(
        r#""Uh oh..."#,
        &expect![[r#"
            [
                Err(
                    UnterminatedString(
                        Span {
                            lo: 0,
                            hi: 0,
                        },
                    ),
                ),
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
                Err(
                    UnterminatedString(
                        Span {
                            lo: 0,
                            hi: 0,
                        },
                    ),
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 9,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 3,
                            hi: 4,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 4,
                            hi: 6,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 5,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Err(
                    UnterminatedString(
                        Span {
                            lo: 3,
                            hi: 3,
                        },
                    ),
                ),
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
                Err(
                    UnterminatedString(
                        Span {
                            lo: 0,
                            hi: 0,
                        },
                    ),
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 4,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 3,
                            hi: 5,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Open(
                            Bracket,
                        ),
                        span: Span {
                            lo: 5,
                            hi: 6,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Int(
                            Decimal,
                        ),
                        span: Span {
                            lo: 6,
                            hi: 7,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Close(
                            Bracket,
                        ),
                        span: Span {
                            lo: 7,
                            hi: 8,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 8,
                            hi: 10,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 3,
                            hi: 4,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 4,
                            hi: 7,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 7,
                            hi: 8,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 8,
                            hi: 10,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Normal,
                        ),
                        span: Span {
                            lo: 3,
                            hi: 7,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 7,
                            hi: 9,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 3,
                            hi: 6,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 6,
                            hi: 7,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 7,
                            hi: 9,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 9,
                            hi: 11,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 0,
                            hi: 7,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 7,
                            hi: 8,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Plus,
                        ),
                        span: Span {
                            lo: 9,
                            hi: 10,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                DollarQuote,
                                LBrace,
                            ),
                        ),
                        span: Span {
                            lo: 11,
                            hi: 18,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 18,
                            hi: 19,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 19,
                            hi: 21,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: String(
                            Interpolated(
                                RBrace,
                                Quote,
                            ),
                        ),
                        span: Span {
                            lo: 21,
                            hi: 27,
                        },
                    },
                ),
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
                Err(
                    Unknown(
                        '#',
                        Span {
                            lo: 0,
                            hi: 1,
                        },
                    ),
                ),
                Err(
                    Unknown(
                        '#',
                        Span {
                            lo: 1,
                            hi: 2,
                        },
                    ),
                ),
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
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 10,
                            hi: 11,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: DocComment,
                        span: Span {
                            lo: 0,
                            hi: 10,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 11,
                            hi: 12,
                        },
                    },
                ),
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
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 12,
                            hi: 13,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn unfinished_generic() {
    check(
        "'  T",
        &expect![[r#"
            [
                Err(
                    Incomplete(
                        Ident,
                        AposIdent,
                        Whitespace,
                        Span {
                            lo: 1,
                            hi: 3,
                        },
                    ),
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 3,
                            hi: 4,
                        },
                    },
                ),
            ]
        "#]],
    );
}
#[test]
fn unfinished_generic_2() {
    check(
        "'// test
         T",
        &expect![[r#"
            [
                Err(
                    Incomplete(
                        Ident,
                        AposIdent,
                        Comment(
                            Normal,
                        ),
                        Span {
                            lo: 1,
                            hi: 8,
                        },
                    ),
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 18,
                            hi: 19,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn unfinished_generic_3() {
    check(
        "'    T",
        &expect![[r#"
            [
                Err(
                    Incomplete(
                        Ident,
                        AposIdent,
                        Whitespace,
                        Span {
                            lo: 1,
                            hi: 5,
                        },
                    ),
                ),
                Ok(
                    Token {
                        kind: Ident,
                        span: Span {
                            lo: 5,
                            hi: 6,
                        },
                    },
                ),
            ]
        "#]],
    );
}
#[test]
fn correct_generic() {
    check(
        "'T",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: AposIdent,
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
            ]
        "#]],
    );
}
#[test]
fn generic_missing_ident() {
    check(
        "'",
        &expect![[r#"
            [
                Err(
                    IncompleteEof(
                        Ident,
                        AposIdent,
                        Span {
                            lo: 1,
                            hi: 1,
                        },
                    ),
                ),
            ]
        "#]],
    );
}

#[test]
fn generic_underscore_name() {
    check(
        "'_",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: AposIdent,
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
            ]
        "#]],
    );
}
