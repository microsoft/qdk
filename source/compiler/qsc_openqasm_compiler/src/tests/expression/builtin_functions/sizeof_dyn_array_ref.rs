// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp as check;
use expect_test::expect;

#[test]
fn sizeof_with_non_existent_dimension_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 1] a) {
            sizeof(a, 4);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[]) : Unit {
                Std.OpenQASM.Builtin.sizeof_1(a, 4);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_1_dimensional_array_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 1] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[], d : Int) : Unit {
                Std.OpenQASM.Builtin.sizeof_1(a, d);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_2_dimensional_array_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 2] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[][], d : Int) : Unit {
                Std.OpenQASM.Builtin.sizeof_2(a, d);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_3_dimensional_array_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 3] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[][][], d : Int) : Unit {
                Std.OpenQASM.Builtin.sizeof_3(a, d);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_4_dimensional_array_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 4] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[][][][], d : Int) : Unit {
                Std.OpenQASM.Builtin.sizeof_4(a, d);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_5_dimensional_array_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 5] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[][][][][], d : Int) : Unit {
                Std.OpenQASM.Builtin.sizeof_5(a, d);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_6_dimensional_array_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 6] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[][][][][][], d : Int) : Unit {
                Std.OpenQASM.Builtin.sizeof_6(a, d);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_7_dimensional_array_generates_correct_qsharp() {
    let source = "
        def f(readonly array[int, #dim = 7] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[][][][][][][], d : Int) : Unit {
                Std.OpenQASM.Builtin.sizeof_7(a, d);
            }
        "#]],
    );
}

#[test]
fn sizeof_with_8_dimensional_array_errors() {
    let source = "
        def f(readonly array[int, #dim = 8] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x arrays with more than 7 dimensions are not supported
               ,-[Test.qasm:2:15]
             1 | 
             2 |         def f(readonly array[int, #dim = 8] a, uint d) {
               :               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             3 |             sizeof(a, d);
               `----

            Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: (unknown, uint)
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[Test.qasm:3:13]
             2 |         def f(readonly array[int, #dim = 8] a, uint d) {
             3 |             sizeof(a, d);
               :             ^^^^^^^^^^^^
             4 |         }
               `----
        "#]],
    );
}

#[test]
fn sizeof_with_0_dimensional_array_errors() {
    let source = "
        def f(readonly array[int, #dim = 0] a, uint d) {
            sizeof(a, d);
        }
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x arrays with 0 dimensions are not supported
               ,-[Test.qasm:2:15]
             1 | 
             2 |         def f(readonly array[int, #dim = 0] a, uint d) {
               :               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             3 |             sizeof(a, d);
               `----

            Qasm.Lowerer.NoValidOverloadForBuiltinFunction

              x There is no valid overload of `sizeof` for inputs: (unknown, uint)
              | Overloads available are:
              |     fn sizeof(array[_, ...], const uint) -> const uint
              |     fn sizeof(array[_, #dim = _], uint) -> uint
               ,-[Test.qasm:3:13]
             2 |         def f(readonly array[int, #dim = 0] a, uint d) {
             3 |             sizeof(a, d);
               :             ^^^^^^^^^^^^
             4 |         }
               `----
        "#]],
    );
}
