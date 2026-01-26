// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::expect;
use qsc_data_structures::language_features::LanguageFeatures;

#[test]
fn explicit_namespace_overrides_implicit() {
    let result = format!(
        "{:#?}",
        crate::namespaces(
            "namespace Explicit {}",
            Some("code/src/Implicit.qs"),
            LanguageFeatures::default()
        )
    );
    expect![[r#"
        (
            [
                Namespace {
                    id: NodeId(
                        4294967295,
                    ),
                    span: Span {
                        lo: 0,
                        hi: 21,
                    },
                    doc: "",
                    name: [
                        Ident {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 10,
                                hi: 18,
                            },
                            name: "Explicit",
                        },
                    ],
                    items: [],
                    kind: Block,
                },
            ],
            [],
        )"#]]
    .assert_eq(&result);
}

#[test]
fn fixup_bad_namespace_name_with_dash() {
    let result = format!(
        "{:#?}",
        crate::namespaces(
            "operation Main() : Unit {}",
            Some("code/src/Foo-Bar.qs"),
            LanguageFeatures::default()
        )
    );
    expect![[r#"
        (
            [
                Namespace {
                    id: NodeId(
                        4294967295,
                    ),
                    span: Span {
                        lo: 0,
                        hi: 26,
                    },
                    doc: "",
                    name: [
                        Ident {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 0,
                                hi: 26,
                            },
                            name: "code",
                        },
                        Ident {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 0,
                                hi: 26,
                            },
                            name: "src",
                        },
                        Ident {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 0,
                                hi: 26,
                            },
                            name: "Foo_Bar",
                        },
                    ],
                    items: [
                        Item {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 0,
                                hi: 26,
                            },
                            doc: "",
                            attrs: [],
                            kind: Callable(
                                CallableDecl {
                                    id: NodeId(
                                        4294967295,
                                    ),
                                    span: Span {
                                        lo: 0,
                                        hi: 26,
                                    },
                                    kind: Operation,
                                    name: Ident {
                                        id: NodeId(
                                            4294967295,
                                        ),
                                        span: Span {
                                            lo: 10,
                                            hi: 14,
                                        },
                                        name: "Main",
                                    },
                                    generics: [],
                                    input: Pat {
                                        id: NodeId(
                                            4294967295,
                                        ),
                                        span: Span {
                                            lo: 14,
                                            hi: 16,
                                        },
                                        kind: Tuple(
                                            [],
                                        ),
                                    },
                                    output: Ty {
                                        id: NodeId(
                                            4294967295,
                                        ),
                                        span: Span {
                                            lo: 19,
                                            hi: 23,
                                        },
                                        kind: Path(
                                            Ok(
                                                Path {
                                                    id: NodeId(
                                                        4294967295,
                                                    ),
                                                    span: Span {
                                                        lo: 19,
                                                        hi: 23,
                                                    },
                                                    segments: None,
                                                    name: Ident {
                                                        id: NodeId(
                                                            4294967295,
                                                        ),
                                                        span: Span {
                                                            lo: 19,
                                                            hi: 23,
                                                        },
                                                        name: "Unit",
                                                    },
                                                },
                                            ),
                                        ),
                                    },
                                    functors: None,
                                    body: Block(
                                        Block {
                                            id: NodeId(
                                                4294967295,
                                            ),
                                            span: Span {
                                                lo: 24,
                                                hi: 26,
                                            },
                                            stmts: [],
                                        },
                                    ),
                                },
                            ),
                        },
                    ],
                    kind: Implicit,
                },
            ],
            [],
        )"#]]
    .assert_eq(&result);
}

#[test]
fn reject_bad_namespace_name_starts_with_number() {
    let result = format!(
        "{:#?}",
        crate::namespaces(
            "operation Main() : Unit {}",
            Some("code/src/123Bar.qs"),
            LanguageFeatures::default()
        )
    );
    expect![[r#"
        (
            [],
            [
                Error(
                    InvalidFileName(
                        Span {
                            lo: 0,
                            hi: 26,
                        },
                        "123Bar",
                    ),
                ),
            ],
        )"#]]
    .assert_eq(&result);
}

#[test]
fn implicit_namespace_with_incomplete_items() {
    let result = format!(
        "{:#?}",
        crate::namespaces(
            "
operation Main() : Unit {}
oper",
            Some("code/src/Foo.qs"),
            LanguageFeatures::default()
        )
    );
    expect![[r#"
        (
            [
                Namespace {
                    id: NodeId(
                        4294967295,
                    ),
                    span: Span {
                        lo: 1,
                        hi: 32,
                    },
                    doc: "",
                    name: [
                        Ident {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 1,
                                hi: 32,
                            },
                            name: "code",
                        },
                        Ident {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 1,
                                hi: 32,
                            },
                            name: "src",
                        },
                        Ident {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 1,
                                hi: 32,
                            },
                            name: "Foo",
                        },
                    ],
                    items: [
                        Item {
                            id: NodeId(
                                4294967295,
                            ),
                            span: Span {
                                lo: 1,
                                hi: 27,
                            },
                            doc: "",
                            attrs: [],
                            kind: Callable(
                                CallableDecl {
                                    id: NodeId(
                                        4294967295,
                                    ),
                                    span: Span {
                                        lo: 1,
                                        hi: 27,
                                    },
                                    kind: Operation,
                                    name: Ident {
                                        id: NodeId(
                                            4294967295,
                                        ),
                                        span: Span {
                                            lo: 11,
                                            hi: 15,
                                        },
                                        name: "Main",
                                    },
                                    generics: [],
                                    input: Pat {
                                        id: NodeId(
                                            4294967295,
                                        ),
                                        span: Span {
                                            lo: 15,
                                            hi: 17,
                                        },
                                        kind: Tuple(
                                            [],
                                        ),
                                    },
                                    output: Ty {
                                        id: NodeId(
                                            4294967295,
                                        ),
                                        span: Span {
                                            lo: 20,
                                            hi: 24,
                                        },
                                        kind: Path(
                                            Ok(
                                                Path {
                                                    id: NodeId(
                                                        4294967295,
                                                    ),
                                                    span: Span {
                                                        lo: 20,
                                                        hi: 24,
                                                    },
                                                    segments: None,
                                                    name: Ident {
                                                        id: NodeId(
                                                            4294967295,
                                                        ),
                                                        span: Span {
                                                            lo: 20,
                                                            hi: 24,
                                                        },
                                                        name: "Unit",
                                                    },
                                                },
                                            ),
                                        ),
                                    },
                                    functors: None,
                                    body: Block(
                                        Block {
                                            id: NodeId(
                                                4294967295,
                                            ),
                                            span: Span {
                                                lo: 25,
                                                hi: 27,
                                            },
                                            stmts: [],
                                        },
                                    ),
                                },
                            ),
                        },
                    ],
                    kind: Implicit,
                },
            ],
            [
                Error(
                    Token(
                        Eof,
                        Ident,
                        Span {
                            lo: 28,
                            hi: 32,
                        },
                    ),
                ),
            ],
        )"#]]
    .assert_eq(&result);
}
