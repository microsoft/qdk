// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_stmt_to_qsharp;

use expect_test::expect;
use miette::Report;

#[test]
fn implicit_bitness_default_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Complex(0., 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn explicit_bitness_default_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float[42]] x;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Complex(0., 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_double_img_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float] x = 1.01im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(0., 1.01);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_img_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float] x = 1im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(0., 1.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_explicit_bitness_double_img_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float[42]] x = 1.01im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(0., 1.01);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_explicit_bitness_int_img_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float[42]] x = 1im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(0., 1.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_double_img_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = 1.01im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Complex(0., 1.01);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_img_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = 1im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Complex(0., 1.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// This test is based on spec decls which show this exact case
#[test]
fn implicit_bitness_int_img_only_tab_between_suffix_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = 1	im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Complex(0., 1.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_double_real_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float] x = 1.01;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(1.01, 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_int_real_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float] x = 1;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(1., 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_double_real_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = 1.01;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Complex(1.01, 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_int_real_only_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = 1;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.Complex(1., 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_simple_double_pos_im_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = 1.1 + 2.2im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.PlusC(Std.Math.Complex(1.1, 0.), Std.Math.Complex(0., 2.2));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_simple_double_pos_im_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float] x = 1.1 + 2.2im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(1.1, 2.2);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_simple_double_neg_im_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = 1.1 - 2.2im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.MinusC(Std.Math.Complex(1.1, 0.), Std.Math.Complex(0., 2.2));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_simple_double_neg_im_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float] x = 1.1 - 2.2im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(1.1, -2.2);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn implicit_bitness_simple_double_neg_real_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        complex[float] x = -1.1 + 2.2im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        mutable x = Std.Math.PlusC(Std.Math.Complex(-1.1, 0.), Std.Math.Complex(0., 2.2));
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_implicit_bitness_simple_double_neg_real_decl() -> miette::Result<(), Vec<Report>> {
    let source = "
        const complex[float] x = -1.1 + 2.2im;
    ";

    let qsharp = compile_qasm_stmt_to_qsharp(source)?;
    expect![[r#"
        let x = Std.Math.Complex(-1.1, 2.2);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
