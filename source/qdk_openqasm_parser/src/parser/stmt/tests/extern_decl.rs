// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::parser::tests::check;

use crate::parser::stmt::parse;

#[test]
fn missing_semicolon_err() {
    check(
        parse,
        "extern x()",
        &expect![[r#"
            Stmt [0-10]:
                annotations: <empty>
                kind: ExternDecl [0-10]:
                    ident: Ident [7-8] "x"
                    parameters: <empty>
                    return_type: <none>

            [
                Error(
                    Token(
                        Semicolon,
                        Eof,
                        Span {
                            lo: 10,
                            hi: 10,
                        },
                    ),
                ),
            ]"#]],
    );
}

#[test]
fn bit_param_bit_ret_decl() {
    check(
        parse,
        "extern x(bit) -> bit;",
        &expect![[r#"
            Stmt [0-21]:
                annotations: <empty>
                kind: ExternDecl [0-21]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-12]: ScalarType [9-12]: BitType [9-12]:
                            size: <none>
                    return_type: ScalarType [17-20]: BitType [17-20]:
                        size: <none>"#]],
    );
}

#[test]
fn sized_bit_param_bit_ret_decl() {
    check(
        parse,
        "extern x(bit[n]) -> bit;",
        &expect![[r#"
            Stmt [0-24]:
                annotations: <empty>
                kind: ExternDecl [0-24]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-15]: ScalarType [9-15]: BitType [9-15]:
                            size: Expr [13-14]: Ident [13-14] "n"
                    return_type: ScalarType [20-23]: BitType [20-23]:
                        size: <none>"#]],
    );
}

#[test]
fn sized_creg_param_bit_ret_decl() {
    check(
        parse,
        "extern x(creg[n]) -> bit;",
        &expect![[r#"
            Stmt [0-25]:
                annotations: <empty>
                kind: ExternDecl [0-25]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-16]: ScalarType [9-16]: BitType [9-16]:
                            size: Expr [14-15]: Ident [14-15] "n"
                    return_type: ScalarType [21-24]: BitType [21-24]:
                        size: <none>"#]],
    );
}

#[test]
fn creg_param_bit_ret_decl() {
    check(
        parse,
        "extern x(creg) -> bit;",
        &expect![[r#"
            Stmt [0-22]:
                annotations: <empty>
                kind: ExternDecl [0-22]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-13]: ScalarType [9-13]: BitType [9-13]:
                            size: <none>
                    return_type: ScalarType [18-21]: BitType [18-21]:
                        size: <none>"#]],
    );
}

#[test]
fn readonly_array_arg_with_int_dims() {
    check(
        parse,
        "extern x(readonly array[int[8], 2, 10]);",
        &expect![[r#"
            Stmt [0-40]:
                annotations: <empty>
                kind: ExternDecl [0-40]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-38]: StaticArrayReferenceType [9-38]:
                            mutability: ReadOnly
                            base_type: ArrayBaseTypeKind IntType [24-30]:
                                size: Expr [28-29]: Lit: Int(8)
                            dimensions:
                                Expr [32-33]: Lit: Int(2)
                                Expr [35-37]: Lit: Int(10)
                    return_type: <none>"#]],
    );
}

#[test]
fn readonly_array_arg_with_dim() {
    check(
        parse,
        "extern x(readonly array[int[8], #dim = 1]);",
        &expect![[r#"
            Stmt [0-43]:
                annotations: <empty>
                kind: ExternDecl [0-43]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-41]: DynArrayReferenceType [9-41]:
                            mutability: ReadOnly
                            base_type: ArrayBaseTypeKind IntType [24-30]:
                                size: Expr [28-29]: Lit: Int(8)
                            dimensions: Expr [39-40]: Lit: Int(1)
                    return_type: <none>"#]],
    );
}

#[test]
fn mutable_array_arg() {
    check(
        parse,
        "extern x(mutable array[int[8], #dim = 1]);",
        &expect![[r#"
            Stmt [0-42]:
                annotations: <empty>
                kind: ExternDecl [0-42]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-40]: DynArrayReferenceType [9-40]:
                            mutability: Mutable
                            base_type: ArrayBaseTypeKind IntType [23-29]:
                                size: Expr [27-28]: Lit: Int(8)
                            dimensions: Expr [38-39]: Lit: Int(1)
                    return_type: <none>"#]],
    );
}

#[test]
fn unexpected_ident_in_params() {
    check(
        parse,
        "extern x(creg c) -> bit;",
        &expect![[r#"
            Error(
                Token(
                    Close(
                        Paren,
                    ),
                    Identifier,
                    Span {
                        lo: 14,
                        hi: 15,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn annotation() {
    check(
        parse,
        r#"@test.annotation
        extern x(creg) -> bit;"#,
        &expect![[r#"
            Stmt [0-47]:
                annotations:
                    Annotation [0-16]:
                        identifier: test.annotation
                        value: <none>
                        value_span: <none>
                kind: ExternDecl [25-47]:
                    ident: Ident [32-33] "x"
                    parameters:
                        [34-38]: ScalarType [34-38]: BitType [34-38]:
                            size: <none>
                    return_type: ScalarType [43-46]: BitType [43-46]:
                        size: <none>"#]],
    );
}

#[test]
fn missing_ty_error() {
    check(
        parse,
        "extern x() -> ;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar type",
                    Semicolon,
                    Span {
                        lo: 14,
                        hi: 15,
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
        "extern x(,);",
        &expect![[r#"
            Stmt [0-12]:
                annotations: <empty>
                kind: ExternDecl [0-12]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-9]: ScalarType [0-0]: Err
                    return_type: <none>

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 9,
                            hi: 9,
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
        "extern x(int,,int);",
        &expect![[r#"
            Stmt [0-19]:
                annotations: <empty>
                kind: ExternDecl [0-19]:
                    ident: Ident [7-8] "x"
                    parameters:
                        [9-12]: ScalarType [9-12]: IntType [9-12]:
                            size: <none>
                        [13-13]: ScalarType [0-0]: Err
                        [14-17]: ScalarType [14-17]: IntType [14-17]:
                            size: <none>
                    return_type: <none>

            [
                Error(
                    MissingSeqEntry(
                        Span {
                            lo: 13,
                            hi: 13,
                        },
                    ),
                ),
            ]"#]],
    );
}
