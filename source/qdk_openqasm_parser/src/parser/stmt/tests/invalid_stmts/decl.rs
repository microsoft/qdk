// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::{stmt::parse, tests::check};
use expect_test::expect;

#[test]
fn missing_ident() {
    check(
        parse,
        "float;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Semicolon,
                Span {
                    lo: 5,
                    hi: 6,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "uint[8];",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Semicolon,
                Span {
                    lo: 7,
                    hi: 8,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "qreg[4];",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "creg[4];",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[float[32]];",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Semicolon,
                Span {
                    lo: 18,
                    hi: 19,
                },
            ),
        )
    "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn incorrect_designators() {
    check(
        parse,
        "int[8, 8] myvar;",
        &expect![[r#"
        Stmt [0-16]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-16]:
                type: ScalarType [0-9]: IntType [0-9]:
                    size: Expr [4-5]: Lit: Int(8)
                ident: Ident [10-15] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Close(
                        Bracket,
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
    check(
        parse,
        "uint[8, 8] myvar;",
        &expect![[r#"
        Stmt [0-17]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-17]:
                type: ScalarType [0-10]: UIntType [0-10]:
                    size: Expr [5-6]: Lit: Int(8)
                ident: Ident [11-16] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Close(
                        Bracket,
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
    check(
        parse,
        "float[8, 8] myvar;",
        &expect![[r#"
        Stmt [0-18]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-18]:
                type: ScalarType [0-11]: FloatType [0-11]:
                    size: Expr [6-7]: Lit: Int(8)
                ident: Ident [12-17] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Close(
                        Bracket,
                    ),
                    Comma,
                    Span {
                        lo: 7,
                        hi: 8,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "angle[8, 8] myvar;",
        &expect![[r#"
        Stmt [0-18]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-18]:
                type: ScalarType [0-11]: AngleType [0-11]:
                    size: Expr [6-7]: Lit: Int(8)
                ident: Ident [12-17] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Close(
                        Bracket,
                    ),
                    Comma,
                    Span {
                        lo: 7,
                        hi: 8,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "bool[4] myvar;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "bool[4, 4] myvar;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "bit[4, 4] myvar;",
        &expect![[r#"
        Stmt [0-16]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-16]:
                type: ScalarType [0-9]: BitType [0-9]:
                    size: Expr [4-5]: Lit: Int(4)
                ident: Ident [10-15] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Close(
                        Bracket,
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
    check(
        parse,
        "creg[2] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "creg[2, 2] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "qreg[2] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "qreg[2, 2] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "identifier",
                Open(
                    Bracket,
                ),
                Span {
                    lo: 4,
                    hi: 5,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[32] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Literal(
                    Integer(
                        Decimal,
                    ),
                ),
                Span {
                    lo: 8,
                    hi: 10,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[mytype] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Identifier,
                Span {
                    lo: 8,
                    hi: 14,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[float[32], float[32]] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Comma,
                Span {
                    lo: 17,
                    hi: 18,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[qreg] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    QReg,
                ),
                Span {
                    lo: 8,
                    hi: 12,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[creg] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    CReg,
                ),
                Span {
                    lo: 8,
                    hi: 12,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[qreg[8]] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    QReg,
                ),
                Span {
                    lo: 8,
                    hi: 12,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "complex[creg[8]] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Keyword(
                    CReg,
                ),
                Span {
                    lo: 8,
                    hi: 12,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn bad_array_specifiers() {
    check(
        parse,
        "array myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Identifier,
                Span {
                    lo: 6,
                    hi: 11,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "array[8] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Literal(
                    Integer(
                        Decimal,
                    ),
                ),
                Span {
                    lo: 6,
                    hi: 7,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "array[not_a_type, 4] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Identifier,
                Span {
                    lo: 6,
                    hi: 16,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "array[int[8], int[8], 2] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Comma,
                Span {
                    lo: 20,
                    hi: 21,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn invalid_identifiers() {
    check(
        parse,
        "int[8] int;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Type(
                    Int,
                ),
                Span {
                    lo: 7,
                    hi: 10,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "int[8] def;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Keyword(
                    Def,
                ),
                Span {
                    lo: 7,
                    hi: 10,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "int[8] 0;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Literal(
                    Integer(
                        Decimal,
                    ),
                ),
                Span {
                    lo: 7,
                    hi: 8,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "int[8] input;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Paren,
                ),
                Keyword(
                    Input,
                ),
                Span {
                    lo: 7,
                    hi: 12,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn bad_assignments() {
    check(
        parse,
        "int[8] myvar = end;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Brace,
                ),
                Keyword(
                    End,
                ),
                Span {
                    lo: 15,
                    hi: 18,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "int[8] myvar =;",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Brace,
                ),
                Semicolon,
                Span {
                    lo: 14,
                    hi: 15,
                },
            ),
        )
    "#]],
    );
    check(
        parse,
        "float[32] myvar_f = int[32] myvar_i = 2;",
        &expect![[r#"
            Error(
                Token(
                    Open(
                        Paren,
                    ),
                    Identifier,
                    Span {
                        lo: 28,
                        hi: 35,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn array_initialiser_uses_braces() {
    check(
        parse,
        "array[uint[8], 4] myvar = [4, 5, 6, 7];",
        &expect![[r#"
        Error(
            Token(
                Open(
                    Brace,
                ),
                Open(
                    Bracket,
                ),
                Span {
                    lo: 26,
                    hi: 27,
                },
            ),
        )
    "#]],
    );
}

#[test]
fn cant_use_arithmetic_on_the_entire_initialiser() {
    check(
        parse,
        "array[uint[8], 4] myvar = 2 * {1, 2, 3, 4};",
        &expect![[r#"
            Error(
                Rule(
                    "expression",
                    Open(
                        Brace,
                    ),
                    Span {
                        lo: 30,
                        hi: 31,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn backed_arrays_cant_use_dim() {
    check(
        parse,
        "array[uint[8], #dim=2] myvar;",
        &expect![[r#"
            Error(
                Rule(
                    "scalar or array type",
                    Keyword(
                        Dim,
                    ),
                    Span {
                        lo: 15,
                        hi: 19,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn cant_have_more_than_one_type_specification() {
    check(
        parse,
        "array[int[8], int[8]] myvar;",
        &expect![[r#"
        Error(
            Rule(
                "scalar or array type",
                Close(
                    Bracket,
                ),
                Span {
                    lo: 20,
                    hi: 21,
                },
            ),
        )
    "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn incorrect_orders() {
    check(
        parse,
        "myvar: int[8];",
        &expect![[r#"
        Stmt [0-5]:
            annotations: <empty>
            kind: ExprStmt [0-5]:
                expr: Expr [0-5]: Ident [0-5] "myvar"

        [
            Error(
                Token(
                    Semicolon,
                    Colon,
                    Span {
                        lo: 5,
                        hi: 6,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "myvar int[8];",
        &expect![[r#"
        Stmt [0-5]:
            annotations: <empty>
            kind: ExprStmt [0-5]:
                expr: Expr [0-5]: Ident [0-5] "myvar"

        [
            Error(
                Token(
                    Semicolon,
                    Type(
                        Int,
                    ),
                    Span {
                        lo: 6,
                        hi: 9,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "int myvar[8];",
        &expect![[r#"
        Stmt [0-9]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-9]:
                type: ScalarType [0-3]: IntType [0-3]:
                    size: <none>
                ident: Ident [4-9] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Semicolon,
                    Open(
                        Bracket,
                    ),
                    Span {
                        lo: 9,
                        hi: 10,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "uint myvar[8];",
        &expect![[r#"
        Stmt [0-10]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-10]:
                type: ScalarType [0-4]: UIntType [0-4]:
                    size: <none>
                ident: Ident [5-10] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Semicolon,
                    Open(
                        Bracket,
                    ),
                    Span {
                        lo: 10,
                        hi: 11,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "float myvar[32];",
        &expect![[r#"
        Stmt [0-11]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-11]:
                type: ScalarType [0-5]: FloatType [0-5]:
                    size: <none>
                ident: Ident [6-11] "myvar"
                init_expr: <none>

        [
            Error(
                Token(
                    Semicolon,
                    Open(
                        Bracket,
                    ),
                    Span {
                        lo: 11,
                        hi: 12,
                    },
                ),
            ),
        ]"#]],
    );
}

#[test]
fn compound_assigments() {
    check(
        parse,
        "int[8] myvar1, myvar2;",
        &expect![[r#"
        Stmt [0-13]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-13]:
                type: ScalarType [0-6]: IntType [0-6]:
                    size: Expr [4-5]: Lit: Int(8)
                ident: Ident [7-13] "myvar1"
                init_expr: <none>

        [
            Error(
                Token(
                    Semicolon,
                    Comma,
                    Span {
                        lo: 13,
                        hi: 14,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "int[8] myvari, float[32] myvarf;",
        &expect![[r#"
        Stmt [0-13]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-13]:
                type: ScalarType [0-6]: IntType [0-6]:
                    size: Expr [4-5]: Lit: Int(8)
                ident: Ident [7-13] "myvari"
                init_expr: <none>

        [
            Error(
                Token(
                    Semicolon,
                    Comma,
                    Span {
                        lo: 13,
                        hi: 14,
                    },
                ),
            ),
        ]"#]],
    );
    check(
        parse,
        "int[8] myvari float[32] myvarf;",
        &expect![[r#"
        Stmt [0-13]:
            annotations: <empty>
            kind: ClassicalDeclarationStmt [0-13]:
                type: ScalarType [0-6]: IntType [0-6]:
                    size: Expr [4-5]: Lit: Int(8)
                ident: Ident [7-13] "myvari"
                init_expr: <none>

        [
            Error(
                Token(
                    Semicolon,
                    Type(
                        Float,
                    ),
                    Span {
                        lo: 14,
                        hi: 19,
                    },
                ),
            ),
        ]"#]],
    );
}
