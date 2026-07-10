// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::semantic::tests::check_stmt_kinds;
use expect_test::expect;

#[test]
fn box_can_contain_barrier() {
    check_stmt_kinds(
        r#"
            qubit q;
            box {
              barrier q;
            }
        "#,
        &expect![[r#"
            QubitDeclaration [13-21]:
                symbol_id: 8
            BoxStmt [34-78]:
                duration: <none>
                body:
                    Stmt [54-64]:
                        annotations: <empty>
                        kind: BarrierStmt [54-64]:
                            operands:
                                GateOperand [62-63]:
                                    kind: Expr [62-63]:
                                        ty: qubit
                                        kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn box_can_have_a_duration() {
    check_stmt_kinds(
        r#"
            box [5ns] {}
        "#,
        &expect![[r#"
            BoxStmt [13-25]:
                duration: Expr [18-21]:
                    ty: const duration
                    const_value: Duration(5.0 ns)
                    kind: Lit: Duration(5.0 ns)
                body: <empty>
        "#]],
    );
}

#[test]
fn box_cannot_have_a_negative_duration() {
    check_stmt_kinds(
        r#"
            const duration d = 5ns * -1.0;
            box [d] {}
        "#,
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [13-43]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [13-43]:
                            symbol_id: 8
                            ty_span: [19-27]
                            ty_exprs: <empty>
                            init_expr: Expr [32-42]:
                                ty: const duration
                                const_value: Duration(-5.0 ns)
                                kind: BinaryOpExpr:
                                    op: Mul
                                    lhs: Expr [32-35]:
                                        ty: const duration
                                        kind: Lit: Duration(5.0 ns)
                                    rhs: Expr [39-42]:
                                        ty: const float
                                        kind: UnaryOpExpr [39-42]:
                                            op: Neg
                                            expr: Expr [39-42]:
                                                ty: const float
                                                kind: Lit: Float(1.0)
                    Stmt [56-66]:
                        annotations: <empty>
                        kind: BoxStmt [56-66]:
                            duration: Expr [61-62]:
                                ty: const duration
                                const_value: Duration(-5.0 ns)
                                kind: SymbolId(8)
                            body: <empty>

            [Qasm.Lowerer.DesignatorMustBePositiveDuration

              x designator must be a positive duration
               ,-[test:3:18]
             2 |             const duration d = 5ns * -1.0;
             3 |             box [d] {}
               :                  ^
             4 |         
               `----
            ]"#]],
    );
}

#[test]
fn box_can_contain_delay() {
    check_stmt_kinds(
        r#"
            include "stdgates.inc";
            qubit q;
            const duration a = 300ns;
            stretch c = 2 * a;
            box [c] {
              delay[a] q;
            }
        "#,
        &expect![[r#"
            QubitDeclaration [49-57]:
                symbol_id: 40
            ClassicalDeclarationStmt [70-95]:
                symbol_id: 41
                ty_span: [76-84]
                ty_exprs: <empty>
                init_expr: Expr [89-94]:
                    ty: const duration
                    const_value: Duration(300.0 ns)
                    kind: Lit: Duration(300.0 ns)
            ClassicalDeclarationStmt [108-126]:
                symbol_id: 42
                ty_span: [108-115]
                ty_exprs: <empty>
                init_expr: Expr [120-125]:
                    ty: stretch
                    const_value: Duration(600.0 ns)
                    kind: BinaryOpExpr:
                        op: Mul
                        lhs: Expr [120-121]:
                            ty: const int
                            kind: Lit: Int(2)
                        rhs: Expr [124-125]:
                            ty: const duration
                            kind: SymbolId(41)
            BoxStmt [139-188]:
                duration: Expr [144-145]:
                    ty: stretch
                    const_value: Duration(600.0 ns)
                    kind: SymbolId(42)
                body:
                    Stmt [163-174]:
                        annotations: <empty>
                        kind: DelayStmt [163-174]:
                            duration: Expr [169-170]:
                                ty: const duration
                                const_value: Duration(300.0 ns)
                                kind: SymbolId(41)
                            qubits:
                                GateOperand [172-173]:
                                    kind: Expr [172-173]:
                                        ty: qubit
                                        kind: SymbolId(40)
        "#]],
    );
}

#[test]
fn box_can_contain_reset() {
    check_stmt_kinds(
        r#"
            include "stdgates.inc";
            qubit q;
            box {
              reset q;
            }
        "#,
        &expect![[r#"
            QubitDeclaration [49-57]:
                symbol_id: 40
            BoxStmt [70-112]:
                duration: <none>
                body:
                    Stmt [90-98]:
                        annotations: <empty>
                        kind: ResetStmt [90-98]:
                            reset_token_span: [90-95]
                            operand: GateOperand [96-97]:
                                kind: Expr [96-97]:
                                    ty: qubit
                                    kind: SymbolId(40)
        "#]],
    );
}

#[test]
fn box_can_contain_gate_call() {
    check_stmt_kinds(
        r#"
            include "stdgates.inc";
            qubit q;
            box {
              x q;
            }
        "#,
        &expect![[r#"
            QubitDeclaration [49-57]:
                symbol_id: 40
            BoxStmt [70-108]:
                duration: <none>
                body:
                    Stmt [90-94]:
                        annotations: <empty>
                        kind: GateCall [90-94]:
                            modifiers: <empty>
                            symbol_id: 9
                            gate_name_span: [90-91]
                            args: <empty>
                            qubits:
                                GateOperand [92-93]:
                                    kind: Expr [92-93]:
                                        ty: qubit
                                        kind: SymbolId(40)
                            duration: <none>
                            classical_arity: 0
                            quantum_arity: 1
        "#]],
    );
}

#[test]
fn box_can_contain_gphase() {
    check_stmt_kinds(
        r#"
            box {
              gphase(pi);
            }
        "#,
        &expect![[r#"
            BoxStmt [13-58]:
                duration: <none>
                body:
                    Stmt [33-44]:
                        annotations: <empty>
                        kind: GateCall [33-44]:
                            modifiers: <empty>
                            symbol_id: 1
                            gate_name_span: [33-39]
                            args:
                                Expr [40-42]:
                                    ty: angle
                                    kind: Cast [40-42]:
                                        ty: angle
                                        ty_exprs: <empty>
                                        expr: Expr [40-42]:
                                            ty: const float
                                            kind: SymbolId(2)
                                        kind: Implicit
                            qubits: <empty>
                            duration: <none>
                            classical_arity: 1
                            quantum_arity: 0
        "#]],
    );
}

#[test]
fn box_can_contain_box() {
    check_stmt_kinds(
        r#"
            qubit q;
            box {
              box {
                barrier q;
              }
            }
        "#,
        &expect![[r#"
            QubitDeclaration [13-21]:
                symbol_id: 8
            BoxStmt [34-116]:
                duration: <none>
                body:
                    Stmt [54-102]:
                        annotations: <empty>
                        kind: BoxStmt [54-102]:
                            duration: <none>
                            body:
                                Stmt [76-86]:
                                    annotations: <empty>
                                    kind: BarrierStmt [76-86]:
                                        operands:
                                            GateOperand [84-85]:
                                                kind: Expr [84-85]:
                                                    ty: qubit
                                                    kind: SymbolId(8)
        "#]],
    );
}

#[test]
fn with_invalid_instruction_fails() {
    check_stmt_kinds(
        "box {
        2 + 4;
    }",
        &expect![[r#"
            Program:
                version: <none>
                pragmas: <empty>
                statements:
                    Stmt [0-26]:
                        annotations: <empty>
                        kind: BoxStmt [0-26]:
                            duration: <none>
                            body:
                                Stmt [14-20]:
                                    annotations: <empty>
                                    kind: ExprStmt [14-20]:
                                        expr: Expr [14-19]:
                                            ty: const int
                                            kind: BinaryOpExpr:
                                                op: Add
                                                lhs: Expr [14-15]:
                                                    ty: const int
                                                    kind: Lit: Int(2)
                                                rhs: Expr [18-19]:
                                                    ty: const int
                                                    kind: Lit: Int(4)

            [Qasm.Lowerer.ClassicalStmtInBox

              x invalid classical statement in box
               ,-[test:2:9]
             1 | box {
             2 |         2 + 4;
               :         ^^^^^^
             3 |     }
               `----
            ]"#]],
    );
}

#[test]
fn with_duration_fails() {
    check_stmt_kinds(
        "box [4us] { }",
        &expect![[r#"
            BoxStmt [0-13]:
                duration: Expr [5-8]:
                    ty: const duration
                    const_value: Duration(4.0 us)
                    kind: Lit: Duration(4.0 us)
                body: <empty>
        "#]],
    );
}
