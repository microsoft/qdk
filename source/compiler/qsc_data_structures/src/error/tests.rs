// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::WithSource;
use crate::source::SourceMap;
use crate::span::Span;
use expect_test::expect;
use miette::Diagnostic;
use std::{error::Error, fmt::Write, iter, str::from_utf8};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
enum TestError {
    #[error("Error: {0}")]
    #[diagnostic(code("Qsc.Test.Error.NoSpans"))]
    NoSpans(String),
    #[error("Error: {0}")]
    #[diagnostic(code("Qsc.Test.Error.TwoSpans"))]
    TwoSpans(
        String,
        #[label("first label")] Span,
        #[label("second label")] Span,
    ),
}

#[test]
fn no_files() {
    let sources = SourceMap::default();
    let error = TestError::NoSpans("value".into());
    let formatted_error = format_error(&WithSource::from_map(&sources, error));

    expect![[r#"
        Error: value
    "#]]
    .assert_eq(&formatted_error);
}

#[test]
fn error_spans_two_files() {
    let test1_contents = "namespace Foo {}";
    let test2_contents = "namespace Bar {}";
    let mut sources = SourceMap::default();
    let test1_offset = sources.push("test1.qs".into(), test1_contents.into());
    let test2_offset = sources.push("test2.qs".into(), test2_contents.into());

    let error = TestError::TwoSpans(
        "value".into(),
        span_with_offset(test1_offset, 10, 13),
        span_with_offset(test2_offset, 10, 13),
    );

    let formatted_error = format_error(&WithSource::from_map(&sources, error));

    expect![[r#"
        Error: value
          [first label] [test1.qs] [Foo]
          [second label] [test2.qs] [Bar]
    "#]]
    .assert_eq(&formatted_error);
}

#[test]
fn error_spans_begin() {
    let test1_contents = "namespace Foo {}";
    let test2_contents = "namespace Bar {}";
    let mut sources = SourceMap::default();
    let test1_offset = sources.push("test1.qs".into(), test1_contents.into());
    let test2_offset = sources.push("test2.qs".into(), test2_contents.into());

    let error = TestError::TwoSpans(
        "value".into(),
        span_with_offset(test1_offset, 0, 13),
        span_with_offset(test2_offset, 0, 13),
    );

    let formatted_error = format_error(&WithSource::from_map(&sources, error));

    expect![[r#"
        Error: value
          [first label] [test1.qs] [namespace Foo]
          [second label] [test2.qs] [namespace Bar]
    "#]]
    .assert_eq(&formatted_error);
}

#[allow(clippy::cast_possible_truncation)]
#[test]
fn error_spans_eof() {
    let test1_contents = "namespace Foo {}";
    let test2_contents = "namespace Bar {}";
    let mut sources = SourceMap::default();
    let test1_offset = sources.push("test1.qs".into(), test1_contents.into());
    let test2_offset = sources.push("test2.qs".into(), test2_contents.into());

    let error = TestError::TwoSpans(
        "value".into(),
        span_with_offset(
            test1_offset,
            test1_contents.len() as u32,
            test1_contents.len() as u32,
        ),
        span_with_offset(
            test2_offset,
            test2_contents.len() as u32,
            test2_contents.len() as u32,
        ),
    );

    let formatted_error = format_error(&WithSource::from_map(&sources, error));

    expect![[r#"
        Error: value
          [first label] [test1.qs] []
          [second label] [test2.qs] []
    "#]]
    .assert_eq(&formatted_error);
}

#[test]
fn resolve_spans() {
    let test1_contents = "namespace Foo {}";
    let test2_contents = "namespace Bar {}";
    let mut sources = SourceMap::default();
    let test1_offset = sources.push("test1.qs".into(), test1_contents.into());
    let test2_offset = sources.push("test2.qs".into(), test2_contents.into());

    let error = TestError::TwoSpans(
        "value".into(),
        span_with_offset(test1_offset, 10, 13),
        span_with_offset(test2_offset, 10, 13),
    );

    let with_source = WithSource::from_map(&sources, error);

    let resolved_spans = with_source
        .labels()
        .expect("expected labels to exist")
        .map(|l| {
            let resolved = with_source.resolve_span(l.inner());
            (
                resolved.0.name.to_string(),
                resolved.1.offset(),
                resolved.1.len(),
            )
        })
        .collect::<Vec<_>>();

    expect![[r#"
        [
            (
                "test1.qs",
                10,
                3,
            ),
            (
                "test2.qs",
                10,
                3,
            ),
        ]
    "#]]
    .assert_debug_eq(&resolved_spans);
}

fn span_with_offset(offset: u32, lo: u32, hi: u32) -> Span {
    Span {
        lo: lo + offset,
        hi: hi + offset,
    }
}

fn format_error(error: &WithSource<TestError>) -> String {
    let mut s = String::new();
    write!(s, "{error}").expect("writing should succeed");
    for e in iter::successors(error.source(), |&e| e.source()) {
        write!(s, ": {e}").expect("writing should succeed");
    }
    for label in error.labels().into_iter().flatten() {
        let span = error
            .source_code()
            .expect("expected valid source code")
            .read_span(label.inner(), 0, 0)
            .expect("expected to be able to read span");

        write!(
            s,
            "\n  [{}] [{}] [{}]",
            label.label().unwrap_or(""),
            span.name().expect("expected source file name"),
            from_utf8(span.data()).expect("expected valid utf-8 string"),
        )
        .expect("writing should succeed");
    }
    writeln!(s).expect("writing should succeed");
    s
}
