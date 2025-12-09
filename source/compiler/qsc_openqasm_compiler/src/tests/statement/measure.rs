// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{check_qasm_to_qsharp, compile_qasm_to_qsharp};
use expect_test::expect;
use miette::Report;
use std::fmt::Write;

#[test]
fn single_qubit_can_be_measured_into_single_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit c;
        qubit q;
        c = measure q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = Zero;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        set c = Std.Intrinsic.M(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn single_qubit_can_be_arrow_measured_into_single_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit c;
        qubit q;
        measure q -> c;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = Zero;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        set c = Std.Intrinsic.M(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn indexed_single_qubit_can_be_measured_into_indexed_bit_register()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[1] c;
        qubit[1] q;
        c[0] = measure q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = [Zero];
        let q = QIR.Runtime.AllocateQubitArray(1);
        set c[0] = Std.Intrinsic.M(q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn range_indexed_qubit_register_can_be_measured_into_indexed_bit_register()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[5] c;
        qubit[5] q;
        c[1:3] = measure q[2:4];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = [Zero, Zero, Zero, Zero, Zero];
        let q = QIR.Runtime.AllocateQubitArray(5);
        set c[1..3] = Std.Measurement.MeasureEachZ(q[2..4]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unindexed_single_qubit_can_be_measured_into_indexed_bit_register()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[1] c;
        qubit[1] q;
        c = measure q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = [Zero];
        let q = QIR.Runtime.AllocateQubitArray(1);
        set c = Std.Measurement.MeasureEachZ(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn measure_zero_length_qubits_into_register_fails() {
    let source = r#"
        bit[0] c;
        qubit[0] q;
        c = measure q;
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.ExprMustBePositiveInt

          x quantum register size must be a positive integer
           ,-[Test.qasm:3:15]
         2 |         bit[0] c;
         3 |         qubit[0] q;
           :               ^
         4 |         c = measure q;
           `----
    "#]],
    );
}

#[test]
fn measuring_register_into_bit_is_converted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit c;
        qubit[1] q;
        c = measure q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = Zero;
        let q = QIR.Runtime.AllocateQubitArray(1);
        set c = Std.OpenQASM.Convert.ResultArrayAsResultBE(Std.Measurement.MeasureEachZ(q));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn indexed_single_qubit_can_be_measured_into_single_bit_register() -> miette::Result<(), Vec<Report>>
{
    let source = r#"
        bit c;
        qubit[1] q;
        c = measure q[0];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = Zero;
        let q = QIR.Runtime.AllocateQubitArray(1);
        set c = Std.Intrinsic.M(q[0]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn measuring_hardware_qubits_generates_an_error() {
    let source = r#"
        bit c;
        c = measure $2;
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("Measuring HW qubit should have generated an error");
    };

    let mut errs_string = String::new();

    for err in errs {
        writeln!(&mut errs_string, "{err:?}").expect("");
    }

    expect![[r#"
        Qasm.Compiler.NotSupported

          x hardware qubit operands are not supported
           ,-[Test.qasm:3:21]
         2 |         bit c;
         3 |         c = measure $2;
           :                     ^^
         4 |     
           `----

    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn value_from_measurement_can_be_dropped() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        qubit q;
        measure q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        Std.Intrinsic.M(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn range_indexed_qubit_register_measure_arrow_into_indexed_bit_register()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[5] c;
        qubit[5] q;
        measure q[2:4] -> c[1:3];
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = [Zero, Zero, Zero, Zero, Zero];
        let q = QIR.Runtime.AllocateQubitArray(5);
        set c[1..3] = Std.Measurement.MeasureEachZ(q[2..4]);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unindexed_single_qubit_with_measure_arrow_into_indexed_bit_register()
-> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[1] c;
        qubit[1] q;
        measure q -> c;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = [Zero];
        let q = QIR.Runtime.AllocateQubitArray(1);
        set c = Std.Measurement.MeasureEachZ(q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn measure_arrow_zero_length_qubits_into_register_fails() {
    let source = r#"
        bit[0] c;
        qubit[0] q;
        measure q -> c;
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.ExprMustBePositiveInt

              x quantum register size must be a positive integer
               ,-[Test.qasm:3:15]
             2 |         bit[0] c;
             3 |         qubit[0] q;
               :               ^
             4 |         measure q -> c;
               `----
        "#]],
    );
}

#[test]
fn measuring_arrow_register_into_bit_fails_with_type_error() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit c;
        qubit[1] q;
        measure q -> c;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable c = Zero;
        let q = QIR.Runtime.AllocateQubitArray(1);
        set c = Std.OpenQASM.Convert.ResultArrayAsResultBE(Std.Measurement.MeasureEachZ(q));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
