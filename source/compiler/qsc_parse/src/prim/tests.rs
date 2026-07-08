// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{ident, opt, pat, seq};
use crate::{
    Error, ErrorKind,
    completion::WordKinds,
    expr::expr,
    keyword::Keyword,
    lex::{ClosedBinOp, TokenKind},
    scan::ParserContext,
    tests::{check, check_opt, check_seq},
};
use expect_test::expect;
use qsc_ast::ast::PathKind;
use qsc_data_structures::{language_features::LanguageFeatures, span::Span};

fn path(s: &mut ParserContext) -> Result<PathKind, Error> {
    super::recovering_path(s, WordKinds::empty())
}

#[test]
fn ident_basic() {
    check(ident, "foo", &expect![[r#"Ident _id_ [0-3] "foo""#]]);
}

#[test]
fn ident_num_suffix() {
    check(ident, "foo2", &expect![[r#"Ident _id_ [0-4] "foo2""#]]);
}

#[test]
fn ident_underscore_prefix() {
    check(ident, "_foo", &expect![[r#"Ident _id_ [0-4] "_foo""#]]);
}

#[test]
fn ident_num_prefix() {
    check(
        ident,
        "2foo",
        &expect![[r#"
            Error(
                Rule(
                    "identifier",
                    Int(
                        Decimal,
                    ),
                    Span {
                        lo: 0,
                        hi: 1,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn ident_keyword() {
    for keyword in enum_iterator::all::<Keyword>() {
        let mut scanner = ParserContext::new(keyword.as_str(), LanguageFeatures::default());
        let actual = ident(&mut scanner);
        let span = Span {
            lo: 0,
            hi: keyword
                .as_str()
                .len()
                .try_into()
                .expect("keyword length should fit into u32"),
        };

        let expected = Error::new(match keyword {
            Keyword::And => {
                ErrorKind::Rule("identifier", TokenKind::ClosedBinOp(ClosedBinOp::And), span)
            }
            Keyword::Or => {
                ErrorKind::Rule("identifier", TokenKind::ClosedBinOp(ClosedBinOp::Or), span)
            }
            _ => ErrorKind::Rule("identifier", TokenKind::Keyword(keyword), span),
        });

        assert_eq!(actual, Err(expected), "{keyword}");
    }
}

#[test]
fn path_single() {
    check(
        path,
        "Foo",
        &expect![[r#"Path _id_ [0-3] (Ident _id_ [0-3] "Foo")"#]],
    );
}

#[test]
fn path_double() {
    check(
        path,
        "Foo.Bar",
        &expect![[r#"
            Path _id_ [0-7]:
                Ident _id_ [0-3] "Foo"
                Ident _id_ [4-7] "Bar""#]],
    );
}

#[test]
fn path_triple() {
    check(
        path,
        "Foo.Bar.Baz",
        &expect![[r#"
            Path _id_ [0-11]:
                Ident _id_ [0-3] "Foo"
                Ident _id_ [4-7] "Bar"
                Ident _id_ [8-11] "Baz""#]],
    );
}

#[test]
fn path_trailing_dot() {
    check(
        path,
        "Foo.Bar.",
        &expect![[r#"
            Err IncompletePath [0-8]:
                Ident _id_ [0-3] "Foo"
                Ident _id_ [4-7] "Bar"

            [
                Error(
                    Rule(
                        "identifier",
                        Eof,
                        Span {
                            lo: 8,
                            hi: 8,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn path_followed_by_keyword() {
    check(
        path,
        "Foo.Bar.in",
        &expect![[r#"
            Err IncompletePath [0-10]:
                Ident _id_ [0-3] "Foo"
                Ident _id_ [4-7] "Bar"

            [
                Error(
                    Rule(
                        "identifier",
                        Keyword(
                            In,
                        ),
                        Span {
                            lo: 8,
                            hi: 10,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn pat_bind() {
    check(
        pat,
        "foo",
        &expect![[r#"
            Pat _id_ [0-3]: Bind:
                Ident _id_ [0-3] "foo""#]],
    );
}

#[test]
fn pat_bind_ty() {
    check(
        pat,
        "foo : Int",
        &expect![[r#"
            Pat _id_ [0-9]: Bind:
                Ident _id_ [0-3] "foo"
                Type _id_ [6-9]: Path: Path _id_ [6-9] (Ident _id_ [6-9] "Int")"#]],
    );
}

#[test]
fn pat_bind_discard() {
    check(pat, "_", &expect!["Pat _id_ [0-1]: Discard"]);
}

#[test]
fn pat_discard_ty() {
    check(
        pat,
        "_ : Int",
        &expect![[r#"
            Pat _id_ [0-7]: Discard:
                Type _id_ [4-7]: Path: Path _id_ [4-7] (Ident _id_ [4-7] "Int")"#]],
    );
}

#[test]
fn pat_paren() {
    check(
        pat,
        "(foo)",
        &expect![[r#"
            Pat _id_ [0-5]: Paren:
                Pat _id_ [1-4]: Bind:
                    Ident _id_ [1-4] "foo""#]],
    );
}

#[test]
fn pat_singleton_tuple() {
    check(
        pat,
        "(foo,)",
        &expect![[r#"
            Pat _id_ [0-6]: Tuple:
                Pat _id_ [1-4]: Bind:
                    Ident _id_ [1-4] "foo""#]],
    );
}

#[test]
fn pat_tuple() {
    check(
        pat,
        "(foo, bar)",
        &expect![[r#"
            Pat _id_ [0-10]: Tuple:
                Pat _id_ [1-4]: Bind:
                    Ident _id_ [1-4] "foo"
                Pat _id_ [6-9]: Bind:
                    Ident _id_ [6-9] "bar""#]],
    );
}

#[test]
fn pat_tuple_ty_discard() {
    check(
        pat,
        "(foo : Int, _)",
        &expect![[r#"
            Pat _id_ [0-14]: Tuple:
                Pat _id_ [1-10]: Bind:
                    Ident _id_ [1-4] "foo"
                    Type _id_ [7-10]: Path: Path _id_ [7-10] (Ident _id_ [7-10] "Int")
                Pat _id_ [12-13]: Discard"#]],
    );
}

#[test]
fn pat_invalid() {
    check(
        pat,
        "@",
        &expect![[r#"
            Error(
                Rule(
                    "pattern",
                    At,
                    Span {
                        lo: 0,
                        hi: 1,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn pat_missing_ty() {
    check(
        pat,
        "foo :",
        &expect![[r#"
            Pat _id_ [0-5]: Bind:
                Ident _id_ [0-3] "foo"
                Type _id_ [5-5]: Err

            [
                Error(
                    Rule(
                        "type",
                        Eof,
                        Span {
                            lo: 5,
                            hi: 5,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn opt_succeed() {
    check_opt(
        |s| opt(s, path),
        "Foo.Bar",
        &expect![[r#"
            Path _id_ [0-7]:
                Ident _id_ [0-3] "Foo"
                Ident _id_ [4-7] "Bar""#]],
    );
}

#[test]
fn opt_fail_no_consume() {
    check_opt(|s| opt(s, path), "123", &expect!["None"]);
}

#[test]
fn opt_fail_consume() {
    check_opt(
        |s| opt(s, path),
        "Foo.#",
        &expect![[r#"
            Err IncompletePath [0-5]:
                Ident _id_ [0-3] "Foo"

            [
                Error(
                    Lex(
                        Unknown(
                            '#',
                            Span {
                                lo: 4,
                                hi: 5,
                            },
                        ),
                    ),
                ),
                Error(
                    Rule(
                        "identifier",
                        Eof,
                        Span {
                            lo: 5,
                            hi: 5,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn seq_empty() {
    check_seq(|s| seq(s, ident), "", &expect!["(, Missing)"]);
}

#[test]
fn seq_single() {
    check_seq(
        |s| seq(s, ident),
        "foo",
        &expect![[r#"(Ident _id_ [0-3] "foo", Missing)"#]],
    );
}

#[test]
fn seq_double() {
    check_seq(
        |s| seq(s, ident),
        "foo, bar",
        &expect![[r#"
            (Ident _id_ [0-3] "foo",
            Ident _id_ [5-8] "bar", Missing)"#]],
    );
}

#[test]
fn seq_trailing() {
    check_seq(
        |s| seq(s, ident),
        "foo, bar,",
        &expect![[r#"
            (Ident _id_ [0-3] "foo",
            Ident _id_ [5-8] "bar", Present)"#]],
    );
}

#[test]
fn seq_fail_no_consume() {
    check_seq(
        |s| seq(s, ident),
        "foo, 2",
        &expect![[r#"(Ident _id_ [0-3] "foo", Present)"#]],
    );
}

#[test]
fn seq_fail_consume() {
    check_seq(
        |s| seq(s, expr),
        "foo, bar(",
        &expect![[r#"
            (Expr _id_ [0-3]: Path: Path _id_ [0-3] (Ident _id_ [0-3] "foo"),
            Expr _id_ [5-9]: Call:
                Expr _id_ [5-8]: Path: Path _id_ [5-8] (Ident _id_ [5-8] "bar")
                Expr _id_ [8-9]: Unit, Missing)

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        Eof,
                        Span {
                            lo: 9,
                            hi: 9,
                        },
                    ),
                ),
            ]"#]],
    );
}
