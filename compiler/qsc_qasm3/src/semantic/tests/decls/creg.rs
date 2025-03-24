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
                init_expr: Expr [0-0]:
                    ty: Bit(true)
                    kind: Lit: Bit(0)
            [8] Symbol [5-6]:
                name: a
                type: Bit(false)
                qsharp_type: Result
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
                init_expr: Expr [0-0]:
                    ty: BitArray(One(4), true)
                    kind: Lit: Bitstring("0000")
            [8] Symbol [5-6]:
                name: a
                type: BitArray(One(4), false)
                qsharp_type: Result[]
                io_kind: Default"#]],
    );
}
