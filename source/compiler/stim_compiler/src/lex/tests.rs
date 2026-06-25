// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use miette::Report;

mod instruction_name;
mod number;
mod whitespace_and_comments;

/// Check that a stim source lexes to the
/// expected tokens or yields the expected errors.
fn check(source: &str, expect: &Expect) {
    let lexer = crate::lex::Lexer::new(source);
    let buffer = lexer
        .map(|token| match token {
            Ok(token) => {
                let value = source
                    .get(token.span.lo as usize..token.span.hi as usize)
                    .unwrap_or("");
                format!("{}({}) {}", token.kind, value.escape_debug(), token.span)
            }
            Err(err) => {
                format!(
                    "{:?}",
                    Report::new(err).with_source_code(source.to_string())
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    expect.assert_eq(&buffer);
}

#[test]
fn empty_src() {
    check("", &expect![[]]);
}
