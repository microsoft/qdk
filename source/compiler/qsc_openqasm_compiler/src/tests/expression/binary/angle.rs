// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_stmt_to_qsharp;

use expect_test::expect;
use miette::Report;

// Bit shift
#[test]
fn shl() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        uint b = 2;
        angle x = a << b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.__AngleShl__(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn shr() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        uint b = 2;
        angle x = a >> b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleShr(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Bitwise

#[test]
fn andb() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        angle x = a & b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleAndB(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn orb() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        angle x = a | b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleOrB(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn xorb() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        angle x = a ^ b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleXorB(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Comparison

#[test]
fn eq() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        bool x = a == b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleEq(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn neq() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        bool x = a != b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleNeq(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn gt() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        bool x = a > b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleGt(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn gte() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        bool x = a >= b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleGte(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn lt() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        bool x = a < b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleLt(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn lte() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        bool x = a <= b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AngleLte(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Arithmetic

#[test]
fn addition() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        angle x = a + b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.AddAngles(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn subtraction() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        angle x = a - b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.SubtractAngles(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn multiplication_by_uint() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        uint[32] b = 2;
        angle[32] x = a * b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.MultiplyAngleByInt(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn division_by_uint() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        uint[32] b = 2;
        angle x = a / b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.DivideAngleByInt(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn division_by_angle() -> miette::Result<(), Vec<Report>> {
    let source = "
        angle[32] a = 1.0;
        angle[32] b = 2.0;
        uint x = a / b;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.OpenQASM.Angle.DivideAngleByAngle(a, b);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
