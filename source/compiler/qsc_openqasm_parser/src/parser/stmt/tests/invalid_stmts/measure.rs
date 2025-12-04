// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn measure_multiple_qubits() {
    check(
        parse,
        "measure $0, $1;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: MeasureArrowStmt [0-10]:
                    measurement: MeasureExpr [0-10]:
                        operand: GateOperand [8-10]:
                            kind: HardwareQubit [8-10]: 0
                    target: <none>

            [
                Error(
                    Token(
                        Semicolon,
                        Comma,
                        Span {
                            lo: 10,
                            hi: 11,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn assign_measure_multiple_qubits() {
    check(
        parse,
        "a[0:1] = measure $0, $1;",
        &expect![[r#"
            Stmt [0-19]:
                annotations: <empty>
                kind: AssignStmt [0-19]:
                    lhs: IndexedIdent [0-6]:
                        ident: Ident [0-1] "a"
                        index_span: [1-6]
                        indices:
                            IndexList [2-5]:
                                values:
                                    Range [2-5]:
                                        start: Expr [2-3]: Lit: Int(0)
                                        step: <none>
                                        end: Expr [4-5]: Lit: Int(1)
                    rhs: MeasureExpr [9-19]:
                        operand: GateOperand [17-19]:
                            kind: HardwareQubit [17-19]: 0

            [
                Error(
                    Token(
                        Semicolon,
                        Comma,
                        Span {
                            lo: 19,
                            hi: 20,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn assign_arrow() {
    check(
        parse,
        "a = measure $0 -> b;",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: AssignStmt [0-14]:
                    lhs: Ident [0-1] "a"
                    rhs: MeasureExpr [4-14]:
                        operand: GateOperand [12-14]:
                            kind: HardwareQubit [12-14]: 0

            [
                Error(
                    Token(
                        Semicolon,
                        Arrow,
                        Span {
                            lo: 15,
                            hi: 17,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn initialized_creg() {
    check(
        parse,
        "creg a[1] = measure $0;",
        &expect![[r#"
        Stmt [0-9]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-9]:
                type: ScalarType [0-9]: BitType [0-9]:
                    size: Expr [7-8]: Lit: Int(1)
                ident: Ident [5-6] "a"
                init_expr: <none>

        [
            Error(
                Token(
                    Semicolon,
                    Eq,
                    Span {
                        lo: 10,
                        hi: 11,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn invalid_arrow_target() {
    check(
        parse,
        "measure $0 -> creg a[1];",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Keyword(
                    CReg,
                ),
                Span {
                    lo: 14,
                    hi: 18,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "measure $0 -> bit[1] a;",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Type(
                    Bit,
                ),
                Span {
                    lo: 14,
                    hi: 17,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn measure_cant_be_used_in_sub_expressions() {
    check(
        parse,
        "a = 2 * measure $0;",
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Keyword(
                        Measure,
                    ),
                    Span {
                        lo: 8,
                        hi: 15,
                    },
                ),
            )
        "#]],
    );
    check(
        parse,
        "a = (measure $0) + (measure $1);",
        &expect![[r#"
            Error(
                Token(
                    Close(
                        Paren,
                    ),
                    Keyword(
                        Measure,
                    ),
                    Span {
                        lo: 5,
                        hi: 12,
                    },
                ),
            )
        "#]],
    );
}
