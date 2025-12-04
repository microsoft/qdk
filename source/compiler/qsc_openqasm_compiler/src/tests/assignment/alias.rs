// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{compile_fragments, compile_qasm_to_qsharp, fail_on_compilation_errors};
use expect_test::expect;
use miette::Report;

#[test]
#[ignore = "unimplemented"]
fn classical() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit[2] a;
        bit[2] b;
        let c = a[{0,1}] ++ b[1:2];
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
#[ignore = "unimplemented"]
fn quantum() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[5] q1;
        qubit[7] q2;
        let q = q1 ++ q2;
        let c = a[{0,1}] ++ b[1:2];
        let qq = q1[{1,3,4}];
        let qqq = qq ++ q2[1:2:6];
        let d = c;
        let e = d[1];
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
#[ignore = "qasm parser does not support old-style decls yet"]
fn classical_old_style_decls() -> miette::Result<(), Vec<Report>> {
    let source = "
        creg a[2];
        creg b[2];
        let c = a[{0,1}] ++ b[1:2];
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
#[ignore = "qasm parser does not support old-style decls yet"]
fn quantum_old_style_decls() -> miette::Result<(), Vec<Report>> {
    let source = "
        qreg q1[5];
        qreg q2[7];
        let q = q1 ++ q2;
        let c = a[{0,1}] ++ b[1:2];
        let qq = q1[{1,3,4}];
        let qqq = qq ++ q2[1:2:6];
        let d = c;
        let e = d[1];
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
fn can_alias_qubit_registers() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[2] one;
        qubit[10] two;
        // Aliased register of twelve qubits
        let concatenated = one ++ two;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let one = QIR.Runtime.AllocateQubitArray(2);
        let two = QIR.Runtime.AllocateQubitArray(10);
        let concatenated = one + two;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn first_qubit_from_aliased_qreg() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[2] one;
        qubit[10] two;
        let concatenated = one ++ two;
        // First qubit in aliased qubit array
        let first = concatenated[0];
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let one = QIR.Runtime.AllocateQubitArray(2);
        let two = QIR.Runtime.AllocateQubitArray(10);
        let concatenated = one + two;
        let first = concatenated[0];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn last_qubit_from_aliased_qreg() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[2] one;
        qubit[10] two;
        let concatenated = one ++ two;
        // Last qubit in aliased qubit array
        let last = concatenated[-1];
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let one = QIR.Runtime.AllocateQubitArray(2);
        let two = QIR.Runtime.AllocateQubitArray(10);
        let concatenated = one + two;
        let last = concatenated[-1];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
#[ignore = "index sets not yet implemented"]
fn alias_idividual_qubits_from_qreg() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[10] two;
        // Qubits zero, three and five
        let qubit_selection = two[{0, 3, 5}];
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"

    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn alias_range_of_qubits_from_qreg() -> miette::Result<(), Vec<Report>> {
    let source = "
        qubit[2] one;
        qubit[10] two;
        let concatenated = one ++ two;
        let every_second = concatenated[0:2:11];
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let one = QIR.Runtime.AllocateQubitArray(2);
        let two = QIR.Runtime.AllocateQubitArray(10);
        let concatenated = one + two;
        let every_second = concatenated[0..2..11];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
