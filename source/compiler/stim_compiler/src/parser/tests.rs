// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use miette::Report;

mod arguments;

/// Check that a stim source parses to the expected AST,
/// diagnostics, or both.
fn check(source: &str, expect: &Expect) {
    let (ast, errors) = crate::parser::parse(source);
    let mut entries = Vec::new();
    if errors.is_empty() || !ast.items.is_empty() {
        entries.push(ast.to_string());
    }
    for error in errors {
        entries.push(format!(
            "{:?}",
            Report::new(error).with_source_code(source.to_string())
        ));
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
