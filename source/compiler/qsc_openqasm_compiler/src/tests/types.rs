// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp;
use expect_test::expect;

#[test]
fn indexed_type_has_right_dimensions() {
    let source = "
        array[bool, 2, 3, 4] arr_1;
        array[bool, 3, 4] arr_2 = arr_1[0];
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable arr_1 = [[[false, false, false, false], [false, false, false, false], [false, false, false, false]], [[false, false, false, false], [false, false, false, false], [false, false, false, false]]];
        mutable arr_2 = arr_1[0];
    "#]],
    );
}

#[test]
fn sliced_type_has_right_dimensions() {
    let source = "
        array[bool, 5, 1, 2] arr_1;
        array[bool, 3, 1, 2] arr_2 = arr_1[1:3];
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable arr_1 = [[[false, false]], [[false, false]], [[false, false]], [[false, false]], [[false, false]]];
        mutable arr_2 = arr_1[1..3];
    "#]],
    );
}
