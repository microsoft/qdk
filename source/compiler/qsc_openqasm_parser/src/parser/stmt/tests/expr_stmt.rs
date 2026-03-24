// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::stmt::parse;
use crate::parser::tests::check;
use expect_test::expect;

#[test]
fn identifier() {
    check(
        parse,
        "H;",
        &expect![[r#"
            Stmt [0-2]:
                annotations: <empty>
                kind: ExprStmt [0-2]:
                    expr: Expr [0-1]: Ident [0-1] "H""#]],
    );
}

#[test]
fn identifier_plus_number() {
    check(
        parse,
        "H + 2;",
        &expect![[r#"
            Stmt [0-6]:
                annotations: <empty>
                kind: ExprStmt [0-6]:
                    expr: Expr [0-5]: BinaryOpExpr:
                        op: Add
                        lhs: Expr [0-1]: Ident [0-1] "H"
                        rhs: Expr [4-5]: Lit: Int(2)"#]],
    );
}

#[test]
fn assignment() {
    check(
        parse,
        "a = 1;",
        &expect![[r#"
            Stmt [0-6]:
                annotations: <empty>
                kind: AssignStmt [0-6]:
                    lhs: Ident [0-1] "a"
                    rhs: Expr [4-5]: Lit: Int(1)"#]],
    );
}

#[test]
fn index_assignment() {
    check(
        parse,
        "a[0] = 1;",
        &expect![[r#"
            Stmt [0-9]:
                annotations: <empty>
                kind: AssignStmt [0-9]:
                    lhs: IndexedIdent [0-4]:
                        ident: Ident [0-1] "a"
                        index_span: [1-4]
                        indices:
                            IndexList [2-3]:
                                values:
                                    Expr [2-3]: Lit: Int(0)
                    rhs: Expr [7-8]: Lit: Int(1)"#]],
    );
}

#[test]
fn multi_index_assignment() {
    check(
        parse,
        "a[0][1] = 1;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: AssignStmt [0-12]:
                    lhs: IndexedIdent [0-7]:
                        ident: Ident [0-1] "a"
                        index_span: [1-7]
                        indices:
                            IndexList [2-3]:
                                values:
                                    Expr [2-3]: Lit: Int(0)
                            IndexList [5-6]:
                                values:
                                    Expr [5-6]: Lit: Int(1)
                    rhs: Expr [10-11]: Lit: Int(1)"#]],
    );
}

#[test]
fn assignment_op() {
    check(
        parse,
        "a += 1;",
        &expect![[r#"
            Stmt [0-7]:
                annotations: <empty>
                kind: AssignOpStmt [0-7]:
                    op: Add
                    lhs: Ident [0-1] "a"
                    rhs: Expr [5-6]: Lit: Int(1)"#]],
    );
}

#[test]
fn index_assignment_op() {
    check(
        parse,
        "a[0] += 1;",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: AssignOpStmt [0-10]:
                    op: Add
                    lhs: IndexedIdent [0-4]:
                        ident: Ident [0-1] "a"
                        index_span: [1-4]
                        indices:
                            IndexList [2-3]:
                                values:
                                    Expr [2-3]: Lit: Int(0)
                    rhs: Expr [8-9]: Lit: Int(1)"#]],
    );
}

#[test]
fn multi_index_assignment_op() {
    check(
        parse,
        "a[0][1] += 1;",
        &expect![[r#"
            Stmt [0-13]:
                annotations: <empty>
                kind: AssignOpStmt [0-13]:
                    op: Add
                    lhs: IndexedIdent [0-7]:
                        ident: Ident [0-1] "a"
                        index_span: [1-7]
                        indices:
                            IndexList [2-3]:
                                values:
                                    Expr [2-3]: Lit: Int(0)
                            IndexList [5-6]:
                                values:
                                    Expr [5-6]: Lit: Int(1)
                    rhs: Expr [11-12]: Lit: Int(1)"#]],
    );
}

#[test]
fn assignment_and_unop() {
    check(
        parse,
        "c = a && !b;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: AssignStmt [0-12]:
                    lhs: Ident [0-1] "c"
                    rhs: Expr [4-11]: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [4-5]: Ident [4-5] "a"
                        rhs: Expr [9-11]: UnaryOpExpr:
                            op: NotL
                            expr: Expr [10-11]: Ident [10-11] "b""#]],
    );
}

#[test]
fn assignment_unop_and() {
    check(
        parse,
        "d = !a && b;",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: AssignStmt [0-12]:
                    lhs: Ident [0-1] "d"
                    rhs: Expr [4-11]: BinaryOpExpr:
                        op: AndL
                        lhs: Expr [4-6]: UnaryOpExpr:
                            op: NotL
                            expr: Expr [5-6]: Ident [5-6] "a"
                        rhs: Expr [10-11]: Ident [10-11] "b""#]],
    );
}

// These are negative unit tests for gate calls:

#[test]
fn function_call_plus_ident() {
    check(
        parse,
        "Name(2, 3) + a;",
        &expect![[r#"
            Stmt [0-15]:
                annotations: <empty>
                kind: ExprStmt [0-15]:
                    expr: Expr [0-14]: BinaryOpExpr:
                        op: Add
                        lhs: Expr [0-10]: FunctionCall [0-10]:
                            name: Ident [0-4] "Name"
                            args:
                                Expr [5-6]: Lit: Int(2)
                                Expr [8-9]: Lit: Int(3)
                        rhs: Expr [13-14]: Ident [13-14] "a""#]],
    );
}

#[test]
fn function_call() {
    check(
        parse,
        "Name(2, 3);",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ExprStmt [0-11]:
                    expr: Expr [0-10]: FunctionCall [0-10]:
                        name: Ident [0-4] "Name"
                        args:
                            Expr [5-6]: Lit: Int(2)
                            Expr [8-9]: Lit: Int(3)"#]],
    );
}

#[test]
fn indexed_function_call() {
    check(
        parse,
        "Name(2, 3)[1];",
        &expect![[r#"
            Stmt [0-14]:
                annotations: <empty>
                kind: ExprStmt [0-14]:
                    expr: Expr [0-13]: IndexExpr [0-13]:
                        collection: Expr [0-10]: FunctionCall [0-10]:
                            name: Ident [0-4] "Name"
                            args:
                                Expr [5-6]: Lit: Int(2)
                                Expr [8-9]: Lit: Int(3)
                        index: IndexList [11-12]:
                            values:
                                Expr [11-12]: Lit: Int(1)"#]],
    );
}

#[test]
fn multi_indexed_function_call() {
    check(
        parse,
        "Name(2, 3)[1, 0];",
        &expect![[r#"
            Stmt [0-17]:
                annotations: <empty>
                kind: ExprStmt [0-17]:
                    expr: Expr [0-16]: IndexExpr [0-16]:
                        collection: Expr [0-10]: FunctionCall [0-10]:
                            name: Ident [0-4] "Name"
                            args:
                                Expr [5-6]: Lit: Int(2)
                                Expr [8-9]: Lit: Int(3)
                        index: IndexList [11-15]:
                            values:
                                Expr [11-12]: Lit: Int(1)
                                Expr [14-15]: Lit: Int(0)"#]],
    );
}

#[test]
fn ident() {
    check(
        parse,
        "Name;",
        &expect![[r#"
            Stmt [0-5]:
                annotations: <empty>
                kind: ExprStmt [0-5]:
                    expr: Expr [0-4]: Ident [0-4] "Name""#]],
    );
}

#[test]
fn index_expr() {
    check(
        parse,
        "Name[1];",
        &expect![[r#"
            Stmt [0-8]:
                annotations: <empty>
                kind: ExprStmt [0-8]:
                    expr: Expr [0-7]: IndexExpr [0-7]:
                        collection: Expr [0-4]: Ident [0-4] "Name"
                        index: IndexList [5-6]:
                            values:
                                Expr [5-6]: Lit: Int(1)"#]],
    );
}

#[test]
fn index_expr_with_multiple_index_operators_errors() {
    check(
        parse,
        "Name[1][2];",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ExprStmt [0-11]:
                    expr: Expr [0-10]: IndexExpr [0-10]:
                        collection: Expr [0-4]: Ident [0-4] "Name"
                        index: IndexList [5-6]:
                            values:
                                Expr [5-6]: Lit: Int(1)

            [
                Error(
                    MultipleIndexOperators(
                        Span {
                            lo: 0,
                            hi: 10,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn cast_expr() {
    check(
        parse,
        "bit(0);",
        &expect![[r#"
            Stmt [0-7]:
                annotations: <empty>
                kind: ExprStmt [0-7]:
                    expr: Expr [0-6]: Cast [0-6]:
                        type: ScalarType [0-3]: BitType [0-3]:
                            size: <none>
                        arg: Expr [4-5]: Lit: Int(0)"#]],
    );
}

#[test]
fn cast_expr_with_designator() {
    check(
        parse,
        "bit[45](0);",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: ExprStmt [0-11]:
                    expr: Expr [0-10]: Cast [0-10]:
                        type: ScalarType [0-7]: BitType [0-7]:
                            size: Expr [4-6]: Lit: Int(45)
                        arg: Expr [8-9]: Lit: Int(0)"#]],
    );
}
