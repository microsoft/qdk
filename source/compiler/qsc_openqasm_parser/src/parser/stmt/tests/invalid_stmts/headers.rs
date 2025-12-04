// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn invalid_version_type() {
    check(
        parse,
        "OPENQASM int;",
        &expect![[r#"
        Error(
            Rule(
                "statement",
                Keyword(
                    OpenQASM,
                ),
                Span {
                    lo: 0,
                    hi: 8,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn invalid_version_literal() {
    check(
        parse,
        "OPENQASM 'hello, world';",
        &expect![[r#"
        Error(
            Rule(
                "statement",
                Keyword(
                    OpenQASM,
                ),
                Span {
                    lo: 0,
                    hi: 8,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn invalid_version_missing_dot() {
    check(
        parse,
        "OPENQASM 3 3;",
        &expect![[r#"
        Error(
            Rule(
                "statement",
                Keyword(
                    OpenQASM,
                ),
                Span {
                    lo: 0,
                    hi: 8,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn invalid_version() {
    check(
        parse,
        "OPENQASM 3.x;",
        &expect![[r#"
        Error(
            Rule(
                "statement",
                Keyword(
                    OpenQASM,
                ),
                Span {
                    lo: 0,
                    hi: 8,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn include_int() {
    check(
        parse,
        "include 3;",
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

#[test]
fn include_include() {
    check(
        parse,
        "include include;",
        &expect![[r#"
        Error(
            Rule(
                "string literal",
                Keyword(
                    Include,
                ),
                Span {
                    lo: 8,
                    hi: 15,
                },
            ),
        )

        [
            Error(
                Token(
                    Semicolon,
                    Keyword(
                        Include,
                    ),
                    Span {
                        lo: 8,
                        hi: 15,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn include_def() {
    check(
        parse,
        "include def;",
        &expect![[r#"
        Error(
            Rule(
                "string literal",
                Keyword(
                    Def,
                ),
                Span {
                    lo: 8,
                    hi: 11,
                },
            ),
        )

        [
            Error(
                Token(
                    Semicolon,
                    Keyword(
                        Def,
                    ),
                    Span {
                        lo: 8,
                        hi: 11,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn unclosed_string() {
    check(
        parse,
        r#"include "hello;"#,
        &expect![[r#"
        Error(
            Rule(
                "string literal",
                Eof,
                Span {
                    lo: 15,
                    hi: 15,
                },
            ),
        )

        [
            Error(
                Lex(
                    UnterminatedString(
                        Span {
                            lo: 8,
                            hi: 8,
                        },
                    ),
                ),
            ),
            Error(
                Token(
                    Semicolon,
                    Eof,
                    Span {
                        lo: 15,
                        hi: 15,
                    },
                ),
            ),
        ]"#]],
    );
}
