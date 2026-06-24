// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn assign_to_mutable_static_array_ref() {
    let source = "
        def f(mutable array[bool, 2, 3] a) {
            a[0][0] = true;
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-83]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [41-42]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [35-36]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                            Expr [38-39]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [44-83]:
                    Stmt [58-73]:
                        annotations: <empty>
                        kind: AssignStmt [58-73]:
                            lhs: Expr [58-64]:
                                ty: bool
                                kind: IndexedExpr [58-64]:
                                    collection: Expr [58-61]:
                                        ty: mutable array[bool, 3]
                                        kind: IndexedExpr [58-61]:
                                            collection: Expr [58-59]:
                                                ty: mutable array[bool, 2, 3]
                                                kind: SymbolId(9)
                                            index: Expr [60-61]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [63-64]:
                                        ty: const int
                                        kind: Lit: Int(0)
                            rhs: Expr [68-72]:
                                ty: bool
                                kind: Lit: Bool(true)
        "#]],
    );
}

#[test]
fn assign_literal_with_wrong_type_to_mutable_static_array_ref_errors() {
    let source = "
        def f(mutable array[int, 2, 3] a) {
            a[0][0] = 3 im;
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-82]:
                        annotations: <empty>
                        kind: DefStmt [9-82]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [40-41]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [34-35]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                        Expr [37-38]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [43-82]:
                                Stmt [57-72]:
                                    annotations: <empty>
                                    kind: AssignStmt [57-72]:
                                        lhs: Expr [57-63]:
                                            ty: int
                                            kind: IndexedExpr [57-63]:
                                                collection: Expr [57-60]:
                                                    ty: mutable array[int, 3]
                                                    kind: IndexedExpr [57-60]:
                                                        collection: Expr [57-58]:
                                                            ty: mutable array[int, 2, 3]
                                                            kind: SymbolId(9)
                                                        index: Expr [59-60]:
                                                            ty: const int
                                                            kind: Lit: Int(0)
                                                index: Expr [62-63]:
                                                    ty: const int
                                                    kind: Lit: Int(0)
                                        rhs: Expr [57-72]:
                                            ty: unknown
                                            kind: Err

            [Qasm.Lowerer.CannotCastLiteral

              x cannot cast literal expression of type const complex[float] to type int
               ,-[test:3:13]
             2 |         def f(mutable array[int, 2, 3] a) {
             3 |             a[0][0] = 3 im;
               :             ^^^^^^^^^^^^^^^
             4 |         }
               `----
            ]"#]],
    );
}

#[test]
fn assign_variable_with_wrong_type_to_mutable_static_array_ref_errors() {
    let source = "
        def f(mutable array[int, 2, 3] a) {
            complex b = 2 im;
            a[0][0] = b;
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-109]:
                        annotations: <empty>
                        kind: DefStmt [9-109]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [40-41]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [34-35]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                        Expr [37-38]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [43-109]:
                                Stmt [57-74]:
                                    annotations: <empty>
                                    kind: ClassicalDeclarationStmt [57-74]:
                                        symbol_id: 10
                                        ty_span: [57-64]
                                        ty_exprs: <empty>
                                        init_expr: Expr [69-73]:
                                            ty: complex[float]
                                            kind: Lit: Complex(0.0, 2.0)
                                Stmt [87-99]:
                                    annotations: <empty>
                                    kind: AssignStmt [87-99]:
                                        lhs: Expr [87-93]:
                                            ty: int
                                            kind: IndexedExpr [87-93]:
                                                collection: Expr [87-90]:
                                                    ty: mutable array[int, 3]
                                                    kind: IndexedExpr [87-90]:
                                                        collection: Expr [87-88]:
                                                            ty: mutable array[int, 2, 3]
                                                            kind: SymbolId(9)
                                                        index: Expr [89-90]:
                                                            ty: const int
                                                            kind: Lit: Int(0)
                                                index: Expr [92-93]:
                                                    ty: const int
                                                    kind: Lit: Int(0)
                                        rhs: Expr [97-98]:
                                            ty: complex[float]
                                            kind: SymbolId(10)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type complex[float] to type int
               ,-[test:4:23]
             3 |             complex b = 2 im;
             4 |             a[0][0] = b;
               :                       ^
             5 |         }
               `----
            ]"#]],
    );
}

#[test]
fn assign_indexed_mutable_static_array_ref_to_variable_with_wrong_type_errors() {
    let source = "
        def f(mutable array[int, 2, 3] a) {
            angle b = a[0][0];
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-85]:
                        annotations: <empty>
                        kind: DefStmt [9-85]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [40-41]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [34-35]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                        Expr [37-38]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [43-85]:
                                Stmt [57-75]:
                                    annotations: <empty>
                                    kind: ClassicalDeclarationStmt [57-75]:
                                        symbol_id: 10
                                        ty_span: [57-62]
                                        ty_exprs: <empty>
                                        init_expr: Expr [67-73]:
                                            ty: int
                                            kind: IndexedExpr [67-73]:
                                                collection: Expr [67-70]:
                                                    ty: mutable array[int, 3]
                                                    kind: IndexedExpr [67-70]:
                                                        collection: Expr [67-68]:
                                                            ty: mutable array[int, 2, 3]
                                                            kind: SymbolId(9)
                                                        index: Expr [69-70]:
                                                            ty: const int
                                                            kind: Lit: Int(0)
                                                index: Expr [72-73]:
                                                    ty: const int
                                                    kind: Lit: Int(0)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int to type angle
               ,-[test:3:23]
             2 |         def f(mutable array[int, 2, 3] a) {
             3 |             angle b = a[0][0];
               :                       ^^^^^^
             4 |         }
               `----
            ]"#]],
    );
}

#[test]
fn assign_indexed_mutable_static_array_ref_to_variable() {
    let source = "
        def f(mutable array[int, 2, 3] a) {
            int b = a[0][0];
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-83]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [40-41]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [34-35]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                            Expr [37-38]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [43-83]:
                    Stmt [57-73]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [57-73]:
                            symbol_id: 10
                            ty_span: [57-60]
                            ty_exprs: <empty>
                            init_expr: Expr [65-71]:
                                ty: int
                                kind: IndexedExpr [65-71]:
                                    collection: Expr [65-68]:
                                        ty: mutable array[int, 3]
                                        kind: IndexedExpr [65-68]:
                                            collection: Expr [65-66]:
                                                ty: mutable array[int, 2, 3]
                                                kind: SymbolId(9)
                                            index: Expr [67-68]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [70-71]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn assign_indexed_readonly_static_array_ref_to_variable() {
    let source = "
        def f(readonly array[int, 2, 3] a) {
            int b = a[0][0];
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-84]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [41-42]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [35-36]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                            Expr [38-39]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [44-84]:
                    Stmt [58-74]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [58-74]:
                            symbol_id: 10
                            ty_span: [58-61]
                            ty_exprs: <empty>
                            init_expr: Expr [66-72]:
                                ty: int
                                kind: IndexedExpr [66-72]:
                                    collection: Expr [66-69]:
                                        ty: readonly array[int, 3]
                                        kind: IndexedExpr [66-69]:
                                            collection: Expr [66-67]:
                                                ty: readonly array[int, 2, 3]
                                                kind: SymbolId(9)
                                            index: Expr [68-69]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [71-72]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn assign_to_readonly_static_array_ref_errors() {
    let source = "
        def f(readonly array[bool, 2, 3] a) {
            a[0][0] = true;
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-84]:
                        annotations: <empty>
                        kind: DefStmt [9-84]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [42-43]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [36-37]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                        Expr [39-40]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [45-84]:
                                Stmt [59-74]:
                                    annotations: <empty>
                                    kind: AssignStmt [59-74]:
                                        lhs: Expr [59-66]:
                                            ty: unknown
                                            kind: Err
                                        rhs: Expr [59-74]:
                                            ty: unknown
                                            kind: Err

            [Qasm.Lowerer.CannotUpdateReadonlyArrayRef

              x cannot update readonly array reference a
               ,-[test:3:13]
             2 |         def f(readonly array[bool, 2, 3] a) {
             3 |             a[0][0] = true;
               :             ^
             4 |         }
               `----
              help: mutable array references must be declared with the keyword `mutable`
            ]"#]],
    );
}

#[test]
fn classical_indexing_assign_to_mutable_static_array_ref() {
    let source = r#"
        def f(mutable array[int[32], 2, 3] a) {
            a[0, 0][5:8] = "1010";
        }
    "#;
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-93]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [44-45]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                            Expr [38-39]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                            Expr [41-42]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [47-93]:
                    Stmt [61-83]:
                        annotations: <empty>
                        kind: IndexedClassicalTypeAssignStmt [61-83]:
                            lhs: Expr [61-67]:
                                ty: int[32]
                                kind: IndexedExpr [61-67]:
                                    collection: Expr [61-64]:
                                        ty: mutable array[int[32], 3]
                                        kind: IndexedExpr [61-64]:
                                            collection: Expr [61-62]:
                                                ty: mutable array[int[32], 2, 3]
                                                kind: SymbolId(9)
                                            index: Expr [63-64]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [66-67]:
                                        ty: const int
                                        kind: Lit: Int(0)
                            rhs: Expr [76-82]:
                                ty: bit[4]
                                kind: Lit: Bitstring("1010")
                            indices:
                                Range [69-72]:
                                    start: Expr [69-70]:
                                        ty: const int
                                        const_value: Int(26)
                                        kind: Lit: Int(26)
                                    step: Expr [69-72]:
                                        ty: const int
                                        const_value: Int(-1)
                                        kind: Lit: Int(-1)
                                    end: Expr [71-72]:
                                        ty: const int
                                        const_value: Int(23)
                                        kind: Lit: Int(23)
        "#]],
    );
}

#[test]
fn classical_indexing_assign_to_readonly_static_array_ref_errors() {
    let source = r#"
        def f(readonly array[int[32], 2, 3] a) {
            a[0, 0][5:8] = "1010";
        }
    "#;
    check_stmt_kinds(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-94]:
                        annotations: <empty>
                        kind: DefStmt [9-94]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [45-46]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [34-36]:
                                            ty: const uint
                                            const_value: Int(32)
                                            kind: Lit: Int(32)
                                        Expr [39-40]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                        Expr [42-43]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [48-94]:
                                Stmt [62-84]:
                                    annotations: <empty>
                                    kind: AssignStmt [62-84]:
                                        lhs: Expr [62-74]:
                                            ty: unknown
                                            kind: Err
                                        rhs: Expr [62-84]:
                                            ty: unknown
                                            kind: Err

            [Qasm.Lowerer.CannotUpdateReadonlyArrayRef

              x cannot update readonly array reference a
               ,-[test:3:13]
             2 |         def f(readonly array[int[32], 2, 3] a) {
             3 |             a[0, 0][5:8] = "1010";
               :             ^
             4 |         }
               `----
              help: mutable array references must be declared with the keyword `mutable`
            ]"#]],
    );
}
