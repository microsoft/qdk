// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests in this file need to check that const exprs are
//! evaluatable at lowering time. To do that we use them in
//! contexts where they need to be const-evaluated, like array
//! sizes or type widths.

use crate::tests::{check_qasm_to_qsharp, compile_qasm_to_qsharp};
use expect_test::expect;
use miette::Report;

#[test]
fn const_exprs_are_eagerly_evaluated() {
    let source = "
        const int a = 2;
        const int b = 3;
        const int c = a + b;
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 2;
        let b = 3;
        let c = 5;
    "#]],
    );
}

#[test]
fn const_exprs_work_in_bitarray_size_position() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 1;
        const int b = 2 + a;
        const int c = a + 3;
        bit[b] r1;
        bit[c] r2;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        let b = 3;
        let c = 4;
        mutable r1 = [Zero, Zero, Zero];
        mutable r2 = [Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn const_decl_with_non_const_init_expression_fails() {
    let source = r#"
        const int c = a + b;
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.UndefinedSymbol

          x undefined symbol: a
           ,-[Test.qasm:2:23]
         1 | 
         2 |         const int c = a + b;
           :                       ^
         3 |     
           `----

        Qasm.Lowerer.UndefinedSymbol

          x undefined symbol: b
           ,-[Test.qasm:2:27]
         1 | 
         2 |         const int c = a + b;
           :                           ^
         3 |     
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn const_exprs_implicit_cast_work_in_bitarray_size_position() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 1;
        const float b = 2.0 + a;
        const float c = a + 3.0;
        bit[b] r1;
        bit[c] r2;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        let b = 3.;
        let c = 4.;
        mutable r1 = [Zero, Zero, Zero];
        mutable r2 = [Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn non_const_exprs_fail_in_bitarray_size_position() {
    let source = r#"
        const int a = 1;
        int b = 2 + a;
        int c = a + 3;
        bit[b] r1;
        bit[c] r2;
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.ExprMustBeConst

          x expression must be const
           ,-[Test.qasm:5:13]
         4 |         int c = a + 3;
         5 |         bit[b] r1;
           :             ^
         6 |         bit[c] r2;
           `----

        Qasm.Lowerer.ExprMustBeConst

          x expression must be const
           ,-[Test.qasm:6:13]
         5 |         bit[b] r1;
         6 |         bit[c] r2;
           :             ^
         7 |     
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn can_assign_const_expr_to_non_const_decl() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 1;
        const int b = 2;
        int c = a + b;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        let b = 2;
        mutable c = a + b;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn ident_const() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 1;
        bit[a] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn indexed_ident() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[2] a = "01";
        bit[a[1]] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [Zero, One];
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// UnaryOp Float

#[test]
fn unary_op_neg_float() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const float a = -1.0;
        const float b = -a;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = -1.;
        let b = 1.;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unary_op_neg_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = -1;
        const int b = -a;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = -1;
        let b = 1;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unary_op_neg_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = -1.0;
        const bit b = a;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 7573658969935327,
            Size = 53
        };
        let b = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unary_op_negb_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint[3] a = 5;
        const uint[3] b = ~a;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 5;
        let b = 2;
        mutable r = [Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn unary_op_negb_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const bit b = ~a;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unary_op_negb_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit a = 0;
        const bit b = ~a;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = Zero;
        let b = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unary_op_negb_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[3] a = "101";
        const uint[3] b = ~a;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [One, Zero, One];
        let b = 2;
        mutable r = [Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// BinaryOp

#[test]
fn lhs_ty_equals_rhs_ty_assumption_holds() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 1;
        const float b = 2.0;
        const uint c = a + b;
        bit[c] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        let b = 2.;
        let c = 3;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// BinaryOp: Bit Shifts

// Shl

#[test]
fn binary_op_shl_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 1;
        const uint b = a << 2;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        let b = 4;
        mutable r = [Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shl_int_literal() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[1 << 3] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shl_overflow() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 1 << 65;
        def const_eval_context() {
            uint b = a;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 0;
        function const_eval_context() : Unit {
            mutable b = 0;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shl_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const angle[32] b = a << 2;
        const bit c = b;
        bit[c] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 2734261104,
            Size = 32
        };
        let c = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shl_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit a = 1;
        const bit b = a << 2;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = One;
        let b = Zero;
        mutable r = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shl_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[3] a = "101";
        const bit[3] b = a << 2;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [One, Zero, One];
        let b = [One, Zero, Zero];
        mutable r = [Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shl_creg_fails() {
    let source = r#"
        const creg a[3] = "101";
        const creg b[3] = a << 2;
        bit[b] r;
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Parser.Rule

          x expected scalar type, found keyword `creg`
           ,-[Test.qasm:2:15]
         1 | 
         2 |         const creg a[3] = "101";
           :               ^^^^
         3 |         const creg b[3] = a << 2;
           `----

        Qasm.Parser.Rule

          x expected scalar type, found keyword `creg`
           ,-[Test.qasm:3:15]
         2 |         const creg a[3] = "101";
         3 |         const creg b[3] = a << 2;
           :               ^^^^
         4 |         bit[b] r;
           `----

        Qasm.Lowerer.UndefinedSymbol

          x undefined symbol: b
           ,-[Test.qasm:4:13]
         3 |         const creg b[3] = a << 2;
         4 |         bit[b] r;
           :             ^
         5 |     
           `----
    "#]]
    .assert_eq(&errs_string);
}

// Shr

#[test]
fn binary_op_shr_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 5;
        const uint b = a >> 2;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 5;
        let b = 1;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shr_int_literal() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        bit[1 >> 3] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable r = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shr_overflow() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 1 >> 65;
        def const_eval_context() {
            uint b = a;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 0;
        function const_eval_context() : Unit {
            mutable b = 0;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shr_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const angle[32] b = a >> 2;
        const bit c = b;
        bit[c] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 170891319,
            Size = 32
        };
        let c = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shr_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit a = 1;
        const bit b = a >> 2;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = One;
        let b = Zero;
        mutable r = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shr_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[4] a = "1011";
        const bit[4] b = a >> 2;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [One, Zero, One, One];
        let b = [Zero, Zero, One, Zero];
        mutable r = [Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_shr_creg_fails() {
    let source = r#"
        const creg a[4] = "1011";
        const creg b[4] = a >> 2;
        bit[b] r;
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Parser.Rule

          x expected scalar type, found keyword `creg`
           ,-[Test.qasm:2:15]
         1 | 
         2 |         const creg a[4] = "1011";
           :               ^^^^
         3 |         const creg b[4] = a >> 2;
           `----

        Qasm.Parser.Rule

          x expected scalar type, found keyword `creg`
           ,-[Test.qasm:3:15]
         2 |         const creg a[4] = "1011";
         3 |         const creg b[4] = a >> 2;
           :               ^^^^
         4 |         bit[b] r;
           `----

        Qasm.Lowerer.UndefinedSymbol

          x undefined symbol: b
           ,-[Test.qasm:4:13]
         3 |         const creg b[4] = a >> 2;
         4 |         bit[b] r;
           :             ^
         5 |     
           `----
    "#]]
    .assert_eq(&errs_string);
}

// BinaryOp: Bitwise

// AndB

#[test]
fn binary_op_andb_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 5;
        const uint b = a & 6;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 5;
        let b = 4;
        mutable r = [Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_andb_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const angle[32] b = 2.0;
        const angle[32] c = a & b;
        const bit d = c;
        bit[d] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 1367130551,
            Size = 32
        };
        let c = new Std.OpenQASM.Angle.Angle {
            Value = 3948692,
            Size = 32
        };
        let d = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_andb_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit a = 1;
        const bit b = a & 0;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = One;
        let b = Zero;
        mutable r = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_andb_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[4] a = "1011";
        const bit[4] b = a & "0110";
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [One, Zero, One, One];
        let b = [Zero, Zero, One, Zero];
        mutable r = [Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// OrB

#[test]
fn binary_op_orb_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 5;
        const uint b = a | 6;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 5;
        let b = 7;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_orb_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const angle[32] b = 2.0;
        const angle[32] c = a | b;
        const bool d = c;
        bit[d] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 1367130551,
            Size = 32
        };
        let c = new Std.OpenQASM.Angle.Angle {
            Value = 2046747135,
            Size = 32
        };
        let d = true;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_orb_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit a = 1;
        const bit b = a | 0;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = One;
        let b = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_orb_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[3] a = "001";
        const bit[3] b = a | "100";
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [Zero, Zero, One];
        let b = [One, Zero, One];
        mutable r = [Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// XorB

#[test]
fn binary_op_xorb_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 5;
        const uint b = a ^ 6;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 5;
        let b = 3;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_xorb_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const angle[32] b = 2.0;
        const angle[32] c = a ^ b;
        const bit d = c;
        bit[d] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 1367130551,
            Size = 32
        };
        let c = new Std.OpenQASM.Angle.Angle {
            Value = 2042798443,
            Size = 32
        };
        let d = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_xorb_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit a = 1;
        const bit b = a ^ 1;
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = One;
        let b = Zero;
        mutable r = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_xorb_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[4] a = "1011";
        const bit[4] b = a ^ "1110";
        bit[b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [One, Zero, One, One];
        let b = [Zero, One, Zero, One];
        mutable r = [Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Binary Logical

#[test]
fn binary_op_andl_bool() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bool f = false;
        const bool t = true;
        bit[f && f] r1;
        bit[f && t] r2;
        bit[t && f] r3;
        bit[t && t] r4;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let f = false;
        let t = true;
        mutable r1 = [];
        mutable r2 = [];
        mutable r3 = [];
        mutable r4 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_orl_bool() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bool f = false;
        const bool t = true;
        bit[f || f] r1;
        bit[f || t] r2;
        bit[t || f] r3;
        bit[t || t] r4;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let f = false;
        let t = true;
        mutable r1 = [];
        mutable r2 = [Zero];
        mutable r3 = [Zero];
        mutable r4 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// BinaryOp: Comparison

// Eq

#[test]
fn binary_op_comparison_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 2;
        bit[a == a] r1;
        bit[a != a] r2;
        bit[a > a] r3;
        bit[a >= a] r4;
        bit[a < a] r5;
        bit[a <= a] r6;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 2;
        mutable r1 = [Zero];
        mutable r2 = [];
        mutable r3 = [];
        mutable r4 = [Zero];
        mutable r5 = [];
        mutable r6 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_comparison_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 2;
        bit[a == a] r1;
        bit[a != a] r2;
        bit[a > a] r3;
        bit[a >= a] r4;
        bit[a < a] r5;
        bit[a <= a] r6;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 2;
        mutable r1 = [Zero];
        mutable r2 = [];
        mutable r3 = [];
        mutable r4 = [Zero];
        mutable r5 = [];
        mutable r6 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_comparison_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 2.0;
        bit[a == a] r1;
        bit[a != a] r2;
        bit[a > a] r3;
        bit[a >= a] r4;
        bit[a < a] r5;
        bit[a <= a] r6;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 1367130551,
            Size = 32
        };
        mutable r1 = [Zero];
        mutable r2 = [];
        mutable r3 = [];
        mutable r4 = [Zero];
        mutable r5 = [];
        mutable r6 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_comparison_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit a = 1;
        bit[a == a] r1;
        bit[a != a] r2;
        bit[a > a] r3;
        bit[a >= a] r4;
        bit[a < a] r5;
        bit[a <= a] r6;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = One;
        mutable r1 = [Zero];
        mutable r2 = [];
        mutable r3 = [];
        mutable r4 = [Zero];
        mutable r5 = [];
        mutable r6 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_comparison_bitarray() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bit[2] a = "10";
        bit[a == a] r1;
        bit[a != a] r2;
        bit[a > a] r3;
        bit[a >= a] r4;
        bit[a < a] r5;
        bit[a <= a] r6;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = [One, Zero];
        mutable r1 = [Zero];
        mutable r2 = [];
        mutable r3 = [];
        mutable r4 = [Zero];
        mutable r5 = [];
        mutable r6 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// BinaryOp: Arithmetic

// Add

#[test]
fn binary_op_add_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 1;
        const int b = 2;
        bit[a + b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        let b = 2;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_add_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 1;
        const uint b = 2;
        bit[a + b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1;
        let b = 2;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_add_float() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const float a = 1.0;
        const float b = 2.0;
        bit[a + b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 1.;
        let b = 2.;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_add_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const angle[32] b = 2.0;
        const bit c = a + b;
        bit[c] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 1367130551,
            Size = 32
        };
        let c = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Sub

#[test]
fn binary_op_sub_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 3;
        const int b = 2;
        bit[a - b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 3;
        let b = 2;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_sub_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 3;
        const uint b = 2;
        bit[a - b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 3;
        let b = 2;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_sub_float() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const float a = 3.0;
        const float b = 2.0;
        bit[a - b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 3.;
        let b = 2.;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_sub_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const angle[32] b = 2.0;
        const bit c = a - b;
        bit[c] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 1367130551,
            Size = 32
        };
        let c = One;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Mul

#[test]
fn binary_op_mul_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 3;
        const int b = 2;
        bit[a * b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 3;
        let b = 2;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_mul_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 3;
        const uint b = 2;
        bit[a * b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 3;
        let b = 2;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_mul_float() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const float a = 3.0;
        const float b = 2.0;
        bit[a * b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 3.;
        let b = 2.;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_mul_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 1.0;
        const uint b = 2;
        const bit c1 = a * b;
        const bit c2 = b * a;
        bit[c1] r1;
        bit[c2] r2;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 683565276,
            Size = 32
        };
        let b = 2;
        let c1 = One;
        let c2 = One;
        mutable r1 = [Zero];
        mutable r2 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Div

#[test]
fn binary_op_div_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 6;
        const int b = 2;
        bit[a / b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 6;
        let b = 2;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_div_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 6;
        const uint b = 2;
        bit[a / b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 6;
        let b = 2;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_div_float() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const float a = 6.0;
        const float b = 2.0;
        bit[a / b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 6.;
        let b = 2.;
        mutable r = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn binary_op_div_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const angle[32] a = 12.0;
        const angle[48] b = 6.0;
        const uint c = 2;
        const bit d = a / b;
        const bit e = a / c;
        bit[d] r1;
        bit[e] r2;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = new Std.OpenQASM.Angle.Angle {
            Value = 3907816011,
            Size = 32
        };
        let b = new Std.OpenQASM.Angle.Angle {
            Value = 268788803401062,
            Size = 48
        };
        let c = 2;
        let d = Zero;
        let e = One;
        mutable r1 = [];
        mutable r2 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Mod

#[test]
fn binary_op_mod_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 8;
        bit[a % 3] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 8;
        mutable r = [Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_mod_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 8;
        bit[a % 3] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 8;
        mutable r = [Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Pow

#[test]
fn binary_op_pow_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 2;
        const int b = 3;
        bit[a ** b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 2;
        let b = 3;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_pow_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const uint a = 2;
        const uint b = 3;
        bit[a ** b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 2;
        let b = 3;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_pow_float() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const float a = 2.0;
        const float b = 3.0;
        bit[a ** b] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 2.;
        let b = 3.;
        mutable r = [Zero, Zero, Zero, Zero, Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

// Cast

#[test]
fn cast_to_bool() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const int a = 0;
        const uint b = 1;
        const float c = 2.0;
        const angle[32] d = 2.0;
        const bit e = 1;

        const bool s1 = a;
        const bool s2 = b;
        const bool s3 = c;
        const bool s4 = d;
        const bool s5 = e;

        bit[s1] r1;
        bit[s2] r2;
        bit[s3] r3;
        bit[s4] r4;
        bit[s5] r5;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = 0;
        let b = 1;
        let c = 2.;
        let d = new Std.OpenQASM.Angle.Angle {
            Value = 1367130551,
            Size = 32
        };
        let e = One;
        let s1 = false;
        let s2 = true;
        let s3 = true;
        let s4 = true;
        let s5 = true;
        mutable r1 = [];
        mutable r2 = [Zero];
        mutable r3 = [Zero];
        mutable r4 = [Zero];
        mutable r5 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cast_to_int() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bool a = true;
        const uint b = 2;
        const float c = 3.0;
        const bit d = 0;

        const int s1 = a;
        const int s2 = b;
        const int s3 = c;
        const int s4 = d;

        bit[s1] r1;
        bit[s2] r2;
        bit[s3] r3;
        bit[s4] r4;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = true;
        let b = 2;
        let c = 3.;
        let d = Zero;
        let s1 = 1;
        let s2 = 2;
        let s3 = 3;
        let s4 = 0;
        mutable r1 = [Zero];
        mutable r2 = [Zero, Zero];
        mutable r3 = [Zero, Zero, Zero];
        mutable r4 = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cast_to_uint() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bool a = true;
        const uint b = 2;
        const float c = 3.0;
        const bit d = 0;

        const uint s1 = a;
        const uint s2 = b;
        const uint s3 = c;
        const uint s4 = d;

        bit[s1] r1;
        bit[s2] r2;
        bit[s3] r3;
        bit[s4] r4;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = true;
        let b = 2;
        let c = 3.;
        let d = Zero;
        let s1 = 1;
        let s2 = 2;
        let s3 = 3;
        let s4 = 0;
        mutable r1 = [Zero];
        mutable r2 = [Zero, Zero];
        mutable r3 = [Zero, Zero, Zero];
        mutable r4 = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cast_to_float() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bool a = true;
        const int b = 2;
        const uint c = 3;

        const float s1 = a;
        const float s2 = b;
        const float s3 = c;

        bit[s1] r1;
        bit[s2] r2;
        bit[s3] r3;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = true;
        let b = 2;
        let c = 3;
        let s1 = 1.;
        let s2 = 2.;
        let s3 = 3.;
        mutable r1 = [Zero];
        mutable r2 = [Zero, Zero];
        mutable r3 = [Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]

fn cast_to_angle() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const float a1 = 2.0;
        const angle[32] b1 = a1;
        const bit s1 = b1;
        bit[s1] r1;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a1 = 2.;
        let b1 = new Std.OpenQASM.Angle.Angle {
            Value = 2867080569611330,
            Size = 53
        };
        let s1 = One;
        mutable r1 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn cast_to_bit() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const bool a = false;
        const int b = 1;
        const uint c = 2;
        const angle[32] d = 3.0;

        const bit s1 = a;
        const bit s2 = b;
        const bit s3 = c;
        const bit s4 = d;

        bit[s1] r1;
        bit[s2] r2;
        bit[s3] r3;
        bit[s4] r4;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        let a = false;
        let b = 1;
        let c = 2;
        let d = new Std.OpenQASM.Angle.Angle {
            Value = 2050695827,
            Size = 32
        };
        let s1 = Zero;
        let s2 = One;
        let s3 = One;
        let s4 = One;
        mutable r1 = [];
        mutable r2 = [Zero];
        mutable r3 = [Zero];
        mutable r4 = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn binary_op_err_type_fails() {
    let source = r#"
        int[a + b] x = 2;
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.UndefinedSymbol

          x undefined symbol: a
           ,-[Test.qasm:2:13]
         1 | 
         2 |         int[a + b] x = 2;
           :             ^
         3 |     
           `----

        Qasm.Lowerer.UndefinedSymbol

          x undefined symbol: b
           ,-[Test.qasm:2:17]
         1 | 
         2 |         int[a + b] x = 2;
           :                 ^
         3 |     
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn binary_op_non_const_type_fails() {
    let source = r#"
        const int a = 2;
        int b = 3;
        int[a + b] x = 2;
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.ExprMustBeConst

          x expression must be const
           ,-[Test.qasm:4:13]
         3 |         int b = 3;
         4 |         int[a + b] x = 2;
           :             ^^^^^
         5 |     
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn fuzzer_issue_2294() {
    let source = r#"
        ctrl(5/_)@l
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Parser.Token

          x expected `;`, found EOF
           ,-[Test.qasm:3:5]
         2 |         ctrl(5/_)@l
         3 |     
           `----

        Qasm.Parser.MissingGateCallOperands

          x missing gate call operands
           ,-[Test.qasm:2:9]
         1 | 
         2 |         ctrl(5/_)@l
           :         ^^^^^^^^^^^
         3 |     
           `----

        Qasm.Lowerer.UndefinedSymbol

          x undefined symbol: _
           ,-[Test.qasm:2:16]
         1 | 
         2 |         ctrl(5/_)@l
           :                ^
         3 |     
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn binary_op_with_non_supported_types_fails() {
    let source = r#"
        const int a = 2 / 0s;
        def f() { a; }
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.CannotApplyOperatorToTypes

          x cannot apply operator Div to types const int and const duration
           ,-[Test.qasm:2:23]
         1 | 
         2 |         const int a = 2 / 0s;
           :                       ^^^^^^
         3 |         def f() { a; }
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn division_of_int_by_zero_int_errors() {
    let source = r#"
        const int a = 2 / 0;
        def f() { a; }
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.DivisionByZero

          x division by zero error during const evaluation
           ,-[Test.qasm:2:23]
         1 | 
         2 |         const int a = 2 / 0;
           :                       ^^^^^
         3 |         def f() { a; }
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn division_of_angle_by_zero_int_errors() {
    let source = r#"
        const angle a = 2.0;
        const angle b = a / 0;
        def f() { b; }
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.DivisionByZero

          x division by zero error during const evaluation
           ,-[Test.qasm:3:25]
         2 |         const angle a = 2.0;
         3 |         const angle b = a / 0;
           :                         ^^^^^
         4 |         def f() { b; }
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn division_by_zero_float_errors() {
    let source = r#"
        const float a = 2.0 / 0.0;
        def f() { a; }
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.DivisionByZero

          x division by zero error during const evaluation
           ,-[Test.qasm:2:25]
         1 | 
         2 |         const float a = 2.0 / 0.0;
           :                         ^^^^^^^^^
         3 |         def f() { a; }
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn division_by_zero_angle_errors() {
    let source = r#"
        const angle a = 2.0;
        const angle b = 0.0;
        const uint c = a / b;
        def f() { c; }
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.DivisionByZero

          x division by zero error during const evaluation
           ,-[Test.qasm:4:24]
         3 |         const angle b = 0.0;
         4 |         const uint c = a / b;
           :                        ^^^^^
         5 |         def f() { c; }
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn modulo_of_int_by_zero_int_errors() {
    let source = r#"
        const int a = 2 % 0;
        def f() { a; }
    "#;

    let Err(errs) = compile_qasm_to_qsharp(source) else {
        panic!("should have generated an error");
    };
    let errs: Vec<_> = errs.iter().map(|e| format!("{e:?}")).collect();
    let errs_string = errs.join("\n");
    expect![[r#"
        Qasm.Lowerer.DivisionByZero

          x division by zero error during const evaluation
           ,-[Test.qasm:2:23]
         1 | 
         2 |         const int a = 2 % 0;
           :                       ^^^^^
         3 |         def f() { a; }
           `----
    "#]]
    .assert_eq(&errs_string);
}

#[test]
fn wrong_type_as_modifer_arg_fails() {
    let source = r#"
        const int n = 2.0;
        ctrl(n) @ x q;
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        Qasm.Lowerer.CannotCastLiteral

          x cannot cast literal expression of type const float to type const int
           ,-[Test.qasm:2:9]
         1 | 
         2 |         const int n = 2.0;
           :         ^^^^^^^^^^^^^^^^^^
         3 |         ctrl(n) @ x q;
           `----
    "#]],
    );
}
