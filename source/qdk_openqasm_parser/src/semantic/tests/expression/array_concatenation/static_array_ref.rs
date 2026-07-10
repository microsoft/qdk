// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds as check;
use expect_test::expect;

#[test]
fn array_concatenation_has_the_right_type() {
    let source = "
    def f(readonly array[int, 3] a, readonly array[int, 4] b, mutable array[int, 7] c) {
        c = a ++ b;
    }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [5-115]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [34-35]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [31-32]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                    DefParameter [60-61]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [57-58]:
                                ty: const uint
                                const_value: Int(4)
                                kind: Lit: Int(4)
                    DefParameter [85-86]:
                        symbol_id: 11
                        ty_exprs:
                            Expr [82-83]:
                                ty: const uint
                                const_value: Int(7)
                                kind: Lit: Int(7)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [88-115]:
                    Stmt [98-109]:
                        annotations: <empty>
                        kind: AssignStmt [98-109]:
                            lhs: Expr [98-99]:
                                ty: mutable array[int, 7]
                                kind: SymbolId(11)
                            rhs: Expr [102-108]:
                                ty: readonly array[int, 7]
                                kind: ConcatExpr [102-108]:
                                    operands:
                                        Expr [102-103]:
                                            ty: readonly array[int, 3]
                                            kind: SymbolId(9)
                                        Expr [107-108]:
                                            ty: readonly array[int, 4]
                                            kind: SymbolId(10)
        "#]],
    );
}

#[test]
fn array_can_be_concatenated_with_itself() {
    let source = "
    def f(readonly array[int, 3] a, mutable array[int, 6] c) {
        c = a ++ a;
    }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [5-89]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [34-35]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [31-32]:
                                ty: const uint
                                const_value: Int(3)
                                kind: Lit: Int(3)
                    DefParameter [59-60]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [56-57]:
                                ty: const uint
                                const_value: Int(6)
                                kind: Lit: Int(6)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [62-89]:
                    Stmt [72-83]:
                        annotations: <empty>
                        kind: AssignStmt [72-83]:
                            lhs: Expr [72-73]:
                                ty: mutable array[int, 6]
                                kind: SymbolId(10)
                            rhs: Expr [76-82]:
                                ty: readonly array[int, 6]
                                kind: ConcatExpr [76-82]:
                                    operands:
                                        Expr [76-77]:
                                            ty: readonly array[int, 3]
                                            kind: SymbolId(9)
                                        Expr [81-82]:
                                            ty: readonly array[int, 3]
                                            kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn array_concatenation_with_different_widths_errors() {
    let source = "
    def f(readonly array[int[8], 3] a, readonly array[int[16], 4] b, mutable array[int[8], 7] c) {
        c = a ++ b;
    }
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-125]:
                        annotations: <empty>
                        kind: DefStmt [5-125]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [37-38]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [30-31]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [34-35]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                                DefParameter [67-68]:
                                    symbol_id: 10
                                    ty_exprs:
                                        Expr [59-61]:
                                            ty: const uint
                                            const_value: Int(16)
                                            kind: Lit: Int(16)
                                        Expr [64-65]:
                                            ty: const uint
                                            const_value: Int(4)
                                            kind: Lit: Int(4)
                                DefParameter [95-96]:
                                    symbol_id: 11
                                    ty_exprs:
                                        Expr [88-89]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [92-93]:
                                            ty: const uint
                                            const_value: Int(7)
                                            kind: Lit: Int(7)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [98-125]:
                                Stmt [108-119]:
                                    annotations: <empty>
                                    kind: AssignStmt [108-119]:
                                        lhs: Expr [108-109]:
                                            ty: mutable array[int[8], 7]
                                            kind: SymbolId(11)
                                        rhs: Expr [112-118]:
                                            ty: unknown
                                            kind: ConcatExpr [112-118]:
                                                operands:
                                                    Expr [112-113]:
                                                        ty: readonly array[int[8], 3]
                                                        kind: SymbolId(9)
                                                    Expr [117-118]:
                                                        ty: readonly array[int[16], 4]
                                                        kind: SymbolId(10)

            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: readonly
              | array[int[8], 3], readonly array[int[16], 4]
               ,-[test:3:13]
             2 |     def f(readonly array[int[8], 3] a, readonly array[int[16], 4] b, mutable array[int[8], 7] c) {
             3 |         c = a ++ b;
               :             ^^^^^^
             4 |     }
               `----
            ]"#]],
    );
}

#[test]
fn array_concatenation_with_different_types_errors() {
    let source = "
    def f(readonly array[int[8], 3] a, readonly array[uint[8], 4] b, mutable array[int[8], 7] c) {
        c = a ++ b;
    }
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-125]:
                        annotations: <empty>
                        kind: DefStmt [5-125]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [37-38]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [30-31]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [34-35]:
                                            ty: const uint
                                            const_value: Int(3)
                                            kind: Lit: Int(3)
                                DefParameter [67-68]:
                                    symbol_id: 10
                                    ty_exprs:
                                        Expr [60-61]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [64-65]:
                                            ty: const uint
                                            const_value: Int(4)
                                            kind: Lit: Int(4)
                                DefParameter [95-96]:
                                    symbol_id: 11
                                    ty_exprs:
                                        Expr [88-89]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [92-93]:
                                            ty: const uint
                                            const_value: Int(7)
                                            kind: Lit: Int(7)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [98-125]:
                                Stmt [108-119]:
                                    annotations: <empty>
                                    kind: AssignStmt [108-119]:
                                        lhs: Expr [108-109]:
                                            ty: mutable array[int[8], 7]
                                            kind: SymbolId(11)
                                        rhs: Expr [112-118]:
                                            ty: unknown
                                            kind: ConcatExpr [112-118]:
                                                operands:
                                                    Expr [112-113]:
                                                        ty: readonly array[int[8], 3]
                                                        kind: SymbolId(9)
                                                    Expr [117-118]:
                                                        ty: readonly array[uint[8], 4]
                                                        kind: SymbolId(10)

            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: readonly
              | array[int[8], 3], readonly array[uint[8], 4]
               ,-[test:3:13]
             2 |     def f(readonly array[int[8], 3] a, readonly array[uint[8], 4] b, mutable array[int[8], 7] c) {
             3 |         c = a ++ b;
               :             ^^^^^^
             4 |     }
               `----
            ]"#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn multidimensional_array_concatenation_has_the_right_type() {
    let source = "
    def f(readonly array[int, 4, 2] a, readonly array[int, 5, 2] b, mutable array[int, 9, 2] c) {
        c = a ++ b;
    }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [5-124]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [37-38]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [31-32]:
                                ty: const uint
                                const_value: Int(4)
                                kind: Lit: Int(4)
                            Expr [34-35]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                    DefParameter [66-67]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [60-61]:
                                ty: const uint
                                const_value: Int(5)
                                kind: Lit: Int(5)
                            Expr [63-64]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                    DefParameter [94-95]:
                        symbol_id: 11
                        ty_exprs:
                            Expr [88-89]:
                                ty: const uint
                                const_value: Int(9)
                                kind: Lit: Int(9)
                            Expr [91-92]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [97-124]:
                    Stmt [107-118]:
                        annotations: <empty>
                        kind: AssignStmt [107-118]:
                            lhs: Expr [107-108]:
                                ty: mutable array[int, 9, 2]
                                kind: SymbolId(11)
                            rhs: Expr [111-117]:
                                ty: readonly array[int, 9, 2]
                                kind: ConcatExpr [111-117]:
                                    operands:
                                        Expr [111-112]:
                                            ty: readonly array[int, 4, 2]
                                            kind: SymbolId(9)
                                        Expr [116-117]:
                                            ty: readonly array[int, 5, 2]
                                            kind: SymbolId(10)
        "#]],
    );
}

#[test]
fn multidimensional_array_can_be_concatenated_with_itself() {
    let source = "
    def f(readonly array[int, 4, 2] a, mutable array[int, 8, 2] c) {
        c = a ++ a;
    }
    ";

    check(
        source,
        &expect![[r#"
            DefStmt [5-95]:
                symbol_id: 8
                has_qubit_params: false
                parameters:
                    DefParameter [37-38]:
                        symbol_id: 9
                        ty_exprs:
                            Expr [31-32]:
                                ty: const uint
                                const_value: Int(4)
                                kind: Lit: Int(4)
                            Expr [34-35]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                    DefParameter [65-66]:
                        symbol_id: 10
                        ty_exprs:
                            Expr [59-60]:
                                ty: const uint
                                const_value: Int(8)
                                kind: Lit: Int(8)
                            Expr [62-63]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                return_type_span: [0-0]
                return_ty_exprs: <empty>
                body: Block [68-95]:
                    Stmt [78-89]:
                        annotations: <empty>
                        kind: AssignStmt [78-89]:
                            lhs: Expr [78-79]:
                                ty: mutable array[int, 8, 2]
                                kind: SymbolId(10)
                            rhs: Expr [82-88]:
                                ty: readonly array[int, 8, 2]
                                kind: ConcatExpr [82-88]:
                                    operands:
                                        Expr [82-83]:
                                            ty: readonly array[int, 4, 2]
                                            kind: SymbolId(9)
                                        Expr [87-88]:
                                            ty: readonly array[int, 4, 2]
                                            kind: SymbolId(9)
        "#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn multidimensional_array_concatenation_with_different_widths_errors() {
    let source = "
    def f(readonly array[int[8], 4, 2] a, readonly array[int[16], 5, 2] b, mutable array[int[8], 9, 2] c) {
        c = a ++ b;
    }
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-134]:
                        annotations: <empty>
                        kind: DefStmt [5-134]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [40-41]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [30-31]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [34-35]:
                                            ty: const uint
                                            const_value: Int(4)
                                            kind: Lit: Int(4)
                                        Expr [37-38]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                DefParameter [73-74]:
                                    symbol_id: 10
                                    ty_exprs:
                                        Expr [62-64]:
                                            ty: const uint
                                            const_value: Int(16)
                                            kind: Lit: Int(16)
                                        Expr [67-68]:
                                            ty: const uint
                                            const_value: Int(5)
                                            kind: Lit: Int(5)
                                        Expr [70-71]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                DefParameter [104-105]:
                                    symbol_id: 11
                                    ty_exprs:
                                        Expr [94-95]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [98-99]:
                                            ty: const uint
                                            const_value: Int(9)
                                            kind: Lit: Int(9)
                                        Expr [101-102]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [107-134]:
                                Stmt [117-128]:
                                    annotations: <empty>
                                    kind: AssignStmt [117-128]:
                                        lhs: Expr [117-118]:
                                            ty: mutable array[int[8], 9, 2]
                                            kind: SymbolId(11)
                                        rhs: Expr [121-127]:
                                            ty: unknown
                                            kind: ConcatExpr [121-127]:
                                                operands:
                                                    Expr [121-122]:
                                                        ty: readonly array[int[8], 4, 2]
                                                        kind: SymbolId(9)
                                                    Expr [126-127]:
                                                        ty: readonly array[int[16], 5, 2]
                                                        kind: SymbolId(10)

            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: readonly
              | array[int[8], 4, 2], readonly array[int[16], 5, 2]
               ,-[test:3:13]
             2 |     def f(readonly array[int[8], 4, 2] a, readonly array[int[16], 5, 2] b, mutable array[int[8], 9, 2] c) {
             3 |         c = a ++ b;
               :             ^^^^^^
             4 |     }
               `----
            ]"#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn multidimensional_array_concatenation_with_different_types_errors() {
    let source = "
    def f(readonly array[int[8], 4, 2] a, readonly array[uint[8], 5, 2] b, mutable array[int[8], 9, 2] c) {
        c = a ++ b;
    }
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-134]:
                        annotations: <empty>
                        kind: DefStmt [5-134]:
                            symbol_id: 8
                            has_qubit_params: false
                            parameters:
                                DefParameter [40-41]:
                                    symbol_id: 9
                                    ty_exprs:
                                        Expr [30-31]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [34-35]:
                                            ty: const uint
                                            const_value: Int(4)
                                            kind: Lit: Int(4)
                                        Expr [37-38]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                DefParameter [73-74]:
                                    symbol_id: 10
                                    ty_exprs:
                                        Expr [63-64]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [67-68]:
                                            ty: const uint
                                            const_value: Int(5)
                                            kind: Lit: Int(5)
                                        Expr [70-71]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                                DefParameter [104-105]:
                                    symbol_id: 11
                                    ty_exprs:
                                        Expr [94-95]:
                                            ty: const uint
                                            const_value: Int(8)
                                            kind: Lit: Int(8)
                                        Expr [98-99]:
                                            ty: const uint
                                            const_value: Int(9)
                                            kind: Lit: Int(9)
                                        Expr [101-102]:
                                            ty: const uint
                                            const_value: Int(2)
                                            kind: Lit: Int(2)
                            return_type_span: [0-0]
                            return_ty_exprs: <empty>
                            body: Block [107-134]:
                                Stmt [117-128]:
                                    annotations: <empty>
                                    kind: AssignStmt [117-128]:
                                        lhs: Expr [117-118]:
                                            ty: mutable array[int[8], 9, 2]
                                            kind: SymbolId(11)
                                        rhs: Expr [121-127]:
                                            ty: unknown
                                            kind: ConcatExpr [121-127]:
                                                operands:
                                                    Expr [121-122]:
                                                        ty: readonly array[int[8], 4, 2]
                                                        kind: SymbolId(9)
                                                    Expr [126-127]:
                                                        ty: readonly array[uint[8], 5, 2]
                                                        kind: SymbolId(10)

            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: readonly
              | array[int[8], 4, 2], readonly array[uint[8], 5, 2]
               ,-[test:3:13]
             2 |     def f(readonly array[int[8], 4, 2] a, readonly array[uint[8], 5, 2] b, mutable array[int[8], 9, 2] c) {
             3 |         c = a ++ b;
               :             ^^^^^^
             4 |     }
               `----
            ]"#]],
    );
}
