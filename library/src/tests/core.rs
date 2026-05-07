// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::test_expression;
use qsc::interpret::{Value, ValueRange};

// Tests for the core namespace

#[test]
fn check_range_1_5() {
    test_expression(
        "{let r = 1..5; [RangeStart(r), RangeStep(r), RangeEnd(r)]}",
        &Value::Array(vec![Value::Int(1), Value::Int(1), Value::Int(5)].into()),
    );
}

#[test]
fn check_range_2_3_4() {
    test_expression(
        "{let r = 2..3..4; [RangeStart(r), RangeStep(r), RangeEnd(r)]}",
        &Value::Array(vec![Value::Int(2), Value::Int(3), Value::Int(4)].into()),
    );
}

#[test]
fn check_range_2_3_n4() {
    test_expression(
        "{let r = 2..3..-4; [RangeStart(r), RangeStep(r), RangeEnd(r)]}",
        &Value::Array(vec![Value::Int(2), Value::Int(3), Value::Int(-4)].into()),
    );
}

#[test]
fn check_range_n2_3_n4() {
    test_expression(
        "{let r = -2..3..-4; [RangeStart(r), RangeStep(r), RangeEnd(r)]}",
        &Value::Array(vec![Value::Int(-2), Value::Int(3), Value::Int(-4)].into()),
    );
}

#[test]
fn check_range_n2_n3_n4() {
    test_expression(
        "{let r = -2..-3..-4; [RangeStart(r), RangeStep(r), RangeEnd(r)]}",
        &Value::Array(vec![Value::Int(-2), Value::Int(-3), Value::Int(-4)].into()),
    );
}

#[test]
fn check_range_empty_1_5() {
    test_expression("IsRangeEmpty(1..5)", &Value::Bool(false));
}

#[test]
fn check_range_empty_1_10_5() {
    test_expression("IsRangeEmpty(1..10..5)", &Value::Bool(false));
}

#[test]
fn check_range_empty_3_2() {
    test_expression("IsRangeEmpty(3..2)", &Value::Bool(true));
}

#[test]
fn check_range_empty_2_n1_3() {
    test_expression("IsRangeEmpty(2..-1..3)", &Value::Bool(true));
}

#[test]
fn check_range_empty_n2_n1_n3() {
    test_expression("IsRangeEmpty(-2..-1..-3)", &Value::Bool(false));
}

#[test]
fn check_range_reverse_1_5() {
    test_expression(
        "RangeReverse(1..5)",
        &Value::Range(
            ValueRange {
                start: Some(5),
                step: -1,
                end: Some(1),
            }
            .into(),
        ),
    );
}

#[test]
fn check_range_reverse_1_n1_5() {
    test_expression(
        "RangeReverse(1..-1..5)",
        &Value::Range(
            ValueRange {
                start: Some(5),
                step: 1,
                end: Some(1),
            }
            .into(),
        ),
    );
}

#[test]
fn check_range_reverse_1_7_10() {
    test_expression(
        "RangeReverse(1..7..10)",
        &Value::Range(
            ValueRange {
                start: Some(8),
                step: -7,
                end: Some(1),
            }
            .into(),
        ),
    );
}
