// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::parser::tests::check;

use crate::parser::stmt::parse;

#[test]
fn no_qubits_no_classical() {
    check(
        parse,
        "gate c0q0 {}",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: Gate [0-12]:
                    ident: Ident [5-9] "c0q0"
                    parameters: <empty>
                    qubits: <empty>
                    body: Block [10-12]: <empty>"#]],
    );
}

#[test]
fn no_qubits_no_classical_with_parens() {
    check(
        parse,
        "gate c0q0() {}",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: Gate [0-14]:
                    ident: Ident [5-9] "c0q0"
                    parameters: <empty>
                    qubits: <empty>
                    body: Block [12-14]: <empty>"#]],
    );
}

#[test]
fn one_qubit_no_classical() {
    check(
        parse,
        "gate c0q1 a {}",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: Gate [0-14]:
                    ident: Ident [5-9] "c0q1"
                    parameters: <empty>
                    qubits:
                        Ident [10-11] "a"
                    body: Block [12-14]: <empty>"#]],
    );
}

#[test]
fn two_qubits_no_classical() {
    check(
        parse,
        "gate c0q2 a, b {}",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: Gate [0-17]:
                    ident: Ident [5-9] "c0q2"
                    parameters: <empty>
                    qubits:
                        Ident [10-11] "a"
                        Ident [13-14] "b"
                    body: Block [15-17]: <empty>"#]],
    );
}

#[test]
fn three_qubits_trailing_comma_no_classical() {
    check(
        parse,
        "gate c0q3 a, b, c, {}",
        &expect![[r#"
            Stmt [0-21]:
                annotations: <empty>
                kind: Gate [0-21]:
                    ident: Ident [5-9] "c0q3"
                    parameters: <empty>
                    qubits:
                        Ident [10-11] "a"
                        Ident [13-14] "b"
                        Ident [16-17] "c"
                    body: Block [19-21]: <empty>"#]],
    );
}

#[test]
fn no_qubits_one_classical() {
    check(
        parse,
        "gate c1q0(a) {}",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: Gate [0-15]:
                    ident: Ident [5-9] "c1q0"
                    parameters:
                        Ident [10-11] "a"
                    qubits: <empty>
                    body: Block [13-15]: <empty>"#]],
    );
}

#[test]
fn no_qubits_two_classical() {
    check(
        parse,
        "gate c2q0(a, b) {}",
        &expect![[r#"
            Stmt [0-18]:
                annotations: <empty>
                kind: Gate [0-18]:
                    ident: Ident [5-9] "c2q0"
                    parameters:
                        Ident [10-11] "a"
                        Ident [13-14] "b"
                    qubits: <empty>
                    body: Block [16-18]: <empty>"#]],
    );
}

#[test]
fn no_qubits_three_classical() {
    check(
        parse,
        "gate c3q0(a, b, c) {}",
        &expect![[r#"
            Stmt [0-21]:
                annotations: <empty>
                kind: Gate [0-21]:
                    ident: Ident [5-9] "c3q0"
                    parameters:
                        Ident [10-11] "a"
                        Ident [13-14] "b"
                        Ident [16-17] "c"
                    qubits: <empty>
                    body: Block [19-21]: <empty>"#]],
    );
}

#[test]
fn one_qubit_one_classical() {
    check(
        parse,
        "gate c1q1(a) b {}",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: Gate [0-17]:
                    ident: Ident [5-9] "c1q1"
                    parameters:
                        Ident [10-11] "a"
                    qubits:
                        Ident [13-14] "b"
                    body: Block [15-17]: <empty>"#]],
    );
}

#[test]
fn two_qubits_two_classical() {
    check(
        parse,
        "gate c2q2(a, b) c, d {}",
        &expect![[r#"
            Stmt [0-23]:
                annotations: <empty>
                kind: Gate [0-23]:
                    ident: Ident [5-9] "c2q2"
                    parameters:
                        Ident [10-11] "a"
                        Ident [13-14] "b"
                    qubits:
                        Ident [16-17] "c"
                        Ident [19-20] "d"
                    body: Block [21-23]: <empty>"#]],
    );
}

#[test]
fn two_qubits_two_classical_with_body() {
    check(
        parse,
        "gate c2q2(a, b) c, d { float[32] x = a - b; }",
        &expect![[r#"
            Stmt [0-45]:
                annotations: <empty>
                kind: Gate [0-45]:
                    ident: Ident [5-9] "c2q2"
                    parameters:
                        Ident [10-11] "a"
                        Ident [13-14] "b"
                    qubits:
                        Ident [16-17] "c"
                        Ident [19-20] "d"
                    body: Block [21-45]:
                        Stmt [23-43]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [23-43]:
                                type: ScalarType [23-32]: FloatType [23-32]:
                                    size: Expr [29-31]: Lit: Int(32)
                                ident: Ident [33-34] "x"
                                init_expr: Expr [37-42]: BinaryOpExpr:
                                    op: Sub
                                    lhs: Expr [37-38]: Ident [37-38] "a"
                                    rhs: Expr [41-42]: Ident [41-42] "b""#]],
    );
}
