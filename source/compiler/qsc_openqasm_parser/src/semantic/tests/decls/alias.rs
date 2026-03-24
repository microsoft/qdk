// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::semantic::tests::check_last_stmt;

#[test]
fn can_alias_qubit_registers() {
    check_last_stmt(
        r#"
        qubit[2] one;
        qubit[10] two;
        // Aliased register of twelve qubits
        let concatenated = one ++ two;
        "#,
        &expect![[r#"
            AliasDeclStmt [99-129]:
                symbol_id: 10
                exprs:
                    Expr [118-121]:
                        ty: qubit[2]
                        kind: SymbolId(8)
                    Expr [125-128]:
                        ty: qubit[10]
                        kind: SymbolId(9)"#]],
    );
}

#[test]
fn first_qubit_from_aliased_qreg() {
    check_last_stmt(
        r#"
        qubit[2] one;
        qubit[10] two;
        let concatenated = one ++ two;
        // First qubit in aliased qubit array
        let first = concatenated[0];
        "#,
        &expect![[r#"
            AliasDeclStmt [139-167]:
                symbol_id: 11
                exprs:
                    Expr [151-165]:
                        ty: qubit
                        kind: IndexedExpr [151-165]:
                            collection: Expr [151-163]:
                                ty: qubit[12]
                                kind: SymbolId(10)
                            index: Expr [164-165]:
                                ty: const int
                                kind: Lit: Int(0)"#]],
    );
}

#[test]
fn can_alias_bit_registers() {
    check_last_stmt(
        r#"
        bit[2] one;
        bit[10] two;
        // Aliased register of twelve bits
        let concatenated = one ++ two;
        "#,
        &expect![[r#"
            AliasDeclStmt [93-123]:
                symbol_id: 10
                exprs:
                    Expr [112-115]:
                        ty: bit[2]
                        kind: SymbolId(8)
                    Expr [119-122]:
                        ty: bit[10]
                        kind: SymbolId(9)"#]],
    );
}

#[test]
fn first_bit_from_aliased_creg() {
    check_last_stmt(
        r#"
        bit[2] one;
        bit[10] two;
        let concatenated = one ++ two;
        // First bit in aliased bit array
        bit first = concatenated[0];
        "#,
        &expect![[r#"
            ClassicalDeclarationStmt [131-159]:
                symbol_id: 11
                ty_span: [131-134]
                ty_exprs: <empty>
                init_expr: Expr [143-157]:
                    ty: bit
                    kind: IndexedExpr [143-157]:
                        collection: Expr [143-155]:
                            ty: bit[12]
                            kind: SymbolId(10)
                        index: Expr [156-157]:
                            ty: const int
                            kind: Lit: Int(0)"#]],
    );
}

#[test]
fn inconsistent_types_raise_error() {
    check_last_stmt(
        r#"
        // InconsistentTypesInAlias
        qubit[2] alias_component_1;
        bit[5] alias_component_2;
        let alias_1 = alias_component_1 ++ alias_component_2;
        "#,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [45-72]:
                        annotations: <empty>
                        kind: QubitArrayDeclaration [45-72]:
                            symbol_id: 8
                            size: Expr [51-52]:
                                ty: const uint
                                const_value: Int(2)
                                kind: Lit: Int(2)
                            size_span: [51-52]
                    Stmt [81-106]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [81-106]:
                            symbol_id: 9
                            ty_span: [81-87]
                            ty_exprs:
                                Expr [85-86]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                            init_expr: Expr [81-106]:
                                ty: const bit[5]
                                kind: Lit: Bitstring("00000")
                    Stmt [115-168]:
                        annotations: <empty>
                        kind: AliasDeclStmt [115-168]:
                            symbol_id: 10
                            exprs:
                                Expr [129-146]:
                                    ty: qubit[2]
                                    kind: SymbolId(8)
                                Expr [150-167]:
                                    ty: bit[5]
                                    kind: SymbolId(9)

            [Qasm.Lowerer.InconsistentTypesInAlias

              x inconsistent types in alias expression: qubit[2], bit[5]
               ,-[test:5:23]
             4 |         bit[5] alias_component_2;
             5 |         let alias_1 = alias_component_1 ++ alias_component_2;
               :                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             6 |         
               `----
            ]"#]],
    );
}

#[test]
fn invalid_types_raise_error() {
    check_last_stmt(
        r#"
        // InvalidTypesInAlias
        bit[2] alias_component_3;
        int alias_component_4;
        let alias_2 = alias_component_3 ++ alias_component_4;
        "#,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [40-65]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [40-65]:
                            symbol_id: 8
                            ty_span: [40-46]
                            ty_exprs:
                                Expr [44-45]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [40-65]:
                                ty: const bit[2]
                                kind: Lit: Bitstring("00")
                    Stmt [74-96]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [74-96]:
                            symbol_id: 9
                            ty_span: [74-77]
                            ty_exprs: <empty>
                            init_expr: Expr [74-96]:
                                ty: const int
                                kind: Lit: Int(0)
                    Stmt [105-158]:
                        annotations: <empty>
                        kind: AliasDeclStmt [105-158]:
                            symbol_id: 10
                            exprs:
                                Expr [119-136]:
                                    ty: bit[2]
                                    kind: SymbolId(8)
                                Expr [140-157]:
                                    ty: int
                                    kind: SymbolId(9)

            [Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: int
               ,-[test:5:44]
             4 |         int alias_component_4;
             5 |         let alias_2 = alias_component_3 ++ alias_component_4;
               :                                            ^^^^^^^^^^^^^^^^^
             6 |         
               `----
              help: aliases can only be applied to quantum bits and registers
            ]"#]],
    );
}

#[test]
fn invalid_types_raise_error_for_each() {
    check_last_stmt(
        r#"
        array[int, 5] alias_component_0;
        bit[2] alias_component_1;
        int alias_component_2;
        let alias = alias_component_0 ++ alias_component_1 ++ alias_component_2;
        "#,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-41]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-41]:
                            symbol_id: 8
                            ty_span: [9-22]
                            ty_exprs:
                                Expr [20-21]:
                                    ty: const uint
                                    const_value: Int(5)
                                    kind: Lit: Int(5)
                            init_expr: Expr [9-41]:
                                ty: array[int, 5]
                                kind: Lit:     array:
                                        Expr [9-41]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [9-41]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [9-41]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [9-41]:
                                            ty: const int
                                            kind: Lit: Int(0)
                                        Expr [9-41]:
                                            ty: const int
                                            kind: Lit: Int(0)
                    Stmt [50-75]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [50-75]:
                            symbol_id: 9
                            ty_span: [50-56]
                            ty_exprs:
                                Expr [54-55]:
                                    ty: const uint
                                    const_value: Int(2)
                                    kind: Lit: Int(2)
                            init_expr: Expr [50-75]:
                                ty: const bit[2]
                                kind: Lit: Bitstring("00")
                    Stmt [84-106]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [84-106]:
                            symbol_id: 10
                            ty_span: [84-87]
                            ty_exprs: <empty>
                            init_expr: Expr [84-106]:
                                ty: const int
                                kind: Lit: Int(0)
                    Stmt [115-187]:
                        annotations: <empty>
                        kind: AliasDeclStmt [115-187]:
                            symbol_id: 11
                            exprs:
                                Expr [127-144]:
                                    ty: array[int, 5]
                                    kind: SymbolId(8)
                                Expr [148-165]:
                                    ty: bit[2]
                                    kind: SymbolId(9)
                                Expr [169-186]:
                                    ty: int
                                    kind: SymbolId(10)

            [Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: array[int, 5]
               ,-[test:5:21]
             4 |         int alias_component_2;
             5 |         let alias = alias_component_0 ++ alias_component_1 ++ alias_component_2;
               :                     ^^^^^^^^^^^^^^^^^
             6 |         
               `----
              help: aliases can only be applied to quantum bits and registers
            , Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: int
               ,-[test:5:63]
             4 |         int alias_component_2;
             5 |         let alias = alias_component_0 ++ alias_component_1 ++ alias_component_2;
               :                                                               ^^^^^^^^^^^^^^^^^
             6 |         
               `----
              help: aliases can only be applied to quantum bits and registers
            ]"#]],
    );
}

#[test]
fn bit_alias_errors() {
    check_last_stmt(
        r#"
        bit b;
        // Aliased register of twelve qubits
        let b_alias = b;
        "#,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [9-15]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [9-15]:
                            symbol_id: 8
                            ty_span: [9-12]
                            ty_exprs: <empty>
                            init_expr: Expr [9-15]:
                                ty: const bit
                                kind: Lit: Bit(0)
                    Stmt [69-85]:
                        annotations: <empty>
                        kind: AliasDeclStmt [69-85]:
                            symbol_id: 9
                            exprs:
                                Expr [83-84]:
                                    ty: bit
                                    kind: SymbolId(8)

            [Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: bit
               ,-[test:4:23]
             3 |         // Aliased register of twelve qubits
             4 |         let b_alias = b;
               :                       ^
             5 |         
               `----
              help: aliases can only be applied to quantum bits and registers
            ]"#]],
    );
}

#[test]
fn can_alias_qubit() {
    check_last_stmt(
        r#"
        qubit q;
        // Aliased register of twelve qubits
        let q_alias = q;
        "#,
        &expect![[r#"
            AliasDeclStmt [71-87]:
                symbol_id: 9
                exprs:
                    Expr [85-86]:
                        ty: qubit
                        kind: SymbolId(8)"#]],
    );
}
