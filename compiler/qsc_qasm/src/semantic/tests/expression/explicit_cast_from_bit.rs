// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! These unit tests check six properties for each target type.
//! Let's call the type we are casting from `T` and the type we are casting to `Q`.
//! We want to test that for each type `Q` we correctly:
//!   1. cast from T to Q.
//!   2. cast from T to Q[n].
//!   3. cast from T[n] to Q.
//!   4. cast from T[n] to Q[n].
//!   5. cast from T[n] to Q[m] when n > m; a truncating cast.
//!   6. cast from T[n] to Q[m] when n < m; an expanding cast.

use crate::semantic::tests::check_stmt_kinds as check;
use expect_test::expect;

//===============
// Casts to bool
//===============

#[test]
fn bit_to_bool() {
    let source = "
        bit a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-32]:
                expr: Expr [24-31]:
                    ty: Bool(false)
                    kind: Cast [24-31]:
                        ty: Bool(false)
                        expr: Expr [29-30]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_bool() {
    let source = "
        bit[32] a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                init_expr: Expr [0-0]:
                    ty: BitArray(32, true)
                    kind: Lit: Bitstring("00000000000000000000000000000000")
            ExprStmt [28-36]:
                expr: Expr [28-35]:
                    ty: Bool(false)
                    kind: Cast [28-35]:
                        ty: Bool(false)
                        expr: Expr [33-34]:
                            ty: BitArray(32, false)
                            kind: SymbolId(8)
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn bit_to_duration_fails() {
    let source = "
        bit a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            init_expr: Expr [0-0]:
                                ty: Bit(true)
                                kind: Lit: Bit(0)
                    Stmt [24-36]:
                        annotations: <empty>
                        kind: ExprStmt [24-36]:
                            expr: Expr [24-35]:
                                ty: Bit(false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type Bit(false) to type Duration(false)
               ,-[test:3:9]
             2 |         bit a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_duration_fails() {
    let source = "
        bit[32] a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-40]:
                        annotations: <empty>
                        kind: ExprStmt [28-40]:
                            expr: Expr [28-39]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Duration(false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

//=========================
// Casts to int and int[n]
//=========================

#[test]
fn bit_to_int() {
    let source = "
        bit a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-31]:
                expr: Expr [24-30]:
                    ty: Int(None, false)
                    kind: Cast [24-30]:
                        ty: Int(None, false)
                        expr: Expr [28-29]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bit_to_sized_int() {
    let source = "
        bit a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-35]:
                expr: Expr [24-34]:
                    ty: Int(Some(32), false)
                    kind: Cast [24-34]:
                        ty: Int(Some(32), false)
                        expr: Expr [32-33]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
#[ignore = "this should fail but we are using this cast for bitarray BinOps (we cast to int first)"]
fn bitarray_to_int_fails() {
    let source = "
        bit[32] a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-35]:
                        annotations: <empty>
                        kind: ExprStmt [28-35]:
                            expr: Expr [32-33]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Int(None,
              | false)
               ,-[test:3:13]
             2 |         bit[32] a;
             3 |         int(a);
               :             ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_int() {
    let source = "
        bit[32] a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                init_expr: Expr [0-0]:
                    ty: BitArray(32, true)
                    kind: Lit: Bitstring("00000000000000000000000000000000")
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: Int(Some(32), false)
                    kind: Cast [28-38]:
                        ty: Int(Some(32), false)
                        expr: Expr [36-37]:
                            ty: BitArray(32, false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_sized_int_truncating_fails() {
    let source = "
        bit[32] a;
        int[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Int(Some(16),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         int[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_int_expanding_fails() {
    let source = "
        bit[32] a;
        int[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Int(Some(64),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         int[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn bit_to_uint() {
    let source = "
        bit a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-32]:
                expr: Expr [24-31]:
                    ty: UInt(None, false)
                    kind: Cast [24-31]:
                        ty: UInt(None, false)
                        expr: Expr [29-30]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bit_to_sized_uint() {
    let source = "
        bit a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-36]:
                expr: Expr [24-35]:
                    ty: UInt(Some(32), false)
                    kind: Cast [24-35]:
                        ty: UInt(Some(32), false)
                        expr: Expr [33-34]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
#[ignore = "this should fail but we are using this cast for bitarray bit shifts (we cast to uint first)"]
fn bitarray_to_uint_fails() {
    let source = "
        bit[32] a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-36]:
                        annotations: <empty>
                        kind: ExprStmt [28-36]:
                            expr: Expr [33-34]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type UInt(None,
              | false)
               ,-[test:3:14]
             2 |         bit[32] a;
             3 |         uint(a);
               :              ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_uint() {
    let source = "
        bit[32] a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                init_expr: Expr [0-0]:
                    ty: BitArray(32, true)
                    kind: Lit: Bitstring("00000000000000000000000000000000")
            ExprStmt [28-40]:
                expr: Expr [28-39]:
                    ty: UInt(Some(32), false)
                    kind: Cast [28-39]:
                        ty: UInt(Some(32), false)
                        expr: Expr [37-38]:
                            ty: BitArray(32, false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_sized_uint_truncating_fails() {
    let source = "
        bit[32] a;
        uint[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-40]:
                        annotations: <empty>
                        kind: ExprStmt [28-40]:
                            expr: Expr [28-39]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type UInt(Some(16),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         uint[16](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_uint_expanding_fails() {
    let source = "
        bit[32] a;
        uint[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-40]:
                        annotations: <empty>
                        kind: ExprStmt [28-40]:
                            expr: Expr [28-39]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type UInt(Some(64),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         uint[64](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn bit_to_float() {
    let source = "
        bit a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-33]:
                expr: Expr [24-32]:
                    ty: Float(None, false)
                    kind: Cast [24-32]:
                        ty: Float(None, false)
                        expr: Expr [30-31]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bit_to_sized_float() {
    let source = "
        bit a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-37]:
                expr: Expr [24-36]:
                    ty: Float(Some(32), false)
                    kind: Cast [24-36]:
                        ty: Float(Some(32), false)
                        expr: Expr [34-35]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_float_fails() {
    let source = "
        bit[32] a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-37]:
                        annotations: <empty>
                        kind: ExprStmt [28-37]:
                            expr: Expr [28-36]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Float(None,
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_float_fails() {
    let source = "
        bit[32] a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Float(Some(32),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         float[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_float_truncating_fails() {
    let source = "
        bit[32] a;
        float[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Float(Some(16),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         float[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_float_expanding_fails() {
    let source = "
        bit[32] a;
        float[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Float(Some(64),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         float[64](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn bit_to_angle_fails() {
    let source = "
        bit a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            init_expr: Expr [0-0]:
                                ty: Bit(true)
                                kind: Lit: Bit(0)
                    Stmt [24-33]:
                        annotations: <empty>
                        kind: ExprStmt [24-33]:
                            expr: Expr [24-32]:
                                ty: Bit(false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type Bit(false) to type Angle(None, false)
               ,-[test:3:9]
             2 |         bit a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bit_to_sized_angle_fails() {
    let source = "
        bit a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            init_expr: Expr [0-0]:
                                ty: Bit(true)
                                kind: Lit: Bit(0)
                    Stmt [24-37]:
                        annotations: <empty>
                        kind: ExprStmt [24-37]:
                            expr: Expr [24-36]:
                                ty: Bit(false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type Bit(false) to type Angle(Some(32), false)
               ,-[test:3:9]
             2 |         bit a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_angle_fails() {
    let source = "
        bit[32] a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-37]:
                        annotations: <empty>
                        kind: ExprStmt [28-37]:
                            expr: Expr [28-36]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Angle(None,
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_angle() {
    let source = "
        bit[32] a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                init_expr: Expr [0-0]:
                    ty: BitArray(32, true)
                    kind: Lit: Bitstring("00000000000000000000000000000000")
            ExprStmt [28-41]:
                expr: Expr [28-40]:
                    ty: Angle(Some(32), false)
                    kind: Cast [28-40]:
                        ty: Angle(Some(32), false)
                        expr: Expr [38-39]:
                            ty: BitArray(32, false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_sized_angle_truncating_fails() {
    let source = "
        bit[32] a;
        angle[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Angle(Some(16),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         angle[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_angle_expanding_fails() {
    let source = "
        bit[32] a;
        angle[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Angle(Some(64),
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         angle[64](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

//=================================
// Casts to complex and complex[n]
//=================================

#[test]
fn bit_to_complex_fails() {
    let source = "
        bit a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            init_expr: Expr [0-0]:
                                ty: Bit(true)
                                kind: Lit: Bit(0)
                    Stmt [24-35]:
                        annotations: <empty>
                        kind: ExprStmt [24-35]:
                            expr: Expr [24-34]:
                                ty: Bit(false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type Bit(false) to type Complex(None, false)
               ,-[test:3:9]
             2 |         bit a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bit_to_sized_complex_fails() {
    let source = "
        bit a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            init_expr: Expr [0-0]:
                                ty: Bit(true)
                                kind: Lit: Bit(0)
                    Stmt [24-46]:
                        annotations: <empty>
                        kind: ExprStmt [24-46]:
                            expr: Expr [24-45]:
                                ty: Bit(false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type Bit(false) to type Complex(Some(32), false)
               ,-[test:3:9]
             2 |         bit a;
             3 |         complex[float[32]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_complex_fails() {
    let source = "
        bit[32] a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type Complex(None,
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_complex_fails() {
    let source = "
        bit[32] a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-50]:
                        annotations: <empty>
                        kind: ExprStmt [28-50]:
                            expr: Expr [28-49]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type
              | Complex(Some(32), false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         complex[float[32]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_complex_truncating_fails() {
    let source = "
        bit[32] a;
        complex[float[16]](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-50]:
                        annotations: <empty>
                        kind: ExprStmt [28-50]:
                            expr: Expr [28-49]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type
              | Complex(Some(16), false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         complex[float[16]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_sized_complex_expanding_fails() {
    let source = "
        bit[32] a;
        complex[float[64]](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-50]:
                        annotations: <empty>
                        kind: ExprStmt [28-50]:
                            expr: Expr [28-49]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type
              | Complex(Some(64), false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         complex[float[64]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn bit_to_bit() {
    let source = "
        bit a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-31]:
                expr: Expr [24-30]:
                    ty: Bit(false)
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bit_to_bitarray() {
    let source = "
        bit a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            ExprStmt [24-35]:
                expr: Expr [24-34]:
                    ty: BitArray(32, false)
                    kind: Cast [24-34]:
                        ty: BitArray(32, false)
                        expr: Expr [32-33]:
                            ty: Bit(false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_bit() {
    let source = "
        bit[32] a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                init_expr: Expr [0-0]:
                    ty: BitArray(32, true)
                    kind: Lit: Bitstring("00000000000000000000000000000000")
            ExprStmt [28-35]:
                expr: Expr [28-34]:
                    ty: Bit(false)
                    kind: Cast [28-34]:
                        ty: Bit(false)
                        expr: Expr [32-33]:
                            ty: BitArray(32, false)
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_bitarray() {
    let source = "
        bit[32] a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                init_expr: Expr [0-0]:
                    ty: BitArray(32, true)
                    kind: Lit: Bitstring("00000000000000000000000000000000")
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: BitArray(32, false)
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn bitarray_to_bitarray_truncating_fails() {
    let source = "
        bit[32] a;
        bit[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type BitArray(16,
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         bit[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn bitarray_to_bitarray_expanding_fails() {
    let source = "
        bit[32] a;
        bit[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            init_expr: Expr [0-0]:
                                ty: BitArray(32, true)
                                kind: Lit: Bitstring("00000000000000000000000000000000")
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: BitArray(32, false)
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type BitArray(32, false) to type BitArray(64,
              | false)
               ,-[test:3:9]
             2 |         bit[32] a;
             3 |         bit[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}
