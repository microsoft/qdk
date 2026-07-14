// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn multiple_defcalgrammar_in_same_stmt() {
    check(
        parse,
        r#"defcalgrammar "openpulse" defcalgrammar "openpulse";"#,
        &expect![[r#"
            Stmt [0-25]:
                annotations: <empty>
                kind: CalibrationGrammarStmt [0-25]:
                    name: openpulse

            [
                Error(
                    Token(
                        Semicolon,
                        Keyword(
                            DefCalGrammar,
                        ),
                        Span {
                            lo: 26,
                            hi: 39,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn defcalgrammar_with_wrong_literal_kind() {
    check(
        parse,
        "defcalgrammar 3;",
        &expect![[r#"
        Error(
            Rule(
                "string literal",
                Literal(
                    Integer(
                        Decimal,
                    ),
                ),
                Span {
                    lo: 14,
                    hi: 15,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn defcal_bad_signature() {
    check(
        parse,
        "defcal x $0 -> int[8] -> int[8] {}",
        &expect![[r#"
            Stmt [0-34]:
                annotations: <empty>
                kind: DefCalStmt [0-34]:
                    content: defcal x $0 -> int[8] -> int[8] {}"#]],
    );
}
