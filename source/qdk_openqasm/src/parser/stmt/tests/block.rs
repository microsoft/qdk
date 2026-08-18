// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse_block;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn nested_blocks() {
    check(
        parse_block,
        "
    {
        {
            int x = 1;
            {
                x = 2;
            }
        }
    }",
        &expect![[r#"
            Block [5-106]:
                Stmt [15-100]:
                    annotations: <empty>
                    kind: Block [15-100]:
                        Stmt [29-39]:
                            annotations: <empty>
                            kind: ClassicalDeclarationStmt [29-39]:
                                type: ScalarType [29-32]: IntType [29-32]:
                                    size: <none>
                                ident: Ident [33-34] "x"
                                init_expr: Expr [37-38]: Lit: Int(1)
                        Stmt [52-90]:
                            annotations: <empty>
                            kind: Block [52-90]:
                                Stmt [70-76]:
                                    annotations: <empty>
                                    kind: AssignStmt [70-76]:
                                        lhs: Ident [70-71] "x"
                                        rhs: Expr [74-75]: Lit: Int(2)"#]],
    );
}
