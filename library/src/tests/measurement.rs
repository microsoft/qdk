// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::test_expression;
use indoc::indoc;
use qsc::interpret::Value;

// Tests for Std.Measurement namespace

#[test]
fn check_measure_all_z() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[2];
            let result = Std.Measurement.MeasureAllZ(register);
            ResetAll(register);
            result
        }"#},
        &Value::RESULT_ZERO,
    );
    test_expression(
        indoc! {r#"{
            use register = Qubit[2];
            Std.Arrays.ForEach(X, register);
            let result = Std.Measurement.MeasureAllZ(register);
            ResetAll(register);
            result
        }"#},
        &Value::RESULT_ZERO,
    );
    test_expression(
        indoc! {r#"{
            use register = Qubit[2];
            X(register[0]);
            let result = Std.Measurement.MeasureAllZ(register);
            ResetAll(register);
            result
        }"#},
        &Value::RESULT_ONE,
    );
    test_expression(
        indoc! {r#"{
            use register = Qubit[2];
            X(register[1]);
            let result = Std.Measurement.MeasureAllZ(register);
            ResetAll(register);
            result
        }"#},
        &Value::RESULT_ONE,
    );
}

#[test]
fn check_measure_each_z() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[3];
            X(register[0]);
            X(register[2]);
            let results = Std.Measurement.MeasureEachZ(register);
            ResetAll(register);
            results
        }"#},
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ZERO, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_mreset_each_z() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[3];
            X(register[0]);
            X(register[2]);
            let resultsA = Std.Measurement.MResetEachZ(register);
            let resultsB = Std.Measurement.MeasureEachZ(register);
            (resultsA, resultsB)
        }"#},
        &Value::Tuple(
            vec![
                Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ZERO, Value::RESULT_ONE].into()),
                Value::Array(
                    vec![Value::RESULT_ZERO, Value::RESULT_ZERO, Value::RESULT_ZERO].into(),
                ),
            ]
            .into(),
            None,
        ),
    );
}

#[test]
fn check_mreset_x() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[2];
            X(register[1]);
            Std.Canon.ApplyToEach(H, register);
            let r0 = Std.Measurement.MResetX(register[0]);
            let r1 = Std.Measurement.MResetX(register[1]);
            [r0, r1]
        }"#},
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_mreset_y() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[2];
            X(register[1]);
            Std.Canon.ApplyToEach(H, register);
            Std.Canon.ApplyToEach(S, register);
            let r0 = Std.Measurement.MResetY(register[0]);
            let r1 = Std.Measurement.MResetY(register[1]);
            [r0, r1]
        }"#},
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_mreset_z() {
    test_expression(
        indoc! {r#"{
            use register = Qubit[2];
            X(register[1]);
            let r0 = Std.Measurement.MResetZ(register[0]);
            let r1 = Std.Measurement.MResetZ(register[1]);
            [r0, r1]
        }"#},
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ONE].into()),
    );
}

#[test]
fn check_measure_integer() {
    test_expression(
        {
            "{
                use q = Qubit[16];
                ApplyXorInPlace(45967, q);
                let result = MeasureInteger(q);
                ResetAll(q);
                return result;
            }"
        },
        &Value::Int(45967),
    );
}
