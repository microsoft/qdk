// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn simple_while() {
    check(
        parse,
        "
    while (x != 2) {
        a = 0;
    }",
        &expect![[r#"
            Stmt [5-42]:
                annotations: <empty>
                kind: WhileLoop [5-42]:
                    condition: Expr [12-18]: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [12-13]: Ident [12-13] "x"
                        rhs: Expr [17-18]: Lit: Int(2)
                    body: Stmt [20-42]:
                        annotations: <empty>
                        kind: Block [20-42]:
                            Stmt [30-36]:
                                annotations: <empty>
                                kind: AssignStmt [30-36]:
                                    lhs: Ident [30-31] "a"
                                    rhs: Expr [34-35]: Lit: Int(0)"#]],
    );
}

#[test]
fn empty_while() {
    check(
        parse,
        "while (true) {}",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: WhileLoop [0-15]:
                    condition: Expr [7-11]: Lit: Bool(true)
                    body: Stmt [13-15]:
                        annotations: <empty>
                        kind: Block [13-15]: <empty>"#]],
    );
}

#[test]
fn while_stmt_body() {
    check(
        parse,
        "
    while (x != 2)
        a = 0;",
        &expect![[r#"
            Stmt [5-34]:
                annotations: <empty>
                kind: WhileLoop [5-34]:
                    condition: Expr [12-18]: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [12-13]: Ident [12-13] "x"
                        rhs: Expr [17-18]: Lit: Int(2)
                    body: Stmt [28-34]:
                        annotations: <empty>
                        kind: AssignStmt [28-34]:
                            lhs: Ident [28-29] "a"
                            rhs: Expr [32-33]: Lit: Int(0)"#]],
    );
}

#[test]
fn while_loop_with_continue_stmt() {
    check(
        parse,
        "
    while (x != 2) {
        a = 0;
        continue;
    }",
        &expect![[r#"
            Stmt [5-60]:
                annotations: <empty>
                kind: WhileLoop [5-60]:
                    condition: Expr [12-18]: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [12-13]: Ident [12-13] "x"
                        rhs: Expr [17-18]: Lit: Int(2)
                    body: Stmt [20-60]:
                        annotations: <empty>
                        kind: Block [20-60]:
                            Stmt [30-36]:
                                annotations: <empty>
                                kind: AssignStmt [30-36]:
                                    lhs: Ident [30-31] "a"
                                    rhs: Expr [34-35]: Lit: Int(0)
                            Stmt [45-54]:
                                annotations: <empty>
                                kind: ContinueStmt [45-54]"#]],
    );
}

#[test]
fn while_loop_with_break_stmt() {
    check(
        parse,
        "
    while (x != 2) {
        a = 0;
        break;
    }",
        &expect![[r#"
            Stmt [5-57]:
                annotations: <empty>
                kind: WhileLoop [5-57]:
                    condition: Expr [12-18]: BinaryOpExpr:
                        op: Neq
                        lhs: Expr [12-13]: Ident [12-13] "x"
                        rhs: Expr [17-18]: Lit: Int(2)
                    body: Stmt [20-57]:
                        annotations: <empty>
                        kind: Block [20-57]:
                            Stmt [30-36]:
                                annotations: <empty>
                                kind: AssignStmt [30-36]:
                                    lhs: Ident [30-31] "a"
                                    rhs: Expr [34-35]: Lit: Int(0)
                            Stmt [45-51]:
                                annotations: <empty>
                                kind: BreakStmt [45-51]"#]],
    );
}

#[test]
fn single_stmt_while_stmt() {
    check(
        parse,
        "while (x) z q;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: WhileLoop [0-14]:
                    condition: Expr [7-8]: Ident [7-8] "x"
                    body: Stmt [10-14]:
                        annotations: <empty>
                        kind: GateCall [10-14]:
                            modifiers: <empty>
                            name: Ident [10-11] "z"
                            args: <empty>
                            duration: <none>
                            qubits:
                                GateOperand [12-13]:
                                    kind: Ident [12-13] "q""#]],
    );
}

#[test]
fn annotations_in_single_stmt_while_stmt() {
    check(
        parse,
        "
    while (x)
        @foo
        @bar
        x = 5;",
        &expect![[r#"
            Stmt [5-55]:
                annotations: <empty>
                kind: WhileLoop [5-55]:
                    condition: Expr [12-13]: Ident [12-13] "x"
                    body: Stmt [23-55]:
                        annotations:
                            Annotation [23-27]:
                                identifier: foo
                                value: <none>
                                value_span: <none>
                            Annotation [36-40]:
                                identifier: bar
                                value: <none>
                                value_span: <none>
                        kind: AssignStmt [49-55]:
                            lhs: Ident [49-50] "x"
                            rhs: Expr [53-54]: Lit: Int(5)"#]],
    );
}

#[test]
fn nested_single_stmt_while_stmt() {
    check(
        parse,
        "while (x) while (y) z q;",
        &expect![[r#"
            Stmt [0-24]:
                annotations: <empty>
                kind: WhileLoop [0-24]:
                    condition: Expr [7-8]: Ident [7-8] "x"
                    body: Stmt [10-24]:
                        annotations: <empty>
                        kind: WhileLoop [10-24]:
                            condition: Expr [17-18]: Ident [17-18] "y"
                            body: Stmt [20-24]:
                                annotations: <empty>
                                kind: GateCall [20-24]:
                                    modifiers: <empty>
                                    name: Ident [20-21] "z"
                                    args: <empty>
                                    duration: <none>
                                    qubits:
                                        GateOperand [22-23]:
                                            kind: Ident [22-23] "q""#]],
    );
}
