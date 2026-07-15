// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn simple_if_stmt() {
    check(
        parse,
        "
    if (x == y) {
        a = 0;
    } else {
        a = 1;
    }
    ",
        &expect![[r#"
            Stmt [5-67]:
                annotations: <empty>
                kind: IfStmt [5-67]:
                    condition: Expr [9-15]: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [9-10]: Ident [9-10] "x"
                        rhs: Expr [14-15]: Ident [14-15] "y"
                    if_body: Stmt [17-39]:
                        annotations: <empty>
                        kind: Block [17-39]:
                            Stmt [27-33]:
                                annotations: <empty>
                                kind: AssignStmt [27-33]:
                                    lhs: Ident [27-28] "a"
                                    rhs: Expr [31-32]: Lit: Int(0)
                    else_body: Stmt [45-67]:
                        annotations: <empty>
                        kind: Block [45-67]:
                            Stmt [55-61]:
                                annotations: <empty>
                                kind: AssignStmt [55-61]:
                                    lhs: Ident [55-56] "a"
                                    rhs: Expr [59-60]: Lit: Int(1)"#]],
    );
}

#[test]
fn if_stmt_missing_else() {
    check(
        parse,
        "
    if (x == y) {
        a = 0;
    }
    ",
        &expect![[r#"
            Stmt [5-39]:
                annotations: <empty>
                kind: IfStmt [5-39]:
                    condition: Expr [9-15]: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [9-10]: Ident [9-10] "x"
                        rhs: Expr [14-15]: Ident [14-15] "y"
                    if_body: Stmt [17-39]:
                        annotations: <empty>
                        kind: Block [17-39]:
                            Stmt [27-33]:
                                annotations: <empty>
                                kind: AssignStmt [27-33]:
                                    lhs: Ident [27-28] "a"
                                    rhs: Expr [31-32]: Lit: Int(0)
                    else_body: <none>"#]],
    );
}

#[test]
fn nested_if_stmts() {
    check(
        parse,
        "
    if (x == y) {
        if (x1 == y1) {
            a = 0;
        } else {
            a = 1;
        }
    } else {
        if (x2 == y2) {
            a = 2;
        } else {
            a = 3;
        }
    }
    ",
        &expect![[r#"
            Stmt [5-215]:
                annotations: <empty>
                kind: IfStmt [5-215]:
                    condition: Expr [9-15]: BinaryOpExpr:
                        op: Eq
                        lhs: Expr [9-10]: Ident [9-10] "x"
                        rhs: Expr [14-15]: Ident [14-15] "y"
                    if_body: Stmt [17-113]:
                        annotations: <empty>
                        kind: Block [17-113]:
                            Stmt [27-107]:
                                annotations: <empty>
                                kind: IfStmt [27-107]:
                                    condition: Expr [31-39]: BinaryOpExpr:
                                        op: Eq
                                        lhs: Expr [31-33]: Ident [31-33] "x1"
                                        rhs: Expr [37-39]: Ident [37-39] "y1"
                                    if_body: Stmt [41-71]:
                                        annotations: <empty>
                                        kind: Block [41-71]:
                                            Stmt [55-61]:
                                                annotations: <empty>
                                                kind: AssignStmt [55-61]:
                                                    lhs: Ident [55-56] "a"
                                                    rhs: Expr [59-60]: Lit: Int(0)
                                    else_body: Stmt [77-107]:
                                        annotations: <empty>
                                        kind: Block [77-107]:
                                            Stmt [91-97]:
                                                annotations: <empty>
                                                kind: AssignStmt [91-97]:
                                                    lhs: Ident [91-92] "a"
                                                    rhs: Expr [95-96]: Lit: Int(1)
                    else_body: Stmt [119-215]:
                        annotations: <empty>
                        kind: Block [119-215]:
                            Stmt [129-209]:
                                annotations: <empty>
                                kind: IfStmt [129-209]:
                                    condition: Expr [133-141]: BinaryOpExpr:
                                        op: Eq
                                        lhs: Expr [133-135]: Ident [133-135] "x2"
                                        rhs: Expr [139-141]: Ident [139-141] "y2"
                                    if_body: Stmt [143-173]:
                                        annotations: <empty>
                                        kind: Block [143-173]:
                                            Stmt [157-163]:
                                                annotations: <empty>
                                                kind: AssignStmt [157-163]:
                                                    lhs: Ident [157-158] "a"
                                                    rhs: Expr [161-162]: Lit: Int(2)
                                    else_body: Stmt [179-209]:
                                        annotations: <empty>
                                        kind: Block [179-209]:
                                            Stmt [193-199]:
                                                annotations: <empty>
                                                kind: AssignStmt [193-199]:
                                                    lhs: Ident [193-194] "a"
                                                    rhs: Expr [197-198]: Lit: Int(3)"#]],
    );
}

#[test]
fn single_stmt_if_stmt() {
    check(
        parse,
        "if (x) z q;",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: IfStmt [0-11]:
                    condition: Expr [4-5]: Ident [4-5] "x"
                    if_body: Stmt [7-11]:
                        annotations: <empty>
                        kind: GateCall [7-11]:
                            modifiers: <empty>
                            name: Ident [7-8] "z"
                            args: <empty>
                            duration: <none>
                            qubits:
                                GateOperand [9-10]:
                                    kind: Ident [9-10] "q"
                    else_body: <none>"#]],
    );
}

#[test]
fn annotations_in_single_stmt_if_stmt() {
    check(
        parse,
        "
    if (x)
        @foo
        @bar
        x = 5;",
        &expect![[r#"
            Stmt [5-52]:
                annotations: <empty>
                kind: IfStmt [5-52]:
                    condition: Expr [9-10]: Ident [9-10] "x"
                    if_body: Stmt [20-52]:
                        annotations:
                            Annotation [20-24]:
                                identifier: foo
                                value: <none>
                                value_span: <none>
                            Annotation [33-37]:
                                identifier: bar
                                value: <none>
                                value_span: <none>
                        kind: AssignStmt [46-52]:
                            lhs: Ident [46-47] "x"
                            rhs: Expr [50-51]: Lit: Int(5)
                    else_body: <none>"#]],
    );
}

#[test]
fn nested_single_stmt_if_stmt() {
    check(
        parse,
        "if (x) if (y) z q;",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: IfStmt [0-18]:
                    condition: Expr [4-5]: Ident [4-5] "x"
                    if_body: Stmt [7-18]:
                        annotations: <empty>
                        kind: IfStmt [7-18]:
                            condition: Expr [11-12]: Ident [11-12] "y"
                            if_body: Stmt [14-18]:
                                annotations: <empty>
                                kind: GateCall [14-18]:
                                    modifiers: <empty>
                                    name: Ident [14-15] "z"
                                    args: <empty>
                                    duration: <none>
                                    qubits:
                                        GateOperand [16-17]:
                                            kind: Ident [16-17] "q"
                            else_body: <none>
                    else_body: <none>"#]],
    );
}

#[test]
fn nested_single_stmt_if_else_stmt() {
    check(
        parse,
        "if (x) if (y) z q; else if (a) if (b) h q;",
        &expect![[r#"
            Stmt [0-42]:
                annotations: <empty>
                kind: IfStmt [0-42]:
                    condition: Expr [4-5]: Ident [4-5] "x"
                    if_body: Stmt [7-42]:
                        annotations: <empty>
                        kind: IfStmt [7-42]:
                            condition: Expr [11-12]: Ident [11-12] "y"
                            if_body: Stmt [14-18]:
                                annotations: <empty>
                                kind: GateCall [14-18]:
                                    modifiers: <empty>
                                    name: Ident [14-15] "z"
                                    args: <empty>
                                    duration: <none>
                                    qubits:
                                        GateOperand [16-17]:
                                            kind: Ident [16-17] "q"
                            else_body: Stmt [24-42]:
                                annotations: <empty>
                                kind: IfStmt [24-42]:
                                    condition: Expr [28-29]: Ident [28-29] "a"
                                    if_body: Stmt [31-42]:
                                        annotations: <empty>
                                        kind: IfStmt [31-42]:
                                            condition: Expr [35-36]: Ident [35-36] "b"
                                            if_body: Stmt [38-42]:
                                                annotations: <empty>
                                                kind: GateCall [38-42]:
                                                    modifiers: <empty>
                                                    name: Ident [38-39] "h"
                                                    args: <empty>
                                                    duration: <none>
                                                    qubits:
                                                        GateOperand [40-41]:
                                                            kind: Ident [40-41] "q"
                                            else_body: <none>
                                    else_body: <none>
                    else_body: <none>"#]],
    );
}

#[test]
fn single_stmt_if_stmt_else_stmt() {
    check(
        parse,
        "if (x) z q; else x q;",
        &expect![[r#"
            Stmt [0-21]:
                annotations: <empty>
                kind: IfStmt [0-21]:
                    condition: Expr [4-5]: Ident [4-5] "x"
                    if_body: Stmt [7-11]:
                        annotations: <empty>
                        kind: GateCall [7-11]:
                            modifiers: <empty>
                            name: Ident [7-8] "z"
                            args: <empty>
                            duration: <none>
                            qubits:
                                GateOperand [9-10]:
                                    kind: Ident [9-10] "q"
                    else_body: Stmt [17-21]:
                        annotations: <empty>
                        kind: GateCall [17-21]:
                            modifiers: <empty>
                            name: Ident [17-18] "x"
                            args: <empty>
                            duration: <none>
                            qubits:
                                GateOperand [19-20]:
                                    kind: Ident [19-20] "q""#]],
    );
}
