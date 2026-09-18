// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod broadcasting;

use expect_test::Expect;
use miette::Report;

#[allow(dead_code)]
fn check(source: &str, expect: &Expect) {
    let (parser_ast, parser_errors) = crate::parser::parse(source);
    assert!(
        parser_errors.is_empty(),
        "semantic tests require syntactically valid input"
    );

    let (semantic_ast, errors) = crate::semantic::lower(parser_ast);
    let actual = if errors.is_empty() {
        semantic_ast.to_string()
    } else {
        errors
            .into_iter()
            .map(|error| {
                format!(
                    "{:?}",
                    Report::new(error).with_source_code(source.to_string())
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    expect.assert_eq(&actual);
}
