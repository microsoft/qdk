// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::get_signature_help;
use crate::{Encoding, test_utils::compile_with_markers};
use expect_test::{Expect, expect};
use indoc::indoc;

/// Asserts that the signature help given at the cursor position matches the expected signature help.
/// The cursor position is indicated by a `↘` marker in the source text.
fn check(source_with_markers: &str, expect: &Expect) {
    let (compilation, cursor_position, _) = compile_with_markers(source_with_markers, true);
    let actual = get_signature_help(&compilation, "<source>", cursor_position, Encoding::Utf8)
        .expect("Expected a signature help.");
    expect.assert_debug_eq(&actual);
}

#[test]
fn first_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar() : Unit {
                Foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double, z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    35,
                                    45,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn mid_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar() : Unit {
                Foo(12↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double, z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    35,
                                    45,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn second_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1,↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double, z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    35,
                                    45,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn last_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1, 1.2,↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double, z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    35,
                                    45,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 3,
            }
        "#]],
    );
}

#[test]
fn insert_second_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1,↘, "Four")
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double, z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    35,
                                    45,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn revisit_second_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1, 2.↘3, "Four")
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double, z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    35,
                                    45,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_call_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar(a : Int, b : Double) : Double { b }
            operation Baz() : Unit {
                Foo(1, Bar(↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Bar(a : Int, b : Double) : Double",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn nested_call_second_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double, z : String) : Unit {}
            operation Bar(a : Int, b : Double) : Double { b }
            operation Baz() : Unit {
                Foo(1, Bar(2,↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Bar(a : Int, b : Double) : Double",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn tuple_argument() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : (Int, Double), z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1, ↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : (Int, Double), z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    53,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    40,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    42,
                                    52,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn tuple_argument_first_item() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : (Int, Double), z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1, (↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : (Int, Double), z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    53,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    40,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    42,
                                    52,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn tuple_argument_last_item() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : (Int, Double), z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2,↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : (Int, Double), z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    53,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    40,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    42,
                                    52,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn tuple_argument_after_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : (Int, Double), z : String) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2, 3.0),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : (Int, Double), z : String) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    53,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    40,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    42,
                                    52,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 3,
            }
        "#]],
    );
}

#[test]
fn arguments_in_nested_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, ↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn first_inner_argument_in_nested_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 3,
            }
        "#]],
    );
}

#[test]
fn second_inner_argument_in_nested_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.3,↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 4,
            }
        "#]],
    );
}

#[test]
fn argument_after_nested_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.3, "Four"),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 5,
            }
        "#]],
    );
}

#[test]
fn argument_end_of_nested_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.3, "Four")↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn argument_nested_tuple_after_last() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.3, "Four" ↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_mismatch_after() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, 2,↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 5,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_mismatch_mid() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, 12↘3)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_mismatch_before() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1↘2, 123)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_not_enough_end() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(v : Int, (w : Double, x : String, y : Int), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.0, "Three")↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(v : Int, (w : Double, x : String, y : Int), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    67,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    56,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    48,
                                    55,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    58,
                                    66,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_not_enough_after() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(v : Int, (w : Double, x : String, y : Int), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.0, "Three"),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(v : Int, (w : Double, x : String, y : Int), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    67,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    56,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    48,
                                    55,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    58,
                                    66,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 6,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_not_enough_single_end() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.0)↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_not_enough_single_after() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2.0),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 5,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_not_enough_empty_end() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, ()↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_tuple_not_enough_empty_after() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(w : Int, (x : Double, y : String), z : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    58,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    47,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    36,
                                    46,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    49,
                                    57,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 5,
            }
        "#]],
    );
}

#[test]
fn nested_empty_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, (), y : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1,↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, (), y : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    36,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    25,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    27,
                                    35,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_empty_tuple_mid() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, (), y : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, (), y : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    36,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    25,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    27,
                                    35,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_empty_tuple_end() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, (), y : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, ()↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, (), y : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    36,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    25,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    27,
                                    35,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn nested_empty_tuple_after() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, (), y : Bool) : Unit {}
            operation Bar() : Unit {
                Foo(1, (),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, (), y : Bool) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    36,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    25,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    27,
                                    35,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 3,
            }
        "#]],
    );
}

#[test]
fn multi_nested_tuple() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(a : Int, (b : Int, (c : Int, d : Int), e : Int), f : Int) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2, (3, 4), 5),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(a : Int, (b : Int, (c : Int, d : Int), e : Int), f : Int) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    71,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    61,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    31,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    33,
                                    51,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    34,
                                    41,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    43,
                                    50,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    53,
                                    60,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    63,
                                    70,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 8,
            }
        "#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn documentation_test() {
    check(
        indoc! {r#"
        namespace Test {
            /// # Summary
            /// This is the operation `Foo`.
            /// # Input
            /// ## a
            /// This is the parameter `a`.
            /// ## b
            /// This is the parameter `b`.
            /// ## c
            /// This is the parameter `c`.
            /// ## d
            /// This is the parameter `d`.
            /// ## e
            /// This is the parameter `e`.
            /// ## f
            /// This is the parameter `f`.
            operation Foo(a : Int, (b : Int, (c : Int, d : Int), e : Int), f : Int) : Unit {}
            operation Bar() : Unit {
                Foo(1, (2, (3, 4), 5),↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(a : Int, (b : Int, (c : Int, d : Int), e : Int), f : Int) : Unit",
                        documentation: Some(
                            "This is the operation `Foo`.",
                        ),
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    71,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: Some(
                                    "This is the parameter `a`.",
                                ),
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    61,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    24,
                                    31,
                                ),
                                documentation: Some(
                                    "This is the parameter `b`.",
                                ),
                            },
                            ParameterInformation {
                                label: (
                                    33,
                                    51,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    34,
                                    41,
                                ),
                                documentation: Some(
                                    "This is the parameter `c`.",
                                ),
                            },
                            ParameterInformation {
                                label: (
                                    43,
                                    50,
                                ),
                                documentation: Some(
                                    "This is the parameter `d`.",
                                ),
                            },
                            ParameterInformation {
                                label: (
                                    53,
                                    60,
                                ),
                                documentation: Some(
                                    "This is the parameter `e`.",
                                ),
                            },
                            ParameterInformation {
                                label: (
                                    63,
                                    70,
                                ),
                                documentation: Some(
                                    "This is the parameter `f`.",
                                ),
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 8,
            }
        "#]],
    );
}

#[test]
fn single_parameter_end() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(a : Int) : Unit {}
            operation Bar() : Unit {
                Foo(1↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(a : Int) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    22,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn single_parameter_after() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(a : Int) : Unit {}
            operation Bar() : Unit {
                Foo(1,↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(a : Int) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    22,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 0,
            }
        "#]],
    );
}

#[test]
fn single_parameter_before() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(a : Int) : Unit {}
            operation Bar() : Unit {
                Foo(↘ 1)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(a : Int) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    22,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_local_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit {}
            operation Bar() : Unit {
                let foo = Foo;
                foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Int, Int, Int) => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    16,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    5,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    7,
                                    10,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    12,
                                    15,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_array_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit {}
            operation Bar() : Unit {
                let foo = [Foo];
                foo[0](↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Int, Int, Int) => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    16,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    5,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    7,
                                    10,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    12,
                                    15,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_block_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit {}
            operation Bar() : Unit {
                ({ Foo }(↘))
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Int, Int, Int) => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    16,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    5,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    7,
                                    10,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    12,
                                    15,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_unresolved_lambda_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Bar() : Unit {
                let foo = (x, y, z) => {};
                foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((?, ?, ?) => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    10,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    3,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    5,
                                    6,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    8,
                                    9,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_partially_resolved_lambda_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Bar() : Unit {
                let foo = (x, y, z) => {};
                foo(1, ↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Int, ?, ?) => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    12,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    5,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    7,
                                    8,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    10,
                                    11,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn indirect_resolved_lambda_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Bar() : Unit {
                let foo = (x, y, z) => {};
                foo(1, 2, 3);
                foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Int, Int, Int) => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    16,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    5,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    7,
                                    10,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    12,
                                    15,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn controlled_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit is Ctl {}
            operation Bar() : Unit {
                Controlled Foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Qubit[], (Int, Int, Int)) => Unit is Ctl)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    27,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    9,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    11,
                                    26,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    12,
                                    15,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    17,
                                    20,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    22,
                                    25,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn double_controlled_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit is Ctl {}
            operation Bar() : Unit {
                Controlled Controlled Foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Qubit[], (Qubit[], (Int, Int, Int))) => Unit is Ctl)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    38,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    9,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    11,
                                    37,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    12,
                                    19,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    21,
                                    36,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    22,
                                    25,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    27,
                                    30,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    32,
                                    35,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn partial_application_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Int, z : Int) : Unit {}
            operation Bar() : Unit {
                let foo = Foo(1, _, _);
                foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Int, Int) => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    11,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    5,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    7,
                                    10,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_no_params_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo() : Unit {
                let foo = Foo;
                foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "(Unit => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    5,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 0,
            }
        "#]],
    );
}

#[test]
fn indirect_single_param_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int) : Unit {
                let foo = Foo;
                foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "(Int => Unit)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn udt_constructor_call() {
    check(
        indoc! {r#"
        namespace Test {
            newtype Foo = (fst : Int, snd : Double);
            operation Bar(x : Int) : Unit {
                let foo = Foo(↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "((Int, Double) -> Foo)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    14,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    2,
                                    5,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    7,
                                    13,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn std_callable_with_udt() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Udt {
            TakesUdt(↘)
        }
    }
    "#,
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "function TakesUdt(input : Udt) : Udt",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    17,
                                    30,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    18,
                                    29,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_callable_with_std_udt_args() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Udt {
            let callee = TakesUdt;
            callee(↘)
        }
    }
    "#,
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "(Udt -> Udt)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_callable_with_std_udt() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Unit {
            let fn = UdtFn((x) -> x);
            fn!(↘)
        }
    }
    "#,
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "(Int -> Int)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_callable_with_std_udt_with_params() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Unit {
            let fn = UdtFnWithUdtParams(TakesUdt);
            fn!(↘)
        }
    }
    "#,
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "(Udt -> Udt)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_callable_with_std_struct() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Unit {
            let fn = new StructFn { inner = (x) -> x };
            fn::inner(↘)
        }
    }
    "#,
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "(Int -> Int)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    1,
                                    4,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn indirect_callable_with_std_struct_with_params() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Unit {
            let fn = new StructFnWithStructParams { inner = TakesStruct };
            fn::inner(↘)
        }
    }
    "#,
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "(FakeStruct -> FakeStruct)",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    1,
                                    11,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    1,
                                    11,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn call_with_type_param() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo<'A, 'B>(a : 'A, b : 'B) : 'B { b }
            operation Bar() : Unit {
                Foo(1,↘)
                let x = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo<'A, 'B>(a : 'A, b : 'B) : 'B",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    21,
                                    37,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    22,
                                    28,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    30,
                                    36,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn std_callable_with_type_params() {
    check(
        r#"
    namespace Test {
        open FakeStdLib;
        operation Foo() : Unit {
            let temp = FakeWithTypeParam(↘);
        }
    }
    "#,
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation FakeWithTypeParam<'A>(a : 'A) : 'A",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    31,
                                    39,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    32,
                                    38,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn missing_close_paren_empty_args() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double) : Unit {}
            operation Bar() : Unit {
                Foo(↘
                let z = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 1,
            }
        "#]],
    );
}

#[test]
fn missing_close_paren_second_arg() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int, y : Double) : Unit {}
            operation Bar() : Unit {
                Foo(1, ↘
                let z = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Foo(x : Int, y : Double) : Unit",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    34,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    33,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}

#[test]
fn missing_close_paren_nested_call() {
    check(
        indoc! {r#"
        namespace Test {
            operation Foo(x : Int) : Unit {}
            operation Baz(a : Int, b : Int) : Int { a + b }
            operation Bar() : Unit {
                Foo(Baz(1, ↘
                let z = 3;
            }
        }
    "#},
        &expect![[r#"
            SignatureHelp {
                signatures: [
                    SignatureInformation {
                        label: "operation Baz(a : Int, b : Int) : Int",
                        documentation: None,
                        parameters: [
                            ParameterInformation {
                                label: (
                                    13,
                                    31,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    14,
                                    21,
                                ),
                                documentation: None,
                            },
                            ParameterInformation {
                                label: (
                                    23,
                                    30,
                                ),
                                documentation: None,
                            },
                        ],
                    },
                ],
                active_signature: 0,
                active_parameter: 2,
            }
        "#]],
    );
}
