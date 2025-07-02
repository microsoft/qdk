// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
    tests::{fail_on_compilation_errors, gen_qsharp},
};
use expect_test::expect;
use miette::Report;

use super::{compile_qasm_to_qir, compile_with_config};

#[test]
fn using_re_semantics_removes_output() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        output bit[2] c;
        qubit[2] q;
        input float[64] theta;
        input int[64] beta;
        output float[64] gamma;
        output float[64] delta;
        rz(theta) q[0];
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    "#;
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::ResourceEstimation,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config).expect("parse failed");
    fail_on_compilation_errors(&unit);
    let qsharp = gen_qsharp(&unit.package);
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            operation Test(theta : Double, beta : Int) : Unit {
                mutable c = [Zero, Zero];
                let q = QIR.Runtime.AllocateQubitArray(2);
                mutable gamma = 0.;
                mutable delta = 0.;
                rz(Std.OpenQASM.Angle.DoubleAsAngle(theta, 53), q[0]);
                h(q[0]);
                cx(q[0], q[1]);
                set c[0] = Std.Intrinsic.M(q[0]);
                set c[1] = Std.Intrinsic.M(q[1]);
            }
        }"#]]
    .assert_eq(&qsharp);

    Ok(())
}

#[test]
fn using_qasm_semantics_captures_all_classical_decls_as_output() -> miette::Result<(), Vec<Report>>
{
    let source = r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        output bit[2] c;
        qubit[2] q;
        input float[64] theta;
        input int[64] beta;
        output float[64] gamma;
        output float[64] delta;
        rz(theta) q[0];
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    "#;

    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config).expect("parse failed");
    fail_on_compilation_errors(&unit);
    let qsharp = gen_qsharp(&unit.package);
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            operation Test(theta : Double, beta : Int) : (Result[], Double, Double) {
                mutable c = [Zero, Zero];
                let q = QIR.Runtime.AllocateQubitArray(2);
                mutable gamma = 0.;
                mutable delta = 0.;
                rz(Std.OpenQASM.Angle.DoubleAsAngle(theta, 53), q[0]);
                h(q[0]);
                cx(q[0], q[1]);
                set c[0] = Std.Intrinsic.M(q[0]);
                set c[1] = Std.Intrinsic.M(q[1]);
                (c, gamma, delta)
            }
        }"#]]
    .assert_eq(&qsharp);

    Ok(())
}

#[test]
fn using_qiskit_semantics_only_bit_array_is_captured_and_reversed()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        output bit[2] c;
        qubit[2] q;
        input float[64] theta;
        input int[64] beta;
        output float[64] gamma;
        output float[64] delta;
        rz(theta) q[0];
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    "#;
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config).expect("parse failed");
    fail_on_compilation_errors(&unit);
    let qsharp = gen_qsharp(&unit.package);
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            operation Test(theta : Double, beta : Int) : Result[] {
                mutable c = [Zero, Zero];
                let q = QIR.Runtime.AllocateQubitArray(2);
                mutable gamma = 0.;
                mutable delta = 0.;
                rz(Std.OpenQASM.Angle.DoubleAsAngle(theta, 53), q[0]);
                h(q[0]);
                cx(q[0], q[1]);
                set c[0] = Std.Intrinsic.M(q[0]);
                set c[1] = Std.Intrinsic.M(q[1]);
                Std.Arrays.Reversed(c)
            }
        }"#]]
    .assert_eq(&qsharp);

    Ok(())
}

#[test]
fn using_qiskit_semantics_multiple_bit_arrays_are_reversed_in_order_and_reversed_in_content()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
OPENQASM 3.0;
include "stdgates.inc";
output bit[2] c;
output bit[3] c2;
qubit[5] q;
input float[64] theta;
input int[64] beta;
output float[64] gamma;
output float[64] delta;
rz(theta) q[0];
h q[0];
cx q[0], q[1];
x q[2];
id q[3];
x q[4];
c[0] = measure q[0];
c[1] = measure q[1];
c2[0] = measure q[2];
c2[1] = measure q[3];
c2[2] = measure q[4];
    "#;
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config).expect("parse failed");
    fail_on_compilation_errors(&unit);
    let package = unit.package;
    let qsharp = gen_qsharp(&package.clone());
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            operation Test(theta : Double, beta : Int) : (Result[], Result[]) {
                mutable c = [Zero, Zero];
                mutable c2 = [Zero, Zero, Zero];
                let q = QIR.Runtime.AllocateQubitArray(5);
                mutable gamma = 0.;
                mutable delta = 0.;
                rz(Std.OpenQASM.Angle.DoubleAsAngle(theta, 53), q[0]);
                h(q[0]);
                cx(q[0], q[1]);
                x(q[2]);
                id(q[3]);
                x(q[4]);
                set c[0] = Std.Intrinsic.M(q[0]);
                set c[1] = Std.Intrinsic.M(q[1]);
                set c2[0] = Std.Intrinsic.M(q[2]);
                set c2[1] = Std.Intrinsic.M(q[3]);
                set c2[2] = Std.Intrinsic.M(q[4]);
                (Std.Arrays.Reversed(c2), Std.Arrays.Reversed(c))
            }
        }"#]]
    .assert_eq(&qsharp);

    Ok(())
}

#[test]
fn qir_generation_using_qiskit_semantics_multiple_bit_arrays_are_reversed_in_order_and_reversed_in_content()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
OPENQASM 3.0;
include "stdgates.inc";
#pragma qdk.qir.profile Adaptive_RI
output bit[2] c;
output bit[3] c2;
qubit[5] q;
float[64] theta = 0.5;
int[64] beta = 4;
output float[64] gamma;
output float[64] delta;
rz(theta) q[0];
h q[0];
cx q[0], q[1];
x q[2];
id q[3];
x q[4];
barrier q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
c2[0] = measure q[2];
c2[1] = measure q[3];
c2[2] = measure q[4];
    "#;

    let qir = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__rz__body(double 0.4999999999999997, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
          call void @__quantum__rt__array_record_output(i64 3, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
          call void @__quantum__rt__array_record_output(i64 2, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret i64 0
        }

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__barrier__body()

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
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
fn qir_generation_for_box_with_simulatable_intrinsic() -> miette::Result<(), Vec<Report>> {
    let source = r#"
    OPENQASM 3.0;
    include "stdgates.inc";
    #pragma qdk.qir.profile Adaptive_RI
    #pragma qdk.box.open box_begin
    #pragma qdk.box.close box_end

    @SimulatableIntrinsic
    def box_begin() {}

    @SimulatableIntrinsic
    def box_end() {}

    qubit q;
    box {
        x q;
    }
    output bit c;
    c = measure q;
    "#;

    let qir = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define void @ENTRYPOINT__main() #0 {
        block_0:
          call void @box_begin()
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @box_end()
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
          ret void
        }

        declare void @box_begin()

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @box_end()

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
    .assert_eq(&qir);

    Ok(())
}

#[test]
fn qir_generation_for_box_with_qdk_qir_intrinsic() -> miette::Result<(), Vec<Report>> {
    let source = r#"
    OPENQASM 3.0;
    include "stdgates.inc";
    #pragma qdk.qir.profile Adaptive_RI
    #pragma qdk.box.open box_begin
    #pragma qdk.box.close box_end

    @qdk.qir.intrinsic
    def box_begin() {}

    @qdk.qir.intrinsic
    def box_end() {}

    qubit q;
    box {
        x q;
    }
    output bit c;
    c = measure q;
    "#;

    let qir = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define void @ENTRYPOINT__main() #0 {
        block_0:
          call void @box_begin()
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @box_end()
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
          ret void
        }

        declare void @box_begin()

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @box_end()

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
    .assert_eq(&qir);

    Ok(())
}
