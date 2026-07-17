// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn include_with_no_literal() {
    check(
        parse,
        "include;",
        &expect![[r#"
        Error(
            Rule(
                "string literal",
                Semicolon,
                Span {
                    lo: 7,
                    hi: 8,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn include_with_non_string_literal() {
    check(
        parse,
        "include 5;",
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
                    lo: 8,
                    hi: 9,
                },
            ),
        )
    "#]],
    );
}
