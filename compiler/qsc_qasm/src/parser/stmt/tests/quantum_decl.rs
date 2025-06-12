// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::parser::tests::check;

use crate::parser::stmt::parse;

#[test]
fn quantum_decl() {
    check(
        parse,
        "qubit q;",
        &expect![[r#"
            Stmt [0-8]:
                annotations: <empty>
                kind: QubitDeclaration [0-8]:
                    ty_span: [0-5]
                    ident: Ident [6-7] "q"
                    size: <none>"#]],
    );
}

#[test]
fn annotated_quantum_decl() {
    check(
        parse,
        r#"
        @a.b.c 123
        qubit q;"#,
        &expect![[r#"
            Stmt [9-36]:
                annotations:
                    Annotation [9-19]:
                        identifier: "a.b.c"
                        value: "123"
                kind: QubitDeclaration [28-36]:
                    ty_span: [28-33]
                    ident: Ident [34-35] "q"
                    size: <none>"#]],
    );
}

#[test]
fn multi_annotated_quantum_decl() {
    check(
        parse,
        r#"
        @g.h dolor sit amet, consectetur adipiscing elit
        @d.e.f
        @a.b.c 123
        qubit q;"#,
        &expect![[r#"
            Stmt [9-108]:
                annotations:
                    Annotation [9-57]:
                        identifier: "g.h"
                        value: "dolor sit amet, consectetur adipiscing elit"
                    Annotation [66-72]:
                        identifier: "d.e.f"
                        value: <none>
                    Annotation [81-91]:
                        identifier: "a.b.c"
                        value: "123"
                kind: QubitDeclaration [100-108]:
                    ty_span: [100-105]
                    ident: Ident [106-107] "q"
                    size: <none>"#]],
    );
}

#[test]
fn quantum_decl_missing_name() {
    check(
        parse,
        "qubit;",
        &expect![[r#"
            Error(
                Rule(
                    "identifier",
                    Semicolon,
                    Span {
                        lo: 5,
                        hi: 6,
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn quantum_decl_with_designator() {
    check(
        parse,
        "qubit[5] qubits;",
        &expect![[r#"
            Stmt [0-16]:
                annotations: <empty>
                kind: QubitDeclaration [0-16]:
                    ty_span: [0-8]
                    ident: Ident [9-15] "qubits"
                    size: Expr [6-7]: Lit: Int(5)"#]],
    );
}

#[test]
fn quantum_decl_with_designator_missing_name() {
    check(
        parse,
        "qubit[5]",
        &expect![[r#"
            Error(
                Rule(
                    "identifier",
                    Eof,
                    Span {
                        lo: 8,
                        hi: 8,
                    },
                ),
            )
        "#]],
    );
}
