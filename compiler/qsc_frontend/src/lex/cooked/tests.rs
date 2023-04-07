// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Lexer, Token, TokenKind};
use crate::lex::Delim;
use expect_test::{expect, Expect};
use qsc_ast::ast::Span;

fn check(input: &str, expect: &Expect) {
    let actual: Vec<_> = Lexer::new(input).collect();
    expect.assert_debug_eq(&actual);
}

fn op_string(kind: TokenKind) -> Option<String> {
    match kind {
        TokenKind::Apos => Some("'".to_string()),
        TokenKind::At => Some("@".to_string()),
        TokenKind::Bang => Some("!".to_string()),
        TokenKind::Bar => Some("|".to_string()),
        TokenKind::BinOpEq(op) => Some(format!("{op}=")),
        TokenKind::Close(Delim::Brace) => Some("}".to_string()),
        TokenKind::Close(Delim::Bracket) => Some("]".to_string()),
        TokenKind::Close(Delim::Paren) => Some(")".to_string()),
        TokenKind::ClosedBinOp(op) => Some(op.to_string()),
        TokenKind::Colon => Some(":".to_string()),
        TokenKind::ColonColon => Some("::".to_string()),
        TokenKind::Comma => Some(",".to_string()),
        TokenKind::Dot => Some(".".to_string()),
        TokenKind::DotDot => Some("..".to_string()),
        TokenKind::DotDotDot => Some("...".to_string()),
        TokenKind::Eq => Some("=".to_string()),
        TokenKind::EqEq => Some("==".to_string()),
        TokenKind::FatArrow => Some("=>".to_string()),
        TokenKind::Gt => Some(">".to_string()),
        TokenKind::Gte => Some(">=".to_string()),
        TokenKind::LArrow => Some("<-".to_string()),
        TokenKind::Lt => Some("<".to_string()),
        TokenKind::Lte => Some("<=".to_string()),
        TokenKind::Ne => Some("!=".to_string()),
        TokenKind::Open(Delim::Brace) => Some("{".to_string()),
        TokenKind::Open(Delim::Bracket) => Some("[".to_string()),
        TokenKind::Open(Delim::Paren) => Some("(".to_string()),
        TokenKind::Question => Some("?".to_string()),
        TokenKind::RArrow => Some("->".to_string()),
        TokenKind::Semi => Some(";".to_string()),
        TokenKind::TildeTildeTilde => Some("~~~".to_string()),
        TokenKind::WSlash => Some("w/".to_string()),
        TokenKind::WSlashEq => Some("w/=".to_string()),
        TokenKind::BigInt(_)
        | TokenKind::Eof
        | TokenKind::Float
        | TokenKind::Ident
        | TokenKind::Int(_)
        | TokenKind::String => None,
    }
}

#[test]
fn basic_ops() {
    for kind in enum_iterator::all() {
        let Some(input) = op_string(kind) else { continue };
        let actual: Vec<_> = Lexer::new(&input).collect();
        let len = input.len();
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
                        Amp,
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
                        Amp,
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
                        Amp,
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
                Err(
                    Incomplete(
                        Amp,
                        ClosedBinOp(
                            AmpAmpAmp,
                        ),
                        Unknown,
                        Span {
                            lo: 1,
                            hi: 5,
                        },
                    ),
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
fn amp_amp_amp_amp_amp_amp() {
    check(
        "&&&&&&",
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
fn caret_caret() {
    check(
        "^^",
        &expect![[r#"
            [
                Err(
                    IncompleteEof(
                        Caret,
                        ClosedBinOp(
                            CaretCaretCaret,
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
fn and_ws_eq() {
    check(
        "and =",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: ClosedBinOp(
                            And,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 3,
                        },
                    },
                ),
                Ok(
                    Token {
                        kind: Eq,
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
fn w() {
    check(
        "w",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: Ident,
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
fn w_slash_eq_ident() {
    check(
        "w/=foo",
        &expect![[r#"
            [
                Ok(
                    Token {
                        kind: WSlashEq,
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
                        kind: String,
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
                        kind: String,
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
fn string_missing_quote() {
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
