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
fn uint_to_bool() {
    let source = "
        uint a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-33]:
                expr: Expr [25-32]:
                    ty: bool
                    kind: Cast [25-32]:
                        ty: bool
                        ty_exprs: <empty>
                        expr: Expr [30-31]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_bool() {
    let source = "
        uint[32] a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-37]:
                expr: Expr [29-36]:
                    ty: bool
                    kind: Cast [29-36]:
                        ty: bool
                        ty_exprs: <empty>
                        expr: Expr [34-35]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn uint_to_duration_fails() {
    let source = "
        uint a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-16]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-16]:
                            symbol_id: 8
                            ty_span: [9-13]
                            ty_exprs: <empty>
                            init_expr: Expr [9-16]:
                                ty: const uint
                                kind: Lit: Int(0)
                    Stmt [25-37]:
                        annotations: <empty>
                        kind: ExprStmt [25-37]:
                            expr: Expr [25-36]:
                                ty: uint
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type duration
               ,-[test:3:9]
             2 |         uint a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_uint_to_duration_fails() {
    let source = "
        uint[32] a;
        duration(a);
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
                            ty_exprs:
                                Expr [14-16]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-20]:
                                ty: const uint[32]
                                kind: Lit: Int(0)
                    Stmt [29-41]:
                        annotations: <empty>
                        kind: ExprStmt [29-41]:
                            expr: Expr [29-40]:
                                ty: uint[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type duration
               ,-[test:3:9]
             2 |         uint[32] a;
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
fn uint_to_int() {
    let source = "
        uint a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-32]:
                expr: Expr [25-31]:
                    ty: int
                    kind: Cast [25-31]:
                        ty: int
                        ty_exprs: <empty>
                        expr: Expr [29-30]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn uint_to_sized_int() {
    let source = "
        uint a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-36]:
                expr: Expr [25-35]:
                    ty: int[32]
                    kind: Cast [25-35]:
                        ty: int[32]
                        ty_exprs:
                            Expr [29-31]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [33-34]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_int() {
    let source = "
        uint[32] a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-36]:
                expr: Expr [29-35]:
                    ty: int
                    kind: Cast [29-35]:
                        ty: int
                        ty_exprs: <empty>
                        expr: Expr [33-34]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_int() {
    let source = "
        uint[32] a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-40]:
                expr: Expr [29-39]:
                    ty: int[32]
                    kind: Cast [29-39]:
                        ty: int[32]
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [37-38]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_int_truncating() {
    let source = "
        uint[32] a;
        int[16](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-40]:
                expr: Expr [29-39]:
                    ty: int[16]
                    kind: Cast [29-39]:
                        ty: int[16]
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [37-38]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_int_expanding() {
    let source = "
        uint[32] a;
        int[64](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-40]:
                expr: Expr [29-39]:
                    ty: int[64]
                    kind: Cast [29-39]:
                        ty: int[64]
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [37-38]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn uint_to_uint() {
    let source = "
        uint a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-33]:
                expr: Expr [25-32]:
                    ty: uint
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn uint_to_sized_uint() {
    let source = "
        uint a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-37]:
                expr: Expr [25-36]:
                    ty: uint[32]
                    kind: Cast [25-36]:
                        ty: uint[32]
                        ty_exprs:
                            Expr [30-32]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [34-35]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_uint() {
    let source = "
        uint[32] a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-37]:
                expr: Expr [29-36]:
                    ty: uint
                    kind: Cast [29-36]:
                        ty: uint
                        ty_exprs: <empty>
                        expr: Expr [34-35]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_uint() {
    let source = "
        uint[32] a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-41]:
                expr: Expr [29-40]:
                    ty: uint[32]
                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_uint_truncating() {
    let source = "
        uint[32] a;
        uint[16](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-41]:
                expr: Expr [29-40]:
                    ty: uint[16]
                    kind: Cast [29-40]:
                        ty: uint[16]
                        ty_exprs:
                            Expr [34-36]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [38-39]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_uint_expanding() {
    let source = "
        uint[32] a;
        uint[64](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-41]:
                expr: Expr [29-40]:
                    ty: uint[64]
                    kind: Cast [29-40]:
                        ty: uint[64]
                        ty_exprs:
                            Expr [34-36]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [38-39]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn uint_to_float() {
    let source = "
        uint a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-34]:
                expr: Expr [25-33]:
                    ty: float
                    kind: Cast [25-33]:
                        ty: float
                        ty_exprs: <empty>
                        expr: Expr [31-32]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn uint_to_sized_float() {
    let source = "
        uint a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-38]:
                expr: Expr [25-37]:
                    ty: float[32]
                    kind: Cast [25-37]:
                        ty: float[32]
                        ty_exprs:
                            Expr [31-33]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [35-36]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_float() {
    let source = "
        uint[32] a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-38]:
                expr: Expr [29-37]:
                    ty: float
                    kind: Cast [29-37]:
                        ty: float
                        ty_exprs: <empty>
                        expr: Expr [35-36]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_float() {
    let source = "
        uint[32] a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-42]:
                expr: Expr [29-41]:
                    ty: float[32]
                    kind: Cast [29-41]:
                        ty: float[32]
                        ty_exprs:
                            Expr [35-37]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [39-40]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_float_truncating() {
    let source = "
        uint[32] a;
        float[16](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-42]:
                expr: Expr [29-41]:
                    ty: float[16]
                    kind: Cast [29-41]:
                        ty: float[16]
                        ty_exprs:
                            Expr [35-37]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [39-40]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_float_expanding() {
    let source = "
        uint[32] a;
        float[64](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-42]:
                expr: Expr [29-41]:
                    ty: float[64]
                    kind: Cast [29-41]:
                        ty: float[64]
                        ty_exprs:
                            Expr [35-37]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [39-40]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn uint_to_angle_fails() {
    let source = "
        uint a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-16]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-16]:
                            symbol_id: 8
                            ty_span: [9-13]
                            ty_exprs: <empty>
                            init_expr: Expr [9-16]:
                                ty: const uint
                                kind: Lit: Int(0)
                    Stmt [25-34]:
                        annotations: <empty>
                        kind: ExprStmt [25-34]:
                            expr: Expr [25-33]:
                                ty: uint
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type angle
               ,-[test:3:9]
             2 |         uint a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn uint_to_sized_angle_fails() {
    let source = "
        uint a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-16]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-16]:
                            symbol_id: 8
                            ty_span: [9-13]
                            ty_exprs: <empty>
                            init_expr: Expr [9-16]:
                                ty: const uint
                                kind: Lit: Int(0)
                    Stmt [25-38]:
                        annotations: <empty>
                        kind: ExprStmt [25-38]:
                            expr: Expr [25-37]:
                                ty: uint
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type angle[32]
               ,-[test:3:9]
             2 |         uint a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_uint_to_angle_fails() {
    let source = "
        uint[32] a;
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
                            ty_exprs:
                                Expr [14-16]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-20]:
                                ty: const uint[32]
                                kind: Lit: Int(0)
                    Stmt [29-38]:
                        annotations: <empty>
                        kind: ExprStmt [29-38]:
                            expr: Expr [29-37]:
                                ty: uint[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle
               ,-[test:3:9]
             2 |         uint[32] a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_uint_to_sized_angle_fails() {
    let source = "
        uint[32] a;
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
                            ty_exprs:
                                Expr [14-16]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-20]:
                                ty: const uint[32]
                                kind: Lit: Int(0)
                    Stmt [29-42]:
                        annotations: <empty>
                        kind: ExprStmt [29-42]:
                            expr: Expr [29-41]:
                                ty: uint[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle[32]
               ,-[test:3:9]
             2 |         uint[32] a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_uint_to_sized_angle_truncating_fails() {
    let source = "
        uint[32] a;
        angle[16](a);
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
                            ty_exprs:
                                Expr [14-16]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-20]:
                                ty: const uint[32]
                                kind: Lit: Int(0)
                    Stmt [29-42]:
                        annotations: <empty>
                        kind: ExprStmt [29-42]:
                            expr: Expr [29-41]:
                                ty: uint[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle[16]
               ,-[test:3:9]
             2 |         uint[32] a;
             3 |         angle[16](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_uint_to_sized_angle_expanding_fails() {
    let source = "
        uint[32] a;
        angle[64](a);
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
                            ty_exprs:
                                Expr [14-16]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-20]:
                                ty: const uint[32]
                                kind: Lit: Int(0)
                    Stmt [29-42]:
                        annotations: <empty>
                        kind: ExprStmt [29-42]:
                            expr: Expr [29-41]:
                                ty: uint[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type angle[64]
               ,-[test:3:9]
             2 |         uint[32] a;
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
fn uint_to_complex() {
    let source = "
        uint a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-36]:
                expr: Expr [25-35]:
                    ty: complex[float]
                    kind: Cast [25-35]:
                        ty: complex[float]
                        ty_exprs: <empty>
                        expr: Expr [33-34]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn uint_to_sized_complex() {
    let source = "
        uint a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-47]:
                expr: Expr [25-46]:
                    ty: complex[float[32]]
                    kind: Cast [25-46]:
                        ty: complex[float[32]]
                        ty_exprs:
                            Expr [39-41]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [44-45]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_complex() {
    let source = "
        uint[32] a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-40]:
                expr: Expr [29-39]:
                    ty: complex[float]
                    kind: Cast [29-39]:
                        ty: complex[float]
                        ty_exprs: <empty>
                        expr: Expr [37-38]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_complex() {
    let source = "
        uint[32] a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-51]:
                expr: Expr [29-50]:
                    ty: complex[float[32]]
                    kind: Cast [29-50]:
                        ty: complex[float[32]]
                        ty_exprs:
                            Expr [43-45]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [48-49]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_complex_truncating() {
    let source = "
        uint[32] a;
        complex[float[16]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-51]:
                expr: Expr [29-50]:
                    ty: complex[float[16]]
                    kind: Cast [29-50]:
                        ty: complex[float[16]]
                        ty_exprs:
                            Expr [43-45]:
                                ty: const uint
                                const_value: Int(16)
                                kind: Lit: Int(16)
                        expr: Expr [48-49]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_sized_complex_expanding() {
    let source = "
        uint[32] a;
        complex[float[64]](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-51]:
                expr: Expr [29-50]:
                    ty: complex[float[64]]
                    kind: Cast [29-50]:
                        ty: complex[float[64]]
                        ty_exprs:
                            Expr [43-45]:
                                ty: const uint
                                const_value: Int(64)
                                kind: Lit: Int(64)
                        expr: Expr [48-49]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn uint_to_bit() {
    let source = "
        uint a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-16]:
                symbol_id: 8
                ty_span: [9-13]
                ty_exprs: <empty>
                init_expr: Expr [9-16]:
                    ty: const uint
                    kind: Lit: Int(0)
            ExprStmt [25-32]:
                expr: Expr [25-31]:
                    ty: bit
                    kind: Cast [25-31]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [29-30]:
                            ty: uint
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn uint_to_bitarray_fails() {
    let source = "
        uint a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-16]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-16]:
                            symbol_id: 8
                            ty_span: [9-13]
                            ty_exprs: <empty>
                            init_expr: Expr [9-16]:
                                ty: const uint
                                kind: Lit: Int(0)
                    Stmt [25-36]:
                        annotations: <empty>
                        kind: ExprStmt [25-36]:
                            expr: Expr [25-35]:
                                ty: uint
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint to type bit[32]
               ,-[test:3:9]
             2 |         uint a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_uint_to_bit() {
    let source = "
        uint[32] a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-36]:
                expr: Expr [29-35]:
                    ty: bit
                    kind: Cast [29-35]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [33-34]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_bitarray() {
    let source = "
        uint[32] a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-17]
                ty_exprs:
                    Expr [14-16]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [9-20]:
                    ty: const uint[32]
                    kind: Lit: Int(0)
            ExprStmt [29-40]:
                expr: Expr [29-39]:
                    ty: bit[32]
                    kind: Cast [29-39]:
                        ty: bit[32]
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                        expr: Expr [37-38]:
                            ty: uint[32]
                            kind: SymbolId(8)
                        kind: Explicit
        "#]],
    );
}

#[test]
fn sized_uint_to_bitarray_truncating_fails() {
    let source = "
        uint[32] a;
        bit[16](a);
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
                            ty_exprs:
                                Expr [14-16]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-20]:
                                ty: const uint[32]
                                kind: Lit: Int(0)
                    Stmt [29-40]:
                        annotations: <empty>
                        kind: ExprStmt [29-40]:
                            expr: Expr [29-39]:
                                ty: uint[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type bit[16]
               ,-[test:3:9]
             2 |         uint[32] a;
             3 |         bit[16](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn sized_uint_to_bitarray_expanding_fails() {
    let source = "
        uint[32] a;
        bit[64](a);
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
                            ty_exprs:
                                Expr [14-16]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [9-20]:
                                ty: const uint[32]
                                kind: Lit: Int(0)
                    Stmt [29-40]:
                        annotations: <empty>
                        kind: ExprStmt [29-40]:
                            expr: Expr [29-39]:
                                ty: uint[32]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type uint[32] to type bit[64]
               ,-[test:3:9]
             2 |         uint[32] a;
             3 |         bit[64](a);
               :         ^^^^^^^^^^
             4 |     
               `----
            ]"#]],
    );
}
