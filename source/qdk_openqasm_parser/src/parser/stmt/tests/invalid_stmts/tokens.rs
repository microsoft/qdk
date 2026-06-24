// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
#[allow(clippy::too_many_lines)]
fn bad_tokens() {
    check(
        parse,
        "#;",
        &expect![[r#"
            Stmt [1-2]:
                annotations: <empty>
                kind: Err

            [
                Error(
                    Lex(
                        Incomplete(
                            Ident,
                            Identifier,
                            Single(
                                Semi,
                            ),
                            Span {
                                lo: 1,
                                hi: 2,
                            },
                        ),
                    ),
                ),
                Error(
                    EmptyStatement(
                        Span {
                            lo: 1,
                            hi: 2,
                        },
                    ),
                ),
            ]"#]],
    );

    check(
        parse,
        "3x;",
        &expect![[r#"
        Error(
            ExpectedItem(
                Identifier,
                Span {
                    lo: 0,
                    hi: 1,
                },
            ),
        )
    "#]],
    );

    check(
        parse,
        "x@x;",
        &expect![[r#"
        Stmt [0-1]:
            annotations: <empty>
            kind: ExprStmt [0-1]:
                expr: Expr [0-1]: Ident [0-1] "x"

        [
            Error(
                Token(
                    Semicolon,
                    At,
                    Span {
                        lo: 1,
                        hi: 2,
                    },
                ),
            ),
        ]"#]],
    );

    check(
        parse,
        "3.4.3;",
        &expect![[r#"
        Stmt [0-3]:
            annotations: <empty>
            kind: ExprStmt [0-3]:
                expr: Expr [0-3]: Lit: Float(3.4)

        [
            Error(
                Token(
                    Semicolon,
                    Literal(
                        Float,
                    ),
                    Span {
                        lo: 3,
                        hi: 5,
                    },
                ),
            ),
        ]"#]],
    );

    check(
        parse,
        "3.4e3e3;",
        &expect![[r#"
        Error(
            ExpectedItem(
                Identifier,
                Span {
                    lo: 0,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn bad_integer_literals() {
    check(
        parse,
        "3_4_;",
        &expect![[r#"
            Stmt [4-5]:
                annotations: <empty>
                kind: Err

            [
                Error(
                    Lex(
                        Unknown(
                            '3',
                            Span {
                                lo: 0,
                                hi: 4,
                            },
                        ),
                    ),
                ),
                Error(
                    EmptyStatement(
                        Span {
                            lo: 4,
                            hi: 5,
                        },
                    ),
                ),
            ]"#]],
    );

    check(
        parse,
        "0b123;",
        &expect![[r#"
        Stmt [0-3]:
            annotations: <empty>
            kind: ExprStmt [0-3]:
                expr: Expr [0-3]: Lit: Int(1)

        [
            Error(
                Token(
                    Semicolon,
                    Literal(
                        Integer(
                            Decimal,
                        ),
                    ),
                    Span {
                        lo: 3,
                        hi: 5,
                    },
                ),
            ),
        ]"#]],
    );

    check(
        parse,
        "0B123;",
        &expect![[r#"
        Stmt [0-3]:
            annotations: <empty>
            kind: ExprStmt [0-3]:
                expr: Expr [0-3]: Lit: Int(1)

        [
            Error(
                Token(
                    Semicolon,
                    Literal(
                        Integer(
                            Decimal,
                        ),
                    ),
                    Span {
                        lo: 3,
                        hi: 5,
                    },
                ),
            ),
        ]"#]],
    );

    check(
        parse,
        "0o789;",
        &expect![[r#"
        Stmt [0-3]:
            annotations: <empty>
            kind: ExprStmt [0-3]:
                expr: Expr [0-3]: Lit: Int(7)

        [
            Error(
                Token(
                    Semicolon,
                    Literal(
                        Integer(
                            Decimal,
                        ),
                    ),
                    Span {
                        lo: 3,
                        hi: 5,
                    },
                ),
            ),
        ]"#]],
    );

    check(
        parse,
        "0O789;",
        &expect![[r#"
        Stmt [0-3]:
            annotations: <empty>
            kind: ExprStmt [0-3]:
                expr: Expr [0-3]: Lit: Int(7)

        [
            Error(
                Token(
                    Semicolon,
                    Literal(
                        Integer(
                            Decimal,
                        ),
                    ),
                    Span {
                        lo: 3,
                        hi: 5,
                    },
                ),
            ),
        ]"#]],
    );

    check(
        parse,
        "0x12g;",
        &expect![[r#"
        Error(
            ExpectedItem(
                Identifier,
                Span {
                    lo: 0,
                    hi: 4,
                },
            ),
        )
    "#]],
    );

    check(
        parse,
        "0X12g;",
        &expect![[r#"
        Error(
            ExpectedItem(
                Identifier,
                Span {
                    lo: 0,
                    hi: 4,
                },
            ),
        )
    "#]],
    );

    check(
        parse,
        "12af;",
        &expect![[r#"
        Error(
            ExpectedItem(
                Identifier,
                Span {
                    lo: 0,
                    hi: 2,
                },
            ),
        )
    "#]],
    );
}
