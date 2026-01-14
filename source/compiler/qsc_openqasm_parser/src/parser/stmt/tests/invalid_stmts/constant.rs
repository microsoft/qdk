// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn const_decl_missing_type_and_init() {
    check(
        parse,
        "const myvar;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
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
fn const_decl_eq_missing_type_and_init() {
    check(
        parse,
        "const myvar = ;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
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
fn const_decl_missing_type() {
    check(
        parse,
        "const myvar = 8.0;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
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
fn invalid_input() {
    check(
        parse,
        "input const myvar = 8;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    Const,
                ),
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
fn invalid_output() {
    check(
        parse,
        "output const myvar = 8;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    Const,
                ),
                Span {
                    lo: 7,
                    hi: 12,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn invalid_const_input() {
    check(
        parse,
        "const input myvar = 8;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
                    Keyword(
                        Input,
                    ),
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
fn invalid_const_output() {
    check(
        parse,
        "const output myvar = 8;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
                    Keyword(
                        Output,
                    ),
                    Span {
                        lo: 6,
                        hi: 12,
                    },
                ),
            )
        "#]],
    );
}
