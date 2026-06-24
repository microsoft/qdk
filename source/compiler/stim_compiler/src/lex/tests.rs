// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

/// Check that a stim source parses to the
/// expected AST or yields the expected errors.
fn check(source: &str, expect: &Expect) {
    let lexer = crate::lex::Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    let buffer = tokens
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    expect.assert_eq(&buffer);
}

#[test]
fn empty_src() {
    check("", &expect![[r#""#]]);
}
