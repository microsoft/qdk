// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_classical_decl;

#[test]
fn with_no_init_expr_has_generated_lit_expr() {
    check_classical_decl(
        "creg a;",
        &expect![[r#"
            ClassicalDeclarationStmt [0-7]:
                symbol_id: 8
                ty_span: [0-7]
                ty_exprs: <empty>
                init_expr: Expr [0-7]:
                    ty: const bit
                    kind: Lit: Bit(0)
            [8] Symbol [5-6]:
                name: a
                type: bit
                ty_span: [0-7]
                io_kind: Default"#]],
    );
}

#[test]
fn array_with_no_init_expr_has_generated_lit_expr() {
    check_classical_decl(
        "creg a[4];",
        &expect![[r#"
            ClassicalDeclarationStmt [0-10]:
                symbol_id: 8
                ty_span: [0-10]
                ty_exprs:
                    Expr [7-8]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                init_expr: Expr [0-10]:
                    ty: const bit[4]
                    kind: Lit: Bitstring("0000")
            [8] Symbol [5-6]:
                name: a
                type: bit[4]
                ty_span: [0-10]
                io_kind: Default"#]],
    );
}
