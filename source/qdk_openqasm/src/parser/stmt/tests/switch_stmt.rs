// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse_switch_stmt, tests::check};
use expect_test::expect;

#[test]
fn simple_switch() {
    check(
        parse_switch_stmt,
        "
        switch (x) {
          case 1 {}
          default {}
        }
    ",
        &expect![[r#"
            SwitchStmt [9-72]:
                target: Expr [17-18]: Ident [17-18] "x"
                cases:
                    SwitchCase [32-41]:
                        labels:
                            Expr [37-38]: Lit: Int(1)
                        block: Block [39-41]: <empty>
                default_case: Block [60-62]: <empty>"#]],
    );
}

#[test]
fn no_cases_no_default() {
    check(
        parse_switch_stmt,
        "
        switch (x) {}
    ",
        &expect![[r#"
            SwitchStmt [9-22]:
                target: Expr [17-18]: Ident [17-18] "x"
                cases: <empty>
                default_case: <none>

            [
                Error(
                    MissingSwitchCases(
                        Span {
                            lo: 21,
                            hi: 21,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn no_cases() {
    check(
        parse_switch_stmt,
        "
        switch (x) {
          default {}
        }
    ",
        &expect![[r#"
            SwitchStmt [9-52]:
                target: Expr [17-18]: Ident [17-18] "x"
                cases: <empty>
                default_case: Block [40-42]: <empty>

            [
                Error(
                    MissingSwitchCases(
                        Span {
                            lo: 32,
                            hi: 21,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn no_default() {
    check(
        parse_switch_stmt,
        "
        switch (x) {
          case 0, 1 {}
        }
    ",
        &expect![[r#"
            SwitchStmt [9-54]:
                target: Expr [17-18]: Ident [17-18] "x"
                cases:
                    SwitchCase [32-44]:
                        labels:
                            Expr [37-38]: Lit: Int(0)
                            Expr [40-41]: Lit: Int(1)
                        block: Block [42-44]: <empty>
                default_case: <none>"#]],
    );
}

#[test]
fn case_with_no_labels() {
    check(
        parse_switch_stmt,
        "
        switch (x) {
          case {}
        }
    ",
        &expect![[r#"
            SwitchStmt [9-49]:
                target: Expr [17-18]: Ident [17-18] "x"
                cases:
                    SwitchCase [32-39]:
                        labels: <empty>
                        block: Block [37-39]: <empty>
                default_case: <none>

            [
                Error(
                    MissingSwitchCaseLabels(
                        Span {
                            lo: 32,
                            hi: 36,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn multiple_cases() {
    check(
        parse_switch_stmt,
        "
        switch (x) {
          case 0 { int x = 0; }
          case 1 { int y = 1; }
        }
    ",
        &expect![[r#"
            SwitchStmt [9-95]:
                target: Expr [17-18]: Ident [17-18] "x"
                cases:
                    SwitchCase [32-53]:
                        labels:
                            Expr [37-38]: Lit: Int(0)
                        block: Block [39-53]:
                            Stmt [41-51]:
                                annotations: <empty>
                                kind: ClassicalDeclarationStmt [41-51]:
                                    type: ScalarType [41-44]: IntType [41-44]:
                                        size: <none>
                                    ident: Ident [45-46] "x"
                                    init_expr: Expr [49-50]: Lit: Int(0)
                    SwitchCase [64-85]:
                        labels:
                            Expr [69-70]: Lit: Int(1)
                        block: Block [71-85]:
                            Stmt [73-83]:
                                annotations: <empty>
                                kind: ClassicalDeclarationStmt [73-83]:
                                    type: ScalarType [73-76]: IntType [73-76]:
                                        size: <none>
                                    ident: Ident [77-78] "y"
                                    init_expr: Expr [81-82]: Lit: Int(1)
                default_case: <none>"#]],
    );
}
