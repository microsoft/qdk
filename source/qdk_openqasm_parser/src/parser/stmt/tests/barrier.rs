// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn barrier() {
    check(
        parse,
        "barrier r, q[0], $2;",
        &expect![[r#"
            Stmt [0-20]:
                annotations: <empty>
                kind: BarrierStmt [0-20]:
                    operands:
                        GateOperand [8-9]:
                            kind: Ident [8-9] "r"
                        GateOperand [11-15]:
                            kind: IndexedIdent [11-15]:
                                ident: Ident [11-12] "q"
                                index_span: [12-15]
                                indices:
                                    IndexList [13-14]:
                                        values:
                                            Expr [13-14]: Lit: Int(0)
                        GateOperand [17-19]:
                            kind: HardwareQubit [17-19]: 2"#]],
    );
}

#[test]
fn barrier_no_args() {
    check(
        parse,
        "barrier;",
        &expect![[r#"
            Stmt [0-8]:
                annotations: <empty>
                kind: BarrierStmt [0-8]:
                    operands: <empty>"#]],
    );
}
