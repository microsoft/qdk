// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{test_expression, test_expression_fails};
use qsc::interpret::Value;

#[test]
fn check_classical_function_incrementer() {
    test_expression(
        indoc::indoc! {"{
            let n = 10;
            use q1 = Qubit[n];
            ApplyXorInPlaceL(100L, q1);
            Std.ArithmeticTestUtils.ApplyClassicalFunction((x) -> x + 11L, q1);
            Std.Measurement.MeasureBigInt(q1)
        }"},
        &Value::BigInt(111i64.into()),
    );
}

#[test]
fn check_classical_function_double() {
    test_expression(
        indoc::indoc! {"{
            let n = 10;
            use q1 = Qubit[n];
            ApplyXorInPlaceL(100L, q1);
            Std.ArithmeticTestUtils.ApplyClassicalFunction((x) -> x * 2L, q1);
            Std.Measurement.MeasureBigInt(q1)
        }"},
        &Value::BigInt(200i64.into()),
    );
}

#[test]
fn check_classical_function_adder() {
    test_expression(
        indoc::indoc! {"{
            use (q1, q2) = (Qubit[10], Qubit[10]);
            ApplyXorInPlaceL(100L, q1);
            ApplyXorInPlaceL(55L, q2);
            Std.ArithmeticTestUtils.ApplyClassicalFunctionN(args -> [args[0], args[0]+args[1]], [q1, q2]);
            ResetAll(q1);
            Std.Measurement.MeasureBigInt(q2)
        }"},
        &Value::BigInt(155i64.into()),
    );
}

#[test]
fn check_classical_function_non_injective() {
    let err = test_expression_fails(indoc::indoc! {"{
        use qs = Qubit[5];
        H(qs[0]);
        Std.ArithmeticTestUtils.ApplyClassicalFunction((x) -> 0L, qs);
        ResetAll(qs);
    }"});
    assert!(err.contains("function must be injective"));
}

#[test]
fn check_classical_function_negative() {
    let err = test_expression_fails(indoc::indoc! {"{
        use qs = Qubit[5];
        Std.ArithmeticTestUtils.ApplyClassicalFunction((x) -> x-1L, qs);
        ResetAll(qs);
    }"});
    assert!(err.contains("function result must be non-negative"));
}

#[test]
fn check_classical_function_error_in_function() {
    let err = test_expression_fails(indoc::indoc! {"{
        use qs = Qubit[5];
        Std.ArithmeticTestUtils.ApplyClassicalFunction((x) -> 1L/x, qs);
        ResetAll(qs);
    }"});
    assert!(err.contains("division by zero"));
}
