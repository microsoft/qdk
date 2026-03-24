// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn input_missing_ident() {
    check(
        parse,
        "input int[8];",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Semicolon,
                Span {
                    lo: 12,
                    hi: 13,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn output_missing_ident() {
    check(
        parse,
        "output int[8];",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Semicolon,
                Span {
                    lo: 13,
                    hi: 14,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn input_qreg_missing_ident() {
    check(
        parse,
        "input qreg myvar[4];",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    QReg,
                ),
                Span {
                    lo: 6,
                    hi: 10,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn output_qreg_missing_ident() {
    check(
        parse,
        "output qreg myvar[4];",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    QReg,
                ),
                Span {
                    lo: 7,
                    hi: 11,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn initialized_input() {
    check(
        parse,
        "input int[8] myvar = 32;",
        &expect![[r#"
        Stmt [0-18]:
            annotations: <empty>
            kind: IODeclaration [0-18]:
                io_keyword: input
                type: ScalarType [6-12]: IntType [6-12]:
                    size: Expr [10-11]: Lit: Int(8)
                ident: Ident [13-18] "myvar"

        [
            Error(
                Token(
                    Semicolon,
                    Eq,
                    Span {
                        lo: 19,
                        hi: 20,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn initialized_output() {
    check(
        parse,
        "output int[8] myvar = 32;",
        &expect![[r#"
        Stmt [0-19]:
            annotations: <empty>
            kind: IODeclaration [0-19]:
                io_keyword: output
                type: ScalarType [7-13]: IntType [7-13]:
                    size: Expr [11-12]: Lit: Int(8)
                ident: Ident [14-19] "myvar"

        [
            Error(
                Token(
                    Semicolon,
                    Eq,
                    Span {
                        lo: 20,
                        hi: 21,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn input_missing_type() {
    check(
        parse,
        "input myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Identifier,
                Span {
                    lo: 6,
                    hi: 11,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn output_missing_type() {
    check(
        parse,
        "output myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Identifier,
                Span {
                    lo: 7,
                    hi: 12,
                },
            ),
        )
    "#]],
    );
}
