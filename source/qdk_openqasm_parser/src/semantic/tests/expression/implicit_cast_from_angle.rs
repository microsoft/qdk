// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decls;

#[test]
fn to_bit_implicitly() {
    let input = "
        angle x = 42.;
        bit y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: const angle
                    kind: Lit: Angle(4.300888156922483)
            [8] Symbol [15-16]:
                name: x
                type: angle
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-42]:
                symbol_id: 9
                ty_span: [32-35]
                ty_exprs: <empty>
                init_expr: Expr [40-41]:
                    ty: bit
                    kind: Cast [40-41]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [40-41]:
                            ty: angle
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [36-37]:
                name: y
                type: bit
                ty_span: [32-35]
                io_kind: Default
        "#]],
    );
}

#[test]
fn explicit_width_to_bit_implicitly_fails() {
    let input = "
        angle[64] x = 42.;
        bit y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-27]:
                symbol_id: 8
                ty_span: [9-18]
                ty_exprs:
                    Expr [15-17]:
                        ty: const uint
                        const_value: Int(64)
                        kind: Lit: Int(64)
                init_expr: Expr [23-26]:
                    ty: const angle[64]
                    kind: Lit: Angle(4.300888156922483)
            [8] Symbol [19-20]:
                name: x
                type: angle[64]
                ty_span: [9-18]
                io_kind: Default
            ClassicalDeclarationStmt [36-46]:
                symbol_id: 9
                ty_span: [36-39]
                ty_exprs: <empty>
                init_expr: Expr [44-45]:
                    ty: bit
                    kind: Cast [44-45]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [44-45]:
                            ty: angle[64]
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [40-41]:
                name: y
                type: bit
                ty_span: [36-39]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_bool_implicitly() {
    let input = "
        angle x = 42.;
        bool y = x;
    ";
    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: const angle
                    kind: Lit: Angle(4.300888156922483)
            [8] Symbol [15-16]:
                name: x
                type: angle
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-43]:
                symbol_id: 9
                ty_span: [32-36]
                ty_exprs: <empty>
                init_expr: Expr [41-42]:
                    ty: bool
                    kind: Cast [41-42]:
                        ty: bool
                        ty_exprs: <empty>
                        expr: Expr [41-42]:
                            ty: angle
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [37-38]:
                name: y
                type: bool
                ty_span: [32-36]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_implicit_int_implicitly_fails() {
    let input = "
        angle x = 42.;
        int y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-42]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-42]:
                            symbol_id: 9
                            ty_span: [32-35]
                            ty_exprs: <empty>
                            init_expr: Expr [40-41]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type int
               ,-[test:3:17]
             2 |         angle x = 42.;
             3 |         int y = x;
               :                 ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_explicit_int_implicitly_fails() {
    let input = "
        angle x = 42.;
        int[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-46]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-46]:
                            symbol_id: 9
                            ty_span: [32-39]
                            ty_exprs:
                                Expr [36-38]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [44-45]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type int[32]
               ,-[test:3:21]
             2 |         angle x = 42.;
             3 |         int[32] y = x;
               :                     ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_implicit_uint_implicitly_fails() {
    let input = "
        angle x = 42.;
        uint y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-43]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-43]:
                            symbol_id: 9
                            ty_span: [32-36]
                            ty_exprs: <empty>
                            init_expr: Expr [41-42]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type uint
               ,-[test:3:18]
             2 |         angle x = 42.;
             3 |         uint y = x;
               :                  ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn negative_lit_to_implicit_uint_implicitly_fails() {
    let input = "
        angle x = -42.;
        uint y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-24]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-24]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [20-23]:
                                ty: angle
                                kind: Cast [20-23]:
                                    ty: angle
                                    ty_exprs: <empty>
                                    expr: Expr [20-23]:
                                        ty: const float
                                        kind: UnaryOpExpr [20-23]:
                                            op: Neg
                                            expr: Expr [20-23]:
                                                ty: const float
                                                kind: Lit: Float(42.0)
                                    kind: Implicit
                    Stmt [33-44]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [33-44]:
                            symbol_id: 9
                            ty_span: [33-37]
                            ty_exprs: <empty>
                            init_expr: Expr [42-43]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type uint
               ,-[test:3:18]
             2 |         angle x = -42.;
             3 |         uint y = x;
               :                  ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_explicit_uint_implicitly_fails() {
    let input = "
        angle x = 42.;
        uint[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-47]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-47]:
                            symbol_id: 9
                            ty_span: [32-40]
                            ty_exprs:
                                Expr [37-39]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [45-46]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type uint[32]
               ,-[test:3:22]
             2 |         angle x = 42.;
             3 |         uint[32] y = x;
               :                      ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_explicit_bigint_implicitly_fails() {
    let input = "
        angle x = 42.;
        int[65] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-46]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-46]:
                            symbol_id: 9
                            ty_span: [32-39]
                            ty_exprs:
                                Expr [36-38]:
                                    ty: const uint
                                    const_value: Int(65)
                                    kind: Lit: Int(65)
                            init_expr: Expr [44-45]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type int[65]
               ,-[test:3:21]
             2 |         angle x = 42.;
             3 |         int[65] y = x;
               :                     ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_implicit_float_implicitly_fails() {
    let input = "
        angle x = 42.;
        float y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-44]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-44]:
                            symbol_id: 9
                            ty_span: [32-37]
                            ty_exprs: <empty>
                            init_expr: Expr [42-43]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type float
               ,-[test:3:19]
             2 |         angle x = 42.;
             3 |         float y = x;
               :                   ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_explicit_float_implicitly_fails() {
    let input = "
        angle x = 42.;
        float[32] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-48]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-48]:
                            symbol_id: 9
                            ty_span: [32-41]
                            ty_exprs:
                                Expr [38-40]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [46-47]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type float[32]
               ,-[test:3:23]
             2 |         angle x = 42.;
             3 |         float[32] y = x;
               :                       ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_implicit_complex_implicitly_fails() {
    let input = "
        angle x = 42.;
        complex[float] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-53]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-53]:
                            symbol_id: 9
                            ty_span: [32-46]
                            ty_exprs: <empty>
                            init_expr: Expr [51-52]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type complex[float]
               ,-[test:3:28]
             2 |         angle x = 42.;
             3 |         complex[float] y = x;
               :                            ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_explicit_complex_implicitly_fails() {
    let input = "
        angle x = 42.;
        complex[float[32]] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-23]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-23]:
                            symbol_id: 8
                            ty_span: [9-14]
                            ty_exprs: <empty>
                            init_expr: Expr [19-22]:
                                ty: const angle
                                kind: Lit: Angle(4.300888156922483)
                    Stmt [32-57]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-57]:
                            symbol_id: 9
                            ty_span: [32-50]
                            ty_exprs:
                                Expr [46-48]:
                                    ty: const uint
                                    const_value: Int(32)
                                    kind: Lit: Int(32)
                            init_expr: Expr [55-56]:
                                ty: angle
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type complex[float[32]]
               ,-[test:3:32]
             2 |         angle x = 42.;
             3 |         complex[float[32]] y = x;
               :                                ^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_angle_implicitly() {
    let input = "
        angle x = 42.;
        angle y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: const angle
                    kind: Lit: Angle(4.300888156922483)
            [8] Symbol [15-16]:
                name: x
                type: angle
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-44]:
                symbol_id: 9
                ty_span: [32-37]
                ty_exprs: <empty>
                init_expr: Expr [42-43]:
                    ty: angle
                    kind: SymbolId(8)
            [9] Symbol [38-39]:
                name: y
                type: angle
                ty_span: [32-37]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_explicit_angle_implicitly() {
    let input = "
        angle x = 42.;
        angle[4] y = x;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-14]
                ty_exprs: <empty>
                init_expr: Expr [19-22]:
                    ty: const angle
                    kind: Lit: Angle(4.300888156922483)
            [8] Symbol [15-16]:
                name: x
                type: angle
                ty_span: [9-14]
                io_kind: Default
            ClassicalDeclarationStmt [32-47]:
                symbol_id: 9
                ty_span: [32-40]
                ty_exprs:
                    Expr [38-39]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                init_expr: Expr [45-46]:
                    ty: angle[4]
                    kind: Cast [45-46]:
                        ty: angle[4]
                        ty_exprs: <empty>
                        expr: Expr [45-46]:
                            ty: angle
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [41-42]:
                name: y
                type: angle[4]
                ty_span: [32-40]
                io_kind: Default
        "#]],
    );
}

#[test]
fn width_promotion() {
    let input = "
        angle[32] x = 1.0;
        angle[48] y = 2.0;
        bit z = x / y;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-27]:
                symbol_id: 8
                ty_span: [9-18]
                ty_exprs:
                    Expr [15-17]:
                        ty: const uint
                        const_value: Int(32)
                        kind: Lit: Int(32)
                init_expr: Expr [23-26]:
                    ty: const angle[32]
                    kind: Lit: Angle(1.000000000619646)
            [8] Symbol [19-20]:
                name: x
                type: angle[32]
                ty_span: [9-18]
                io_kind: Default
            ClassicalDeclarationStmt [36-54]:
                symbol_id: 9
                ty_span: [36-45]
                ty_exprs:
                    Expr [42-44]:
                        ty: const uint
                        const_value: Int(48)
                        kind: Lit: Int(48)
                init_expr: Expr [50-53]:
                    ty: const angle[48]
                    kind: Lit: Angle(1.999999999999999)
            [9] Symbol [46-47]:
                name: y
                type: angle[48]
                ty_span: [36-45]
                io_kind: Default
            ClassicalDeclarationStmt [63-77]:
                symbol_id: 10
                ty_span: [63-66]
                ty_exprs: <empty>
                init_expr: Expr [71-76]:
                    ty: bit
                    kind: Cast [71-76]:
                        ty: bit
                        ty_exprs: <empty>
                        expr: Expr [71-76]:
                            ty: uint[48]
                            kind: BinaryOpExpr:
                                op: Div
                                lhs: Expr [71-72]:
                                    ty: angle[48]
                                    kind: Cast [71-72]:
                                        ty: angle[48]
                                        ty_exprs: <empty>
                                        expr: Expr [71-72]:
                                            ty: angle[32]
                                            kind: SymbolId(8)
                                        kind: Implicit
                                rhs: Expr [75-76]:
                                    ty: angle[48]
                                    kind: SymbolId(9)
                        kind: Implicit
            [10] Symbol [67-68]:
                name: z
                type: bit
                ty_span: [63-66]
                io_kind: Default
        "#]],
    );
}
