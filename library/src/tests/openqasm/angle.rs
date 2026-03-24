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
