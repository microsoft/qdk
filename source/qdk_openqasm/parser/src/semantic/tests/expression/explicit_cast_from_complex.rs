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
fn complex_to_bool_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-36]:
                        annotations: <empty>
                        kind: ExprStmt [28-36]:
                            expr: Expr [28-35]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type bool
               ,-[test:3:9]
             2 |         complex a;
             3 |         bool(a);
               :         ^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_bool_fails() {
    let source = "
        complex[float[32]] a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-47]:
                        annotations: <empty>
                        kind: ExprStmt [39-47]:
                            expr: Expr [39-46]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bool
               ,-[test:3:9]
             2 |         complex[float[32]] a;
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
fn complex_to_duration_fails() {
    let source = "
        complex a;
        duration(a);
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-40]:
                        annotations: <empty>
                        kind: ExprStmt [28-40]:
                            expr: Expr [28-39]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type duration
               ,-[test:3:9]
             2 |         complex a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_duration_fails() {
    let source = "
        complex[float[32]] a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-51]:
                        annotations: <empty>
                        kind: ExprStmt [39-51]:
                            expr: Expr [39-50]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type duration
               ,-[test:3:9]
             2 |         complex[float[32]] a;
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
fn complex_to_int_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-35]:
                        annotations: <empty>
                        kind: ExprStmt [28-35]:
                            expr: Expr [28-34]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type int
               ,-[test:3:9]
             2 |         complex a;
             3 |         int(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn complex_to_sized_int_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type int[32]
               ,-[test:3:9]
             2 |         complex a;
             3 |         int[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_int_fails() {
    let source = "
        complex[float[32]] a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-46]:
                        annotations: <empty>
                        kind: ExprStmt [39-46]:
                            expr: Expr [39-45]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         int(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_int_fails() {
    let source = "
        complex[float[32]] a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-50]:
                        annotations: <empty>
                        kind: ExprStmt [39-50]:
                            expr: Expr [39-49]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int[32]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         int[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_int_truncating_fails() {
    let source = "
        complex[float[32]] a;
        int[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-50]:
                        annotations: <empty>
                        kind: ExprStmt [39-50]:
                            expr: Expr [39-49]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int[16]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         int[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_int_expanding_fails() {
    let source = "
        complex[float[32]] a;
        int[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-50]:
                        annotations: <empty>
                        kind: ExprStmt [39-50]:
                            expr: Expr [39-49]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type int[64]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
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
fn complex_to_uint_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-36]:
                        annotations: <empty>
                        kind: ExprStmt [28-36]:
                            expr: Expr [28-35]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type uint
               ,-[test:3:9]
             2 |         complex a;
             3 |         uint(a);
               :         ^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn complex_to_sized_uint_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-40]:
                        annotations: <empty>
                        kind: ExprStmt [28-40]:
                            expr: Expr [28-39]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type uint[32]
               ,-[test:3:9]
             2 |         complex a;
             3 |         uint[32](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_uint_fails() {
    let source = "
        complex[float[32]] a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-47]:
                        annotations: <empty>
                        kind: ExprStmt [39-47]:
                            expr: Expr [39-46]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         uint(a);
               :         ^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_uint_fails() {
    let source = "
        complex[float[32]] a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-51]:
                        annotations: <empty>
                        kind: ExprStmt [39-51]:
                            expr: Expr [39-50]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint[32]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         uint[32](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_uint_truncating_fails() {
    let source = "
        complex[float[32]] a;
        uint[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-51]:
                        annotations: <empty>
                        kind: ExprStmt [39-51]:
                            expr: Expr [39-50]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint[16]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         uint[16](a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_uint_expanding_fails() {
    let source = "
        complex[float[32]] a;
        uint[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-51]:
                        annotations: <empty>
                        kind: ExprStmt [39-51]:
                            expr: Expr [39-50]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type uint[64]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
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
fn complex_to_float_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-37]:
                        annotations: <empty>
                        kind: ExprStmt [28-37]:
                            expr: Expr [28-36]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type float
               ,-[test:3:9]
             2 |         complex a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn complex_to_sized_float_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type float[32]
               ,-[test:3:9]
             2 |         complex a;
             3 |         float[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_float_fails() {
    let source = "
        complex[float[32]] a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-48]:
                        annotations: <empty>
                        kind: ExprStmt [39-48]:
                            expr: Expr [39-47]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_float_fails() {
    let source = "
        complex[float[32]] a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-52]:
                        annotations: <empty>
                        kind: ExprStmt [39-52]:
                            expr: Expr [39-51]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float[32]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         float[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_float_truncating_fails() {
    let source = "
        complex[float[32]] a;
        float[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-52]:
                        annotations: <empty>
                        kind: ExprStmt [39-52]:
                            expr: Expr [39-51]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float[16]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         float[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_float_expanding_fails() {
    let source = "
        complex[float[32]] a;
        float[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-52]:
                        annotations: <empty>
                        kind: ExprStmt [39-52]:
                            expr: Expr [39-51]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type float[64]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
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
fn complex_to_angle_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-37]:
                        annotations: <empty>
                        kind: ExprStmt [28-37]:
                            expr: Expr [28-36]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type angle
               ,-[test:3:9]
             2 |         complex a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn complex_to_sized_angle_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type angle[32]
               ,-[test:3:9]
             2 |         complex a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_angle_fails() {
    let source = "
        complex[float[32]] a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-48]:
                        annotations: <empty>
                        kind: ExprStmt [39-48]:
                            expr: Expr [39-47]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_angle_fails() {
    let source = "
        complex[float[32]] a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-52]:
                        annotations: <empty>
                        kind: ExprStmt [39-52]:
                            expr: Expr [39-51]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle[32]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_angle_truncating_fails() {
    let source = "
        complex[float[32]] a;
        angle[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-52]:
                        annotations: <empty>
                        kind: ExprStmt [39-52]:
                            expr: Expr [39-51]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle[16]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         angle[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_sized_angle_expanding_fails() {
    let source = "
        complex[float[32]] a;
        angle[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-52]:
                        annotations: <empty>
                        kind: ExprStmt [39-52]:
                            expr: Expr [39-51]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type angle[64]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
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
fn complex_to_complex() {
    let source = "
        complex a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs: <empty>
                init_expr: Expr [9-19]:
                    ty: const complex[float]
                    kind: Lit: Complex(0.0, 0.0)
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: complex[float]
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn complex_to_sized_complex() {
    let source = "
        complex a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs: <empty>
                init_expr: Expr [9-19]:
                    ty: const complex[float]
                    kind: Lit: Complex(0.0, 0.0)
            ExprStmt [28-50]:
                expr: Expr [28-49]:
                    ty: complex[float[32]]
                    kind: Cast [28-49]:
                        ty: complex[float[32]]
                        ty_exprs:
                            Expr [42-44]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [47-48]:
                            ty: complex[float]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_complex_to_complex() {
    let source = "
        complex[float[32]] a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-30]:
                symbol_id: 8
                ty_span: [9-27]
                ty_exprs:
                    Expr [23-25]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-30]:
                    ty: const complex[float[32]]
                    kind: Lit: Complex(0.0, 0.0)
            ExprStmt [39-50]:
                expr: Expr [39-49]:
                    ty: complex[float]
                    kind: Cast [39-49]:
                        ty: complex[float]
                        ty_exprs: <empty>
                        expr: Expr [47-48]:
                            ty: complex[float[32]]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_complex() {
    let source = "
        complex[float[32]] a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-30]:
                symbol_id: 8
                ty_span: [9-27]
                ty_exprs:
                    Expr [23-25]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-30]:
                    ty: const complex[float[32]]
                    kind: Lit: Complex(0.0, 0.0)
            ExprStmt [39-61]:
                expr: Expr [39-60]:
                    ty: complex[float[32]]
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_complex_truncating() {
    let source = "
        complex[float[32]] a;
        complex[float[16]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-30]:
                symbol_id: 8
                ty_span: [9-27]
                ty_exprs:
                    Expr [23-25]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-30]:
                    ty: const complex[float[32]]
                    kind: Lit: Complex(0.0, 0.0)
            ExprStmt [39-61]:
                expr: Expr [39-60]:
                    ty: complex[float[32]]
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn sized_complex_to_sized_complex_expanding() {
    let source = "
        complex[float[32]] a;
        complex[float[64]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-30]:
                symbol_id: 8
                ty_span: [9-27]
                ty_exprs:
                    Expr [23-25]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-30]:
                    ty: const complex[float[32]]
                    kind: Lit: Complex(0.0, 0.0)
            ExprStmt [39-61]:
                expr: Expr [39-60]:
                    ty: complex[float[64]]
                    kind: Cast [39-60]:
                        ty: complex[float[64]]
                        ty_exprs:
                            Expr [53-55]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [58-59]:
                            ty: complex[float[32]]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn complex_to_bit_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-35]:
                        annotations: <empty>
                        kind: ExprStmt [28-35]:
                            expr: Expr [28-34]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type bit
               ,-[test:3:9]
             2 |         complex a;
             3 |         bit(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn complex_to_bitarray_fails() {
    let source = "
        complex a;
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
                                ty: const complex[float]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: complex[float]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type bit[32]
               ,-[test:3:9]
             2 |         complex a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_bit_fails() {
    let source = "
        complex[float[32]] a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-46]:
                        annotations: <empty>
                        kind: ExprStmt [39-46]:
                            expr: Expr [39-45]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         bit(a);
               :         ^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_bitarray_fails() {
    let source = "
        complex[float[32]] a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-50]:
                        annotations: <empty>
                        kind: ExprStmt [39-50]:
                            expr: Expr [39-49]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit[32]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_bitarray_truncating_fails() {
    let source = "
        complex[float[32]] a;
        bit[16](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-50]:
                        annotations: <empty>
                        kind: ExprStmt [39-50]:
                            expr: Expr [39-49]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit[16]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         bit[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_complex_to_bitarray_expanding_fails() {
    let source = "
        complex[float[32]] a;
        bit[64](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-30]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-30]:
                            symbol_id: 8
                            ty_span: [9-27]
                            ty_exprs:
                                Expr [23-25]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-30]:
                                ty: const complex[float[32]]
                                kind: Lit: Complex(0.0, 0.0)
                    Stmt [39-50]:
                        annotations: <empty>
                        kind: ExprStmt [39-50]:
                            expr: Expr [39-49]:
                                ty: complex[float[32]]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float[32]] to type bit[64]
               ,-[test:3:9]
             2 |         complex[float[32]] a;
             3 |         bit[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}
