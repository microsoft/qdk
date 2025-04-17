// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decl;

#[test]
fn with_no_init_expr_has_generated_lit_expr() {
    check_classical_decl(
        "bool a;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-7]:
                symbol_id: 8
                ty_span: [0-4]
                init_expr: Expr [0-0]:
                    ty: Bool(true)
                    kind: Lit: Bool(false)
            [8] Symbol [5-6]:
                name: a
                type: Bool(false)
                qsharp_type: bool
                io_kind: Default"#]],
    );
}

#[test]
#[ignore = "Unimplemented"]
fn array_with_no_init_expr_has_generated_lit_expr() {
    check_classical_decl(
        "array[bool, 4] a;",
        &expect![[r#"
            Program:
                version: <none>
                statements:
                    Stmt [0-17]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [0-17]:
                            symbol_id: 8
                            ty_span: [0-14]
                            init_expr: Expr [0-0]:
                                ty: Err
                                kind: Err

            [Qasm.Compile.Unimplemented

              x this statement is not yet handled during OpenQASM 3 import: semantic type
              | from array type
               ,-[test:1:1]
             1 | array[bool, 4] a;
               : ^^^^^^^^^^^^^^
               `----
            ]"#]],
    );
}

#[test]
fn decl_with_lit_false_init_expr() {
    check_classical_decl(
        "bool a = false;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-15]:
                symbol_id: 8
                ty_span: [0-4]
                init_expr: Expr [9-14]:
                    ty: Bool(false)
                    kind: Lit: Bool(false)
            [8] Symbol [5-6]:
                name: a
                type: Bool(false)
                qsharp_type: bool
                io_kind: Default"#]],
    );
}

#[test]
fn decl_with_lit_true_init_expr() {
    check_classical_decl(
        "bool a = true;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-14]:
                symbol_id: 8
                ty_span: [0-4]
                init_expr: Expr [9-13]:
                    ty: Bool(false)
                    kind: Lit: Bool(true)
            [8] Symbol [5-6]:
                name: a
                type: Bool(false)
                qsharp_type: bool
                io_kind: Default"#]],
    );
}

#[test]
fn const_decl_with_lit_false_init_expr() {
    check_classical_decl(
        "const bool a = false;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-21]:
                symbol_id: 8
                ty_span: [6-10]
                init_expr: Expr [15-20]:
                    ty: Bool(true)
                    kind: Lit: Bool(false)
            [8] Symbol [11-12]:
                name: a
                type: Bool(true)
                qsharp_type: bool
                io_kind: Default"#]],
    );
}

#[test]
fn const_decl_with_lit_true_init_expr() {
    check_classical_decl(
        "const bool a = true;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-20]:
                symbol_id: 8
                ty_span: [6-10]
                init_expr: Expr [15-19]:
                    ty: Bool(true)
                    kind: Lit: Bool(true)
            [8] Symbol [11-12]:
                name: a
                type: Bool(true)
                qsharp_type: bool
                io_kind: Default"#]],
    );
}
