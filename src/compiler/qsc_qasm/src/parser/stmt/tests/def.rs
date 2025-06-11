// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::parser::tests::check;

use crate::parser::stmt::parse;

#[test]
fn minimal() {
    check(
        parse,
        "def x() { }",
        &expect![[r#"
            Stmt [0-11]:
                annotations: <empty>
                kind: DefStmt [0-11]:
                    ident: Ident [4-5] "x"
                    parameters: <empty>
                    return_type: <none>
                    body: Block [8-11]: <empty>"#]],
    );
}

#[test]
fn missing_ty_error() {
    check(
        parse,
        "def x() -> { }",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
                    Open(
                        Brace,
                    ),
                    Span {
                        lo: 11,
                        hi: 12,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn missing_args_with_delim_error() {
    check(
        parse,
        "def x(,) { }",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: DefStmt [0-12]:
                    ident: Ident [4-5] "x"
                    parameters:
                        ScalarTypedParameter [6-6]:
                            type: ScalarType [0-0]: Err
                            ident: Ident [0-0] ""
                    return_type: <none>
                    body: Block [9-12]: <empty>

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 6,
                            hi: 6,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn args_with_extra_delim_err_ty() {
    check(
        parse,
        "def x(int a,,int b) { }",
        &expect![[r#"
            Stmt [0-23]:
                annotations: <empty>
                kind: DefStmt [0-23]:
                    ident: Ident [4-5] "x"
                    parameters:
                        ScalarTypedParameter [6-11]:
                            type: ScalarType [6-9]: IntType [6-9]:
                                size: <none>
                            ident: Ident [10-11] "a"
                        ScalarTypedParameter [12-12]:
                            type: ScalarType [0-0]: Err
                            ident: Ident [0-0] ""
                        ScalarTypedParameter [13-18]:
                            type: ScalarType [13-16]: IntType [13-16]:
                                size: <none>
                            ident: Ident [17-18] "b"
                    return_type: <none>
                    body: Block [20-23]: <empty>

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 12,
                            hi: 12,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn classical_subroutine() {
    check(
        parse,
        "def square(int[32] x) -> int { return x ** 2; }",
        &expect![[r#"
            Stmt [0-47]:
                annotations: <empty>
                kind: DefStmt [0-47]:
                    ident: Ident [4-10] "square"
                    parameters:
                        ScalarTypedParameter [11-20]:
                            type: ScalarType [11-18]: IntType [11-18]:
                                size: Expr [15-17]: Lit: Int(32)
                            ident: Ident [19-20] "x"
                    return_type: ScalarType [25-28]: IntType [25-28]:
                        size: <none>
                    body: Block [29-47]:
                        Stmt [31-45]:
                            annotations: <empty>
                            kind: ReturnStmt [31-45]:
                                expr: Expr [38-44]: BinaryOpExpr:
                                    op: Exp
                                    lhs: Expr [38-39]: Ident [38-39] "x"
                                    rhs: Expr [43-44]: Lit: Int(2)"#]],
    );
}

#[test]
fn quantum_args() {
    check(
        parse,
        "def x(qubit q, qubit[n] qubits) { }",
        &expect![[r#"
            Stmt [0-35]:
                annotations: <empty>
                kind: DefStmt [0-35]:
                    ident: Ident [4-5] "x"
                    parameters:
                        QuantumTypedParameter [6-13]:
                            size: <none>
                            ident: Ident [12-13] "q"
                        QuantumTypedParameter [15-30]:
                            size: Expr [21-22]: Ident [21-22] "n"
                            ident: Ident [24-30] "qubits"
                    return_type: <none>
                    body: Block [32-35]: <empty>"#]],
    );
}

#[test]
fn old_style_args() {
    check(
        parse,
        "def test(creg c, qreg q, creg c2[2], qreg q4[4]) -> int { return x ** 2; }",
        &expect![[r#"
            Stmt [0-74]:
                annotations: <empty>
                kind: DefStmt [0-74]:
                    ident: Ident [4-8] "test"
                    parameters:
                        ScalarTypedParameter [9-15]:
                            type: ScalarType [9-15]: BitType [9-15]:
                                size: <none>
                            ident: Ident [14-15] "c"
                        QuantumTypedParameter [17-23]:
                            size: <none>
                            ident: Ident [22-23] "q"
                        ScalarTypedParameter [25-35]:
                            type: ScalarType [25-35]: BitType [25-35]:
                                size: Expr [33-34]: Lit: Int(2)
                            ident: Ident [30-32] "c2"
                        QuantumTypedParameter [37-47]:
                            size: Expr [45-46]: Lit: Int(4)
                            ident: Ident [42-44] "q4"
                    return_type: ScalarType [52-55]: IntType [52-55]:
                        size: <none>
                    body: Block [56-74]:
                        Stmt [58-72]:
                            annotations: <empty>
                            kind: ReturnStmt [58-72]:
                                expr: Expr [65-71]: BinaryOpExpr:
                                    op: Exp
                                    lhs: Expr [65-66]: Ident [65-66] "x"
                                    rhs: Expr [70-71]: Lit: Int(2)"#]],
    );
}

#[test]
fn readonly_array_arg_with_int_dims() {
    check(
        parse,
        "def specified_sub(readonly array[int[8], 2, 10] arr_arg) {}",
        &expect![[r#"
            Stmt [0-59]:
                annotations: <empty>
                kind: DefStmt [0-59]:
                    ident: Ident [4-17] "specified_sub"
                    parameters:
                        ArrayTypedParameter [18-55]:
                            type: ArrayReferenceType [18-47]:
                                mutability: ReadOnly
                                base_type: ArrayBaseTypeKind IntType [33-39]:
                                    size: Expr [37-38]: Lit: Int(8)
                                dimensions:
                                    Expr [41-42]: Lit: Int(2)
                                    Expr [44-46]: Lit: Int(10)

                            ident: Ident [48-55] "arr_arg"
                    return_type: <none>
                    body: Block [57-59]: <empty>"#]],
    );
}

#[test]
fn readonly_array_arg_with_dim() {
    check(
        parse,
        "def arr_subroutine(readonly array[int[8], #dim = 1] arr_arg) {}",
        &expect![[r#"
            Stmt [0-63]:
                annotations: <empty>
                kind: DefStmt [0-63]:
                    ident: Ident [4-18] "arr_subroutine"
                    parameters:
                        ArrayTypedParameter [19-59]:
                            type: ArrayReferenceType [19-51]:
                                mutability: ReadOnly
                                base_type: ArrayBaseTypeKind IntType [34-40]:
                                    size: Expr [38-39]: Lit: Int(8)
                                dimensions:
                                    Expr [49-50]: Lit: Int(1)

                            ident: Ident [52-59] "arr_arg"
                    return_type: <none>
                    body: Block [61-63]: <empty>"#]],
    );
}

#[test]
fn mutable_array_arg() {
    check(
        parse,
        "def mut_subroutine(mutable array[int[8], #dim = 1] arr_arg) {}",
        &expect![[r#"
            Stmt [0-62]:
                annotations: <empty>
                kind: DefStmt [0-62]:
                    ident: Ident [4-18] "mut_subroutine"
                    parameters:
                        ArrayTypedParameter [19-58]:
                            type: ArrayReferenceType [19-50]:
                                mutability: Mutable
                                base_type: ArrayBaseTypeKind IntType [33-39]:
                                    size: Expr [37-38]: Lit: Int(8)
                                dimensions:
                                    Expr [48-49]: Lit: Int(1)

                            ident: Ident [51-58] "arr_arg"
                    return_type: <none>
                    body: Block [60-62]: <empty>"#]],
    );
}
