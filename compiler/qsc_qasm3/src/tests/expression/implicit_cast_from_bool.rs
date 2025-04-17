// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use miette::Report;

use crate::tests::compile_qasm_to_qsharp;

#[test]
fn to_bit_and_back_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.0;
        input bool a;
        bit _bit0;
        bit _bit1;
        _bit0 = true;
        _bit1 = a;
        _bit0 = _bit1;
        _bit0 = _bit1;
        a = _bit1;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable _bit0 = Zero;
        mutable _bit1 = Zero;
        set _bit0 = One;
        set _bit1 = __BoolAsResult__(a);
        set _bit0 = _bit1;
        set _bit0 = _bit1;
        set a = __ResultAsBool__(_bit1);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_bit_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        bit y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsResult__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_int_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        int y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsInt__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_int_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        int[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsInt__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_uint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        uint y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsInt__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_uint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        uint[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsInt__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_bigint_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        int[65] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsBigInt__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_implicit_float_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        float y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsDouble__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn to_explicit_float_implicitly() -> miette::Result<(), Vec<Report>> {
    let source = "
        bool x = true;
        float[32] y = x;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        mutable x = true;
        mutable y = __BoolAsDouble__(x);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
