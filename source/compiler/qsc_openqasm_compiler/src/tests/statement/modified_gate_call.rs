// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn adj_x_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        inv @ x q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        Adjoint x(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn adj_adj_x_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        inv @ inv @ x q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        Adjoint Adjoint x(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiple_controls_on_x_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[3] q;
        qubit f;
        ctrl(3) @ x q[1], q[0], q[2], f;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(3);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        Controlled x([q[1], q[0], q[2]], f);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn repeated_multi_controls_on_x_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[2] q;
        qubit[3] r;
        qubit f;
        ctrl(2) @ ctrl(3) @ x q[1], r[0], q[0], f, r[1], r[2];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        let r = QIR.Runtime.AllocateQubitArray(3);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        Controlled Controlled x([q[1], r[0]], ([q[0], f, r[1]], r[2]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn repeated_multi_controls_on_x_gate_can_be_mixed_with_inv() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[2] q;
        qubit[3] r;
        qubit f;
        ctrl(2) @ inv @ ctrl(3) @ inv @ x q[1], r[0], q[0], f, r[1], r[2];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(2);
        let r = QIR.Runtime.AllocateQubitArray(3);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        Controlled Adjoint Controlled Adjoint x([q[1], r[0]], ([q[0], f, r[1]], r[2]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiple_controls_on_cx_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[4] q;
        qubit f;
        ctrl(3) @ cx q[1], q[0], q[2], f, q[3];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(4);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        Controlled cx([q[1], q[0], q[2]], (f, q[3]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiple_controls_on_crx_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[4] q;
        qubit f;
        ctrl(3) @ inv @ crx(0.5) q[1], q[0], q[2], f, q[3];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(4);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        Controlled Adjoint crx([q[1], q[0], q[2]], (new Std.OpenQASM.Angle.Angle {
            Value = 716770142402832,
            Size = 53
        }, f, q[3]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn neg_ctrl_can_be_applied_and_wrapped_in_another_modifier() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[4] q;
        qubit f;
        inv @ negctrl(3) @ inv @ crx(0.5) q[1], q[0], q[2], f, q[3];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(4);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        Adjoint ApplyControlledOnInt(0, Adjoint crx, [q[1], q[0], q[2]], (new Std.OpenQASM.Angle.Angle {
            Value = 716770142402832,
            Size = 53
        }, f, q[3]));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn neg_ctrl_can_wrap_another_neg_crtl_modifier() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[6] q;
        qubit f;
        negctrl(3) @ negctrl(2) @ crx(0.5) q[1], q[0], q[2], q[3], q[4], f, q[5];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(6);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        ApplyControlledOnInt(0, ApplyControlledOnInt, [q[1], q[0], q[2]], (0, crx, [q[3], q[4]], (new Std.OpenQASM.Angle.Angle {
            Value = 716770142402832,
            Size = 53
        }, f, q[5])));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn modifiers_can_be_repeated_many_times() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit[6] q;
        qubit f;
        pow(1) @ pow(1) @ pow(1) @ crx(0.5) f, q[5];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.AllocateQubitArray(6);
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        ApplyOperationPowerA(1, ApplyOperationPowerA, (1, ApplyOperationPowerA, (1, crx, (new Std.OpenQASM.Angle.Angle {
            Value = 716770142402832,
            Size = 53
        }, f, q[5]))));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn pow_can_be_applied_on_a_simple_gate() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit f;
        pow(2) @ x f;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let f = QIR.Runtime.__quantum__rt__qubit_allocate();
        ApplyOperationPowerA(2, x, (f));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
