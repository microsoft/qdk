// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{check_qasm_to_qsharp, compile_qasm_to_qir, compile_qasm_to_qsharp};
use expect_test::expect;
use miette::Report;

#[test]
fn funcall_with_no_arguments_generates_correct_qsharp() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def empty() {}
        empty();
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function empty() : Unit {}
        empty();
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn void_function_with_one_argument_generates_correct_qsharp() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def f(int x) {}
        f(2);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function f(x : Int) : Unit {}
        f(2);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn funcall_with_one_argument_generates_correct_qsharp() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def square(int x) -> int {
            return x * x;
        }

        square(2);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function square(x : Int) : Int {
            return x * x;
        }
        square(2);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn funcall_with_two_arguments_generates_correct_qsharp() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def sum(int x, int y) -> int {
            return x + y;
        }

        sum(2, 3);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function sum(x : Int, y : Int) : Int {
            return x + y;
        }
        sum(2, 3);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn funcall_with_qubit_argument() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def parity(qubit[2] qs) -> bit {
            bit a = measure qs[0];
            bit b = measure qs[1];
            return a ^ b;
        }

        qubit[2] qs;
        bit p = parity(qs);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation parity(qs : Qubit[]) : Result {
            if Std.Core.Length(qs) != 2 {
                fail "Argument `qs` is not compatible with its OpenQASM type `qubit[2]`."
            };
            mutable a = Std.Intrinsic.M(qs[0]);
            mutable b = Std.Intrinsic.M(qs[1]);
            return Std.OpenQASM.Convert.IntAsResult(Std.OpenQASM.Convert.ResultAsInt(a) ^^^ Std.OpenQASM.Convert.ResultAsInt(b));
        }
        let qs = QIR.Runtime.AllocateQubitArray(2);
        mutable p = parity(qs);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn funcall_with_too_few_arguments_generates_error() {
    let source = r#"
        def square(int x) -> int {
            return x * x;
        }

        square();
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfClassicalArgs

          x gate expects 1 classical arguments, but 0 were provided
           ,-[Test.qasm:6:9]
         5 | 
         6 |         square();
           :         ^^^^^^^^
         7 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn funcall_with_too_many_arguments_generates_error() {
    let source = r#"
        def square(int x) -> int {
            return x * x;
        }

        square(2, 3);
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfClassicalArgs

          x gate expects 1 classical arguments, but 2 were provided
           ,-[Test.qasm:6:9]
         5 | 
         6 |         square(2, 3);
           :         ^^^^^^^^^^^^
         7 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn funcall_accepts_qubit_argument() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        def h_wrapper(qubit q) {
            h q;
        }

        qubit q;
        h_wrapper(q);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation h_wrapper(q : Qubit) : Unit {
            h(q);
        }
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        h_wrapper(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn classical_decl_initialized_with_funcall() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def square(int x) -> int {
            return x * x;
        }

        int a = square(2);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function square(x : Int) : Int {
            return x * x;
        }
        mutable a = square(2);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn classical_decl_initialized_with_incompatible_funcall_errors() {
    let source = r#"
        def square(float x) -> angle {
            return angle(x * x);
        }

        float a = square(2.0);
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.CannotCast

          x cannot cast expression of type angle to type float
           ,-[Test.qasm:6:19]
         5 | 
         6 |         float a = square(2.0);
           :                   ^^^^^^^^^^^
         7 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn funcall_implicit_arg_cast_uint_to_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def parity(bit[2] arr) -> bit {
            return 1;
        }

        uint[2] x = 2;
        bit p = parity(x);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function parity(arr : Result[]) : Result {
            if Std.Core.Length(arr) != 2 {
                fail "Argument `arr` is not compatible with its OpenQASM type `bit[2]`."
            };
            return Std.OpenQASM.Convert.IntAsResult(1);
        }
        mutable x = 2;
        mutable p = parity(Std.OpenQASM.Convert.IntAsResultArrayBE(x, 2));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn funcall_implicit_arg_cast_uint_to_qubit_errors() {
    let source = r#"
        def parity(qubit[2] arr) -> bit {
            return 1;
        }

        bit p = parity(2);
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.CannotCast

          x cannot cast expression of type const int to type qubit[2]
           ,-[Test.qasm:6:24]
         5 | 
         6 |         bit p = parity(2);
           :                        ^
         7 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn simulatable_intrinsic_on_def_stmt_generates_correct_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        #pragma qdk.qir.profile Adaptive_RI

        @SimulatableIntrinsic
        def my_gate(qubit q) {
            x q;
        }

        qubit q;
        my_gate(q);
        bit result = measure q;
    "#;

    let qsharp = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @empty_tag = internal constant [1 x i8] c"\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @my_gate(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
          ret i64 0
        }

        declare void @my_gate(%Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qdk_qir_intrinsic_on_def_stmt_generates_correct_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        #pragma qdk.qir.profile Adaptive_RI

        @qdk.qir.intrinsic
        def my_gate(qubit q) {
            x q;
        }

        qubit q;
        my_gate(q);
        bit result = measure q;
    "#;

    let qsharp = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define void @ENTRYPOINT__main() #0 {
        block_0:
          call void @my_gate(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
          ret void
        }

        declare void @my_gate(%Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 1, !"int_computations", !"i64"}
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_cast_array_to_static_array_ref() {
    let source = "
        def f(readonly array[int, 4] a) {}
        array[int, 4] a;
        f(a);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[]) : Unit {
                if Std.Core.Length(a) != 4 {
                    fail "Argument `a` is not compatible with its OpenQASM type `readonly array[int, 4]`."
                };
            }
            mutable a = [0, 0, 0, 0];
            f(a);
        "#]],
    );
}

#[test]
fn implicit_cast_to_static_array_ref_with_different_base_ty_errors() {
    let source = "
        def f(readonly array[uint, 4] a) {}
        array[int, 4] a;
        f(a);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.CannotCast

          x cannot cast expression of type array[int, 4] to type readonly array[uint,
          | 4]
           ,-[Test.qasm:4:11]
         3 |         array[int, 4] a;
         4 |         f(a);
           :           ^
         5 |     
           `----
    "#]],
    );
}

#[test]
fn implicit_cast_to_static_array_ref_with_different_shape_errors() {
    let source = "
        def f(readonly array[int, 4] a) {}
        array[int, 5] a;
        f(a);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.CannotCast

          x cannot cast expression of type array[int, 5] to type readonly array[int,
          | 4]
           ,-[Test.qasm:4:11]
         3 |         array[int, 5] a;
         4 |         f(a);
           :           ^
         5 |     
           `----
    "#]],
    );
}

#[test]
fn implicit_cast_array_to_dyn_array_ref() {
    let source = "
        def f(readonly array[int, #dim = 1] a) {}
        array[int, 4] a;
        f(a);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            function f(a : Int[]) : Unit {}
            mutable a = [0, 0, 0, 0];
            f(a);
        "#]],
    );
}

#[test]
fn implicit_cast_to_dyn_array_ref_with_different_base_ty_errors() {
    let source = "
        def f(readonly array[uint, #dim = 1] a) {}
        array[int, 4] a;
        f(a);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.CannotCast

          x cannot cast expression of type array[int, 4] to type readonly array[uint,
          | #dim = 1]
           ,-[Test.qasm:4:11]
         3 |         array[int, 4] a;
         4 |         f(a);
           :           ^
         5 |     
           `----
    "#]],
    );
}

#[test]
fn implicit_cast_to_dyn_array_ref_with_different_shape_errors() {
    let source = "
        def f(readonly array[int, #dim = 2] a) {}
        array[int, 5] a;
        f(a);
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.CannotCast

          x cannot cast expression of type array[int, 5] to type readonly array[int,
          | #dim = 2]
           ,-[Test.qasm:4:11]
         3 |         array[int, 5] a;
         4 |         f(a);
           :           ^
         5 |     
           `----
    "#]],
    );
}
