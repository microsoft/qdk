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
fn int_to_bool() {
    let source = "
        int a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-32]:
                expr: Expr [24-31]:
                    ty: bool
                    kind: Cast [24-31]:
                        ty: bool
                        ty_exprs: <empty>
                        expr: Expr [29-30]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_bool() {
    let source = "
        int[32] a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-36]:
                expr: Expr [28-35]:
                    ty: bool
                    kind: Cast [28-35]:
                        ty: bool
                        ty_exprs: <empty>
                        expr: Expr [33-34]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn int_to_duration_fails() {
    let source = "
        int a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            ty_exprs: <empty>
                            init_expr: Expr [9-15]:
                                ty: const int
                                kind: Lit: Int(0)
                    Stmt [24-36]:
                        annotations: <empty>
                        kind: ExprStmt [24-36]:
                            expr: Expr [24-35]:
                                ty: int
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int to type duration
               ,-[test:3:9]
             2 |         int a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_int_to_duration_fails() {
    let source = "
        int[32] a;
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
                            ty_exprs:
                                Expr [13-15]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-19]:
                                ty: const int[32]
                                kind: Lit: Int(0)
                    Stmt [28-40]:
                        annotations: <empty>
                        kind: ExprStmt [28-40]:
                            expr: Expr [28-39]:
                                ty: int[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int[32] to type duration
               ,-[test:3:9]
             2 |         int[32] a;
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
fn int_to_int() {
    let source = "
        int a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-31]:
                expr: Expr [24-30]:
                    ty: int
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn int_to_sized_int() {
    let source = "
        int a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-35]:
                expr: Expr [24-34]:
                    ty: int[32]
                    kind: Cast [24-34]:
                        ty: int[32]
                        ty_exprs:
                            Expr [28-30]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [32-33]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_int() {
    let source = "
        int[32] a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-35]:
                expr: Expr [28-34]:
                    ty: int
                    kind: Cast [28-34]:
                        ty: int
                        ty_exprs: <empty>
                        expr: Expr [32-33]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_int() {
    let source = "
        int[32] a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: int[32]
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn sized_int_to_sized_int_truncating() {
    let source = "
        int[32] a;
        int[16](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: int[16]
                    kind: Cast [28-38]:
                        ty: int[16]
                        ty_exprs:
                            Expr [32-34]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [36-37]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_int_expanding() {
    let source = "
        int[32] a;
        int[64](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: int[64]
                    kind: Cast [28-38]:
                        ty: int[64]
                        ty_exprs:
                            Expr [32-34]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [36-37]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn int_to_uint() {
    let source = "
        int a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-32]:
                expr: Expr [24-31]:
                    ty: uint
                    kind: Cast [24-31]:
                        ty: uint
                        ty_exprs: <empty>
                        expr: Expr [29-30]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn int_to_sized_uint() {
    let source = "
        int a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-36]:
                expr: Expr [24-35]:
                    ty: uint[32]
                    kind: Cast [24-35]:
                        ty: uint[32]
                        ty_exprs:
                            Expr [29-31]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [33-34]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_uint() {
    let source = "
        int[32] a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-36]:
                expr: Expr [28-35]:
                    ty: uint
                    kind: Cast [28-35]:
                        ty: uint
                        ty_exprs: <empty>
                        expr: Expr [33-34]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_uint() {
    let source = "
        int[32] a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-40]:
                expr: Expr [28-39]:
                    ty: uint[32]
                    kind: Cast [28-39]:
                        ty: uint[32]
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [37-38]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_uint_truncating() {
    let source = "
        int[32] a;
        uint[16](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-40]:
                expr: Expr [28-39]:
                    ty: uint[16]
                    kind: Cast [28-39]:
                        ty: uint[16]
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [37-38]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_uint_expanding() {
    let source = "
        int[32] a;
        uint[64](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-40]:
                expr: Expr [28-39]:
                    ty: uint[64]
                    kind: Cast [28-39]:
                        ty: uint[64]
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [37-38]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn int_to_float() {
    let source = "
        int a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-33]:
                expr: Expr [24-32]:
                    ty: float
                    kind: Cast [24-32]:
                        ty: float
                        ty_exprs: <empty>
                        expr: Expr [30-31]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn int_to_sized_float() {
    let source = "
        int a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-37]:
                expr: Expr [24-36]:
                    ty: float[32]
                    kind: Cast [24-36]:
                        ty: float[32]
                        ty_exprs:
                            Expr [30-32]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [34-35]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_float() {
    let source = "
        int[32] a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-37]:
                expr: Expr [28-36]:
                    ty: float
                    kind: Cast [28-36]:
                        ty: float
                        ty_exprs: <empty>
                        expr: Expr [34-35]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_float() {
    let source = "
        int[32] a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-41]:
                expr: Expr [28-40]:
                    ty: float[32]
                    kind: Cast [28-40]:
                        ty: float[32]
                        ty_exprs:
                            Expr [34-36]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [38-39]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_float_truncating() {
    let source = "
        int[32] a;
        float[16](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-41]:
                expr: Expr [28-40]:
                    ty: float[16]
                    kind: Cast [28-40]:
                        ty: float[16]
                        ty_exprs:
                            Expr [34-36]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [38-39]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_float_expanding() {
    let source = "
        int[32] a;
        float[64](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-41]:
                expr: Expr [28-40]:
                    ty: float[64]
                    kind: Cast [28-40]:
                        ty: float[64]
                        ty_exprs:
                            Expr [34-36]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [38-39]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn int_to_angle_fails() {
    let source = "
        int a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            ty_exprs: <empty>
                            init_expr: Expr [9-15]:
                                ty: const int
                                kind: Lit: Int(0)
                    Stmt [24-33]:
                        annotations: <empty>
                        kind: ExprStmt [24-33]:
                            expr: Expr [24-32]:
                                ty: int
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int to type angle
               ,-[test:3:9]
             2 |         int a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn int_to_sized_angle_fails() {
    let source = "
        int a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            ty_exprs: <empty>
                            init_expr: Expr [9-15]:
                                ty: const int
                                kind: Lit: Int(0)
                    Stmt [24-37]:
                        annotations: <empty>
                        kind: ExprStmt [24-37]:
                            expr: Expr [24-36]:
                                ty: int
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int to type angle[32]
               ,-[test:3:9]
             2 |         int a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_int_to_angle_fails() {
    let source = "
        int[32] a;
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
                            ty_exprs:
                                Expr [13-15]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-19]:
                                ty: const int[32]
                                kind: Lit: Int(0)
                    Stmt [28-37]:
                        annotations: <empty>
                        kind: ExprStmt [28-37]:
                            expr: Expr [28-36]:
                                ty: int[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int[32] to type angle
               ,-[test:3:9]
             2 |         int[32] a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_int_to_sized_angle_fails() {
    let source = "
        int[32] a;
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
                            ty_exprs:
                                Expr [13-15]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-19]:
                                ty: const int[32]
                                kind: Lit: Int(0)
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: int[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int[32] to type angle[32]
               ,-[test:3:9]
             2 |         int[32] a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_int_to_sized_angle_truncating_fails() {
    let source = "
        int[32] a;
        angle[16](a);
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
                            ty_exprs:
                                Expr [13-15]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-19]:
                                ty: const int[32]
                                kind: Lit: Int(0)
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: int[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int[32] to type angle[16]
               ,-[test:3:9]
             2 |         int[32] a;
             3 |         angle[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_int_to_sized_angle_expanding_fails() {
    let source = "
        int[32] a;
        angle[64](a);
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
                            ty_exprs:
                                Expr [13-15]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-19]:
                                ty: const int[32]
                                kind: Lit: Int(0)
                    Stmt [28-41]:
                        annotations: <empty>
                        kind: ExprStmt [28-41]:
                            expr: Expr [28-40]:
                                ty: int[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int[32] to type angle[64]
               ,-[test:3:9]
             2 |         int[32] a;
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
fn int_to_complex() {
    let source = "
        int a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-35]:
                expr: Expr [24-34]:
                    ty: complex[float]
                    kind: Cast [24-34]:
                        ty: complex[float]
                        ty_exprs: <empty>
                        expr: Expr [32-33]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn int_to_sized_complex() {
    let source = "
        int a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-46]:
                expr: Expr [24-45]:
                    ty: complex[float[32]]
                    kind: Cast [24-45]:
                        ty: complex[float[32]]
                        ty_exprs:
                            Expr [38-40]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [43-44]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_complex() {
    let source = "
        int[32] a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: complex[float]
                    kind: Cast [28-38]:
                        ty: complex[float]
                        ty_exprs: <empty>
                        expr: Expr [36-37]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_complex() {
    let source = "
        int[32] a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
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
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_complex_truncating() {
    let source = "
        int[32] a;
        complex[float[16]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-50]:
                expr: Expr [28-49]:
                    ty: complex[float[16]]
                    kind: Cast [28-49]:
                        ty: complex[float[16]]
                        ty_exprs:
                            Expr [42-44]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [47-48]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_sized_complex_expanding() {
    let source = "
        int[32] a;
        complex[float[64]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-50]:
                expr: Expr [28-49]:
                    ty: complex[float[64]]
                    kind: Cast [28-49]:
                        ty: complex[float[64]]
                        ty_exprs:
                            Expr [42-44]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [47-48]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn int_to_bit() {
    let source = "
        int a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-15]:
                symbol_id: 8
                ty_span: [9-12]
                ty_exprs: <empty>
                init_expr: Expr [9-15]:
                    ty: const int
                    kind: Lit: Int(0)
            ExprStmt [24-31]:
                expr: Expr [24-30]:
                    ty: bit
                    kind: Cast [24-30]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [28-29]:
                            ty: int
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn int_to_bitarray_fails() {
    let source = "
        int a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            ty_exprs: <empty>
                            init_expr: Expr [9-15]:
                                ty: const int
                                kind: Lit: Int(0)
                    Stmt [24-35]:
                        annotations: <empty>
                        kind: ExprStmt [24-35]:
                            expr: Expr [24-34]:
                                ty: int
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int to type bit[32]
               ,-[test:3:9]
             2 |         int a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_int_to_bit() {
    let source = "
        int[32] a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-35]:
                expr: Expr [28-34]:
                    ty: bit
                    kind: Cast [28-34]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [32-33]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_bitarray() {
    let source = "
        int[32] a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-19]:
                symbol_id: 8
                ty_span: [9-16]
                ty_exprs:
                    Expr [13-15]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-19]:
                    ty: const int[32]
                    kind: Lit: Int(0)
            ExprStmt [28-39]:
                expr: Expr [28-38]:
                    ty: bit[32]
                    kind: Cast [28-38]:
                        ty: bit[32]
                        ty_exprs:
                            Expr [32-34]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [36-37]:
                            ty: int[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_int_to_bitarray_truncating_fails() {
    let source = "
        int[32] a;
        bit[16](a);
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
                            ty_exprs:
                                Expr [13-15]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-19]:
                                ty: const int[32]
                                kind: Lit: Int(0)
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: int[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int[32] to type bit[16]
               ,-[test:3:9]
             2 |         int[32] a;
             3 |         bit[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_int_to_bitarray_expanding_fails() {
    let source = "
        int[32] a;
        bit[64](a);
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
                            ty_exprs:
                                Expr [13-15]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-19]:
                                ty: const int[32]
                                kind: Lit: Int(0)
                    Stmt [28-39]:
                        annotations: <empty>
                        kind: ExprStmt [28-39]:
                            expr: Expr [28-38]:
                                ty: int[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int[32] to type bit[64]
               ,-[test:3:9]
             2 |         int[32] a;
             3 |         bit[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}
