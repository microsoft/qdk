// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn u_gate_with_two_args() {
    check(
        parse,
        "U (1)(2) $0;",
        &expect![[r#"
        Error(
            Convert(
                "identifier",
                "",
                Span {
                    lo: 0,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn invalid_modifier() {
    check(
        parse,
        "notmodifier @ x $0;",
        &expect![[r#"
        Stmt [0-11]:
            annotations: <empty>
            kind: ExprStmt [0-11]:
                expr: Expr [0-11]: Ident [0-11] "notmodifier"

        [
            Error(
                Token(
                    Semicolon,
                    At,
                    Span {
                        lo: 12,
                        hi: 13,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn pow_without_arg() {
    check(
        parse,
        "pow @ x $0;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                At,
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn pow_with_two_args() {
    check(
        parse,
        "pow(2, 3) @ x $0;",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: GateCall [0-17]:
                    modifiers:
                        QuantumGateModifier [0-11]:
                            modifier_keyword_span: [0-3]
                            kind: Pow Expr [4-5]: Lit: Int(2)
                    name: Ident [12-13] "x"
                    args: <empty>
                    duration: <none>
                    qubits:
                        GateOperand [14-16]:
                            kind: HardwareQubit [14-16]: 0

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        Comma,
                        Span {
                            lo: 5,
                            hi: 6,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn ctrl_with_two_args() {
    check(
        parse,
        "ctrl(2, 3) @ x $0, $1;",
        &expect![[r#"
            Stmt [0-22]:
                annotations: <empty>
                kind: GateCall [0-22]:
                    modifiers:
                        QuantumGateModifier [0-12]:
                            modifier_keyword_span: [0-4]
                            kind: Ctrl Some(Expr { span: Span { lo: 5, hi: 6 }, kind: Lit(Lit { span: Span { lo: 5, hi: 6 }, kind: Int(2) }) })
                    name: Ident [13-14] "x"
                    args: <empty>
                    duration: <none>
                    qubits:
                        GateOperand [15-17]:
                            kind: HardwareQubit [15-17]: 0
                        GateOperand [19-21]:
                            kind: HardwareQubit [19-21]: 1

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        Comma,
                        Span {
                            lo: 6,
                            hi: 7,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn negctrl_with_two_args() {
    check(
        parse,
        "negctrl(2, 3) @ x $0, $1;",
        &expect![[r#"
            Stmt [0-25]:
                annotations: <empty>
                kind: GateCall [0-25]:
                    modifiers:
                        QuantumGateModifier [0-15]:
                            modifier_keyword_span: [0-7]
                            kind: NegCtrl Some(Expr { span: Span { lo: 8, hi: 9 }, kind: Lit(Lit { span: Span { lo: 8, hi: 9 }, kind: Int(2) }) })
                    name: Ident [16-17] "x"
                    args: <empty>
                    duration: <none>
                    qubits:
                        GateOperand [18-20]:
                            kind: HardwareQubit [18-20]: 0
                        GateOperand [22-24]:
                            kind: HardwareQubit [22-24]: 1

            [
                Error(
                    Token(
                        Close(
                            Paren,
                        ),
                        Comma,
                        Span {
                            lo: 9,
                            hi: 10,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn inv_with_arg() {
    check(
        parse,
        "inv(1) @ ctrl @ x $0, $1;",
        &expect![[r#"
            Stmt [0-25]:
                annotations: <empty>
                kind: GateCall [0-25]:
                    modifiers:
                        QuantumGateModifier [0-8]:
                            modifier_keyword_span: [0-3]
                            kind: Inv
                        QuantumGateModifier [9-15]:
                            modifier_keyword_span: [9-13]
                            kind: Ctrl None
                    name: Ident [16-17] "x"
                    args: <empty>
                    duration: <none>
                    qubits:
                        GateOperand [18-20]:
                            kind: HardwareQubit [18-20]: 0
                        GateOperand [22-24]:
                            kind: HardwareQubit [22-24]: 1

            [
                Error(
                    Token(
                        At,
                        Open(
                            Paren,
                        ),
                        Span {
                            lo: 3,
                            hi: 4,
                        },
                    ),
                ),
            ]"#]],
    );
}
