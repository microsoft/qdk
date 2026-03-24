// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn reset_ident() {
    check(
        parse,
        "reset a;",
        &expect![[r#"
            Stmt [0-8]:
                annotations: <empty>
                kind: ResetStmt [0-8]:
                    reset_token_span: [0-5]
                    operand: GateOperand [6-7]:
                        kind: Ident [6-7] "a""#]],
    );
}

#[test]
fn reset_indexed_ident() {
    check(
        parse,
        "reset a[1];",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ResetStmt [0-11]:
                    reset_token_span: [0-5]
                    operand: GateOperand [6-10]:
                        kind: IndexedIdent [6-10]:
                            ident: Ident [6-7] "a"
                            index_span: [7-10]
                            indices:
                                IndexList [8-9]:
                                    values:
                                        Expr [8-9]: Lit: Int(1)"#]],
    );
}

#[test]
fn reset_hardware_qubit() {
    check(
        parse,
        "reset $42;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ResetStmt [0-10]:
                    reset_token_span: [0-5]
                    operand: GateOperand [6-9]:
                        kind: HardwareQubit [6-9]: 42"#]],
    );
}
