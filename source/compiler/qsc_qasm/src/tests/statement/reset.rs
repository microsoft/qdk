// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qir;
use crate::{
    CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
    tests::{
        check_qasm_to_qsharp, compile_qasm_to_qsharp, compile_with_config,
        fail_on_compilation_errors, gen_qsharp,
    },
};
use expect_test::expect;
use miette::Report;

#[test]
fn reset_calls_are_generated_from_qasm() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        bit[1] meas;
        qubit[1] q;
        reset q[0];
        h q[0];
        meas[0] = measure q[0];
    "#;

    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config)?;
    fail_on_compilation_errors(&unit);
    let qsharp = gen_qsharp(&unit.package);
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            @EntryPoint()
            operation Test() : Result[] {
                mutable meas = [Zero];
                let q = QIR.Runtime.AllocateQubitArray(1);
                Reset(q[0]);
                h(q[0]);
                set meas[0] = Std.Intrinsic.M(q[0]);
                Std.Arrays.Reversed(meas)
            }
        }"#]]
    .assert_eq(&qsharp);

    Ok(())
}

#[test]
fn reset_with_base_profile_is_rewritten_without_resets() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        #pragma qdk.qir.profile Base
        bit[1] meas;
        qubit[1] q;
        reset q[0];
        h q[0];
        meas[0] = measure q[0];
    "#;

    let qir = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__array_record_output(i64 1, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret i64 0
        }

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]]
    .assert_eq(&qir);

    Ok(())
}

#[test]
fn reset_with_adaptive_ri_profile_generates_reset_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        #pragma qdk.qir.profile Adaptive_RI
        bit[1] meas;
        qubit[1] q;
        reset q[0];
        h q[0];
        meas[0] = measure q[0];
    "#;

    let qir = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__array_record_output(i64 1, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret i64 0
        }

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

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
    .assert_eq(&qir);

    Ok(())
}

#[test]
fn on_a_single_qubit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit q;
        reset q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        Reset(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn on_an_indexed_qubit_register() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[5] q;
        reset q[2];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(5);
        Reset(q[2]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn on_a_span_indexed_qubit_register() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[5] q;
        reset q[1:3];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(5);
        ResetAll(q[1..3]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn on_a_zero_len_qubit_register_fails() {
    let source = r#"
        qubit[0] q;
        reset q;
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.ExprMustBePositiveInt

              x quantum register size must be a positive integer
               ,-[Test.qasm:2:15]
             1 | 
             2 |         qubit[0] q;
               :               ^
             3 |         reset q;
               `----
        "#]],
    );
}

#[test]
fn on_an_unindexed_qubit_register() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[5] q;
        reset q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(5);
        ResetAll(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
