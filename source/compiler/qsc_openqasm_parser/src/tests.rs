// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::Expect;
use miette::Report;

use crate::io::InMemorySourceResolver;
use crate::semantic::QasmSemanticParseResult;
use crate::semantic::parse_source;
use std::sync::Arc;

pub(crate) fn parse<S: Into<Arc<str>>>(
    source: S,
) -> miette::Result<QasmSemanticParseResult, Vec<Report>> {
    let source = source.into();
    let name: Arc<str> = "Test.qasm".into();
    let sources = [(name.clone(), source.clone())];
    let mut resolver = InMemorySourceResolver::from_iter(sources);
    let res = parse_source(source, name, &mut resolver);
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

pub fn check_qasm<S: Into<Arc<str>>>(source: S, expect: &Expect) {
    match parse(source) {
        Ok(res) => {
            // syntaxt succeeded, check for semantic errors
            if res.has_errors() {
                let buffer = res
                    .errors
                    .into_iter()
                    .map(Report::new)
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                expect.assert_eq(&buffer);
            } else {
                panic!("Expected errors but parsing succeeded with program: {res:?}");
            }
        }
        Err(errors) => {
            let buffer = errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            expect.assert_eq(&buffer);
        }
    }
}
