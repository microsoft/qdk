// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn defcalgrammar() {
    check(
        parse,
        r#"defcalgrammar "openpulse";"#,
        &expect![[r#"
            Stmt [0-26]:
                annotations: <empty>
                kind: CalibrationGrammarStmt [0-26]:
                    name: openpulse"#]],
    );
}

#[test]
fn defcalgrammar_with_non_string_literal() {
    check(
        parse,
        r#"defcalgrammar 5;"#,
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
fn defcalgrammar_with_no_literal() {
    check(
        parse,
        r#"defcalgrammar;"#,
        &expect![[r#"
            Error(
                Rule(
                    "string literal",
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
