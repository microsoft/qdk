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
fn duration_to_bool_fails() {
    let source = "
        duration a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-37]:
                        annotations: <empty>
                        kind: ExprStmt [29-37]:
                            expr: Expr [29-36]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type bool
               ,-[test:3:9]
             2 |         duration a;
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
fn duration_to_duration() {
    let source = "
        duration a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs: <empty>
                init_expr: Expr [9-20]:
                    ty: duration
                    kind: Lit: Duration(0.0 s)
            ExprStmt [29-41]:
                expr: Expr [29-40]:
                    ty: duration
                    kind: SymbolId(8)
        "#]],
    );
}

//===================
// Casts to stretch
//===================

#[test]
fn duration_to_stretch_changes_ty() {
    let source = "
        duration a;
        stretch(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs: <empty>
                init_expr: Expr [9-20]:
                    ty: duration
                    kind: Lit: Duration(0.0 s)
            ExprStmt [29-40]:
                expr: Expr [29-39]:
                    ty: stretch
                    kind: SymbolId(8)
        "#]],
    );
}

//=========================
// Casts to int and int[n]
//=========================

#[test]
fn duration_to_int_fails() {
    let source = "
        duration a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-36]:
                        annotations: <empty>
                        kind: ExprStmt [29-36]:
                            expr: Expr [29-35]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type int
               ,-[test:3:9]
             2 |         duration a;
             3 |         int(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn duration_to_sized_int_fails() {
    let source = "
        duration a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-40]:
                        annotations: <empty>
                        kind: ExprStmt [29-40]:
                            expr: Expr [29-39]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type int[32]
               ,-[test:3:9]
             2 |         duration a;
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
fn duration_to_uint_fails() {
    let source = "
        duration a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-37]:
                        annotations: <empty>
                        kind: ExprStmt [29-37]:
                            expr: Expr [29-36]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type uint
               ,-[test:3:9]
             2 |         duration a;
             3 |         uint(a);
               :         ^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn duration_to_sized_uint_fails() {
    let source = "
        duration a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-41]:
                        annotations: <empty>
                        kind: ExprStmt [29-41]:
                            expr: Expr [29-40]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type uint[32]
               ,-[test:3:9]
             2 |         duration a;
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
fn duration_to_float_fails() {
    let source = "
        duration a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-38]:
                        annotations: <empty>
                        kind: ExprStmt [29-38]:
                            expr: Expr [29-37]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type float
               ,-[test:3:9]
             2 |         duration a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn duration_to_sized_float_fails() {
    let source = "
        duration a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-42]:
                        annotations: <empty>
                        kind: ExprStmt [29-42]:
                            expr: Expr [29-41]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type float[32]
               ,-[test:3:9]
             2 |         duration a;
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
fn duration_to_angle_fails() {
    let source = "
        duration a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-38]:
                        annotations: <empty>
                        kind: ExprStmt [29-38]:
                            expr: Expr [29-37]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type angle
               ,-[test:3:9]
             2 |         duration a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn duration_to_sized_angle_fails() {
    let source = "
        duration a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-42]:
                        annotations: <empty>
                        kind: ExprStmt [29-42]:
                            expr: Expr [29-41]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type angle[32]
               ,-[test:3:9]
             2 |         duration a;
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
fn duration_to_complex_fails() {
    let source = "
        duration a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-40]:
                        annotations: <empty>
                        kind: ExprStmt [29-40]:
                            expr: Expr [29-39]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type complex[float]
               ,-[test:3:9]
             2 |         duration a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn duration_to_sized_complex_fails() {
    let source = "
        duration a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-51]:
                        annotations: <empty>
                        kind: ExprStmt [29-51]:
                            expr: Expr [29-50]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type complex[float[32]]
               ,-[test:3:9]
             2 |         duration a;
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
fn duration_to_bit_fails() {
    let source = "
        duration a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-36]:
                        annotations: <empty>
                        kind: ExprStmt [29-36]:
                            expr: Expr [29-35]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type bit
               ,-[test:3:9]
             2 |         duration a;
             3 |         bit(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn duration_to_bitarray_fails() {
    let source = "
        duration a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-17]
                            ty_exprs: <empty>
                            init_expr: Expr [9-20]:
                                ty: duration
                                kind: Lit: Duration(0.0 s)
                    Stmt [29-40]:
                        annotations: <empty>
                        kind: ExprStmt [29-40]:
                            expr: Expr [29-39]:
                                ty: duration
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type duration to type bit[32]
               ,-[test:3:9]
             2 |         duration a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}
