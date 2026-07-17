// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::io::InMemorySourceResolver;
use crate::io::SourceResolver;
use crate::io::{self, SourceResolverContext};

use super::parse_source;
use super::{ParseResult, SourceStatus};
use miette::Report;

use super::prim::FinalSep;
use super::{Parser, scan::ParserContext};
use expect_test::Expect;
use rustc_hash::FxHashMap;
use std::fmt::Display;

pub(crate) fn parse_all<S: Into<Arc<str>>>(
    path: S,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
) -> miette::Result<ParseResult, Vec<Report>> {
    let path = path.into();
    let mut resolver = InMemorySourceResolver::from_iter(sources);
    let (path, source) = resolver
        .resolve(&path, &path)
        .map_err(|e| vec![Report::new(e)])?;
    let res = crate::parser::parse_source(source, path, &mut resolver);
    if res.source.has_errors() {
        let errors = res
            .errors()
            .into_iter()
            .map(|e| Report::new(e.clone()))
            .collect();
        Err(errors)
    } else {
        Ok(res)
    }
}

pub(crate) fn parse<S: Into<Arc<str>>>(source: S) -> miette::Result<ParseResult, Vec<Report>> {
    let source = source.into();
    let mut resolver = InMemorySourceResolver::from_iter([("test".into(), source.clone())]);
    let res = parse_source(source, "test", &mut resolver);
    if res.source.has_errors() {
        let errors = res
            .errors()
            .into_iter()
            .map(|e| Report::new(e.clone()))
            .collect();
        return Err(errors);
    }
    Ok(res)
}

pub(super) fn check<T: Display>(parser: impl Parser<T>, input: &str, expect: &Expect) {
    check_map(parser, input, expect, ToString::to_string);
}

pub(super) fn check_opt<T: Display>(parser: impl Parser<Option<T>>, input: &str, expect: &Expect) {
    check_map(parser, input, expect, |value| match value {
        Some(value) => value.to_string(),
        None => "None".to_string(),
    });
}

pub(super) fn check_seq<T: Display>(
    parser: impl Parser<(Vec<T>, FinalSep)>,
    input: &str,
    expect: &Expect,
) {
    check_map(parser, input, expect, |(values, sep)| {
        format!(
            "({}, {sep:?})",
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",\n")
        )
    });
}

fn check_map<T>(
    mut parser: impl Parser<T>,
    input: &str,
    expect: &Expect,
    f: impl FnOnce(&T) -> String,
) {
    let mut scanner = ParserContext::new(input);
    let result = parser(&mut scanner);
    let errors = scanner.into_errors();
    match result {
        Ok(value) if errors.is_empty() => expect.assert_eq(&f(&value)),
        Ok(value) => expect.assert_eq(&format!("{}\n\n{errors:#?}", f(&value))),
        Err(error) if errors.is_empty() => expect.assert_debug_eq(&error),
        Err(error) => expect.assert_eq(&format!("{error:#?}\n\n{errors:#?}")),
    }
}

#[test]
fn int_version_can_be_parsed() -> miette::Result<(), Vec<Report>> {
    let source = r#"OPENQASM 3;"#;
    let res = parse(source)?;
    assert_eq!(
        Some(format!(
            "{}",
            res.source
                .program()
                .expect("program")
                .version
                .expect("version")
        )),
        Some("3".to_string())
    );
    Ok(())
}

#[test]
fn dotted_version_can_be_parsed() -> miette::Result<(), Vec<Report>> {
    let source = r#"OPENQASM 3.0;"#;
    let res = parse(source)?;
    assert_eq!(
        Some(format!(
            "{}",
            res.source
                .program()
                .expect("program")
                .version
                .expect("version")
        )),
        Some("3.0".to_string())
    );
    Ok(())
}

#[test]
fn programs_with_includes_can_be_parsed() -> miette::Result<(), Vec<Report>> {
    let source0 = r#"OPENQASM 3.0;
    include "stdgates.inc";
    include "source1.qasm";"#;
    let source1 = "";
    let all_sources = [
        ("source0.qasm".into(), source0.into()),
        ("source1.qasm".into(), source1.into()),
    ];

    let res = parse_all("source0.qasm", all_sources)?;
    assert!(res.source.includes().len() == 1);
    Ok(())
}

#[test]
fn programs_with_includes_with_includes_can_be_parsed() -> miette::Result<(), Vec<Report>> {
    let source0 = r#"OPENQASM 3.0;
    include "stdgates.inc";
    include "source1.qasm";
    "#;
    let source1 = r#"include "source2.qasm";
    "#;
    let source2 = "";
    let all_sources = [
        ("source0.qasm".into(), source0.into()),
        ("source1.qasm".into(), source1.into()),
        ("source2.qasm".into(), source2.into()),
    ];

    let res = parse_all("source0.qasm", all_sources)?;
    assert!(res.source.includes().len() == 1);
    assert!(res.source.includes()[0].includes().len() == 1);
    Ok(())
}

#[test]
fn source_snapshot_uses_preorder_ids_and_explicit_status() {
    let source = concat!(
        "OPENQASM 3.0; include \"empty.inc\"; ",
        "include \"missing.inc\"; include \"nested.inc\";"
    );
    let sources = [
        ("empty.inc".into(), "".into()),
        ("nested.inc".into(), "include \"leaf.inc\";".into()),
        ("leaf.inc".into(), "gate leaf q {}".into()),
    ];
    let mut resolver = InMemorySourceResolver::from_iter(sources);
    let result = parse_source(source, "main.qasm", &mut resolver);
    let files = result.source_snapshot.files();

    assert_eq!(files.len(), 5);
    assert_eq!(
        files.iter().map(|file| file.id).collect::<Vec<_>>(),
        (0..5).collect::<Vec<_>>()
    );
    assert_eq!(files[0].status, SourceStatus::Entry);
    assert_eq!(files[1].status, SourceStatus::Resolved);
    assert_eq!(files[1].text.as_ref(), "");
    assert_eq!(files[2].status, SourceStatus::Unresolved);
    assert_eq!(files[2].text.as_ref(), "");
    assert_eq!(files[3].path.as_ref(), "nested.inc");
    assert_eq!(files[4].path.as_ref(), "leaf.inc");
}

struct RenamingResolver {
    sources: FxHashMap<String, (Arc<str>, Arc<str>)>,
    context: SourceResolverContext,
}

impl SourceResolver for RenamingResolver {
    fn ctx(&mut self) -> &mut SourceResolverContext {
        &mut self.context
    }

    fn resolve(
        &mut self,
        path: &Arc<str>,
        original_path: &Arc<str>,
    ) -> miette::Result<(Arc<str>, Arc<str>), io::Error> {
        self.sources.get(path.as_ref()).cloned().ok_or_else(|| {
            io::Error(io::ErrorKind::NotFound(
                crate::span::Span::default(),
                format!("Could not resolve include file: {original_path}"),
            ))
        })
    }
}

fn renaming_resolver(
    sources: impl IntoIterator<Item = (&'static str, &'static str, &'static str)>,
) -> RenamingResolver {
    RenamingResolver {
        sources: sources
            .into_iter()
            .map(|(requested, resolved, text)| {
                (
                    requested.to_string(),
                    (Arc::from(resolved), Arc::from(text)),
                )
            })
            .collect(),
        context: SourceResolverContext::default(),
    }
}

#[test]
fn source_snapshot_records_relative_and_uri_aliases() {
    let source = concat!(
        "OPENQASM 3.0; include \"../shared.inc\"; ",
        "include \"uri.inc\";"
    );
    let mut resolver = renaming_resolver([
        ("pkg/shared.inc", "memory://shared.inc", ""),
        ("pkg/app/uri.inc", "https://example.test/uri.inc", ""),
    ]);
    let result = parse_source(source, "pkg/app/main.qasm", &mut resolver);
    let files = result.source_snapshot.files();

    assert_eq!(
        files[1].aliases.as_ref(),
        [Arc::<str>::from("pkg/shared.inc")]
    );
    assert_eq!(
        files[2].aliases.as_ref(),
        [Arc::<str>::from("pkg/app/uri.inc")]
    );
    assert_eq!(
        result
            .source_snapshot
            .resolve("pkg/shared.inc")
            .map(|file| file.id),
        Some(files[1].id)
    );
    assert_eq!(
        result
            .source_snapshot
            .resolve("memory://shared.inc")
            .map(|file| file.id),
        Some(files[1].id)
    );
    assert_eq!(
        result
            .source_snapshot
            .resolve("pkg/app/uri.inc")
            .map(|file| file.id),
        Some(files[2].id)
    );
    assert_eq!(
        result
            .source_snapshot
            .resolve("https://example.test/uri.inc")
            .map(|file| file.id),
        Some(files[2].id)
    );
}

#[test]
fn same_basename_in_different_directories_has_distinct_aliases() {
    let source = concat!(
        "OPENQASM 3.0; include \"a/shared.inc\"; ",
        "include \"b/shared.inc\";"
    );
    let mut resolver = renaming_resolver([
        ("root/a/shared.inc", "memory://a/shared.inc", ""),
        ("root/b/shared.inc", "memory://b/shared.inc", ""),
    ]);
    let result = parse_source(source, "root/main.qasm", &mut resolver);
    let files = result.source_snapshot.files();

    assert_eq!(
        files[1].aliases.as_ref(),
        [Arc::<str>::from("root/a/shared.inc")]
    );
    assert_eq!(
        files[2].aliases.as_ref(),
        [Arc::<str>::from("root/b/shared.inc")]
    );
    assert_eq!(
        result
            .source_snapshot
            .resolve("root/a/shared.inc")
            .map(|file| file.id),
        Some(files[1].id)
    );
    assert_eq!(
        result
            .source_snapshot
            .resolve("root/b/shared.inc")
            .map(|file| file.id),
        Some(files[2].id)
    );
}

#[test]
#[should_panic(expected = "source alias collision")]
fn source_snapshot_rejects_alias_collisions() {
    let source = concat!(
        "OPENQASM 3.0; include \"one.inc\"; ",
        "include \"two.inc\";"
    );
    let mut resolver = renaming_resolver([
        ("one.inc", "memory://same.inc", ""),
        ("two.inc", "memory://same.inc", ""),
    ]);

    let _ = parse_source(source, "main.qasm", &mut resolver);
}

#[test]
fn resolver_failure_does_not_change_later_include_base_path() {
    let source = concat!(
        "OPENQASM 3.0; include \"missing/first.inc\"; ",
        "include \"second.inc\";"
    );
    let mut resolver =
        InMemorySourceResolver::from_iter([("root/second.inc".into(), "gate second q {}".into())]);
    let result = parse_source(source, "root/main.qasm", &mut resolver);
    let files = result.source_snapshot.files();

    assert_eq!(files[1].status, SourceStatus::Unresolved);
    assert_eq!(files[2].path.as_ref(), "root/second.inc");
    assert_eq!(files[2].status, SourceStatus::Resolved);
}

#[test]
fn duplicate_include_publishes_unresolved_placeholder() {
    let source = concat!(
        "OPENQASM 3.0; include \"shared.inc\"; ",
        "include \"shared.inc\";"
    );
    let mut resolver = InMemorySourceResolver::from_iter([("shared.inc".into(), "".into())]);
    let result = parse_source(source, "main.qasm", &mut resolver);
    let files = result.source_snapshot.files();

    assert_eq!(files.len(), 3);
    assert_eq!(files[1].status, SourceStatus::Resolved);
    assert_eq!(files[2].status, SourceStatus::Unresolved);
    assert!(result.has_errors());
}
