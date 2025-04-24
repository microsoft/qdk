// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests in this file need to check that const exprs are
//! evaluatable at lowering time. To do that we use them in
//! contexts where they need to be const-evaluated, like array
//! sizes or type widths.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 1;
        let b = 2 + a;
        let c = a + 3;
        mutable r1 = [Zero, Zero, Zero];
        mutable r2 = [Zero, Zero, Zero, Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 1;
        let b = 2. + Microsoft.Quantum.Convert.IntAsDouble(a);
        let c = Microsoft.Quantum.Convert.IntAsDouble(a) + 3.;
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

          x designator must be a const expression
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

        Qasm.Lowerer.ExprMustBeConst

          x designator must be a const expression
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 1;
        mutable r = [Zero];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
#[ignore = "indexed ident is not yet supported"]
fn indexed_ident() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        const array[uint, 2] a = {1, 2};
        bit[a[1]] r;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        let a = 1;
        let b = 2;
        mutable c = a + b;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = -1.;
        let b = -a;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = -1;
        let b = -a;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = __DoubleAsAngle__(-1., 32);
        let b = __AngleAsResult__(a);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 5;
        let b = ~~~a;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = __AngleAsResult__(__AngleNotB__(a));
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = Zero;
        let b = ~~~a;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = [One, Zero, One];
        let b = __ResultArrayAsIntBE__(~~~a);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 1;
        let b = 2.;
        let c = Microsoft.Quantum.Math.Truncate(Microsoft.Quantum.Convert.IntAsDouble(a) + b);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 1;
        let b = a <<< 2;
        mutable r = [Zero, Zero, Zero, Zero];
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = __AngleShl__(a, 2);
        let c = __AngleAsResult__(b);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = One;
        let b = if __ResultAsInt__(a) <<< 2 == 0 {
            One
        } else {
            Zero
        };
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = [One, Zero, One];
        let b = __IntAsResultArrayBE__(__ResultArrayAsIntBE__(a) <<< 2, 3);
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

          x expected scalar or array type, found keyword `creg`
           ,-[Test.qasm:2:15]
         1 | 
         2 |         const creg a[3] = "101";
           :               ^^^^
         3 |         const creg b[3] = a << 2;
           `----

        Qasm.Parser.Rule

          x expected scalar or array type, found keyword `creg`
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

        Qasm.Lowerer.CannotCast

          x cannot cast expression of type Err to type UInt(None, true)
           ,-[Test.qasm:4:13]
         3 |         const creg b[3] = a << 2;
         4 |         bit[b] r;
           :             ^
         5 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x expression must be const
           ,-[Test.qasm:4:13]
         3 |         const creg b[3] = a << 2;
         4 |         bit[b] r;
           :             ^
         5 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x designator must be a const expression
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 5;
        let b = a >>> 2;
        mutable r = [Zero];
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = __AngleShr__(a, 2);
        let c = __AngleAsResult__(b);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = One;
        let b = if __ResultAsInt__(a) >>> 2 == 0 {
            One
        } else {
            Zero
        };
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = [One, Zero, One, One];
        let b = __IntAsResultArrayBE__(__ResultArrayAsIntBE__(a) >>> 2, 4);
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

          x expected scalar or array type, found keyword `creg`
           ,-[Test.qasm:2:15]
         1 | 
         2 |         const creg a[4] = "1011";
           :               ^^^^
         3 |         const creg b[4] = a >> 2;
           `----

        Qasm.Parser.Rule

          x expected scalar or array type, found keyword `creg`
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

        Qasm.Lowerer.CannotCast

          x cannot cast expression of type Err to type UInt(None, true)
           ,-[Test.qasm:4:13]
         3 |         const creg b[4] = a >> 2;
         4 |         bit[b] r;
           :             ^
         5 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x expression must be const
           ,-[Test.qasm:4:13]
         3 |         const creg b[4] = a >> 2;
         4 |         bit[b] r;
           :             ^
         5 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x designator must be a const expression
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 5;
        let b = a &&& 6;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = new __Angle__ {
            Value = 1367130551,
            Size = 32
        };
        let c = __AngleAndB__(a, b);
        let d = __AngleAsResult__(c);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = One;
        let b = if __ResultAsInt__(a) &&& 0 == 0 {
            One
        } else {
            Zero
        };
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = [One, Zero, One, One];
        let b = __IntAsResultArrayBE__(__ResultArrayAsIntBE__(a) &&& __ResultArrayAsIntBE__([Zero, One, One, Zero]), 4);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 5;
        let b = a ||| 6;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = new __Angle__ {
            Value = 1367130551,
            Size = 32
        };
        let c = __AngleOrB__(a, b);
        let d = __AngleAsBool__(c);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = One;
        let b = if __ResultAsInt__(a) ||| 0 == 0 {
            One
        } else {
            Zero
        };
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = [Zero, Zero, One];
        let b = __IntAsResultArrayBE__(__ResultArrayAsIntBE__(a) ||| __ResultArrayAsIntBE__([One, Zero, Zero]), 3);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 5;
        let b = a ^^^ 6;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = new __Angle__ {
            Value = 1367130551,
            Size = 32
        };
        let c = __AngleXorB__(a, b);
        let d = __AngleAsResult__(c);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = One;
        let b = if __ResultAsInt__(a) ^^^ 1 == 0 {
            One
        } else {
            Zero
        };
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = [One, Zero, One, One];
        let b = __IntAsResultArrayBE__(__ResultArrayAsIntBE__(a) ^^^ __ResultArrayAsIntBE__([One, One, One, Zero]), 4);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = new __Angle__ {
            Value = 1367130551,
            Size = 32
        };
        let c = __AngleAsResult__(__AddAngles__(a, b));
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = new __Angle__ {
            Value = 1367130551,
            Size = 32
        };
        let c = __AngleAsResult__(__SubtractAngles__(a, b));
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 683565276,
            Size = 32
        };
        let b = 2;
        let c1 = __AngleAsResult__(__MultiplyAngleByInt__(a, b));
        let c2 = __AngleAsResult__(__MultiplyAngleByInt__(a, b));
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = new __Angle__ {
            Value = 3907816011,
            Size = 32
        };
        let b = new __Angle__ {
            Value = 268788803401062,
            Size = 48
        };
        let c = 2;
        let d = if __DivideAngleByAngle__(__ConvertAngleToWidthNoTrunc__(a, 48), b) == 0 {
            One
        } else {
            Zero
        };
        let e = __AngleAsResult__(__DivideAngleByInt__(a, c));
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = 0;
        let b = 1;
        let c = 2.;
        let d = new __Angle__ {
            Value = 1367130551,
            Size = 32
        };
        let e = One;
        let s1 = if a == 0 {
            false
        } else {
            true
        };
        let s2 = if b == 0 {
            false
        } else {
            true
        };
        let s3 = if Microsoft.Quantum.Math.Truncate(c) == 0 {
            false
        } else {
            true
        };
        let s4 = __AngleAsBool__(d);
        let s5 = __ResultAsBool__(e);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = true;
        let b = 2;
        let c = 3.;
        let d = Zero;
        let s1 = __BoolAsInt__(a);
        let s2 = b;
        let s3 = Microsoft.Quantum.Math.Truncate(c);
        let s4 = __ResultAsInt__(d);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = true;
        let b = 2;
        let c = 3.;
        let d = Zero;
        let s1 = __BoolAsInt__(a);
        let s2 = b;
        let s3 = Microsoft.Quantum.Math.Truncate(c);
        let s4 = __ResultAsInt__(d);
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = true;
        let b = 2;
        let c = 3;
        let s1 = __BoolAsDouble__(a);
        let s2 = Microsoft.Quantum.Convert.IntAsDouble(b);
        let s3 = Microsoft.Quantum.Convert.IntAsDouble(c);
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
        const bit a2 = 1;

        const angle[32] b1 = a1;
        const angle[32] b2 = a2;

        const bit s1 = b1;
        const bit s2 = b2;

        bit[s1] r1;
        bit[s2] r2;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a1 = 2.;
        let a2 = One;
        let b1 = __DoubleAsAngle__(a1, 32);
        let b2 = __ResultAsAngle__(a2);
        let s1 = __AngleAsResult__(b1);
        let s2 = __AngleAsResult__(b2);
        mutable r1 = [Zero];
        mutable r2 = [Zero];
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
        import QasmStd.Angle.*;
        import QasmStd.Convert.*;
        import QasmStd.Intrinsic.*;
        let a = false;
        let b = 1;
        let c = 2;
        let d = new __Angle__ {
            Value = 2050695827,
            Size = 32
        };
        let s1 = __BoolAsResult__(a);
        let s2 = if b == 0 {
            One
        } else {
            Zero
        };
        let s3 = if c == 0 {
            One
        } else {
            Zero
        };
        let s4 = __AngleAsResult__(d);
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

        Qasm.Lowerer.CannotCast

          x cannot cast expression of type Err to type UInt(None, true)
           ,-[Test.qasm:2:13]
         1 | 
         2 |         int[a + b] x = 2;
           :             ^^^^^
         3 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x expression must be const
           ,-[Test.qasm:2:13]
         1 | 
         2 |         int[a + b] x = 2;
           :             ^^^^^
         3 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x designator must be a const expression
           ,-[Test.qasm:2:13]
         1 | 
         2 |         int[a + b] x = 2;
           :             ^^^^^
         3 |     
           `----

        Qasm.Lowerer.CannotCastLiteral

          x cannot cast literal expression of type Int(None, true) to type Err
           ,-[Test.qasm:2:9]
         1 | 
         2 |         int[a + b] x = 2;
           :         ^^^^^^^^^^^^^^^^^
         3 |     
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

        Qasm.Lowerer.CannotCast

          x cannot cast expression of type Err to type Float(None, true)
           ,-[Test.qasm:2:16]
         1 | 
         2 |         ctrl(5/_)@l
           :                ^
         3 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x expression must be const
           ,-[Test.qasm:2:16]
         1 | 
         2 |         ctrl(5/_)@l
           :                ^
         3 |     
           `----

        Qasm.Lowerer.ExprMustBeConst

          x ctrl modifier argument must be a const expression
           ,-[Test.qasm:2:14]
         1 | 
         2 |         ctrl(5/_)@l
           :              ^^^
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
        Qasm.Lowerer.CannotCast

          x cannot cast expression of type Duration(true) to type Float(None, true)
           ,-[Test.qasm:2:27]
         1 | 
         2 |         const int a = 2 / 0s;
           :                           ^^
         3 |         def f() { a; }
           `----

        Qasm.Lowerer.UnsupportedBinaryOp

          x Div is not supported between types Float(None, true) and Duration(true)
           ,-[Test.qasm:2:23]
         1 | 
         2 |         const int a = 2 / 0s;
           :                       ^^^^^^
         3 |         def f() { a; }
           `----

        Qasm.Lowerer.ExprMustBeConst

          x a captured variable must be a const expression
           ,-[Test.qasm:3:19]
         2 |         const int a = 2 / 0s;
         3 |         def f() { a; }
           :                   ^
         4 |     
           `----

        Qasm.Compiler.NotSupported

          x timing literals are not supported
           ,-[Test.qasm:2:27]
         1 | 
         2 |         const int a = 2 / 0s;
           :                           ^^
         3 |         def f() { a; }
           `----
    "#]]
    .assert_eq(&errs_string);
}
