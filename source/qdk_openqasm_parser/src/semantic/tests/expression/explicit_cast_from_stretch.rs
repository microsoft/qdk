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
fn stretch_to_bool_fails() {
    let source = "
        stretch a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-36]:
                        annotations: <empty>
                        kind: ExprStmt [28-36]:
                            expr: Expr [28-35]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const bool
               ,-[test:3:9]
             2 |         stretch a;
             3 |         bool(a);
               :         ^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn stretch_to_duration_changes_ty() {
    let source = "
        stretch a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs: <empty>
                init_expr: Expr [9-19]:
                    ty: stretch
                    const_value: Duration(0.0 s)
                    kind: Lit: Duration(0.0 s)
            ExprStmt [28-40]:
                expr: Expr [28-39]:
                    ty: const duration
                    kind: SymbolId(8)
        "#]],
    );
}

//===================
// Casts to stretch
//===================

#[test]
fn stretch_to_stretch() {
    let source = "
        stretch a;
        stretch(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs: <empty>
                init_expr: Expr [9-19]:
                    ty: stretch
                    const_value: Duration(0.0 s)
                    kind: Lit: Duration(0.0 s)
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: stretch
                    kind: SymbolId(8)
        "#]],
    );
}

//=========================
// Casts to int and int[n]
//=========================

#[test]
fn stretch_to_int_fails() {
    let source = "
        stretch a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-35]:
                        annotations: <empty>
                        kind: ExprStmt [28-35]:
                            expr: Expr [28-34]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const int
               ,-[test:3:9]
             2 |         stretch a;
             3 |         int(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn stretch_to_sized_int_fails() {
    let source = "
        stretch a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const int[32]
               ,-[test:3:9]
             2 |         stretch a;
             3 |         int[32](a);
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
fn stretch_to_uint_fails() {
    let source = "
        stretch a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-36]:
                        annotations: <empty>
                        kind: ExprStmt [28-36]:
                            expr: Expr [28-35]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const uint
               ,-[test:3:9]
             2 |         stretch a;
             3 |         uint(a);
               :         ^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn stretch_to_sized_uint_fails() {
    let source = "
        stretch a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-40]:
                        annotations: <empty>
                        kind: ExprStmt [28-40]:
                            expr: Expr [28-39]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const uint[32]
               ,-[test:3:9]
             2 |         stretch a;
             3 |         uint[32](a);
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
fn stretch_to_float_fails() {
    let source = "
        stretch a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-37]:
                        annotations: <empty>
                        kind: ExprStmt [28-37]:
                            expr: Expr [28-36]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const float
               ,-[test:3:9]
             2 |         stretch a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn stretch_to_sized_float_fails() {
    let source = "
        stretch a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const float[32]
               ,-[test:3:9]
             2 |         stretch a;
             3 |         float[32](a);
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
fn stretch_to_angle_fails() {
    let source = "
        stretch a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-37]:
                        annotations: <empty>
                        kind: ExprStmt [28-37]:
                            expr: Expr [28-36]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const angle
               ,-[test:3:9]
             2 |         stretch a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn stretch_to_sized_angle_fails() {
    let source = "
        stretch a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const angle[32]
               ,-[test:3:9]
             2 |         stretch a;
             3 |         angle[32](a);
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
fn stretch_to_complex_fails() {
    let source = "
        stretch a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const complex[float]
               ,-[test:3:9]
             2 |         stretch a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn stretch_to_sized_complex_fails() {
    let source = "
        stretch a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-50]:
                        annotations: <empty>
                        kind: ExprStmt [28-50]:
                            expr: Expr [28-49]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const complex[float[32]]
               ,-[test:3:9]
             2 |         stretch a;
             3 |         complex[float[32]](a);
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
fn stretch_to_bit_fails() {
    let source = "
        stretch a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-35]:
                        annotations: <empty>
                        kind: ExprStmt [28-35]:
                            expr: Expr [28-34]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const bit
               ,-[test:3:9]
             2 |         stretch a;
             3 |         bit(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn stretch_to_bitarray_fails() {
    let source = "
        stretch a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-19]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-19]:
                            symbol_id: 8
                            ty_span: [9-16]
                            ty_exprs: <empty>
                            init_expr: Expr [9-19]:
                                ty: stretch
                                const_value: Duration(0.0 s)
                                kind: Lit: Duration(0.0 s)
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: stretch
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const bit[32]
               ,-[test:3:9]
             2 |         stretch a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}
