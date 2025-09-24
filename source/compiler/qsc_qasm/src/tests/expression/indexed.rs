// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{check_qasm_to_qsharp, compile_qasm_to_qsharp};

use expect_test::expect;
use miette::Report;

#[test]
fn indexed_bit_can_be_implicitly_converted_to_float() {
    let source = "
        bit[5] x;
        if (x[0] == 1.) {
        }
    ";
    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = [Zero, Zero, Zero, Zero, Zero];
        if Std.OpenQASM.Convert.ResultAsDouble(x[0]) == 1. {};
    "#]],
    );
}

#[test]
fn indexed_bit_can_implicitly_convert_to_int() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit[5] x;
        if (x[0] == 1) {
            x[1] = 1;
        }
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = [Zero, Zero, Zero, Zero, Zero];
        if Std.OpenQASM.Convert.ResultAsInt(x[0]) == 1 {
            set x[1] = One;
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn indexed_bit_can_implicitly_convert_to_bool() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit[5] x;
        if (x[0]) {
            x[1] = 1;
        }
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = [Zero, Zero, Zero, Zero, Zero];
        if Std.OpenQASM.Convert.ResultAsBool(x[0]) {
            set x[1] = One;
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bit_indexed_ty_is_same_as_element_ty() -> miette::Result<(), Vec<Report>> {
    let source = "
        bit[5] x;
        bit y = x[0];
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = [Zero, Zero, Zero, Zero, Zero];
        mutable y = x[0];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bool_indexed_ty_is_same_as_element_ty() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[bool, 5] x;
        bool y = x[0];
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable x = [false, false, false, false, false];
        mutable y = x[0];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bitstring_slicing() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        bit[5] ans = "10101";
        qubit qq;
        if(ans[0:3] == 4) x qq;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable ans = [One, Zero, One, Zero, One];
        let qq = QIR.Runtime.__quantum__rt__qubit_allocate();
        if Std.OpenQASM.Convert.ResultArrayAsIntBE(ans[0..3]) == 4 {
            x(qq);
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn bitstring_slicing_with_step() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        bit[5] ans = "10101";
        qubit qq;
        if(ans[0:3:2] == 4) x qq;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable ans = [One, Zero, One, Zero, One];
        let qq = QIR.Runtime.__quantum__rt__qubit_allocate();
        if Std.OpenQASM.Convert.ResultArrayAsIntBE(ans[0..3..2]) == 4 {
            x(qq);
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn index_set_in_non_alias_stmt_fails() {
    let source = r#"
        include "stdgates.inc";
        bit[5] ans = "10101";
        qubit qq;
        if(ans[{1, 3}] == 4) x qq;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.IndexSetOnlyAllowedInAliasStmt

          x index sets are only allowed in alias statements
           ,-[Test.qasm:5:16]
         4 |         qubit qq;
         5 |         if(ans[{1, 3}] == 4) x qq;
           :                ^^^^^^
         6 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn indexed_ident_with_omitted_start() {
    let source = r#"
        array[int, 5] a;
        a[:3];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [0, 0, 0, 0, 0];
            a[...3];
        "#]],
    );
}

#[test]
fn indexed_ident_with_omitted_stop() {
    let source = r#"
        array[int, 5] a;
        a[2:];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [0, 0, 0, 0, 0];
        a[2...];
    "#]],
    );
}

#[test]
fn indexed_uint() {
    let source = r#"
        uint[4] a = 0b1111;
        a[1];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 15;
            Std.OpenQASM.Convert.IntAsResultArrayBE(a, 4)[2];
        "#]],
    );
}

#[test]
fn indexed_const_uint() {
    let source = r#"
        const uint[4] a = 0b1111;
        const bit b = a[1];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 15;
        let b = One;
    "#]],
    );
}

#[test]
fn indexed_uint_with_step() {
    let source = r#"
        uint[4] a = 0b1111;
        a[0:2:];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 15;
            Std.OpenQASM.Convert.IntAsResultArrayBE(a, 4)[3..-2..0];
        "#]],
    );
}

#[test]
fn indexed_const_uint_with_step() {
    let source = r#"
        const uint[4] a = 0b1011;
        const bit[2] b = a[0:2:];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            let a = 11;
            let b = [Zero, One];
        "#]],
    );
}

#[test]
fn indexed_angle() {
    let source = r#"
        angle[4] a = pi;
        a[1];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI(), 4);
            Std.OpenQASM.Angle.AngleAsResultArrayBE(a)[2];
        "#]],
    );
}

#[test]
fn indexed_const_angle() {
    let source = r#"
        const angle[4] a = pi;
        const bit b = a[1];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            let a = new Std.OpenQASM.Angle.Angle {
                Value = 4503599627370496,
                Size = 53
            };
            let b = Zero;
        "#]],
    );
}

#[test]
fn indexed_angle_with_step() {
    let source = r#"
        angle[4] a = pi;
        a[0:2:];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Std.OpenQASM.Angle.DoubleAsAngle(Std.Math.PI(), 4);
            Std.OpenQASM.Angle.AngleAsResultArrayBE(a)[3..-2..0];
        "#]],
    );
}

#[test]
fn indexed_const_angle_with_step() {
    let source = r#"
        const angle[4] a = pi;
        const bit[2] b = a[0:2:];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            let a = new Std.OpenQASM.Angle.Angle {
                Value = 4503599627370496,
                Size = 53
            };
            let b = [Zero, Zero];
        "#]],
    );
}

#[test]
fn index_into_array_and_then_into_int() {
    let source = r#"
        array[int[4], 3] a = {1, 2, 3};
        bit b = a[1][1];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [1, 2, 3];
            mutable b = Std.OpenQASM.Convert.IntAsResultArrayBE(a[1], 4)[2];
        "#]],
    );
}

#[test]
fn negative_index_edge_case_succeeds() {
    let source = r#"
        const bit[4] a = "1010";
        const bit b = a[-4];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            let a = [One, Zero, One, Zero];
            let b = One;
        "#]],
    );
}

#[test]
fn negative_index_out_of_bounds_errors() {
    let source = r#"
        const bit[4] a = "1010";
        const bit b = a[-5];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.IndexOutOfBounds

              x index must be in the range [-4, 3] but it was -5
               ,-[Test.qasm:3:26]
             2 |         const bit[4] a = "1010";
             3 |         const bit b = a[-5];
               :                          ^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn positive_index_edge_case_succeeds() {
    let source = r#"
        const bit[4] a = "1010";
        const bit b = a[3];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            let a = [One, Zero, One, Zero];
            let b = Zero;
        "#]],
    );
}

#[test]
fn positive_index_out_of_bounds_errors() {
    let source = r#"
        const bit[4] a = "1010";
        const bit b = a[4];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.IndexOutOfBounds

              x index must be in the range [-4, 3] but it was 4
               ,-[Test.qasm:3:25]
             2 |         const bit[4] a = "1010";
             3 |         const bit b = a[4];
               :                         ^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn const_zero_step_errors() {
    let source = r#"
        const bit[4] a = "1010";
        const bit b = a[:"0":];
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            Qasm.Lowerer.ZeroStepInRange

              x range step cannot be zero
               ,-[Test.qasm:3:25]
             2 |         const bit[4] a = "1010";
             3 |         const bit b = a[:"0":];
               :                         ^^^^^
             4 |     
               `----
        "#]],
    );
}
