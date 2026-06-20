// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_stmt_kinds;

#[test]
fn simple_array_slice_has_correct_size() {
    let input = "
        bit[8] a = 16;
        a[1:3];
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                init_expr: Expr [20-22]:
                    ty: const bit[8]
                    kind: Lit: Bitstring("00010000")
            ExprStmt [32-39]:
                expr: Expr [32-37]:
                    ty: bit[3]
                    kind: IndexedExpr [32-37]:
                        collection: Expr [32-33]:
                            ty: bit[8]
                            kind: SymbolId(8)
                        index: Range [34-37]:
                            start: Expr [34-35]:
                                ty: const int
                                const_value: Int(1)
                                kind: Lit: Int(1)
                            step: <none>
                            end: Expr [36-37]:
                                ty: const int
                                const_value: Int(3)
                                kind: Lit: Int(3)
        "#]],
    );
}

#[test]
fn array_slice_with_negative_start_has_correct_size() {
    let input = "
        bit[8] a = 16;
        a[-7:6];
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                init_expr: Expr [20-22]:
                    ty: const bit[8]
                    kind: Lit: Bitstring("00010000")
            ExprStmt [32-40]:
                expr: Expr [32-38]:
                    ty: bit[6]
                    kind: IndexedExpr [32-38]:
                        collection: Expr [32-33]:
                            ty: bit[8]
                            kind: SymbolId(8)
                        index: Range [34-38]:
                            start: Expr [35-36]:
                                ty: const int
                                const_value: Int(-7)
                                kind: UnaryOpExpr [35-36]:
                                    op: Neg
                                    expr: Expr [35-36]:
                                        ty: const int
                                        kind: Lit: Int(7)
                            step: <none>
                            end: Expr [37-38]:
                                ty: const int
                                const_value: Int(6)
                                kind: Lit: Int(6)
        "#]],
    );
}

#[test]
fn array_slice_with_negative_end_has_correct_size() {
    let input = "
        bit[8] a = 16;
        a[1:-2];
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                init_expr: Expr [20-22]:
                    ty: const bit[8]
                    kind: Lit: Bitstring("00010000")
            ExprStmt [32-40]:
                expr: Expr [32-38]:
                    ty: bit[6]
                    kind: IndexedExpr [32-38]:
                        collection: Expr [32-33]:
                            ty: bit[8]
                            kind: SymbolId(8)
                        index: Range [34-38]:
                            start: Expr [34-35]:
                                ty: const int
                                const_value: Int(1)
                                kind: Lit: Int(1)
                            step: <none>
                            end: Expr [37-38]:
                                ty: const int
                                const_value: Int(-2)
                                kind: UnaryOpExpr [37-38]:
                                    op: Neg
                                    expr: Expr [37-38]:
                                        ty: const int
                                        kind: Lit: Int(2)
        "#]],
    );
}

#[test]
fn array_slice_with_non_exact_divisor_step_has_correct_size() {
    let input = "
        bit[8] a = 16;
        a[0:3:7];
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                init_expr: Expr [20-22]:
                    ty: const bit[8]
                    kind: Lit: Bitstring("00010000")
            ExprStmt [32-41]:
                expr: Expr [32-39]:
                    ty: bit[3]
                    kind: IndexedExpr [32-39]:
                        collection: Expr [32-33]:
                            ty: bit[8]
                            kind: SymbolId(8)
                        index: Range [34-39]:
                            start: Expr [34-35]:
                                ty: const int
                                const_value: Int(0)
                                kind: Lit: Int(0)
                            step: Expr [36-37]:
                                ty: const int
                                const_value: Int(3)
                                kind: Lit: Int(3)
                            end: Expr [38-39]:
                                ty: const int
                                const_value: Int(7)
                                kind: Lit: Int(7)
        "#]],
    );
}

#[test]
fn array_slice_with_exact_divisor_step_has_correct_size() {
    let input = "
        bit[8] a = 16;
        a[0:3:6];
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                init_expr: Expr [20-22]:
                    ty: const bit[8]
                    kind: Lit: Bitstring("00010000")
            ExprStmt [32-41]:
                expr: Expr [32-39]:
                    ty: bit[3]
                    kind: IndexedExpr [32-39]:
                        collection: Expr [32-33]:
                            ty: bit[8]
                            kind: SymbolId(8)
                        index: Range [34-39]:
                            start: Expr [34-35]:
                                ty: const int
                                const_value: Int(0)
                                kind: Lit: Int(0)
                            step: Expr [36-37]:
                                ty: const int
                                const_value: Int(3)
                                kind: Lit: Int(3)
                            end: Expr [38-39]:
                                ty: const int
                                const_value: Int(6)
                                kind: Lit: Int(6)
        "#]],
    );
}

#[test]
fn array_slice_with_negative_step_has_correct_size() {
    let input = "
        bit[8] a = 16;
        a[6:-3:0];
    ";

    check_stmt_kinds(
        input,
        &expect![[r#"
            ClassicalDeclarationStmt [9-23]:
                symbol_id: 8
                ty_span: [9-15]
                ty_exprs:
                    Expr [13-14]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                init_expr: Expr [20-22]:
                    ty: const bit[8]
                    kind: Lit: Bitstring("00010000")
            ExprStmt [32-42]:
                expr: Expr [32-40]:
                    ty: bit[3]
                    kind: IndexedExpr [32-40]:
                        collection: Expr [32-33]:
                            ty: bit[8]
                            kind: SymbolId(8)
                        index: Range [34-40]:
                            start: Expr [34-35]:
                                ty: const int
                                const_value: Int(6)
                                kind: Lit: Int(6)
                            step: Expr [37-38]:
                                ty: const int
                                const_value: Int(-3)
                                kind: UnaryOpExpr [37-38]:
                                    op: Neg
                                    expr: Expr [37-38]:
                                        ty: const int
                                        kind: Lit: Int(3)
                            end: Expr [39-40]:
                                ty: const int
                                const_value: Int(0)
                                kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn array_slice_with_zero_step_errors() {
    let input = "
        bit[8] a = 16;
        a[:0:];
    ";

    check_stmt_kinds(
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
                            ty_span: [9-15]
                            ty_exprs:
                                Expr [13-14]:
                                    ty: const uint
                                    const_value: Int(8)
                                    kind: Lit: Int(8)
                            init_expr: Expr [20-22]:
                                ty: const bit[8]
                                kind: Lit: Bitstring("00010000")
                    Stmt [32-39]:
                        annotations: <empty>
                        kind: Err

            [Qasm.Lowerer.ZeroStepInRange

              x range step cannot be zero
               ,-[test:3:11]
             2 |         bit[8] a = 16;
             3 |         a[:0:];
               :           ^^^
             4 |     
               `----
            ]"#]],
    );
}

#[test]
fn index_into_readonly_static_array_ref() {
    let source = "
        def function_with_array_param(readonly array[bool, 2, 3] a) {
            a[0, 0];
        }
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-101]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [66-67]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [60-61]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                            Expr [63-64]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [69-101]:
                    Stmt [83-91]:
                        annotations: <empty>
                        kind: ExprStmt [83-91]:
                            expr: Expr [83-89]:
                                ty: bool
                                kind: IndexedExpr [83-89]:
                                    collection: Expr [83-86]:
                                        ty: readonly array[bool, 3]
                                        kind: IndexedExpr [83-86]:
                                            collection: Expr [83-84]:
                                                ty: readonly array[bool, 2, 3]
                                                kind: SymbolId(9)
                                            index: Expr [85-86]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [88-89]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn index_into_mutable_static_array_ref() {
    let source = "
        def function_with_array_param(mutable array[bool, 2, 3] a) {
            a[0, 0];
        }
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-100]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [65-66]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [59-60]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                            Expr [62-63]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [68-100]:
                    Stmt [82-90]:
                        annotations: <empty>
                        kind: ExprStmt [82-90]:
                            expr: Expr [82-88]:
                                ty: bool
                                kind: IndexedExpr [82-88]:
                                    collection: Expr [82-85]:
                                        ty: mutable array[bool, 3]
                                        kind: IndexedExpr [82-85]:
                                            collection: Expr [82-83]:
                                                ty: mutable array[bool, 2, 3]
                                                kind: SymbolId(9)
                                            index: Expr [84-85]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [87-88]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn index_into_readonly_dyn_array_ref() {
    let source = "
        def function_with_array_param(readonly array[bool, #dim = 2] a) {
            a[0, 0];
        }
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-105]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [70-71]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [67-68]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [73-105]:
                    Stmt [87-95]:
                        annotations: <empty>
                        kind: ExprStmt [87-95]:
                            expr: Expr [87-93]:
                                ty: bool
                                kind: IndexedExpr [87-93]:
                                    collection: Expr [87-90]:
                                        ty: readonly array[bool, #dim = 1]
                                        kind: IndexedExpr [87-90]:
                                            collection: Expr [87-88]:
                                                ty: readonly array[bool, #dim = 2]
                                                kind: SymbolId(9)
                                            index: Expr [89-90]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [92-93]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn index_into_mutable_dyn_array_ref() {
    let source = "
        def function_with_array_param(mutable array[bool, #dim = 2] a) {
            a[0, 0];
        }
    ";

    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-104]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [69-70]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [66-67]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [72-104]:
                    Stmt [86-94]:
                        annotations: <empty>
                        kind: ExprStmt [86-94]:
                            expr: Expr [86-92]:
                                ty: bool
                                kind: IndexedExpr [86-92]:
                                    collection: Expr [86-89]:
                                        ty: mutable array[bool, #dim = 1]
                                        kind: IndexedExpr [86-89]:
                                            collection: Expr [86-87]:
                                                ty: mutable array[bool, #dim = 2]
                                                kind: SymbolId(9)
                                            index: Expr [88-89]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [91-92]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}
