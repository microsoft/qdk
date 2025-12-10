// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn cy_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        cy ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        cy(ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cz_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        cz ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        cz(ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn ch_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        ch ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        ch(ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn sdg_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        sdg q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        sdg(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn tdg_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        tdg q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        tdg(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn crx_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        crx(0.5) ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        crx(new Std.OpenQASM.Angle.Angle {
            Value = 716770142402832,
            Size = 53
        }, ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cry_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        cry(0.5) ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        cry(new Std.OpenQASM.Angle.Angle {
            Value = 716770142402832,
            Size = 53
        }, ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn crz_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        crz(0.5) ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        crz(new Std.OpenQASM.Angle.Angle {
            Value = 716770142402832,
            Size = 53
        }, ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cswap_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit[2] q;
        cswap ctl, q[0], q[1];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let q = QIR.Runtime.AllocateQubitArray(2);
        cswap(ctl, q[0], q[1]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn legacy_cx_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        CX ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        CX(ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn legacy_cphase_gate_can_be_called() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit ctl;
        qubit target;
        cphase(1.0) ctl, target;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let ctl = QIR.Runtime.__quantum__rt__qubit_allocate();
        let target = QIR.Runtime.__quantum__rt__qubit_allocate();
        cphase(new Std.OpenQASM.Angle.Angle {
            Value = 1433540284805665,
            Size = 53
        }, ctl, target);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
