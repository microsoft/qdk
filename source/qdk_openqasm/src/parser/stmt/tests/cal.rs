// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn cal_block_with_unbalanced_braces_errors() {
    check(
        parse,
        "cal { { }",
        &expect![[r#"
        Error(
            Token(
                Close(
                    Brace,
                ),
                Eof,
                Span {
                    lo: 0,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn cal_block_accept_any_tokens_inside() {
    check(
        parse,
        "
    cal {
        faoi foaijdf a;
        fkfm )(
        .314
    }",
        &expect![[r#"
            Stmt [5-69]:
                annotations: <empty>
                kind: CalibrationStmt [5-69]:
                    content: cal {
                            faoi foaijdf a;
                            fkfm )(
                            .314
                        }"#]],
    );
}
