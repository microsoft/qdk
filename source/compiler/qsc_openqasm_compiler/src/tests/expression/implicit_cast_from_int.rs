// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use miette::Report;

use crate::tests::compile_qasm_to_qsharp;

#[test]
fn to_bit_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        bit y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = Std.OpenQASM.Convert.IntAsResult(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_bool_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        bool y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = if x == 0 {
            false
        } else {
            true
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_int_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        int y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = x;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_int_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        int[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = x;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_uint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        uint y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = x;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_uint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        uint[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = x;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_bigint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        int[65] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = Std.Convert.IntAsBigInt(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_float_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        float y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = Std.Convert.IntAsDouble(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_float_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        float[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = Std.Convert.IntAsDouble(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_complex_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        complex[float] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = Std.Math.Complex(Std.Convert.IntAsDouble(x), 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_complex_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        int x = 42;
        complex[float[32]] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = 42;
        mutable y = Std.Math.Complex(Std.Convert.IntAsDouble(x), 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn from_const_0_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        include \"stdgates.inc\";
        qubit q;
        rx(0) q;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let q = QIR.Runtime.__quantum__rt__qubit_allocate();
        rx(new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, q);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
