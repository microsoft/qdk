// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::test_expression;
use indoc::indoc;
use qsc::interpret::Value;

#[test]
fn check_adjust_angle_size_no_truncation_increases_size() {
    test_expression(
        indoc! {r#"{
            import Std.OpenQASM.Angle.*;
            let angle = IntAsAngle(100, 16);
            let adjusted_angle = AdjustAngleSizeNoTruncation(angle, 32);
            adjusted_angle.Size
        }"#},
        &Value::Int(32),
    );
}

#[test]
fn angle_as_double_does_not_round_to_tau() {
    test_expression(
        indoc! {r#"{
            import Std.OpenQASM.Angle.*;
            let angle = IntAsAngle(9223372036854775807, 63);
            AngleAsDouble(angle) < 2.0 * Std.Math.PI()
        }"#},
        &Value::Bool(true),
    );
}

#[test]
fn double_as_angle_rounding_up_to_tau_wraps_to_zero() {
    test_expression(
        indoc! {r#"{
            import Std.OpenQASM.Angle.*;
            let angle = DoubleAsAngle(-0.1, 4);
            (angle.Value, angle.Size)
        }"#},
        &Value::Tuple(vec![Value::Int(0), Value::Int(4)].into(), None),
    );
}
