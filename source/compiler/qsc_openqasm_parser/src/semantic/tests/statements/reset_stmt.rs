// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn on_a_single_qubit() {
    check_stmt_kinds(
        "qubit q;
        reset q;",
        &expect![[r#"
            QubitDeclaration [0-8]:
                symbol_id: 8
            ResetStmt [17-25]:
                reset_token_span: [17-22]
                operand: GateOperand [23-24]:
                    kind: Expr [23-24]:
                        ty: qubit
                        kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn on_an_indexed_qubit_register() {
    check_stmt_kinds(
        "qubit[5] q;
        reset q[2];",
        &expect![[r#"
            QubitArrayDeclaration [0-11]:
                symbol_id: 8
                size: Expr [6-7]:
                    ty: const uint
                    const_value: Int(5)
                    kind: Lit: Int(5)
                size_span: [6-7]
            ResetStmt [20-31]:
                reset_token_span: [20-25]
                operand: GateOperand [26-30]:
                    kind: Expr [26-29]:
                        ty: qubit
                        kind: IndexedExpr [26-29]:
                            collection: Expr [26-27]:
                                ty: qubit[5]
                                kind: SymbolId(8)
                            index: Expr [28-29]:
                                ty: const int
                                kind: Lit: Int(2)
        "#]],
    );
}

#[test]
fn on_a_span_indexed_qubit_register() {
    check_stmt_kinds(
        "qubit[5] q;
        reset q[1:3];",
        &expect![[r#"
            QubitArrayDeclaration [0-11]:
                symbol_id: 8
                size: Expr [6-7]:
                    ty: const uint
                    const_value: Int(5)
                    kind: Lit: Int(5)
                size_span: [6-7]
            ResetStmt [20-33]:
                reset_token_span: [20-25]
                operand: GateOperand [26-32]:
                    kind: Expr [26-31]:
                        ty: qubit[3]
                        kind: IndexedExpr [26-31]:
                            collection: Expr [26-27]:
                                ty: qubit[5]
                                kind: SymbolId(8)
                            index: Range [28-31]:
                                start: Expr [28-29]:
                                    ty: const int
                                    const_value: Int(1)
                                    kind: Lit: Int(1)
                                step: <none>
                                end: Expr [30-31]:
                                    ty: const int
                                    const_value: Int(3)
                                    kind: Lit: Int(3)
        "#]],
    );
}

#[test]
fn on_a_zero_len_qubit_register() {
    check_stmt_kinds(
        "qubit[0] q;
        reset q;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-11]:
                        annotations: <empty>
                        kind: Err
                    Stmt [20-28]:
                        annotations: <empty>
                        kind: ResetStmt [20-28]:
                            reset_token_span: [20-25]
                            operand: GateOperand [26-27]:
                                kind: Expr [26-27]:
                                    ty: unknown
                                    kind: SymbolId(8)

            [Qasm.Lowerer.ExprMustBePositiveInt

              x quantum register size must be a positive integer
               ,-[test:1:7]
             1 | qubit[0] q;
               :       ^
             2 |         reset q;
               `----
            ]"#]],
    );
}

#[test]
fn on_an_unindexed_qubit_register() {
    check_stmt_kinds(
        "qubit[5] q;
        reset q;",
        &expect![[r#"
            QubitArrayDeclaration [0-11]:
                symbol_id: 8
                size: Expr [6-7]:
                    ty: const uint
                    const_value: Int(5)
                    kind: Lit: Int(5)
                size_span: [6-7]
            ResetStmt [20-28]:
                reset_token_span: [20-25]
                operand: GateOperand [26-27]:
                    kind: Expr [26-27]:
                        ty: qubit[5]
                        kind: SymbolId(8)
        "#]],
    );
}
