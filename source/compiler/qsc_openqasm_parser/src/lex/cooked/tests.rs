// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Lexer, Token, TokenKind};
use crate::lex::Delim;
use expect_test::{Expect, expect};
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
        TokenKind::Bang => Some("!".to_string()),
        TokenKind::Tilde => Some("~".to_string()),
        TokenKind::Open(Delim::Brace) => Some("{".to_string()),
        TokenKind::Open(Delim::Bracket) => Some("[".to_string()),
        TokenKind::Open(Delim::Paren) => Some("(".to_string()),
        TokenKind::PlusPlus => Some("++".to_string()),
        TokenKind::Keyword(keyword) => Some(keyword.to_string()),
        TokenKind::Type(type_) => Some(type_.to_string()),
        TokenKind::GPhase => Some("gphase".to_string()),
        TokenKind::DurationOf => Some("durationof".to_string()),
        TokenKind::Semicolon => Some(";".to_string()),
        TokenKind::Arrow => Some("->".to_string()),
        TokenKind::At => Some("@".to_string()),
        TokenKind::ClosedBinOp(op) => Some(op.to_string()),
        TokenKind::BinOpEq(super::ClosedBinOp::AmpAmp | super::ClosedBinOp::BarBar)
        | TokenKind::Literal(_)
        | TokenKind::Annotation
        | TokenKind::Pragma => None,
        TokenKind::BinOpEq(op) => Some(format!("{op}=")),
        TokenKind::ComparisonOp(op) => Some(op.to_string()),
        TokenKind::Identifier => Some("foo".to_string()),
        TokenKind::HardwareQubit => Some("$1".to_string()),
        TokenKind::Eof => Some("EOF".to_string()),
    }
}

#[test]
#[ignore = "Need to talk through how to handle this"]
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
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Amp,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
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
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            AmpAmp,
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
fn amp_plus() {
    check(
        "&+",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            Amp,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 1,
                        },
                    },
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
                        kind: ClosedBinOp(
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
                            AmpAmp,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            AmpAmp,
                        ),
                        span: Span {
                            lo: 2,
                            hi: 4,
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
                        kind: Literal(
                            Integer(
                                Decimal,
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
                        kind: Literal(
                            Integer(
                                Decimal,
                            ),
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
                        kind: Literal(
                            Integer(
                                Decimal,
                            ),
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
fn imag() {
    check(
        "123im",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Literal(
                            Imaginary,
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
fn imag_with_whitespace() {
    check(
        "123 im",
        &expect![[r#"
        [
            Ok(
                Token {
                    kind: Literal(
                        Imaginary,
                    ),
                    span: Span {
                        lo: 0,
                        hi: 6,
                    },
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn imag_with_whitespace_semicolon() {
    check(
        "123 im;",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Literal(
                            Imaginary,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 6,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Semicolon,
                        span: Span {
                            lo: 6,
                            hi: 7,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn negative_imag() {
    check(
        "-123im",
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
                        kind: Literal(
                            Imaginary,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 6,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn positive_imag() {
    check(
        "+123im",
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
                        kind: Literal(
                            Imaginary,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 6,
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
                        kind: Literal(
                            Float,
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
                        kind: Literal(
                            Float,
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
                        kind: Literal(
                            Float,
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
fn leading_point() {
    check(
        ".1",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Literal(
                            Float,
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
fn trailing_point() {
    check(
        "1.",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Literal(
                            Float,
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
fn leading_zero_float() {
    check(
        "0.42",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Literal(
                            Float,
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
fn dot_float() {
    check(
        "..1",
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
                        kind: Literal(
                            Float,
                        ),
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
fn float_dot() {
    check(
        "1..",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Literal(
                            Float,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Dot,
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
fn dot_dot_int_dot_dot() {
    check(
        "..1..",
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
                        kind: Literal(
                            Float,
                        ),
                        span: Span {
                            lo: 1,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Dot,
                        span: Span {
                            lo: 3,
                            hi: 4,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Dot,
                        span: Span {
                            lo: 4,
                            hi: 5,
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
                        kind: Literal(
                            Float,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 2,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Literal(
                            Float,
                        ),
                        span: Span {
                            lo: 2,
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
                        kind: Literal(
                            Float,
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
fn ident() {
    check(
        "foo",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Identifier,
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
                        kind: Literal(
                            String,
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
                        kind: Literal(
                            String,
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
fn string_escape_quote() {
    check(
        r#""\"""#,
        &expect![[r#"
        [
            Ok(
                Token {
                    kind: Literal(
                        String,
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
fn string_escape_single_quote() {
    check(
        r#""\'""#,
        &expect![[r#"
        [
            Ok(
                Token {
                    kind: Literal(
                        String,
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
fn string_escape_newline() {
    check(
        r#""\n""#,
        &expect![[r#"
        [
            Ok(
                Token {
                    kind: Literal(
                        String,
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
fn string_escape_return() {
    check(
        r#""\"""#,
        &expect![[r#"
        [
            Ok(
                Token {
                    kind: Literal(
                        String,
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
fn string_escape_tab() {
    check(
        r#""\t""#,
        &expect![[r#"
        [
            Ok(
                Token {
                    kind: Literal(
                        String,
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
fn string_invalid_escape() {
    check(
        r#""foo\abar" a"#,
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Literal(
                            String,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 10,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Identifier,
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
fn hardware_qubit() {
    check(
        r"$12",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: HardwareQubit,
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
fn unknown() {
    check(
        "##",
        &expect![[r#"
            [
                Err(
                    Incomplete(
                        Ident,
                        Identifier,
                        Single(
                            Sharp,
                        ),
                        Span {
                            lo: 1,
                            hi: 2,
                        },
                    ),
                ),
                Err(
                    IncompleteEof(
                        Ident,
                        Identifier,
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
fn comment() {
    check(
        "//comment\nx",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Identifier,
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
fn block_comment() {
    check(
        "/*comment*/x",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Identifier,
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
                        kind: Identifier,
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
fn annotation() {
    check(
        "@foo.bar 1 2 3;",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Annotation,
                        span: Span {
                            lo: 0,
                            hi: 15,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn pragma() {
    check(
        "pragma",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Pragma,
                        span: Span {
                            lo: 0,
                            hi: 6,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn pragma_ident() {
    check(
        "pragma foo",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Pragma,
                        span: Span {
                            lo: 0,
                            hi: 10,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn sharp_pragma() {
    check(
        "#pragma",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Pragma,
                        span: Span {
                            lo: 0,
                            hi: 7,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn sharp_pragma_ident() {
    check(
        "#pragma foo",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Pragma,
                        span: Span {
                            lo: 0,
                            hi: 11,
                        },
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn dim() {
    check(
        "dim",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Keyword(
                            Dim,
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
fn sharp_dim() {
    check(
        "#dim",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Keyword(
                            Dim,
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
