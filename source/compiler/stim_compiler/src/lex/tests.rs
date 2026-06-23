// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

/// Check that a stim source parses to the
/// expected AST or yields the expected errors.
fn check(_source: &str, _expect: &Expect) {
    todo!("missing error handling for lexer")
}

#[test]
fn empty_src() {
    check("", &expect![[r#""#]]);
}
