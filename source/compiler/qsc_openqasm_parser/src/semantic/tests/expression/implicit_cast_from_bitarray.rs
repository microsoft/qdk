// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::{check_classical_decls, check_stmt_kinds};

#[test]
fn to_int_decl_implicitly() {
    let input = r#"
        bit[5] reg;
        int b = reg;
    "#;

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [9-20]:
                    ty: const bit[5]
                    kind: Lit: Bitstring("00000")
            [8] Symbol [16-19]:
                name: reg
                type: bit[5]
                ty_span: [9-15]
                io_kind: Default
            ClassicalDeclarationStmt [29-41]:
                symbol_id: 9
                ty_span: [29-32]
                ty_exprs: <empty>
                init_expr: Expr [37-40]:
                    ty: int
                    kind: Cast [37-40]:
                        ty: int
                        ty_exprs: <empty>
                        expr: Expr [37-40]:
                            ty: bit[5]
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [33-34]:
                name: b
                type: int
                ty_span: [29-32]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_int_assignment_implicitly() {
    let input = r#"
        bit[5] reg;
        int a;
        a = reg;
    "#;

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [9-20]:
                    ty: const bit[5]
                    kind: Lit: Bitstring("00000")
            ClassicalDeclarationStmt [29-35]:
                symbol_id: 9
                ty_span: [29-32]
                ty_exprs: <empty>
                init_expr: Expr [29-35]:
                    ty: const int
                    kind: Lit: Int(0)
            AssignStmt [44-52]:
                lhs: Expr [44-45]:
                    ty: int
                    kind: SymbolId(9)
                rhs: Expr [48-51]:
                    ty: int
                    kind: Cast [48-51]:
                        ty: int
                        ty_exprs: <empty>
                        expr: Expr [48-51]:
                            ty: bit[5]
                            kind: SymbolId(8)
                        kind: Implicit
        "#]],
    );
}

#[test]
fn to_int_with_equal_width_in_assignment_implicitly() {
    let input = r#"
        bit[5] reg;
        int[5] a;
        a = reg;
    "#;

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [9-20]:
                    ty: const bit[5]
                    kind: Lit: Bitstring("00000")
            ClassicalDeclarationStmt [29-38]:
                symbol_id: 9
                ty_span: [29-35]
                ty_exprs:
                    Expr [33-34]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [29-38]:
                    ty: const int[5]
                    kind: Lit: Int(0)
            AssignStmt [47-55]:
                lhs: Expr [47-48]:
                    ty: int[5]
                    kind: SymbolId(9)
                rhs: Expr [51-54]:
                    ty: int[5]
                    kind: Cast [51-54]:
                        ty: int[5]
                        ty_exprs: <empty>
                        expr: Expr [51-54]:
                            ty: bit[5]
                            kind: SymbolId(8)
                        kind: Implicit
        "#]],
    );
}

#[test]
fn to_int_with_equal_width_in_decl_implicitly() {
    let input = r#"
        bit[5] reg;
        int[5] a = reg;
    "#;

    check_classical_decls(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-20]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [9-20]:
                    ty: const bit[5]
                    kind: Lit: Bitstring("00000")
            [8] Symbol [16-19]:
                name: reg
                type: bit[5]
                ty_span: [9-15]
                io_kind: Default
            ClassicalDeclarationStmt [29-44]:
                symbol_id: 9
                ty_span: [29-35]
                ty_exprs:
                    Expr [33-34]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                init_expr: Expr [40-43]:
                    ty: int[5]
                    kind: Cast [40-43]:
                        ty: int[5]
                        ty_exprs: <empty>
                        expr: Expr [40-43]:
                            ty: bit[5]
                            kind: SymbolId(8)
                        kind: Implicit
            [9] Symbol [36-37]:
                name: a
                type: int[5]
                ty_span: [29-35]
                io_kind: Default
        "#]],
    );
}

#[test]
fn to_int_with_higher_width_implicitly_fails() {
    let input = "
        int[6] a;
        bit[5] reg;
        a = reg;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-18]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-18]:
                            symbol_id: 8
                            ty_span: [9-15]
                            ty_exprs:
                                Expr [13-14]:
                                    ty: const uint
                                    const_value: Int(6)
                                    kind: Lit: Int(6)
                            init_expr: Expr [9-18]:
                                ty: const int[6]
                                kind: Lit: Int(0)
                    Stmt [27-38]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [27-38]:
                            symbol_id: 9
                            ty_span: [27-33]
                            ty_exprs:
                                Expr [31-32]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                            init_expr: Expr [27-38]:
                                ty: const bit[5]
                                kind: Lit: Bitstring("00000")
                    Stmt [47-55]:
                        annotations: <empty>
                        kind: AssignStmt [47-55]:
                            lhs: Expr [47-48]:
                                ty: int[6]
                                kind: SymbolId(8)
                            rhs: Expr [51-54]:
                                ty: bit[5]
                                kind: SymbolId(9)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type bit[5] to type int[6]
               ,-[test:4:13]
             3 |         bit[5] reg;
             4 |         a = reg;
               :             ^^^
             5 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_int_with_higher_width_decl_implicitly_fails() {
    let input = "
        bit[5] reg;
        int[6] a = reg;
    ";
    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-15]
                            ty_exprs:
                                Expr [13-14]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                            init_expr: Expr [9-20]:
                                ty: const bit[5]
                                kind: Lit: Bitstring("00000")
                    Stmt [29-44]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [29-44]:
                            symbol_id: 9
                            ty_span: [29-35]
                            ty_exprs:
                                Expr [33-34]:
                                    ty: const uint
                                    const_value: Int(6)
                                    kind: Lit: Int(6)
                            init_expr: Expr [40-43]:
                                ty: bit[5]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type bit[5] to type int[6]
               ,-[test:3:20]
             2 |         bit[5] reg;
             3 |         int[6] a = reg;
               :                    ^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_int_with_lower_width_implicitly_fails() {
    let input = "
        input int[4] a;
        bit[5] reg;
        a = reg;
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
                        kind: InputDeclaration [9-24]:
                            symbol_id: 8
                            ty_exprs:
                                Expr [19-20]:
                                    ty: const uint
                                    const_value: Int(4)
                                    kind: Lit: Int(4)
                    Stmt [33-44]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [33-44]:
                            symbol_id: 9
                            ty_span: [33-39]
                            ty_exprs:
                                Expr [37-38]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                            init_expr: Expr [33-44]:
                                ty: const bit[5]
                                kind: Lit: Bitstring("00000")
                    Stmt [53-61]:
                        annotations: <empty>
                        kind: AssignStmt [53-61]:
                            lhs: Expr [53-54]:
                                ty: int[4]
                                kind: SymbolId(8)
                            rhs: Expr [57-60]:
                                ty: bit[5]
                                kind: SymbolId(9)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type bit[5] to type int[4]
               ,-[test:4:13]
             3 |         bit[5] reg;
             4 |         a = reg;
               :             ^^^
             5 |     
               `----
            ]"#]],
    );
}

#[test]
fn to_int_with_lower_width_decl_implicitly_fails() {
    let input = "
        bit[5] reg;
        int[4] a = reg;
    ";

    check_classical_decls(
        input,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-20]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-20]:
                            symbol_id: 8
                            ty_span: [9-15]
                            ty_exprs:
                                Expr [13-14]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                            init_expr: Expr [9-20]:
                                ty: const bit[5]
                                kind: Lit: Bitstring("00000")
                    Stmt [29-44]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [29-44]:
                            symbol_id: 9
                            ty_span: [29-35]
                            ty_exprs:
                                Expr [33-34]:
                                    ty: const uint
                                    const_value: Int(4)
                                    kind: Lit: Int(4)
                            init_expr: Expr [40-43]:
                                ty: bit[5]
                                kind: SymbolId(8)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type bit[5] to type int[4]
               ,-[test:3:20]
             2 |         bit[5] reg;
             3 |         int[4] a = reg;
               :                    ^^^
             4 |     
               `----
            ]"#]],
    );
}
