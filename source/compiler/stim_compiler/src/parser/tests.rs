// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

/// Check that a stim source parses to the
/// expected AST or yields the expected errors.
fn check(source: &str, expect: &Expect) {
    let (ast, errors) = crate::parser::parse(source);
    if errors.is_empty() {
        expect.assert_eq(&ast.to_string());
    } else {
        let buffer = errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        expect.assert_eq(&buffer);
    }
}

#[test]
fn empty_src() {
    check("", &expect![[r#"
        Circuit [0-0]:
            items: <empty>"#]]);
}
