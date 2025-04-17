// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{compile_qasm_to_qir, compile_qasm_to_qsharp};
use expect_test::expect;
use miette::Report;
use qsc::target::Profile;

#[test]
fn funcall_with_no_arguments_generates_correct_qsharp() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def empty() {}
        empty();
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        operation parity(qs : Qubit[]) : Result {
            mutable a = QIR.Intrinsic.__quantum__qis__m__body(qs[0]);
            mutable b = QIR.Intrinsic.__quantum__qis__m__body(qs[1]);
            return if __ResultAsInt__(a) ^^^ __ResultAsInt__(b) == 0 {
                One
            } else {
                Zero
            };
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        def square(float x) -> float {
            return x * x;
        }

        bit a = square(2.0);
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.CannotCast

          x cannot cast expression of type Float(None, false) to type Bit(false)
           ,-[Test.qasm:6:17]
         5 | 
         6 |         bit a = square(2.0);
           :                 ^^^^^^^^^^^
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

        bit p = parity(2);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        function parity(arr : Result[]) : Result {
            return if 1 == 0 {
                One
            } else {
                Zero
            };
        }
        mutable p = parity(__IntAsResultArrayBE__(2, 2));
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

          x cannot cast expression of type Int(None, true) to type QubitArray(One(2))
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

        @SimulatableIntrinsic
        def my_gate(qubit q) {
            x q;
        }

        qubit q;
        my_gate(q);
        bit result = measure q;
    "#;

    let qsharp = compile_qasm_to_qir(source, Profile::AdaptiveRI)?;
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
