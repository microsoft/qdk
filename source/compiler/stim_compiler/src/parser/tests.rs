// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

/// Check that a stim source parses to the
/// expected AST or yields the expected errors.
fn check(source: &str, _expect: &Expect) {
    let _ = crate::parser::parse(source);
    todo!("missing error handling for parser")
}

#[test]
fn empty_src() {
    check("", &expect![[r#""#]]);
}
