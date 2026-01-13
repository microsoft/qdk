// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{ident, opt, seq, token};
use crate::{
    keyword::Keyword,
    lex::TokenKind,
    parser::ast::{IncompletePath, Path, PathKind},
    parser::{
        completion::word_kinds::WordKinds,
        error::{Error, ErrorKind},
        scan::ParserContext,
        tests::{check, check_opt, check_seq},
    },
};
use expect_test::expect;

use qsc_data_structures::span::Span;

/// A `path` is a dot-separated list of idents like "Foo.Bar.Baz"
/// this can be a namespace name (in an open statement or namespace declaration),
/// a reference to an item, like `Microsoft.Quantum.Diagnostics.DumpMachine`,
/// or a field access.
///
/// Path parser. If parsing fails, also returns any valid segments
/// that were parsed up to the final `.` token.
pub(super) fn path(
    s: &mut ParserContext,
) -> std::result::Result<Box<Path>, (Error, Option<Box<IncompletePath>>)> {
    let lo = s.peek().span.lo;
    let i = ident(s).map_err(|e| (e, None))?;

    let mut parts = vec![i];
    while token(s, TokenKind::Dot).is_ok() {
        s.expect(WordKinds::PathSegment);
        match ident(s) {
            Ok(ident) => parts.push(ident),
            Err(error) => {
                let trivia_span = s.skip_trivia();
                let keyword = trivia_span.hi == trivia_span.lo
                    && matches!(s.peek().kind, TokenKind::Keyword(_));
                if keyword {
                    // Consume any keyword that comes immediately after the final
                    // dot, assuming it was intended to be part of the path.
                    s.advance();
                }

                return Err((
                    error,
                    Some(Box::new(IncompletePath {
                        span: s.span(lo),
                        segments: parts.into(),
                        keyword,
                    })),
                ));
            }
        }
    }

    let name = parts.pop().expect("path should have at least one part");
    let namespace = if parts.is_empty() {
        None
    } else {
        Some(parts.into())
    };

    Ok(Box::new(Path {
        span: s.span(lo),
        segments: namespace,
        name: name.into(),
    }))
}

/// Recovering [`Path`] parser. Parsing only fails if no segments
/// were successfully parsed. If any segments were successfully parsed,
/// returns a [`PathKind::Err`] containing the segments that were
/// successfully parsed up to the final `.` token.
fn recovering_path(s: &mut ParserContext) -> Result<PathKind, Error> {
    match path(s) {
        Ok(path) => Ok(PathKind::Ok(path)),
        Err((error, Some(incomplete_path))) => {
            s.push_error(error);
            Ok(PathKind::Err(Some(incomplete_path)))
        }
        Err((error, None)) => Err(error),
    }
}

#[test]
fn ident_basic() {
    check(ident, "foo", &expect![[r#"Ident [0-3] "foo""#]]);
}

#[test]
fn ident_num_suffix() {
    check(ident, "foo2", &expect![[r#"Ident [0-4] "foo2""#]]);
}

#[test]
fn ident_underscore_prefix() {
    check(ident, "_foo", &expect![[r#"Ident [0-4] "_foo""#]]);
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
                    Literal(
                        Integer(
                            Decimal,
                        ),
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
#[ignore = "Need to talk through how to handle this"]
fn ident_keyword() {
    for keyword in enum_iterator::all::<Keyword>() {
        let mut scanner = ParserContext::new(keyword.as_str());
        let actual = ident(&mut scanner);
        let span = Span {
            lo: 0,
            hi: keyword
                .as_str()
                .len()
                .try_into()
                .expect("keyword length should fit into u32"),
        };

        let expected = Error::new(ErrorKind::Rule(
            "identifier",
            TokenKind::Keyword(keyword),
            span,
        ));

        assert_eq!(actual, Err(expected), "{keyword}");
    }
}

#[test]
fn path_single() {
    check(
        recovering_path,
        "Foo",
        &expect![[r#"
        Path [0-3]:
            name: Ident [0-3] "Foo"
            segments: <none>"#]],
    );
}

#[test]
fn path_double() {
    check(
        recovering_path,
        "Foo.Bar",
        &expect![[r#"
            Path [0-7]:
                name: Ident [4-7] "Bar"
                segments:
                    Ident [0-3] "Foo""#]],
    );
}

#[test]
fn path_triple() {
    check(
        recovering_path,
        "Foo.Bar.Baz",
        &expect![[r#"
            Path [0-11]:
                name: Ident [8-11] "Baz"
                segments:
                    Ident [0-3] "Foo"
                    Ident [4-7] "Bar""#]],
    );
}

#[test]
fn path_trailing_dot() {
    check(
        recovering_path,
        "Foo.Bar.",
        &expect![[r#"
            Err IncompletePath [0-8]:    segments:
                    Ident [0-3] "Foo"
                    Ident [4-7] "Bar"

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
        recovering_path,
        "Foo.Bar.in",
        &expect![[r#"
            Err IncompletePath [0-10]:    segments:
                    Ident [0-3] "Foo"
                    Ident [4-7] "Bar"

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
fn opt_succeed() {
    check_opt(
        |s| opt(s, recovering_path),
        "Foo.Bar",
        &expect![[r#"
            Path [0-7]:
                name: Ident [4-7] "Bar"
                segments:
                    Ident [0-3] "Foo""#]],
    );
}

#[test]
fn opt_fail_no_consume() {
    check_opt(|s| opt(s, recovering_path), "123", &expect!["None"]);
}

#[test]
fn opt_fail_consume() {
    check_opt(
        |s| opt(s, recovering_path),
        "Foo.$",
        &expect![[r#"
            Err IncompletePath [0-5]:    segments:
                    Ident [0-3] "Foo"

            [
                Error(
                    Lex(
                        Unknown(
                            '$',
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
        &expect![[r#"(Ident [0-3] "foo", Missing)"#]],
    );
}

#[test]
fn seq_double() {
    check_seq(
        |s| seq(s, ident),
        "foo, bar",
        &expect![[r#"
            (Ident [0-3] "foo",
            Ident [5-8] "bar", Missing)"#]],
    );
}

#[test]
fn seq_trailing() {
    check_seq(
        |s| seq(s, ident),
        "foo, bar,",
        &expect![[r#"
            (Ident [0-3] "foo",
            Ident [5-8] "bar", Present)"#]],
    );
}

#[test]
fn seq_fail_no_consume() {
    check_seq(
        |s| seq(s, ident),
        "foo, 2",
        &expect![[r#"(Ident [0-3] "foo", Present)"#]],
    );
}
