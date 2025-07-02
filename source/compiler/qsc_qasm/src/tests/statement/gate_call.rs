// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{compile_qasm_to_qir, compile_qasm_to_qsharp};
use expect_test::expect;
use miette::Report;

#[test]
fn u_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit q;
        U(1.0, 2.0, 3.0) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        U(new Std.OpenQASM.Angle.Angle {
            Value = 1433540284805665,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 4300620854416994,
            Size = 53
        }, q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn u_gate_can_be_called_in_broadcast_qubit_array() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[1] q;
        U(1.0, 2.0, 3.0) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        U(new Std.OpenQASM.Angle.Angle {
            Value = 1433540284805665,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 4300620854416994,
            Size = 53
        }, q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn u_gate_can_be_called_in_broadcast_over_qreg() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qreg q[1];
        U(1.0, 2.0, 3.0) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        U(new Std.OpenQASM.Angle.Angle {
            Value = 1433540284805665,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 4300620854416994,
            Size = 53
        }, q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn gphase_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        gphase(2.0);
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        gphase(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        });
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn custom_gates_can_be_called_bypassing_stdgates() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        gate h a { U(π/2, 0., π) a; gphase(-π/4);}
        gate x a { U(π, 0., π) a; gphase(-π/2);}
        gate cx a, b { ctrl @ x a, b; }
        gate rz(λ) a { gphase(-λ/2); U(0., 0., λ) a; }
        gate rxx(theta) a, b { h a; h b; cx a, b; rz(theta) b; cx a, b; h b; h a; }

        qubit a;
        qubit b;
        x a;
        rxx(π/2) a, b;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation h(a : Qubit) : Unit is Adj + Ctl {
            U(Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793 / 2., 53), new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793, 53), a);
            gphase(Std.OpenQASM.Angle.DoubleAsAngle(-3.141592653589793 / 4., 53));
        }
        operation x(a : Qubit) : Unit is Adj + Ctl {
            U(Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793, 53), new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793, 53), a);
            gphase(Std.OpenQASM.Angle.DoubleAsAngle(-3.141592653589793 / 2., 53));
        }
        operation cx(a : Qubit, b : Qubit) : Unit is Adj + Ctl {
            Controlled x([a], b);
        }
        operation rz(λ : Std.OpenQASM.Angle.Angle, a : Qubit) : Unit is Adj + Ctl {
            gphase(Std.OpenQASM.Angle.DivideAngleByInt(Std.OpenQASM.Angle.NegAngle(λ), 2));
            U(new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, λ, a);
        }
        operation rxx(theta : Std.OpenQASM.Angle.Angle, a : Qubit, b : Qubit) : Unit is Adj + Ctl {
            h(a);
            h(b);
            cx(a, b);
            rz(theta, b);
            cx(a, b);
            h(b);
            h(a);
        }
        let a = QIR.Runtime.__quantum__rt__qubit_allocate();
        let b = QIR.Runtime.__quantum__rt__qubit_allocate();
        x(a);
        rxx(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn x_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        x q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        x(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn barrier_can_be_called_on_single_qubit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        barrier q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        __quantum__qis__barrier__body();
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn barrier_can_be_called_without_qubits() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        barrier;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        __quantum__qis__barrier__body();
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn barrier_generates_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        #pragma qdk.qir.profile Adaptive_RI
        bit[1] c;
        qubit[2] q;
        barrier q[0], q[1];
        barrier q[0];
        barrier;
        barrier q[0], q[1], q[0];
        c[0] = measure q[0];
    "#;

    let qsharp = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__array_record_output(i64 1, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret i64 0
        }

        declare void @__quantum__qis__barrier__body()

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
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
fn barrier_can_be_called_on_two_qubit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[2] q;
        barrier q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        __quantum__qis__barrier__body();
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cx_called_with_one_qubit_generates_error() {
    let source = r#"
        include "stdgates.inc";
        qubit[2] q;
        cx q[0];
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfQubitArgs

          x gate expects 2 qubit arguments, but 1 were provided
           ,-[Test.qasm:4:9]
         3 |         qubit[2] q;
         4 |         cx q[0];
           :         ^^^^^^^^
         5 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn cx_called_with_too_many_qubits_generates_error() {
    let source = r#"
        include "stdgates.inc";
        qubit[3] q;
        cx q[0], q[1], q[2];
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfQubitArgs

          x gate expects 2 qubit arguments, but 3 were provided
           ,-[Test.qasm:4:9]
         3 |         qubit[3] q;
         4 |         cx q[0], q[1], q[2];
           :         ^^^^^^^^^^^^^^^^^^^^
         5 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn rx_gate_with_no_angles_generates_error() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        rx q;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfClassicalArgs

          x gate expects 1 classical arguments, but 0 were provided
           ,-[Test.qasm:4:9]
         3 |         qubit q;
         4 |         rx q;
           :         ^^^^^
         5 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn rx_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        rx(2.0) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        rx(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn rx_gate_with_too_many_angles_generates_error() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        rx(2.0, 3.0) q;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfClassicalArgs

          x gate expects 1 classical arguments, but 2 were provided
           ,-[Test.qasm:4:9]
         3 |         qubit q;
         4 |         rx(2.0, 3.0) q;
           :         ^^^^^^^^^^^^^^^
         5 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn implicit_cast_to_angle_works() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        float a = 2.0;
        rx(a) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        mutable a = 2.;
        rx(Std.OpenQASM.Angle.DoubleAsAngle(a, 53), q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn custom_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        gate my_gate q1, q2 {
            h q1;
            h q2;
        }

        qubit[2] q;
        my_gate q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation my_gate(q1 : Qubit, q2 : Qubit) : Unit is Adj + Ctl {
            h(q1);
            h(q2);
        }
        let q = QIR.Runtime.AllocateQubitArray(2);
        my_gate(q[0], q[1]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn custom_gate_can_be_called_with_inv_modifier() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        gate my_gate q1, q2 {
            h q1;
            h q2;
        }

        qubit[2] q;
        inv @ my_gate q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation my_gate(q1 : Qubit, q2 : Qubit) : Unit is Adj + Ctl {
            h(q1);
            h(q2);
        }
        let q = QIR.Runtime.AllocateQubitArray(2);
        Adjoint my_gate(q[0], q[1]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn custom_gate_can_be_called_with_ctrl_modifier() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        gate my_gate q1, q2 {
            h q1;
            h q2;
        }

        qubit[2] ctl;
        qubit[2] q;
        ctrl(2) @ my_gate ctl[0], ctl[1], q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation my_gate(q1 : Qubit, q2 : Qubit) : Unit is Adj + Ctl {
            h(q1);
            h(q2);
        }
        let ctl = QIR.Runtime.AllocateQubitArray(2);
        let q = QIR.Runtime.AllocateQubitArray(2);
        Controlled my_gate([ctl[0], ctl[1]], (q[0], q[1]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn custom_gate_can_be_called_with_negctrl_modifier() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        gate my_gate q1, q2 {
            h q1;
            h q2;
        }

        qubit[2] ctl;
        qubit[2] q;
        negctrl(2) @ my_gate ctl[0], ctl[1], q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation my_gate(q1 : Qubit, q2 : Qubit) : Unit is Adj + Ctl {
            h(q1);
            h(q2);
        }
        let ctl = QIR.Runtime.AllocateQubitArray(2);
        let q = QIR.Runtime.AllocateQubitArray(2);
        ApplyControlledOnInt(0, my_gate, [ctl[0], ctl[1]], (q[0], q[1]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn custom_gate_can_be_called_with_pow_modifier() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        gate my_gate q1, q2 {
            h q1;
            h q2;
        }

        qubit[2] q;
        pow(2) @ my_gate q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation my_gate(q1 : Qubit, q2 : Qubit) : Unit is Adj + Ctl {
            h(q1);
            h(q2);
        }
        let q = QIR.Runtime.AllocateQubitArray(2);
        ApplyOperationPowerA(2, my_gate, (q[0], q[1]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn simulatable_intrinsic_on_gate_stmt_generates_correct_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        #pragma qdk.qir.profile Adaptive_RI

        @SimulatableIntrinsic
        gate my_gate q {
            x q;
        }

        qubit q;
        my_gate q;
        bit result = measure q;
    "#;

    let qsharp = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @my_gate(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
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
        !4 = !{i32 1, !"int_computations", !"i64"}
    "#]].assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qdk_qir_intrinsic_on_gate_stmt_generates_correct_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        #pragma qdk.qir.profile Adaptive_RI

        @qdk.qir.intrinsic
        gate my_gate q {
            x q;
        }

        qubit q;
        my_gate q;
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
    "#]].assert_eq(&qsharp);
    Ok(())
}

#[test]
fn rxx_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[2] q;
        rxx(2.0) q[1], q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        rxx(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[1], q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn ryy_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[2] q;
        ryy(2.0) q[1], q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        ryy(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[1], q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn rzz_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[2] q;
        rzz(2.0) q[1], q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        rzz(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[1], q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn all_qiskit_stdgates_can_be_called_included() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit[4] q;
        rxx(pi / 2.0) q[1], q[0];
        ryy(pi / 2.0) q[1], q[0];
        rzz(pi / 2.0) q[1], q[0];
        dcx q[0], q[1];
        ecr q[0], q[1];
        r(pi / 2.0, pi / 4.0) q[1];
        rzx(pi / 2.0) q[1], q[0];
        cs q[0], q[1];
        csdg q[0], q[1];
        sxdg q[0];
        csx q[0], q[1];
        cu1(pi / 2.0) q[1], q[0];
        cu3(pi / 2.0, pi / 4.0, pi / 8.0) q[1], q[0];
        rccx q[0], q[1], q[2];
        c3sqrtx q[0], q[1], q[2], q[3];
        c3x q[0], q[1], q[2], q[3];
        rc3x q[0], q[1], q[2], q[3];
        xx_minus_yy(pi / 2.0, pi / 4.0) q[1], q[0];
        xx_plus_yy(pi / 2.0, pi / 4.0) q[1], q[0];
        ccz q[0], q[1], q[2];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(4);
        rxx(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        ryy(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        rzz(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        dcx(q[0], q[1]);
        ecr(q[0], q[1]);
        r(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), q[1]);
        rzx(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        cs(q[0], q[1]);
        csdg(q[0], q[1]);
        sxdg(q[0]);
        csx(q[0], q[1]);
        cu1(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        cu3(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 8., 53), q[1], q[0]);
        rccx(q[0], q[1], q[2]);
        c3sqrtx(q[0], q[1], q[2], q[3]);
        c3x(q[0], q[1], q[2], q[3]);
        rc3x(q[0], q[1], q[2], q[3]);
        xx_minus_yy(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), q[1], q[0]);
        xx_plus_yy(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), q[1], q[0]);
        ccz(q[0], q[1], q[2]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn broadcast_one_qubit_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[2] qs;
        h qs;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let qs = QIR.Runtime.AllocateQubitArray(2);
        h(qs[0]);
        h(qs[1]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn broadcast_two_qubit_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[3] ctrls;
        qubit[3] targets;
        cx ctrls, targets;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctrls = QIR.Runtime.AllocateQubitArray(3);
        let targets = QIR.Runtime.AllocateQubitArray(3);
        cx(ctrls[0], targets[0]);
        cx(ctrls[1], targets[1]);
        cx(ctrls[2], targets[2]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn broadcast_controlled_two_qubit_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[3] ctrls;
        qubit[3] targets;
        inv @ cx ctrls, targets;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctrls = QIR.Runtime.AllocateQubitArray(3);
        let targets = QIR.Runtime.AllocateQubitArray(3);
        Adjoint cx(ctrls[0], targets[0]);
        Adjoint cx(ctrls[1], targets[1]);
        Adjoint cx(ctrls[2], targets[2]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn broadcast_explicitly_controlled_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[3] ctrls;
        qubit[3] targets;
        ctrl @ x ctrls, targets;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctrls = QIR.Runtime.AllocateQubitArray(3);
        let targets = QIR.Runtime.AllocateQubitArray(3);
        Controlled x([ctrls[0]], targets[0]);
        Controlled x([ctrls[1]], targets[1]);
        Controlled x([ctrls[2]], targets[2]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn broadcast_explicitly_controlled_controlled_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[3] ctrls1;
        qubit[3] ctrls2;
        qubit[3] targets;
        ctrl @ ctrl @ x ctrls1, ctrls2, targets;
    "#;
    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctrls1 = QIR.Runtime.AllocateQubitArray(3);
        let ctrls2 = QIR.Runtime.AllocateQubitArray(3);
        let targets = QIR.Runtime.AllocateQubitArray(3);
        Controlled Controlled x([ctrls1[0]], ([ctrls2[0]], targets[0]));
        Controlled Controlled x([ctrls1[1]], ([ctrls2[1]], targets[1]));
        Controlled Controlled x([ctrls1[2]], ([ctrls2[2]], targets[2]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn broadcast_with_qubit_and_register() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit[2] targets;
        ctrl @ x ctl, targets;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let targets = QIR.Runtime.AllocateQubitArray(2);
        Controlled x([ctl], targets[0]);
        Controlled x([ctl], targets[1]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn broadcast_with_different_register_sizes_fails() {
    let source = r#"
        include "stdgates.inc";
        qubit[3] ctrls;
        qubit[2] targets;
        ctrl @ x ctrls, targets;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.BroadcastCallQuantumArgsDisagreeInSize

          x first quantum register is of type qubit[3] but found an argument of type
          | qubit[2]
           ,-[Test.qasm:5:25]
         4 |         qubit[2] targets;
         5 |         ctrl @ x ctrls, targets;
           :                         ^^^^^^^
         6 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn qasm2_u_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        qreg q[1];
        U(1.0, 2.0, 3.0) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        U(new Std.OpenQASM.Angle.Angle {
            Value = 1433540284805665,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 4300620854416994,
            Size = 53
        }, q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_custom_gates_can_be_called_bypassing_stdgates() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        gate u1(lambda) q { U(0.,0.,lambda) q; }
        gate u2(phi,lambda) q { U(pi/2,phi,lambda) q; }
        gate u3(theta,phi,lambda) q { U(theta,phi,lambda) q; }
        gate h a { u2(0.,pi) a; }
        gate x a { u3(pi,0.,pi) a; }
        gate rz(phi) a { u1(phi) a; }
        gate rxx(theta) a, b { h a; h b; CX a, b; rz(theta) b; CX a, b; h b; h a; }

        qubit a;
        qubit b;
        x a;
        rxx(π/2) a, b;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation u1(lambda : Std.OpenQASM.Angle.Angle, q : Qubit) : Unit is Adj + Ctl {
            U(new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, lambda, q);
        }
        operation u2(phi : Std.OpenQASM.Angle.Angle, lambda : Std.OpenQASM.Angle.Angle, q : Qubit) : Unit is Adj + Ctl {
            U(Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793 / 2., 53), phi, lambda, q);
        }
        operation u3(theta : Std.OpenQASM.Angle.Angle, phi : Std.OpenQASM.Angle.Angle, lambda : Std.OpenQASM.Angle.Angle, q : Qubit) : Unit is Adj + Ctl {
            U(theta, phi, lambda, q);
        }
        operation h(a : Qubit) : Unit is Adj + Ctl {
            u2(new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793, 53), a);
        }
        operation x(a : Qubit) : Unit is Adj + Ctl {
            u3(Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793, 53), new Std.OpenQASM.Angle.Angle {
                Value = 0,
                Size = 53
            }, Std.OpenQASM.Angle.DoubleAsAngle(3.141592653589793, 53), a);
        }
        operation rz(phi : Std.OpenQASM.Angle.Angle, a : Qubit) : Unit is Adj + Ctl {
            u1(phi, a);
        }
        operation rxx(theta : Std.OpenQASM.Angle.Angle, a : Qubit, b : Qubit) : Unit is Adj + Ctl {
            h(a);
            h(b);
            CX(a, b);
            rz(theta, b);
            CX(a, b);
            h(b);
            h(a);
        }
        let a = QIR.Runtime.__quantum__rt__qubit_allocate();
        let b = QIR.Runtime.__quantum__rt__qubit_allocate();
        x(a);
        rxx(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_x_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        x q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        x(q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_barrier_can_be_called_on_single_qubit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        barrier q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        __quantum__qis__barrier__body();
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_barrier_can_be_called_without_qubits() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        barrier;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        __quantum__qis__barrier__body();
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_barrier_generates_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        #pragma qdk.qir.profile Adaptive_RI
        creg c[1];
        qubit[2] q;
        barrier q[0], q[1];
        barrier q[0];
        barrier;
        barrier q[0], q[1], q[0];
        c[0] = measure q[0];
    "#;

    let qsharp = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__array_record_output(i64 1, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret i64 0
        }

        declare void @__quantum__qis__barrier__body()

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
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
fn qasm2_barrier_can_be_called_on_two_qubit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[2];
        barrier q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        __quantum__qis__barrier__body();
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_cx_called_with_one_qubit_generates_error() {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[2];
        CX q[0];
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfQubitArgs

          x gate expects 2 qubit arguments, but 1 were provided
           ,-[Test.qasm:5:9]
         4 |         qreg q[2];
         5 |         CX q[0];
           :         ^^^^^^^^
         6 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn qasm2_cx_called_with_too_many_qubits_generates_error() {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[3];
        cx q[0], q[1], q[2];
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfQubitArgs

          x gate expects 2 qubit arguments, but 3 were provided
           ,-[Test.qasm:5:9]
         4 |         qreg q[3];
         5 |         cx q[0], q[1], q[2];
           :         ^^^^^^^^^^^^^^^^^^^^
         6 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn qasm2_rx_gate_with_no_angles_generates_error() {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        rx q;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfClassicalArgs

          x gate expects 1 classical arguments, but 0 were provided
           ,-[Test.qasm:5:9]
         4 |         qreg q[1];
         5 |         rx q;
           :         ^^^^^
         6 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn qasm2_rx_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        rx(2.0) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        rx(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_rx_gate_with_too_many_angles_generates_error() {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        rx(2.0, 3.0) q;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.InvalidNumberOfClassicalArgs

          x gate expects 1 classical arguments, but 2 were provided
           ,-[Test.qasm:5:9]
         4 |         qreg q[1];
         5 |         rx(2.0, 3.0) q;
           :         ^^^^^^^^^^^^^^^
         6 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn qasm2_implicit_cast_to_angle_works() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        rx(2.0) q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(1);
        rx(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_custom_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        gate my_gate q1, q2 {
            h q1;
            h q2;
        }

        qreg q[2];
        my_gate q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        operation my_gate(q1 : Qubit, q2 : Qubit) : Unit is Adj + Ctl {
            h(q1);
            h(q2);
        }
        let q = QIR.Runtime.AllocateQubitArray(2);
        my_gate(q[0], q[1]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_simulatable_intrinsic_on_gate_stmt_generates_correct_qir()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        #pragma qdk.qir.profile Adaptive_RI

        @SimulatableIntrinsic
        gate my_gate q {
            x q;
        }

        qreg q[1];
        my_gate q;
        creg result[1];
        result = measure q;
    "#;

    let qsharp = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @my_gate(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__array_record_output(i64 1, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret i64 0
        }

        declare void @my_gate(%Qubit*)

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
    "#]].assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_qdk_qir_intrinsic_on_gate_stmt_generates_correct_qir() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        #pragma qdk.qir.profile Adaptive_RI

        @qdk.qir.intrinsic
        gate my_gate q {
            x q;
        }

        qreg q[1];
        my_gate q;
        creg result[1];
        result = measure q;
    "#;

    let qsharp = compile_qasm_to_qir(source)?;
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        define void @ENTRYPOINT__main() #0 {
        block_0:
          call void @my_gate(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__array_record_output(i64 1, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret void
        }

        declare void @my_gate(%Qubit*)

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
    "#]].assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_rxx_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[2];
        rxx(2.0) q[1], q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        rxx(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[1], q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_ryy_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[2];
        ryy(2.0) q[1], q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        ryy(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[1], q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_rzz_gate_with_one_angle_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[2];
        rzz(2.0) q[1], q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        rzz(new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        }, q[1], q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_all_qiskit_stdgates_can_be_called_included() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[4];
        rxx(pi / 2.0) q[1], q[0];
        ryy(pi / 2.0) q[1], q[0];
        rzz(pi / 2.0) q[1], q[0];
        dcx q[0], q[1];
        ecr q[0], q[1];
        r(pi / 2.0, pi / 4.0) q[1];
        rzx(pi / 2.0) q[1], q[0];
        cs q[0], q[1];
        csdg q[0], q[1];
        sxdg q[0];
        csx q[0], q[1];
        cu1(pi / 2.0) q[1], q[0];
        cu3(pi / 2.0, pi / 4.0, pi / 8.0) q[1], q[0];
        rccx q[0], q[1], q[2];
        c3sqrtx q[0], q[1], q[2], q[3];
        c3x q[0], q[1], q[2], q[3];
        rc3x q[0], q[1], q[2], q[3];
        xx_minus_yy(pi / 2.0, pi / 4.0) q[1], q[0];
        xx_plus_yy(pi / 2.0, pi / 4.0) q[1], q[0];
        ccz q[0], q[1], q[2];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(4);
        rxx(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        ryy(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        rzz(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        dcx(q[0], q[1]);
        ecr(q[0], q[1]);
        r(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), q[1]);
        rzx(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        cs(q[0], q[1]);
        csdg(q[0], q[1]);
        sxdg(q[0]);
        csx(q[0], q[1]);
        cu1(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), q[1], q[0]);
        cu3(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 8., 53), q[1], q[0]);
        rccx(q[0], q[1], q[2]);
        c3sqrtx(q[0], q[1], q[2], q[3]);
        c3x(q[0], q[1], q[2], q[3]);
        rc3x(q[0], q[1], q[2], q[3]);
        xx_minus_yy(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), q[1], q[0]);
        xx_plus_yy(Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 2., 53), Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI() / 4., 53), q[1], q[0]);
        ccz(q[0], q[1], q[2]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_broadcast_one_qubit_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg qs[2];
        h qs;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let qs = QIR.Runtime.AllocateQubitArray(2);
        h(qs[0]);
        h(qs[1]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_broadcast_two_qubit_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg ctrls[3];
        qreg targets[3];
        cx ctrls, targets;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctrls = QIR.Runtime.AllocateQubitArray(3);
        let targets = QIR.Runtime.AllocateQubitArray(3);
        cx(ctrls[0], targets[0]);
        cx(ctrls[1], targets[1]);
        cx(ctrls[2], targets[2]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn qasm2_broadcast_with_different_register_sizes_fails() {
    let source = r#"
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg ctrls[3];
        qreg targets[2];
        CX ctrls, targets;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.BroadcastCallQuantumArgsDisagreeInSize

          x first quantum register is of type qubit[3] but found an argument of type
          | qubit[2]
           ,-[Test.qasm:6:19]
         5 |         qreg targets[2];
         6 |         CX ctrls, targets;
           :                   ^^^^^^^
         7 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}
