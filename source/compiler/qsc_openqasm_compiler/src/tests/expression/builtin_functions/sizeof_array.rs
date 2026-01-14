// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp as check;
use expect_test::expect;

#[test]
fn sizeof_with_non_existent_dimension_errors() {
    let source = "
        array[int, 1] a;
        sizeof(a, 4);
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.SizeofInvalidDimension

              x requested dimension 4 but array has 1 dimensions
               ,-[Test.qasm:3:9]
             2 |         array[int, 1] a;
             3 |         sizeof(a, 4);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sizeof_with_1_dimensional_array_generates_correct_qsharp() {
    let source = "
        array[int, 1] a;
        sizeof(a);
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [0];
            1;
        "#]],
    );
}

#[test]
fn sizeof_with_2_dimensional_array_generates_correct_qsharp() {
    let source = "
        array[int, 1, 2] a;
        sizeof(a, 1);
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[0, 0]];
            2;
        "#]],
    );
}

#[test]
fn sizeof_with_3_dimensional_array_generates_correct_qsharp() {
    let source = "
        array[int, 1, 1, 3] a;
        sizeof(a, 2);
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[[0, 0, 0]]];
            3;
        "#]],
    );
}

#[test]
fn sizeof_with_4_dimensional_array_generates_correct_qsharp() {
    let source = "
        array[int, 1, 1, 1, 4] a;
        sizeof(a, 3);
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[[[0, 0, 0, 0]]]];
            4;
        "#]],
    );
}

#[test]
fn sizeof_with_5_dimensional_array_generates_correct_qsharp() {
    let source = "
        array[int, 1, 1, 1, 1, 5] a;
        sizeof(a, 4);
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[[[[0, 0, 0, 0, 0]]]]];
            5;
        "#]],
    );
}

#[test]
fn sizeof_with_6_dimensional_array_generates_correct_qsharp() {
    let source = "
        array[int, 1, 1, 1, 1, 1, 6] a;
        sizeof(a, 5);
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[[[[[0, 0, 0, 0, 0, 0]]]]]];
            6;
        "#]],
    );
}

#[test]
fn sizeof_with_7_dimensional_array_generates_correct_qsharp() {
    let source = "
        array[int, 1, 1, 1, 1, 1, 1, 7] a;
        sizeof(a, 6);
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[[[[[[0, 0, 0, 0, 0, 0, 0]]]]]]];
            7;
        "#]],
    );
}

#[test]
fn sizeof_with_8_dimensional_array_errors() {
    let source = "
        array[int, 1, 1, 1, 1, 1, 1, 1, 8] a;
        sizeof(a);
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x arrays with more than 7 dimensions are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         array[int, 1, 1, 1, 1, 1, 1, 1, 8] a;
               :         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             3 |         sizeof(a);
               `----

            Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: (unknown)
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[Test.qasm:3:9]
             2 |         array[int, 1, 1, 1, 1, 1, 1, 1, 8] a;
             3 |         sizeof(a);
               :         ^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sizeof_with_0_dimensional_array_errors() {
    let source = "
        array[int, ] a;
        sizeof(a);
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x arrays with 0 dimensions are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         array[int, ] a;
               :         ^^^^^^^^^^^^
             3 |         sizeof(a);
               `----

            Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: (unknown)
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[Test.qasm:3:9]
             2 |         array[int, ] a;
             3 |         sizeof(a);
               :         ^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn sizeof_with_non_const_dim_errors() {
    let source = "
        array[int, 1] a;
        uint d;
        sizeof(a, d);
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.ExprMustBeConst

              x expression must be const
               ,-[Test.qasm:4:19]
             3 |         uint d;
             4 |         sizeof(a, d);
               :                   ^
             5 |     
               `----

            Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: (array[int, 1], uint)
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[Test.qasm:4:9]
             3 |         uint d;
             4 |         sizeof(a, d);
               :         ^^^^^^^^^^^^
             5 |     
               `----
        "#]],
    );
}
