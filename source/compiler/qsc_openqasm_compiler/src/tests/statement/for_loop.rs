// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn for_loops_can_iterate_over_discrete_set() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        for int i in {1, 5, 10} {
            sum += i;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        for i : Int in [1, 5, 10] {
            set sum = sum + i;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn for_loops_can_have_stmt_bodies() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        for int i in {1, 5, 10}
            sum += i;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        for i : Int in [1, 5, 10] {
            set sum = sum + i;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn for_loops_can_iterate_over_range() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        for int i in [0:2:20] {
            sum += i;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        for i : Int in 0..2..20 {
            set sum = sum + i;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn for_loops_can_iterate_float_set() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        float sum = 0.;
        for float[64] f in {1.2, -3.4, 0.5, 9.8} {
            sum += f;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0.;
        for f : Double in [1.2, -3.4, 0.5, 9.8] {
            set sum = sum + f;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn for_loops_can_iterate_float_array_symbol() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        float sum = 0.;
        array[float[64], 4] my_floats = {1.2, -3.4, 0.5, 9.8};
        for float[64] f in my_floats {
            sum += f;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0.;
        mutable my_floats = [1.2, -3.4, 0.5, 9.8];
        for f : Double in my_floats {
            set sum = sum + f;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Spec says these should work, but they fail to parse:
// bit[5] register;
// for b in register {}
// let alias = register[1:3];
// for b in alias {}?
#[test]
fn for_loops_can_iterate_bit_register() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        const bit[5] reg = "10101";
        for bit b in reg {
            sum += b;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        let reg = [One, Zero, One, Zero, One];
        for b : Result in reg {
            set sum = sum + Std.OpenQASM.Convert.ResultAsInt(b);
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn for_loops_can_iterate_const_bit_register() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        bit[5] reg = "10101";
        for bit b in reg {
            sum += b;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        mutable reg = [One, Zero, One, Zero, One];
        for b : Result in reg {
            set sum = sum + Std.OpenQASM.Convert.ResultAsInt(b);
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn loop_variables_should_be_scoped_to_for_loop() {
    let source = r#"
        int sum = 0;
        for int i in {1, 5, 10} { }
        sum += i;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect!["undefined symbol: i"].assert_eq(&errors[0].to_string());
}
