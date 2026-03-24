// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::{check_err, check_stmt_kinds as check};
use expect_test::expect;

#[test]
fn array_concatenation_in_alias_fails() {
    let source = "
    array[int, 3] a;
    array[int, 4] b;
    let c = a ++ b;
    ";

    check_err(
        source,
        &expect![[r#"
            [Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: array[int, 3]
               ,-[test:4:13]
             3 |     array[int, 4] b;
             4 |     let c = a ++ b;
               :             ^
             5 |     
               `----
              help: aliases can only be applied to quantum bits and registers
            , Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: array[int, 4]
               ,-[test:4:18]
             3 |     array[int, 4] b;
             4 |     let c = a ++ b;
               :                  ^
             5 |     
               `----
              help: aliases can only be applied to quantum bits and registers
            ]"#]],
    );
}

#[test]
fn array_concatenation_has_the_right_type() {
    let source = "
    array[int, 3] a;
    array[int, 4] b;
    array[int, 7] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [5-21]:
                symbol_id: 8
                ty_span: [5-18]
                ty_exprs:
                    Expr [16-17]:
                        ty: const uint
                        const_value: Int(3)
                        kind: Lit: Int(3)
                init_expr: Expr [5-21]:
                    ty: array[int, 3]
                    kind: Lit:     array:
                            Expr [5-21]:
                                ty: const int
                                kind: Lit: Int(0)
                            Expr [5-21]:
                                ty: const int
                                kind: Lit: Int(0)
                            Expr [5-21]:
                                ty: const int
                                kind: Lit: Int(0)
            ClassicalDeclarationStmt [26-42]:
                symbol_id: 9
                ty_span: [26-39]
                ty_exprs:
                    Expr [37-38]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                init_expr: Expr [26-42]:
                    ty: array[int, 4]
                    kind: Lit:     array:
                            Expr [26-42]:
                                ty: const int
                                kind: Lit: Int(0)
                            Expr [26-42]:
                                ty: const int
                                kind: Lit: Int(0)
                            Expr [26-42]:
                                ty: const int
                                kind: Lit: Int(0)
                            Expr [26-42]:
                                ty: const int
                                kind: Lit: Int(0)
            ClassicalDeclarationStmt [47-72]:
                symbol_id: 10
                ty_span: [47-60]
                ty_exprs:
                    Expr [58-59]:
                        ty: const uint
                        const_value: Int(7)
                        kind: Lit: Int(7)
                init_expr: Expr [65-71]:
                    ty: array[int, 7]
                    kind: ConcatExpr [65-71]:
                        operands:
                            Expr [65-66]:
                                ty: array[int, 3]
                                kind: SymbolId(8)
                            Expr [70-71]:
                                ty: array[int, 4]
                                kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn array_can_be_concatenated_with_itself() {
    let source = "
    array[int[8], 3] a;
    array[int[8], 6] c = a ++ a;
    ";

    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [5-24]:
                symbol_id: 8
                ty_span: [5-21]
                ty_exprs:
                    Expr [15-16]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                    Expr [19-20]:
                        ty: const uint
                        const_value: Int(3)
                        kind: Lit: Int(3)
                init_expr: Expr [5-24]:
                    ty: array[int[8], 3]
                    kind: Lit:     array:
                            Expr [5-24]:
                                ty: const int[8]
                                kind: Lit: Int(0)
                            Expr [5-24]:
                                ty: const int[8]
                                kind: Lit: Int(0)
                            Expr [5-24]:
                                ty: const int[8]
                                kind: Lit: Int(0)
            ClassicalDeclarationStmt [29-57]:
                symbol_id: 9
                ty_span: [29-45]
                ty_exprs:
                    Expr [39-40]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                    Expr [43-44]:
                        ty: const uint
                        const_value: Int(6)
                        kind: Lit: Int(6)
                init_expr: Expr [50-56]:
                    ty: array[int[8], 6]
                    kind: ConcatExpr [50-56]:
                        operands:
                            Expr [50-51]:
                                ty: array[int[8], 3]
                                kind: SymbolId(8)
                            Expr [55-56]:
                                ty: array[int[8], 3]
                                kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn array_concatenation_with_different_widths_errors() {
    let source = "
    array[int[8], 3] a;
    array[int[16], 4] b;
    array[int[8], 7] c = a ++ b;
    ";

    check_err(
        source,
        &expect![[r#"
            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 3],
              | array[int[16], 4]
               ,-[test:4:26]
             3 |     array[int[16], 4] b;
             4 |     array[int[8], 7] c = a ++ b;
               :                          ^^^^^^
             5 |     
               `----
            ]"#]],
    );
}

#[test]
fn array_concatenation_with_different_types_errors() {
    let source = "
    array[int[8], 3] a;
    array[uint[8], 4] b;
    array[int[8], 7] c = a ++ b;
    ";

    check_err(
        source,
        &expect![[r#"
            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 3],
              | array[uint[8], 4]
               ,-[test:4:26]
             3 |     array[uint[8], 4] b;
             4 |     array[int[8], 7] c = a ++ b;
               :                          ^^^^^^
             5 |     
               `----
            ]"#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn multidimensional_array_concatenation_has_the_right_type() {
    let source = "
    array[int, 4, 2] a;
    array[int, 5, 2] b;
    array[int, 9, 2] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [5-24]:
                symbol_id: 8
                ty_span: [5-21]
                ty_exprs:
                    Expr [16-17]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                    Expr [19-20]:
                        ty: const uint
                        const_value: Int(2)
                        kind: Lit: Int(2)
                init_expr: Expr [5-24]:
                    ty: array[int, 4, 2]
                    kind: Lit:     array:
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [5-24]:
                                            ty: const int
                                            kind: Lit: Int(0)
            ClassicalDeclarationStmt [29-48]:
                symbol_id: 9
                ty_span: [29-45]
                ty_exprs:
                    Expr [40-41]:
                        ty: const uint
                        const_value: Int(5)
                        kind: Lit: Int(5)
                    Expr [43-44]:
                        ty: const uint
                        const_value: Int(2)
                        kind: Lit: Int(2)
                init_expr: Expr [29-48]:
                    ty: array[int, 5, 2]
                    kind: Lit:     array:
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int, 2]
                                kind: Lit:     array:
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [29-48]:
                                            ty: const int
                                            kind: Lit: Int(0)
            ClassicalDeclarationStmt [53-81]:
                symbol_id: 10
                ty_span: [53-69]
                ty_exprs:
                    Expr [64-65]:
                        ty: const uint
                        const_value: Int(9)
                        kind: Lit: Int(9)
                    Expr [67-68]:
                        ty: const uint
                        const_value: Int(2)
                        kind: Lit: Int(2)
                init_expr: Expr [74-80]:
                    ty: array[int, 9, 2]
                    kind: ConcatExpr [74-80]:
                        operands:
                            Expr [74-75]:
                                ty: array[int, 4, 2]
                                kind: SymbolId(8)
                            Expr [79-80]:
                                ty: array[int, 5, 2]
                                kind: SymbolId(9)
        "#]],
    );
}

#[test]
fn multidimensional_array_can_be_concatenated_with_itself() {
    let source = "
    array[int[8], 4, 2] a;
    array[int[8], 8, 2] c = a ++ a;
    ";

    check(
        source,
        &expect![[r#"
            ClassicalDeclarationStmt [5-27]:
                symbol_id: 8
                ty_span: [5-24]
                ty_exprs:
                    Expr [15-16]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                    Expr [19-20]:
                        ty: const uint
                        const_value: Int(4)
                        kind: Lit: Int(4)
                    Expr [22-23]:
                        ty: const uint
                        const_value: Int(2)
                        kind: Lit: Int(2)
                init_expr: Expr [5-27]:
                    ty: array[int[8], 4, 2]
                    kind: Lit:     array:
                            Expr [0-0]:
                                ty: array[int[8], 2]
                                kind: Lit:     array:
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int[8], 2]
                                kind: Lit:     array:
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int[8], 2]
                                kind: Lit:     array:
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
                            Expr [0-0]:
                                ty: array[int[8], 2]
                                kind: Lit:     array:
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
                                        Expr [5-27]:
                                            ty: const int[8]
                                            kind: Lit: Int(0)
            ClassicalDeclarationStmt [32-63]:
                symbol_id: 9
                ty_span: [32-51]
                ty_exprs:
                    Expr [42-43]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                    Expr [46-47]:
                        ty: const uint
                        const_value: Int(8)
                        kind: Lit: Int(8)
                    Expr [49-50]:
                        ty: const uint
                        const_value: Int(2)
                        kind: Lit: Int(2)
                init_expr: Expr [56-62]:
                    ty: array[int[8], 8, 2]
                    kind: ConcatExpr [56-62]:
                        operands:
                            Expr [56-57]:
                                ty: array[int[8], 4, 2]
                                kind: SymbolId(8)
                            Expr [61-62]:
                                ty: array[int[8], 4, 2]
                                kind: SymbolId(8)
        "#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn multidimensional_array_concatenation_with_different_widths_errors() {
    let source = "
    array[int[8], 4, 2] a;
    array[int[16], 5, 2] b;
    array[int[8], 9, 2] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-27]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [5-27]:
                            symbol_id: 8
                            ty_span: [5-24]
                            ty_exprs:
                                Expr [15-16]:
                                    ty: const uint
                                    const_value: Int(8)
                                    kind: Lit: Int(8)
                                Expr [19-20]:
                                    ty: const uint
                                    const_value: Int(4)
                                    kind: Lit: Int(4)
                                Expr [22-23]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [5-27]:
                                ty: array[int[8], 4, 2]
                                kind: Lit:     array:
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                    Stmt [32-55]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-55]:
                            symbol_id: 9
                            ty_span: [32-52]
                            ty_exprs:
                                Expr [42-44]:
                                    ty: const uint
                                    const_value: Int(16)
                                    kind: Lit: Int(16)
                                Expr [47-48]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                                Expr [50-51]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [32-55]:
                                ty: array[int[16], 5, 2]
                                kind: Lit:     array:
                                        Expr [0-0]:
                                            ty: array[int[16], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[16], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[16], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[16], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[16], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const int[16]
                                                        kind: Lit: Int(0)
                    Stmt [60-91]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [60-91]:
                            symbol_id: 10
                            ty_span: [60-79]
                            ty_exprs:
                                Expr [70-71]:
                                    ty: const uint
                                    const_value: Int(8)
                                    kind: Lit: Int(8)
                                Expr [74-75]:
                                    ty: const uint
                                    const_value: Int(9)
                                    kind: Lit: Int(9)
                                Expr [77-78]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [84-90]:
                                ty: unknown
                                kind: ConcatExpr [84-90]:
                                    operands:
                                        Expr [84-85]:
                                            ty: array[int[8], 4, 2]
                                            kind: SymbolId(8)
                                        Expr [89-90]:
                                            ty: array[int[16], 5, 2]
                                            kind: SymbolId(9)

            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 4, 2],
              | array[int[16], 5, 2]
               ,-[test:4:29]
             3 |     array[int[16], 5, 2] b;
             4 |     array[int[8], 9, 2] c = a ++ b;
               :                             ^^^^^^
             5 |     
               `----
            ]"#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn multidimensional_array_concatenation_with_different_types_errors() {
    let source = "
    array[int[8], 4, 2] a;
    array[uint[8], 5, 2] b;
    array[int[8], 9, 2] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [5-27]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [5-27]:
                            symbol_id: 8
                            ty_span: [5-24]
                            ty_exprs:
                                Expr [15-16]:
                                    ty: const uint
                                    const_value: Int(8)
                                    kind: Lit: Int(8)
                                Expr [19-20]:
                                    ty: const uint
                                    const_value: Int(4)
                                    kind: Lit: Int(4)
                                Expr [22-23]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [5-27]:
                                ty: array[int[8], 4, 2]
                                kind: Lit:     array:
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[int[8], 2]
                                            kind: Lit:     array:
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                                                    Expr [5-27]:
                                                        ty: const int[8]
                                                        kind: Lit: Int(0)
                    Stmt [32-55]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [32-55]:
                            symbol_id: 9
                            ty_span: [32-52]
                            ty_exprs:
                                Expr [43-44]:
                                    ty: const uint
                                    const_value: Int(8)
                                    kind: Lit: Int(8)
                                Expr [47-48]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                                Expr [50-51]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [32-55]:
                                ty: array[uint[8], 5, 2]
                                kind: Lit:     array:
                                        Expr [0-0]:
                                            ty: array[uint[8], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[uint[8], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[uint[8], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[uint[8], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                        Expr [0-0]:
                                            ty: array[uint[8], 2]
                                            kind: Lit:     array:
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                                                    Expr [32-55]:
                                                        ty: const uint[8]
                                                        kind: Lit: Int(0)
                    Stmt [60-91]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [60-91]:
                            symbol_id: 10
                            ty_span: [60-79]
                            ty_exprs:
                                Expr [70-71]:
                                    ty: const uint
                                    const_value: Int(8)
                                    kind: Lit: Int(8)
                                Expr [74-75]:
                                    ty: const uint
                                    const_value: Int(9)
                                    kind: Lit: Int(9)
                                Expr [77-78]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [84-90]:
                                ty: unknown
                                kind: ConcatExpr [84-90]:
                                    operands:
                                        Expr [84-85]:
                                            ty: array[int[8], 4, 2]
                                            kind: SymbolId(8)
                                        Expr [89-90]:
                                            ty: array[uint[8], 5, 2]
                                            kind: SymbolId(9)

            [Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 4, 2],
              | array[uint[8], 5, 2]
               ,-[test:4:29]
             3 |     array[uint[8], 5, 2] b;
             4 |     array[int[8], 9, 2] c = a ++ b;
               :                             ^^^^^^
             5 |     
               `----
            ]"#]],
    );
}
