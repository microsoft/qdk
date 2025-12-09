// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn on_a_single_qubit() {
    check_stmt_kinds(
        "qubit q;
        delay [5ns] q;",
        &expect![[r#"
            QubitDeclaration [0-8]:
                symbol_id: 8
            DelayStmt [17-31]:
                duration: Expr [24-27]:
                    ty: const duration
                    const_value: Duration(5.0 ns)
                    kind: Lit: Duration(5.0 ns)
                qubits:
                    GateOperand [29-30]:
                        kind: Expr [29-30]:
                            ty: qubit
                            kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn cannot_have_a_negative_duration() {
    check_stmt_kinds(
        "qubit q;
        const duration d = 1s - 4s;
        delay [d] q;",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-8]:
                        annotations: <empty>
                        kind: QubitDeclaration [0-8]:
                            symbol_id: 8
                    Stmt [17-44]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [17-44]:
                            symbol_id: 9
                            ty_span: [23-31]
                            ty_exprs: <empty>
                            init_expr: Expr [36-43]:
                                ty: const duration
                                const_value: Duration(-3.0 s)
                                kind: BinaryOpExpr:
                                    op: Sub
                                    lhs: Expr [36-38]:
                                        ty: const duration
                                        kind: Lit: Duration(1.0 s)
                                    rhs: Expr [41-43]:
                                        ty: const duration
                                        kind: Lit: Duration(4.0 s)
                    Stmt [53-65]:
                        annotations: <empty>
                        kind: DelayStmt [53-65]:
                            duration: Expr [60-61]:
                                ty: const duration
                                const_value: Duration(-3.0 s)
                                kind: SymbolId(9)
                            qubits:
                                GateOperand [63-64]:
                                    kind: Expr [63-64]:
                                        ty: qubit
                                        kind: SymbolId(8)

            [Qasm.Lowerer.DesignatorMustBePositiveDuration

              x designator must be a positive duration
               ,-[test:3:16]
             2 |         const duration d = 1s - 4s;
             3 |         delay [d] q;
               :                ^
               `----
            ]"#]],
    );
}

#[test]
fn on_a_single_qubit_with_variables() {
    check_stmt_kinds(
        "qubit q;
        const duration stride = 5ns;
        const int p = 2;
        delay [p * stride] q;",
        &expect![[r#"
            QubitDeclaration [0-8]:
                symbol_id: 8
            ClassicalDeclarationStmt [17-45]:
                symbol_id: 9
                ty_span: [23-31]
                ty_exprs: <empty>
                init_expr: Expr [41-44]:
                    ty: const duration
                    const_value: Duration(5.0 ns)
                    kind: Lit: Duration(5.0 ns)
            ClassicalDeclarationStmt [54-70]:
                symbol_id: 10
                ty_span: [60-63]
                ty_exprs: <empty>
                init_expr: Expr [68-69]:
                    ty: const int
                    const_value: Int(2)
                    kind: Lit: Int(2)
            DelayStmt [79-100]:
                duration: Expr [86-96]:
                    ty: const duration
                    const_value: Duration(10.0 ns)
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [86-87]:
                            ty: const int
                            kind: SymbolId(10)
                        rhs: Expr [90-96]:
                            ty: const duration
                            kind: SymbolId(9)
                qubits:
                    GateOperand [98-99]:
                        kind: Expr [98-99]:
                            ty: qubit
                            kind: SymbolId(8)
        "#]],
    );
}
