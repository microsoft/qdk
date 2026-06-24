// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn assign_to_mutable_dyn_array_ref() {
    let source = "
        def f(mutable array[bool, #dim = 2] a) {
            a[0][0] = true;
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-87]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [45-46]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [42-43]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [48-87]:
                    Stmt [62-77]:
                        annotations: <empty>
                        kind: AssignStmt [62-77]:
                            lhs: Expr [62-68]:
                                ty: bool
                                kind: IndexedExpr [62-68]:
                                    collection: Expr [62-65]:
                                        ty: mutable array[bool, #dim = 1]
                                        kind: IndexedExpr [62-65]:
                                            collection: Expr [62-63]:
                                                ty: mutable array[bool, #dim = 2]
                                                kind: SymbolId(9)
                                            index: Expr [64-65]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [67-68]:
                                        ty: const int
                                        kind: Lit: Int(0)
                            rhs: Expr [72-76]:
                                ty: bool
                                kind: Lit: Bool(true)
        "#]],
    );
}

#[test]
fn assign_to_readonly_dyn_array_ref_errors() {
    let source = "
        def f(readonly array[bool, #dim = 2] a) {
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
                    Stmt [9-88]:
                        annotations: <empty>
                        kind: DefStmt [9-88]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [46-47]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [43-44]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [49-88]:
                                Stmt [63-78]:
                                    annotations: <empty>
                                    kind: AssignStmt [63-78]:
                                        lhs: Expr [63-70]:
                                            ty: unknown
                                            kind: Err
                                        rhs: Expr [63-78]:
                                            ty: unknown
                                            kind: Err

            [Qasm.Lowerer.CannotUpdateReadonlyArrayRef

              x cannot update readonly array reference a
               ,-[test:3:13]
             2 |         def f(readonly array[bool, #dim = 2] a) {
             3 |             a[0][0] = true;
               :             ^
             4 |         }
               `----
              help: mutable array references must be declared with the keyword `mutable`
            ]"#]],
    );
}

#[test]
fn assign_literal_with_wrong_type_to_mutable_static_array_ref_errors() {
    let source = "
        def f(mutable array[int, #dim = 2] a) {
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
                    Stmt [9-86]:
                        annotations: <empty>
                        kind: DefStmt [9-86]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [44-45]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [41-42]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [47-86]:
                                Stmt [61-76]:
                                    annotations: <empty>
                                    kind: AssignStmt [61-76]:
                                        lhs: Expr [61-67]:
                                            ty: int
                                            kind: IndexedExpr [61-67]:
                                                collection: Expr [61-64]:
                                                    ty: mutable array[int, #dim = 1]
                                                    kind: IndexedExpr [61-64]:
                                                        collection: Expr [61-62]:
                                                            ty: mutable array[int, #dim = 2]
                                                            kind: SymbolId(9)
                                                        index: Expr [63-64]:
                                                            ty: const int
                                                            kind: Lit: Int(0)
                                                index: Expr [66-67]:
                                                    ty: const int
                                                    kind: Lit: Int(0)
                                        rhs: Expr [61-76]:
                                            ty: unknown
                                            kind: Err

            [Qasm.Lowerer.CannotCastLiteral

              x cannot cast literal expression of type const complex[float] to type int
               ,-[test:3:13]
             2 |         def f(mutable array[int, #dim = 2] a) {
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
        def f(mutable array[int, #dim = 2] a) {
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
                    Stmt [9-113]:
                        annotations: <empty>
                        kind: DefStmt [9-113]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [44-45]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [41-42]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [47-113]:
                                Stmt [61-78]:
                                    annotations: <empty>
                                    kind: ClassicalDeclarationStmt [61-78]:
                                        symbol_id: 10
                                        ty_span: [61-68]
                                        ty_exprs: <empty>
                                        init_expr: Expr [73-77]:
                                            ty: complex[float]
                                            kind: Lit: Complex(0.0, 2.0)
                                Stmt [91-103]:
                                    annotations: <empty>
                                    kind: AssignStmt [91-103]:
                                        lhs: Expr [91-97]:
                                            ty: int
                                            kind: IndexedExpr [91-97]:
                                                collection: Expr [91-94]:
                                                    ty: mutable array[int, #dim = 1]
                                                    kind: IndexedExpr [91-94]:
                                                        collection: Expr [91-92]:
                                                            ty: mutable array[int, #dim = 2]
                                                            kind: SymbolId(9)
                                                        index: Expr [93-94]:
                                                            ty: const int
                                                            kind: Lit: Int(0)
                                                index: Expr [96-97]:
                                                    ty: const int
                                                    kind: Lit: Int(0)
                                        rhs: Expr [101-102]:
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
        def f(mutable array[int, #dim = 2] a) {
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
                    Stmt [9-89]:
                        annotations: <empty>
                        kind: DefStmt [9-89]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [44-45]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [41-42]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [47-89]:
                                Stmt [61-79]:
                                    annotations: <empty>
                                    kind: ClassicalDeclarationStmt [61-79]:
                                        symbol_id: 10
                                        ty_span: [61-66]
                                        ty_exprs: <empty>
                                        init_expr: Expr [71-77]:
                                            ty: int
                                            kind: IndexedExpr [71-77]:
                                                collection: Expr [71-74]:
                                                    ty: mutable array[int, #dim = 1]
                                                    kind: IndexedExpr [71-74]:
                                                        collection: Expr [71-72]:
                                                            ty: mutable array[int, #dim = 2]
                                                            kind: SymbolId(9)
                                                        index: Expr [73-74]:
                                                            ty: const int
                                                            kind: Lit: Int(0)
                                                index: Expr [76-77]:
                                                    ty: const int
                                                    kind: Lit: Int(0)

            [Qasm.Lowerer.CannotCast

              x cannot cast expression of type int to type angle
               ,-[test:3:23]
             2 |         def f(mutable array[int, #dim = 2] a) {
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
        def f(mutable array[int, #dim = 2] a) {
            int b = a[0][0];
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-87]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [44-45]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [41-42]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [47-87]:
                    Stmt [61-77]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [61-77]:
                            symbol_id: 10
                            ty_span: [61-64]
                            ty_exprs: <empty>
                            init_expr: Expr [69-75]:
                                ty: int
                                kind: IndexedExpr [69-75]:
                                    collection: Expr [69-72]:
                                        ty: mutable array[int, #dim = 1]
                                        kind: IndexedExpr [69-72]:
                                            collection: Expr [69-70]:
                                                ty: mutable array[int, #dim = 2]
                                                kind: SymbolId(9)
                                            index: Expr [71-72]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [74-75]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn assign_indexed_readonly_static_array_ref_to_variable() {
    let source = "
        def f(readonly array[int, #dim = 2] a) {
            int b = a[0][0];
        }
    ";
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-88]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [45-46]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [42-43]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [48-88]:
                    Stmt [62-78]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [62-78]:
                            symbol_id: 10
                            ty_span: [62-65]
                            ty_exprs: <empty>
                            init_expr: Expr [70-76]:
                                ty: int
                                kind: IndexedExpr [70-76]:
                                    collection: Expr [70-73]:
                                        ty: readonly array[int, #dim = 1]
                                        kind: IndexedExpr [70-73]:
                                            collection: Expr [70-71]:
                                                ty: readonly array[int, #dim = 2]
                                                kind: SymbolId(9)
                                            index: Expr [72-73]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [75-76]:
                                        ty: const int
                                        kind: Lit: Int(0)
        "#]],
    );
}

#[test]
fn classical_indexing_assign_to_mutable_dyn_array_ref() {
    let source = r#"
        def f(mutable array[int[32], #dim = 2] a) {
            a[0, 0][5:8] = "1010";
        }
    "#;
    check_stmt_kinds(
        source,
        &expect![[r#"
            DefStmt [9-97]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [48-49]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [33-35]:
                                ty: const uint
                                const_value: Int(32)
                                kind: Lit: Int(32)
                            Expr [45-46]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [51-97]:
                    Stmt [65-87]:
                        annotations: <empty>
                        kind: IndexedClassicalTypeAssignStmt [65-87]:
                            lhs: Expr [65-71]:
                                ty: int[32]
                                kind: IndexedExpr [65-71]:
                                    collection: Expr [65-68]:
                                        ty: mutable array[int[32], #dim = 1]
                                        kind: IndexedExpr [65-68]:
                                            collection: Expr [65-66]:
                                                ty: mutable array[int[32], #dim = 2]
                                                kind: SymbolId(9)
                                            index: Expr [67-68]:
                                                ty: const int
                                                kind: Lit: Int(0)
                                    index: Expr [70-71]:
                                        ty: const int
                                        kind: Lit: Int(0)
                            rhs: Expr [80-86]:
                                ty: bit[4]
                                kind: Lit: Bitstring("1010")
                            indices:
                                Range [73-76]:
                                    start: Expr [73-74]:
                                        ty: const int
                                        const_value: Int(26)
                                        kind: Lit: Int(26)
                                    step: Expr [73-76]:
                                        ty: const int
                                        const_value: Int(-1)
                                        kind: Lit: Int(-1)
                                    end: Expr [75-76]:
                                        ty: const int
                                        const_value: Int(23)
                                        kind: Lit: Int(23)
        "#]],
    );
}

#[test]
fn classical_indexing_assign_to_readonly_static_array_ref_errors() {
    let source = r#"
        def f(readonly array[int[32], #dim = 2] a) {
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
                    Stmt [9-98]:
                        annotations: <empty>
                        kind: DefStmt [9-98]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [49-50]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [34-36]:
                                            ty: const uint
                                            const_value: Int(32)
                                            kind: Lit: Int(32)
                                        Expr [46-47]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [52-98]:
                                Stmt [66-88]:
                                    annotations: <empty>
                                    kind: AssignStmt [66-88]:
                                        lhs: Expr [66-78]:
                                            ty: unknown
                                            kind: Err
                                        rhs: Expr [66-88]:
                                            ty: unknown
                                            kind: Err

            [Qasm.Lowerer.CannotUpdateReadonlyArrayRef

              x cannot update readonly array reference a
               ,-[test:3:13]
             2 |         def f(readonly array[int[32], #dim = 2] a) {
             3 |             a[0, 0][5:8] = "1010";
               :             ^
             4 |         }
               `----
              help: mutable array references must be declared with the keyword `mutable`
            ]"#]],
    );
}
