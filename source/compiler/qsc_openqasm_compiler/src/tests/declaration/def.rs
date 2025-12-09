// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{check_qasm_to_qsharp, compile_qasm_stmt_to_qsharp};
use expect_test::expect;
use miette::Report;

#[test]
fn no_parameters_no_return() -> miette::Result<(), Vec<Report>> {
    let source = "def empty() {}";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        function empty() : Unit {}
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn single_parameter() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def square(int x) -> int {
            return x * x;
        }
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        function square(x : Int) : Int {
            return x * x;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qubit_parameter() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def square(qubit q) -> uint {
            return 1;
        }
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        operation square(q : Qubit) : Int {
            return 1;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qubit_array_parameter() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def square(qubit[3] qs) -> uint {
            return 1;
        }
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        operation square(qs : Qubit[]) : Int {
            if Std.Core.Length(qs) != 3 {
                fail "Argument `qs` is not compatible with its OpenQASM type `qubit[3]`."
            };
            return 1;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_cast_to_function_return_type() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def square(int a) -> bit {
            return a;
        }
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        function square(a : Int) : Result {
            return Std.OpenQASM.Convert.IntAsResult(a);
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn return_from_void_function() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def square(int a) {
            return;
        }
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        function square(a : Int) : Unit {
            return ();
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn return_expr_on_void_function_fails() {
    let source = r#"
        def square(int val) {
            return val;
        }
    "#;

    let Err(errors) = compile_qasm_stmt_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.ReturningExpressionFromVoidSubroutine

          x cannot return an expression from a void subroutine
           ,-[Test.qasm:3:20]
         2 |         def square(int val) {
         3 |             return val;
           :                    ^^^
         4 |         }
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn missing_return_stmt_expr_on_non_void_function_fails() {
    let source = r#"
        def square(int a) -> bit {
            return;
        }
    "#;

    let Err(errors) = compile_qasm_stmt_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.MissingTargetExpressionInReturnStmt

          x return statements on a non-void subroutine should have a target expression
           ,-[Test.qasm:3:13]
         2 |         def square(int a) -> bit {
         3 |             return;
           :             ^^^^^^^
         4 |         }
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn missing_return_in_non_void_function_fails() {
    let source = r#"
        def square(int a) -> bit {
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |         }
               `----
        "#]],
    );
}

#[test]
fn return_from_if_with_else() {
    let source = r#"
        def square(int a) -> bit {
            if (a == 0) {
                return 0;
            } else {
                return 1;
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function square(a : Int) : Result {
                if a == 0 {
                    return Std.OpenQASM.Convert.IntAsResult(0);
                } else {
                    return Std.OpenQASM.Convert.IntAsResult(1);
                };
            }
        "#]],
    );
}

#[test]
fn missing_return_in_else_fails() {
    let source = r#"
        def square(int a) -> bit {
            if (a == 0) {
                return 0;
            } else {
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             if (a == 0) {
               `----
        "#]],
    );
}

#[test]
fn missing_return_in_if_fails() {
    let source = r#"
        def square(int a) -> bit {
            if (a == 0) {
            } else {
                return 0;
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             if (a == 0) {
               `----
        "#]],
    );
}

#[test]
fn missing_return_in_omitted_else_fails() {
    let source = r#"
        def square(int a) -> bit {
            if (a == 0) {
                return 0;
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             if (a == 0) {
               `----
        "#]],
    );
}

#[test]
fn return_from_for_loop() {
    let source = r#"
        def square(int a) -> bit {
            for int i in {1, 2} {
                return 1;
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function square(a : Int) : Result {
                for i : Int in [1, 2] {
                    return Std.OpenQASM.Convert.IntAsResult(1);
                }
            }
        "#]],
    );
}

#[test]
fn missing_return_in_for_loop_fails() {
    let source = r#"
        def square(int a) -> bit {
            for int i in {1, 2} {}
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             for int i in {1, 2} {}
               `----
        "#]],
    );
}

#[test]
fn return_from_while_loop() {
    let source = r#"
        def square(int a) -> bit {
            while (true) {
                return 1;
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function square(a : Int) : Result {
                while true {
                    return Std.OpenQASM.Convert.IntAsResult(1);
                }
            }
        "#]],
    );
}

#[test]
fn missing_return_in_while_loop_fails() {
    let source = r#"
        def square(int a) -> bit {
            while (true) {}
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             while (true) {}
               `----
        "#]],
    );
}

#[test]
fn return_from_switch() {
    let source = r#"
        def square(int a) -> bit {
            switch (a) {
                case 0 { return 1; }
                case 1 { return 0; }
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function square(a : Int) : Result {
                if a == 0 {
                    return Std.OpenQASM.Convert.IntAsResult(1);
                } elif a == 1 {
                    return Std.OpenQASM.Convert.IntAsResult(0);
                };
            }
        "#]],
    );
}

#[test]
fn missing_return_in_switch_case_fails() {
    let source = r#"
        def square(int a) -> bit {
            switch (a) {
                case 0 { return 1; }
                case 1 { }
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             switch (a) {
               `----
        "#]],
    );
}

#[test]
fn missing_return_in_switch_default_case_fails() {
    let source = r#"
        def square(int a) -> bit {
            switch (a) {
                case 0 { return 1; }
                default { }
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             switch (a) {
               `----
        "#]],
    );
}

#[test]
fn return_from_block() {
    let source = r#"
        def square(int a) -> bit {
            {
                return 1;
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function square(a : Int) : Result {
                {
                    return Std.OpenQASM.Convert.IntAsResult(1);
                };
            }
        "#]],
    );
}

#[test]
fn missing_return_in_block_fails() {
    let source = r#"
        def square(int a) -> bit {
            {}
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.NonVoidDefShouldAlwaysReturn

              x non-void def should always return
               ,-[Test.qasm:2:30]
             1 | 
             2 |         def square(int a) -> bit {
               :                              ^^^
             3 |             {}
               `----
        "#]],
    );
}

#[test]
fn capturing_external_variables_const_evaluate_them() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 2;
        const int b = 3;
        const int c = a * b;
        def f() -> int {
            return c;
        }
    "#;

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        function f() : Int {
            return 6;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn capturing_non_const_external_variable_fails() {
    let source = r#"
        int a = 2 << (-3);
        def f() -> int {
            return a;
        }
    "#;

    let Err(errors) = compile_qasm_stmt_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.ExprMustBeConst

          x a captured variable must be a const expression
           ,-[Test.qasm:4:20]
         3 |         def f() -> int {
         4 |             return a;
           :                    ^
         5 |         }
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn capturing_non_const_evaluatable_external_variable_fails() {
    let source = r#"
        const int a = 2 << (-3);
        def f() -> int {
            return a;
        }
    "#;

    let Err(errors) = compile_qasm_stmt_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.NegativeUIntValue

          x uint expression must evaluate to a non-negative value, but it evaluated
          | to -3
           ,-[Test.qasm:2:28]
         1 | 
         2 |         const int a = 2 << (-3);
           :                            ^^^^
         3 |         def f() -> int {
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn end_is_a_valid_return() {
    let source = r#"
        def square(int a) -> bit {
            end;
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function square(a : Int) : Result {
                fail "end";
            }
        "#]],
    );
}

#[test]
fn cannot_redefine_builtin_function() {
    let source = r#"
        def mod(int a) -> bit {
            return 1;
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.RedefinedBuiltinFunction

          x redefined builtin function: mod
           ,-[Test.qasm:2:13]
         1 | 
         2 |         def mod(int a) -> bit {
           :             ^^^
         3 |             return 1;
           `----
    "#]],
    );
}
