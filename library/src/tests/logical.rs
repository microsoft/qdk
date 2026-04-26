// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::test_expression;
use qsc::interpret::Value;

// Tests for Std.Logical namespace

#[test]
fn check_xor() {
    test_expression(
        "Std.Logical.Xor(false, false)",
        &Value::Bool(false),
    );
    test_expression(
        "Std.Logical.Xor(false, true)",
        &Value::Bool(true),
    );
    test_expression(
        "Std.Logical.Xor(true, false)",
        &Value::Bool(true),
    );
    test_expression(
        "Std.Logical.Xor(true, true)",
        &Value::Bool(false),
    );
}
