// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use miette::Report;

mod arguments;
mod blocks;
mod edge_cases;
mod errors;
mod instruction_shapes;
mod numbers;
mod spans;
mod tags;
mod targets;
mod whitespace_and_comments;

/// Check that a stim source parses to the expected AST,
/// diagnostics, or both.
fn check(source: &str, expect: &Expect) {
    let (ast, errors) = crate::parser::parse(source);
    let mut entries = Vec::new();
    let no_errors = errors.is_empty();
    for error in errors {
        entries.push(format!(
            "{:?}",
            Report::new(error).with_source_code(source.to_string())
        ));
    }
    if no_errors || !ast.items.is_empty() {
        entries.push(ast.to_string());
    }
    expect.assert_eq(&entries.join("\n"));
}

#[test]
fn empty_src() {
    check(
        "",
        &expect![[r#"
        Circuit [0-0]:
            items: <empty>"#]],
    );
}
