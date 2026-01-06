// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn missing_target() {
    check(
        parse,
        "switch () {}",
        &expect![[r#"
        Error(
            Rule(
                "expression",
                Close(
                    Paren,
                ),
                Span {
                    lo: 8,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn missing_cases() {
    check(
        parse,
        "switch (i) { x $0 }",
        &expect![[r#"
        Stmt [0-19]:
            annotations: <empty>
            kind: SwitchStmt [0-19]:
                target: Expr [8-9]: Ident [8-9] "i"
                cases: <empty>
                default_case: <none>

        [
            Error(
                MissingSwitchCases(
                    Span {
                        lo: 13,
                        hi: 12,
                    },
                ),
            ),
            Error(
                Token(
                    Close(
                        Brace,
                    ),
                    Identifier,
                    Span {
                        lo: 13,
                        hi: 14,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn missing_case_labels() {
    check(
        parse,
        "switch (i) { case {} }",
        &expect![[r#"
        Stmt [0-22]:
            annotations: <empty>
            kind: SwitchStmt [0-22]:
                target: Expr [8-9]: Ident [8-9] "i"
                cases:
                    SwitchCase [13-20]:
                        labels: <empty>
                        block: Block [18-20]: <empty>
                default_case: <none>

        [
            Error(
                MissingSwitchCaseLabels(
                    Span {
                        lo: 13,
                        hi: 17,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn invalid_label_sequence() {
    check(
        parse,
        "switch (i) { case 1,, {} }",
        &expect![[r#"
        Stmt [0-26]:
            annotations: <empty>
            kind: SwitchStmt [0-26]:
                target: Expr [8-9]: Ident [8-9] "i"
                cases:
                    SwitchCase [13-24]:
                        labels:
                            Expr [18-19]: Lit: Int(1)
                            Expr [20-20]: Err
                        block: Block [22-24]: <empty>
                default_case: <none>

        [
            Error(
                MissingSeqEntry(
                    Span {
                        lo: 20,
                        hi: 20,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn default_case_with_label() {
    check(
        parse,
        "switch (i) { default 0 {} }",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Brace,
                ),
                Literal(
                    Integer(
                        Decimal,
                    ),
                ),
                Span {
                    lo: 21,
                    hi: 22,
                },
            ),
        )

        [
            Error(
                MissingSwitchCases(
                    Span {
                        lo: 13,
                        hi: 12,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn bad_case_syntax() {
    check(
        parse,
        "switch (i) { default, default {} }",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Brace,
                ),
                Comma,
                Span {
                    lo: 20,
                    hi: 21,
                },
            ),
        )

        [
            Error(
                MissingSwitchCases(
                    Span {
                        lo: 13,
                        hi: 12,
                    },
                ),
            ),
        ]"#]],
    );
}
