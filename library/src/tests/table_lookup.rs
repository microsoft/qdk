// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::test_expression_with_lib;
use qsc::interpret::Value;

// Tests for Microsoft.Quantum.TableLookup namespace

const SELECT_TEST_LIB: &str = include_str!("resources/src/select.qs");

#[test]
fn check_select_exhaustive_bitwidth_0() {
    test_expression_with_lib(
        "Test.TestSelect(0, 10)",
        SELECT_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_select_exhaustive_bitwidth_1() {
    test_expression_with_lib(
        "Test.TestSelect(1, 10)",
        SELECT_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_select_exhaustive_bitwidth_2() {
    test_expression_with_lib(
        "Test.TestSelect(2, 10)",
        SELECT_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_select_exhaustive_bitwidth_3() {
    test_expression_with_lib(
        "Test.TestSelect(3, 10)",
        SELECT_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_select_exhaustive_bitwidth_4() {
    test_expression_with_lib(
        "Test.TestSelect(4, 10)",
        SELECT_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_select_fuzz() {
    test_expression_with_lib(
        "Test.TestSelectFuzz(10)",
        SELECT_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}

#[test]
fn check_select_phase() {
    test_expression_with_lib(
        "Test.TestSelectPhase()",
        SELECT_TEST_LIB,
        &Value::Tuple(vec![].into(), None),
    );
}
