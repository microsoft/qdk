// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn measure_identifier() {
    check(
        parse,
        "measure q;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: MeasureArrowStmt [0-10]:
                    measurement: MeasureExpr [0-9]:
                        operand: GateOperand [8-9]:
                            kind: Ident [8-9] "q"
                    target: <none>"#]],
    );
}

#[test]
fn measure_indented_ident() {
    check(
        parse,
        "measure q[2];",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: MeasureArrowStmt [0-13]:
                    measurement: MeasureExpr [0-12]:
                        operand: GateOperand [8-12]:
                            kind: IndexedIdent [8-12]:
                                ident: Ident [8-9] "q"
                                index_span: [9-12]
                                indices:
                                    IndexList [10-11]:
                                        values:
                                            Expr [10-11]: Lit: Int(2)
                    target: <none>"#]],
    );
}

#[test]
fn measure_hardware_qubit() {
    check(
        parse,
        "measure $42;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: MeasureArrowStmt [0-12]:
                    measurement: MeasureExpr [0-11]:
                        operand: GateOperand [8-11]:
                            kind: HardwareQubit [8-11]: 42
                    target: <none>"#]],
    );
}

#[test]
fn measure_arrow_into_ident() {
    check(
        parse,
        "measure q -> a;",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: MeasureArrowStmt [0-15]:
                    measurement: MeasureExpr [0-9]:
                        operand: GateOperand [8-9]:
                            kind: Ident [8-9] "q"
                    target: Ident [13-14] "a""#]],
    );
}

#[test]
fn measure_arrow_into_indented_ident() {
    check(
        parse,
        "measure q -> a[1];",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: MeasureArrowStmt [0-18]:
                    measurement: MeasureExpr [0-9]:
                        operand: GateOperand [8-9]:
                            kind: Ident [8-9] "q"
                    target: IndexedIdent [13-17]:
                        ident: Ident [13-14] "a"
                        index_span: [14-17]
                        indices:
                            IndexList [15-16]:
                                values:
                                    Expr [15-16]: Lit: Int(1)"#]],
    );
}

#[test]
fn assign_measure_stmt() {
    check(
        parse,
        "c = measure q;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: AssignStmt [0-14]:
                    lhs: Ident [0-1] "c"
                    rhs: MeasureExpr [4-13]:
                        operand: GateOperand [12-13]:
                            kind: Ident [12-13] "q""#]],
    );
}
