// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn if_condition_missing_parens() {
    check(
        parse,
        "if true 3;",
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
                    lo: 3,
                    hi: 7,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn decl_in_if_condition() {
    check(
        parse,
        "if (int[8] myvar = 1) { x $0; }",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Identifier,
                Span {
                    lo: 11,
                    hi: 16,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn assignment_in_if_condition() {
    check(
        parse,
        "if (x = 2) { 3; }",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: IfStmt [0-17]:
                    condition: Expr [4-5]: Ident [4-5] "x"
                    if_body: Stmt [11-17]:
                        annotations: <empty>
                        kind: Block [11-17]:
                            Stmt [13-15]:
                                annotations: <empty>
                                kind: ExprStmt [13-15]:
                                    expr: Expr [13-14]: Lit: Int(3)
                    else_body: <none>

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        Eq,
                        Span {
                            lo: 6,
                            hi: 7,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn binary_op_assignment_in_if_condition() {
    check(
        parse,
        "if (x += 2) { 3; }",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: IfStmt [0-18]:
                    condition: Expr [4-5]: Ident [4-5] "x"
                    if_body: Stmt [12-18]:
                        annotations: <empty>
                        kind: Block [12-18]:
                            Stmt [14-16]:
                                annotations: <empty>
                                kind: ExprStmt [14-16]:
                                    expr: Expr [14-15]: Lit: Int(3)
                    else_body: <none>

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        BinOpEq(
                            Plus,
                        ),
                        Span {
                            lo: 6,
                            hi: 8,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn empty_if_block_fails() {
    check(
        parse,
        "if (true);",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: IfStmt [0-10]:
                    condition: Expr [4-8]: Lit: Bool(true)
                    if_body: Stmt [9-10]:
                        annotations: <empty>
                        kind: Err
                    else_body: <none>

            [
                Error(
                    EmptyStatement(
                        Span {
                            lo: 9,
                            hi: 10,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn empty_if_block_else() {
    check(
        parse,
        "if (true) else x $0;",
        &expect![[r#"
        Error(
            Rule(
                "statement",
                Keyword(
                    Else,
                ),
                Span {
                    lo: 10,
                    hi: 14,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn empty_if_block_else_with_condition() {
    check(
        parse,
        "if (true) else (false) x $0;",
        &expect![[r#"
        Error(
            Rule(
                "statement",
                Keyword(
                    Else,
                ),
                Span {
                    lo: 10,
                    hi: 14,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn reset_in_if_condition() {
    check(
        parse,
        "if (reset $0) { x $1; }",
        &expect![[r#"
        Error(
            Rule(
                "expression",
                Keyword(
                    Reset,
                ),
                Span {
                    lo: 4,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}
