// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp;
use expect_test::expect;

#[test]
fn to_static_array_ref() {
    let source = "
    array[bool, 3, 4] arr;
    def f(readonly array[bool, 3, 4] a) {}
    f(arr);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable arr = [[false, false, false, false], [false, false, false, false], [false, false, false, false]];
        function f(a : Bool[][]) : Unit {}
        f(arr);
    "#]],
    );
}

#[test]
fn to_dyn_array_ref() {
    let source = "
    array[bool, 3, 4] arr;
    def f(readonly array[bool, #dim = 2] a) {}
    f(arr);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable arr = [[false, false, false, false], [false, false, false, false], [false, false, false, false]];
        function f(a : Bool[][]) : Unit {}
        f(arr);
    "#]],
    );
}
