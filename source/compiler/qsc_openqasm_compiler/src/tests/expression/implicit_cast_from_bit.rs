// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use miette::Report;

use crate::tests::{check_qasm_to_qsharp, compile_qasm_to_qsharp};

#[test]
fn to_bool_and_back_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        input bit a;
        bool _bool0;
        bool _bool1;
        _bool0 = true;
        _bool1 = a;
        _bool0 = _bool1;
        _bool0 = _bool1;
        a = _bool1;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable _bool0 = false;
        mutable _bool1 = false;
        set _bool0 = true;
        set _bool1 = Std.OpenQASM.Convert.ResultAsBool(a);
        set _bool0 = _bool1;
        set _bool0 = _bool1;
        set a = Std.OpenQASM.Convert.BoolAsResult(_bool1);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_bool_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit x = 1;
        bool y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = One;
        mutable y = Std.OpenQASM.Convert.ResultAsBool(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_int_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit x = 1;
        int y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = One;
        mutable y = Std.OpenQASM.Convert.ResultAsInt(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_int_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit x = 1;
        int[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = One;
        mutable y = Std.OpenQASM.Convert.ResultAsInt(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_uint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit x = 1;
        uint y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = One;
        mutable y = Std.OpenQASM.Convert.ResultAsInt(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_uint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit x = 1;
        uint[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = One;
        mutable y = Std.OpenQASM.Convert.ResultAsInt(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_bigint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit x = 1;
        int[65] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = One;
        mutable y = Std.OpenQASM.Convert.ResultAsBigInt(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_float_implicitly() {
    let source = "
        bit x = 1;
        float y = x;
    ";
    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = One;
        mutable y = Std.OpenQASM.Convert.ResultAsDouble(x);
    "#]],
    );
}
