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
                    kind: Newline,
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
fn block_comment() {
    check(
        "/* comment\n x */",
        &expect![[r#"
            [
                Token {
                    kind: Comment(
                        Block,
                    ),
                    offset: 0,
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
                    kind: Newline,
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
                    kind: String {
                        terminated: true,
                    },
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
                    kind: String {
                        terminated: false,
                    },
                    offset: 0,
                },
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
                Token {
                    kind: String {
                        terminated: true,
                    },
                    offset: 0,
                },
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
                Token {
                    kind: String {
                        terminated: true,
                    },
                    offset: 0,
                },
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
                Token {
                    kind: String {
                        terminated: true,
                    },
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn string_escape_return() {
    check(
        r#""\r""#,
        &expect![[r#"
            [
                Token {
                    kind: String {
                        terminated: true,
                    },
                    offset: 0,
                },
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
                Token {
                    kind: String {
                        terminated: true,
                    },
                    offset: 0,
                },
            ]
        "#]],
    );
}

#[test]
fn string_invalid_escape() {
    check(
        r#""\s""#,
        &expect![[r#"
            [
                Token {
                    kind: String {
                        terminated: true,
                    },
                    offset: 0,
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
fn float_dot() {
    check(
        "0..",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
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
fn float_dot2() {
    check(
        ".0.",
        &expect![[r#"
            [
                Token {
                    kind: Number(
                        Float,
                    ),
                    offset: 0,
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
fn leading_dot_float() {
    check(
        ".0",
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
fn dot_float() {
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
fn dot_dot_float() {
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
                    kind: Number(
                        Float,
                    ),
                    offset: 2,
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
fn incomplete_exponent_lexed_as_float() {
    check(
        "1.e",
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
fn incomplete_exp() {
    check(
        "0e",
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
        "1e",
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
fn incomplete_exp2() {
    check(
        "0.e3_",
        &expect![[r#"
            [
                Token {
                    kind: Unknown,
                    offset: 0,
                },
            ]
        "#]],
    );
    check(
        "1e",
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
                    kind: Single(
                        Sharp,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Single(
                        Sharp,
                    ),
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
                    kind: Number(
                        Float,
                    ),
                    offset: 5,
                },
            ]
        "#]],
    );
}

#[test]
fn fragments() {
    check(
        "im dt ns us µs ms s",
        &expect![[r#"
            [
                Token {
                    kind: LiteralFragment(
                        Imag,
                    ),
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 2,
                },
                Token {
                    kind: LiteralFragment(
                        Dt,
                    ),
                    offset: 3,
                },
                Token {
                    kind: Whitespace,
                    offset: 5,
                },
                Token {
                    kind: LiteralFragment(
                        Ns,
                    ),
                    offset: 6,
                },
                Token {
                    kind: Whitespace,
                    offset: 8,
                },
                Token {
                    kind: LiteralFragment(
                        Us,
                    ),
                    offset: 9,
                },
                Token {
                    kind: Whitespace,
                    offset: 11,
                },
                Token {
                    kind: LiteralFragment(
                        Us,
                    ),
                    offset: 12,
                },
                Token {
                    kind: Whitespace,
                    offset: 15,
                },
                Token {
                    kind: LiteralFragment(
                        Ms,
                    ),
                    offset: 16,
                },
                Token {
                    kind: Whitespace,
                    offset: 18,
                },
                Token {
                    kind: LiteralFragment(
                        S,
                    ),
                    offset: 19,
                },
            ]
        "#]],
    );
}

#[test]
fn identifiers_with_fragment_prefixes() {
    check(
        "imx dtx nsx usx µsx msx sx",
        &expect![[r#"
            [
                Token {
                    kind: Ident,
                    offset: 0,
                },
                Token {
                    kind: Whitespace,
                    offset: 3,
                },
                Token {
                    kind: Ident,
                    offset: 4,
                },
                Token {
                    kind: Whitespace,
                    offset: 7,
                },
                Token {
                    kind: Ident,
                    offset: 8,
                },
                Token {
                    kind: Whitespace,
                    offset: 11,
                },
                Token {
                    kind: Ident,
                    offset: 12,
                },
                Token {
                    kind: Whitespace,
                    offset: 15,
                },
                Token {
                    kind: Ident,
                    offset: 16,
                },
                Token {
                    kind: Whitespace,
                    offset: 20,
                },
                Token {
                    kind: Ident,
                    offset: 21,
                },
                Token {
                    kind: Whitespace,
                    offset: 24,
                },
                Token {
                    kind: Ident,
                    offset: 25,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_underscores_digit() {
    check(
        "___3",
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
fn leading_underscores_ident_dot() {
    check(
        "___3.",
        &expect![[r#"
            [
                Token {
                    kind: Ident,
                    offset: 0,
                },
                Token {
                    kind: Single(
                        Dot,
                    ),
                    offset: 4,
                },
            ]
        "#]],
    );
}

#[test]
fn leading_underscores_binary() {
    check(
        "___0b11",
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
fn leading_underscores_binary_extended() {
    check(
        "___0b11abc",
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
fn leading_underscores_identifier() {
    check(
        "___abc",
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
fn hardware_qubit() {
    check(
        "$12",
        &expect![[r#"
        [
            Token {
                kind: HardwareQubit,
                offset: 0,
            },
        ]
    "#]],
    );
}

#[test]
fn hardware_qubit_dot() {
    check(
        "$2.",
        &expect![[r#"
            [
                Token {
                    kind: HardwareQubit,
                    offset: 0,
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
fn incomplete_hardware_qubit() {
    check(
        "$",
        &expect![[r#"
        [
            Token {
                kind: Unknown,
                offset: 0,
            },
        ]
    "#]],
    );
}

#[test]
fn incomplete_hardware_qubit_identifier() {
    check(
        "$a",
        &expect![[r#"
            [
                Token {
                    kind: Unknown,
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
fn incomplete_hardware_qubit_float() {
    check(
        "$.2",
        &expect![[r#"
            [
                Token {
                    kind: Unknown,
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
fn hardware_qubit_with_underscore_at_end() {
    check(
        "$12_",
        &expect![[r#"
        [
            Token {
                kind: HardwareQubit,
                offset: 0,
            },
            Token {
                kind: Ident,
                offset: 3,
            },
        ]
    "#]],
    );
}

#[test]
fn hardware_qubit_with_underscore_in_the_middle() {
    check(
        "$12_3",
        &expect![[r#"
            [
                Token {
                    kind: HardwareQubit,
                    offset: 0,
                },
                Token {
                    kind: Ident,
                    offset: 3,
                },
            ]
        "#]],
    );
}

#[test]
fn decimal_space_imag_semicolon() {
    check(
        "10  im;",
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
                kind: Whitespace,
                offset: 2,
            },
            Token {
                kind: LiteralFragment(
                    Imag,
                ),
                offset: 4,
            },
            Token {
                kind: Single(
                    Semi,
                ),
                offset: 6,
            },
        ]
    "#]],
    );
}
