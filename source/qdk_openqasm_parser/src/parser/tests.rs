// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::io::InMemorySourceResolver;
use crate::io::SourceResolver;

use super::QasmParseResult;
use super::parse_source;
use miette::Report;

use super::prim::FinalSep;
use super::{Parser, scan::ParserContext};
use expect_test::Expect;
use std::fmt::Display;

pub(crate) fn parse_all<S: Into<Arc<str>>>(
    path: S,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
) -> miette::Result<QasmParseResult, Vec<Report>> {
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

pub(crate) fn parse<S: Into<Arc<str>>>(source: S) -> miette::Result<QasmParseResult, Vec<Report>> {
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
        Some(format!("{}", res.source.program.version.expect("version"))),
        Some("3".to_string())
    );
    Ok(())
}

#[test]
fn dotted_version_can_be_parsed() -> miette::Result<(), Vec<Report>> {
    let source = r#"OPENQASM 3.0;"#;
    let res = parse(source)?;
    assert_eq!(
        Some(format!("{}", res.source.program.version.expect("version"))),
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
