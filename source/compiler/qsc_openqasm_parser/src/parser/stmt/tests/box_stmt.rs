// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn box_stmt() {
    check(
        parse,
        "
    box {
        H q0;
        Rx(2.4) q1;
    }",
        &expect![[r#"
            Stmt [5-50]:
                annotations: <empty>
                kind: BoxStmt [5-50]:
                    duration: <none>
                    body:
                        Stmt [19-24]:
                            annotations: <empty>
                            kind: GateCall [19-24]:
                                modifiers: <empty>
                                name: Ident [19-20] "H"
                                args: <empty>
                                duration: <none>
                                qubits:
                                    GateOperand [21-23]:
                                        kind: Ident [21-23] "q0"
                        Stmt [33-44]:
                            annotations: <empty>
                            kind: GateCall [33-44]:
                                modifiers: <empty>
                                name: Ident [33-35] "Rx"
                                args:
                                    Expr [36-39]: Lit: Float(2.4)
                                duration: <none>
                                qubits:
                                    GateOperand [41-43]:
                                        kind: Ident [41-43] "q1""#]],
    );
}

#[test]
fn box_stmt_with_designator() {
    check(
        parse,
        "
    box[4us] {
        H q0;
        Rx(2.4) q1;
    }",
        &expect![[r#"
            Stmt [5-55]:
                annotations: <empty>
                kind: BoxStmt [5-55]:
                    duration: Expr [9-12]: Lit: Duration(4.0, Us)
                    body:
                        Stmt [24-29]:
                            annotations: <empty>
                            kind: GateCall [24-29]:
                                modifiers: <empty>
                                name: Ident [24-25] "H"
                                args: <empty>
                                duration: <none>
                                qubits:
                                    GateOperand [26-28]:
                                        kind: Ident [26-28] "q0"
                        Stmt [38-49]:
                            annotations: <empty>
                            kind: GateCall [38-49]:
                                modifiers: <empty>
                                name: Ident [38-40] "Rx"
                                args:
                                    Expr [41-44]: Lit: Float(2.4)
                                duration: <none>
                                qubits:
                                    GateOperand [46-48]:
                                        kind: Ident [46-48] "q1""#]],
    );
}
