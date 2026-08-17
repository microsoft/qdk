// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn defcal_block_with_unbalanced_braces_errors() {
    check(
        parse,
        "defcal foo q { { }",
        &expect![[r#"
            Error(
                Token(
                    Close(
                        Brace,
                    ),
                    Eof,
                    Span {
                        lo: 0,
                        hi: 18,
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
    defcal foo(a, b) q0 q1 {
        faoi foaijdf a;
        fkfm )(
        .314
    }",
        &expect![[r#"
            Stmt [5-88]:
                annotations: <empty>
                kind: DefCalStmt [5-88]:
                    content: defcal foo(a, b) q0 q1 {
                            faoi foaijdf a;
                            fkfm )(
                            .314
                        }"#]],
    );
}
