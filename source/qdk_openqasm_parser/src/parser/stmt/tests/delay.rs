// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn delay() {
    check(
        parse,
        "delay[a] q[0], q[1];",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: DelayStmt [0-20]:
                    duration: Expr [6-7]: Ident [6-7] "a"
                    qubits:
                        GateOperand [9-13]:
                            kind: IndexedIdent [9-13]:
                                ident: Ident [9-10] "q"
                                index_span: [10-13]
                                indices:
                                    IndexList [11-12]:
                                        values:
                                            Expr [11-12]: Lit: Int(0)
                        GateOperand [15-19]:
                            kind: IndexedIdent [15-19]:
                                ident: Ident [15-16] "q"
                                index_span: [16-19]
                                indices:
                                    IndexList [17-18]:
                                        values:
                                            Expr [17-18]: Lit: Int(1)"#]],
    );
}
