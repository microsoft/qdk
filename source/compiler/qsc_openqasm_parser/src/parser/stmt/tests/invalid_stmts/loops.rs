// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn for_missing_var_type() {
    check(
        parse,
        "for myvar in { 1, 2, 3 };",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Identifier,
                Span {
                    lo: 4,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn for_multiple_vars() {
    check(
        parse,
        "for myvar1, myvar2 in { 1, 2, 3 } { x $0; }",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
                    Identifier,
                    Span {
                        lo: 4,
                        hi: 10,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn for_missing_var_type_and_invalid_collection() {
    check(
        parse,
        "for myvar in { x $0; } { x $0; }",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Identifier,
                Span {
                    lo: 4,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn for_missing_var_type_and_keyword_in_collection() {
    check(
        parse,
        "for myvar in for { x $0; }",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Identifier,
                Span {
                    lo: 4,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn for_bad_syntax() {
    check(
        parse,
        "for myvar { x $0; }",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Identifier,
                Span {
                    lo: 4,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn for_with_while_syntax() {
    check(
        parse,
        "for (true) { x $0; }",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Open(
                    Paren,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn for_missing_var_and_collection() {
    check(
        parse,
        "for { x $0; }",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Open(
                    Brace,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn for_invalid_var_name() {
    check(
        parse,
        "for for in { 1, 2, 3 } { x $0; }",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Keyword(
                    For,
                ),
                Span {
                    lo: 4,
                    hi: 7,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn for_missing_var() {
    check(
        parse,
        "for in { 1, 2, 3 } { x $0; }",
        &expect![[r#"
        Error(
            Rule(
                "scalar type",
                Keyword(
                    In,
                ),
                Span {
                    lo: 4,
                    hi: 6,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn while_missing_parens() {
    check(
        parse,
        "while true { x $0; }",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Keyword(
                    True,
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
fn while_multi_condition() {
    check(
        parse,
        "while (true) (true) { x $0; }",
        &expect![[r#"
            Stmt [0-19]:
                annotations: <empty>
                kind: WhileLoop [0-19]:
                    condition: Expr [7-11]: Lit: Bool(true)
                    body: Stmt [13-19]:
                        annotations: <empty>
                        kind: ExprStmt [13-19]:
                            expr: Expr [13-19]: Paren Expr [14-18]: Lit: Bool(true)

            [
                Error(
                    Token(
                        Semicolon,
                        Open(
                            Brace,
                        ),
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
fn while_with_for_syntax() {
    check(
        parse,
        "while x in { 1, 2, 3 } { x $0; }",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Identifier,
                Span {
                    lo: 6,
                    hi: 7,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn while_missing_body_fails() {
    check(
        parse,
        "while (true);",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: WhileLoop [0-13]:
                    condition: Expr [7-11]: Lit: Bool(true)
                    body: Stmt [12-13]:
                        annotations: <empty>
                        kind: Err

            [
                Error(
                    EmptyStatement(
                        Span {
                            lo: 12,
                            hi: 13,
                        },
                    ),
                ),
            ]"#]],
    );
}
