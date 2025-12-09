// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp_operation;
use expect_test::expect;
use miette::Report;

#[test]
fn bit_array_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
OPENQASM 3.0;
input bit[2] c;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(c : Result[]) : Unit {
            if Std.Core.Length(c) != 2 {
                fail "Argument `c` is not compatible with its OpenQASM type `bit[2]`."
            };
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bit_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
OPENQASM 3.0;
input bit c;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(c : Result) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bool_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input bool c;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(c : Bool) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn complex_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input complex[float] c;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(c : Std.Math.Complex) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn float_implicit_bitness_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input float f;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(f : Double) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn float_explicit_bitness_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input float[60] f;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(f : Double) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn int_implicit_bitness_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input int i;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(i : Int) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn int_explicit_bitness_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input int[60] i;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(i : Int) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn uint_implicit_bitness_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input uint i;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(i : Int) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn uint_explicit_bitness_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input uint[60] i;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(i : Int) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bigint_explicit_bitness_is_lifted() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input int[65] i;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(i : BigInt) : Unit {
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn lifting_qubit_raises_parse_error() {
    let source = r#"
input qubit q;
"#;

    let Err(error) = compile_qasm_to_qsharp_operation(source) else {
        panic!("Expected error")
    };

    assert!(
        error[0]
            .to_string()
            .contains("expected scalar or array type, found keyword `qubit`")
    );
}

#[test]
fn lifting_angle_raises_compile_error() {
    let source = r#"
input angle a;
"#;

    let Err(error) = compile_qasm_to_qsharp_operation(source) else {
        panic!("Expected error")
    };

    assert_eq!(
        error[0].to_string(),
        "use `float` types for passing input, using `angle` types are not supported"
    );
}

#[test]
fn order_is_preserved_with_multiple_inputs() -> miette::Result<(), Vec<Report>> {
    let source = r#"
input int[65] bi;
input int[6] i;
input uint[60] ui;
input uint u;
input float f;
input bool b;
input bit c;
input complex[float] cf;
input bit[2] b2;
"#;

    let qsharp = compile_qasm_to_qsharp_operation(source)?;
    expect![[r#"
        operation Test(bi : BigInt, i : Int, ui : Int, u : Int, f : Double, b : Bool, c : Result, cf : Std.Math.Complex, b2 : Result[]) : Unit {
            if Std.Core.Length(b2) != 2 {
                fail "Argument `b2` is not compatible with its OpenQASM type `bit[2]`."
            };
            import Std.OpenQASM.Intrinsic.*;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
