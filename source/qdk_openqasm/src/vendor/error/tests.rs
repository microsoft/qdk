// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Vendored from `qsc_data_structures::error::tests`.

use super::{SourceSnapshotSourceCode, WithSource};
use crate::io::InMemorySourceResolver;
use crate::vendor::source::SourceMap;
use crate::vendor::span::Span;
use expect_test::expect;
use miette::{Diagnostic, MietteError, SourceCode, SourceSpan};
use std::{error::Error, fmt::Write, iter, str::from_utf8};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
enum TestError {
    #[error("Error: {0}")]
    #[diagnostic(code("Qdk.Qsc.Test.Error.NoSpans"))]
    NoSpans(String),
    #[error("Error: {0}")]
    #[diagnostic(code("Qdk.Qsc.Test.Error.TwoSpans"))]
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
            let resolved = with_source
                .resolve_span(l.inner())
                .expect("expected labeled span to resolve");
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

#[test]
fn source_snapshot_adapter_resolves_each_source_and_rejects_invalid_spans() {
    let source = "OPENQASM 3.0; include \"child.inc\";";
    let mut resolver =
        InMemorySourceResolver::from_iter([("child.inc".into(), "int child = 1;".into())]);
    let result = crate::parser::parse_source(source, "main.qasm", &mut resolver);
    let adapter = SourceSnapshotSourceCode::new(&result.source_snapshot);
    let entry = &result.source_snapshot.files()[0];
    let child = &result.source_snapshot.files()[1];

    let entry_contents = adapter
        .read_span(
            &SourceSpan::new(
                usize::try_from(entry.offset)
                    .expect("u32 source offset should fit into usize")
                    .into(),
                8,
            ),
            0,
            0,
        )
        .expect("entry span should resolve");
    let child_contents = adapter
        .read_span(
            &SourceSpan::new(
                usize::try_from(child.offset)
                    .expect("u32 source offset should fit into usize")
                    .into(),
                3,
            ),
            0,
            0,
        )
        .expect("included span should resolve");
    let invalid = adapter.read_span(&SourceSpan::new(usize::MAX.into(), 1), 0, 0);

    assert_eq!(entry_contents.name(), Some("main.qasm"));
    assert_eq!(child_contents.name(), Some("child.inc"));
    assert!(matches!(invalid, Err(MietteError::OutOfBounds)));
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
