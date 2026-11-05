// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Lint, LintConfig, LintKind, LintLevel, LintOrGroupConfig,
    lint_groups::LintGroup,
    linter::{CodeAction, remove_duplicates, run_lints_without_deduplication},
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures,
    source::SourceMap,
    target::{Profile, TargetCapabilityFlags},
};
use qsc_frontend::compile::{self, PackageStore};
use qsc_hir::hir::CallableKind;
use qsc_passes::PackageType;

#[test]
fn daisy_chain_lint() {
    check(
        &wrap_in_callable("x = y = z", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "x = y = z",
                    level: Warn,
                    message: "discouraged use of chain assignment",
                    help: "assignment expressions always return `Unit`, so chaining them may not be useful",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn long_daisy_chain_lint() {
    check(
        &wrap_in_callable("x = y = z = z = z", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "x = y = z = z = z",
                    level: Warn,
                    message: "discouraged use of chain assignment",
                    help: "assignment expressions always return `Unit`, so chaining them may not be useful",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn nested_daisy_chain_lint() {
    check(
        &wrap_in_callable("x = y = { a = b = c; z } = z = z", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "x = y = { a = b = c; z } = z = z",
                    level: Warn,
                    message: "discouraged use of chain assignment",
                    help: "assignment expressions always return `Unit`, so chaining them may not be useful",
                    code_action: None,
                },
                SrcLint {
                    source: "a = b = c",
                    level: Warn,
                    message: "discouraged use of chain assignment",
                    help: "assignment expressions always return `Unit`, so chaining them may not be useful",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn set_keyword_lint() {
    check(
        &wrap_in_callable("set x = 3;", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "set",
                    level: Allow,
                    message: "deprecated use of `set` keyword",
                    help: "the `set` keyword is deprecated for assignments and can be removed",
                    code_action: Some(
                        CodeAction {
                            title: "Remove `set` keyword",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 71,
                                        hi: 74,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn lint_group() {
    check_with_config(
        &wrap_in_callable("newtype Foo = ()", CallableKind::Operation),
        Some(&[LintOrGroupConfig::Group(crate::GroupConfig {
            lint_group: LintGroup::Deprecations,
            level: LintLevel::Error,
        })]),
        &expect![[r#"
            [
                SrcLint {
                    source: "newtype Foo = ()",
                    level: Error,
                    message: "deprecated `newtype` declarations",
                    help: "`newtype` declarations are deprecated, use `struct` instead",
                    code_action: None,
                },
                SrcLint {
                    source: "RunProgram",
                    level: Allow,
                    message: "operation does not contain any quantum operations",
                    help: "this callable can be declared as a function instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn multiple_lints() {
    check(
        &wrap_in_callable("let x = ((1 + 2)) / 0;;;;", CallableKind::Operation),
        &expect![[r#"
            [
                SrcLint {
                    source: ";;;",
                    level: Warn,
                    message: "redundant semicolons",
                    help: "remove the redundant semicolons",
                    code_action: Some(
                        CodeAction {
                            title: "Remove redundant semicolons",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 94,
                                        hi: 97,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "((1 + 2)) / 0",
                    level: Error,
                    message: "attempt to divide by zero",
                    help: "division by zero will fail at runtime",
                    code_action: None,
                },
                SrcLint {
                    source: "((1 + 2))",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action: Some(
                        CodeAction {
                            title: "Remove unnecessary parentheses",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 80,
                                        hi: 81,
                                    },
                                ),
                                (
                                    "",
                                    Span {
                                        lo: 88,
                                        hi: 89,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "RunProgram",
                    level: Allow,
                    message: "operation does not contain any quantum operations",
                    help: "this callable can be declared as a function instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn double_parens() {
    check(
        &wrap_in_callable("let x = ((1 + 2));", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "((1 + 2))",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action: Some(
                        CodeAction {
                            title: "Remove unnecessary parentheses",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 79,
                                        hi: 80,
                                    },
                                ),
                                (
                                    "",
                                    Span {
                                        lo: 87,
                                        hi: 88,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn division_by_zero() {
    check(
        &wrap_in_callable("let x = 2 / 0;", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "2 / 0",
                    level: Error,
                    message: "attempt to divide by zero",
                    help: "division by zero will fail at runtime",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn double_equality() {
    check(
        &wrap_in_callable("1.0 == 1.01;", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "1.0 == 1.01",
                    level: Warn,
                    message: "strict comparison of doubles",
                    help: "consider comparing them with some margin of error",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn check_double_equality_with_itself_is_allowed_for_nan_check() {
    check(
        &wrap_in_callable(
            r#"
            let a = 1.0;
            let is_nan = not (a == a);
        "#,
            CallableKind::Function,
        ),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn double_inequality() {
    check(
        &wrap_in_callable("1.0 != 1.01;", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "1.0 != 1.01",
                    level: Warn,
                    message: "strict comparison of doubles",
                    help: "consider comparing them with some margin of error",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn needless_parens_in_assignment() {
    check(
        &wrap_in_callable("let x = (42);", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "(42)",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action: Some(
                        CodeAction {
                            title: "Remove unnecessary parentheses",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 79,
                                        hi: 80,
                                    },
                                ),
                                (
                                    "",
                                    Span {
                                        lo: 82,
                                        hi: 83,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn needless_parens() {
    check(
        &wrap_in_callable("let x = (2) + (5 * 4 * (2 ^ 10));", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "(2)",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action: Some(
                        CodeAction {
                            title: "Remove unnecessary parentheses",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 79,
                                        hi: 80,
                                    },
                                ),
                                (
                                    "",
                                    Span {
                                        lo: 81,
                                        hi: 82,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "(5 * 4 * (2 ^ 10))",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action: Some(
                        CodeAction {
                            title: "Remove unnecessary parentheses",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 85,
                                        hi: 86,
                                    },
                                ),
                                (
                                    "",
                                    Span {
                                        lo: 102,
                                        hi: 103,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "(2 ^ 10)",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action: Some(
                        CodeAction {
                            title: "Remove unnecessary parentheses",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 94,
                                        hi: 95,
                                    },
                                ),
                                (
                                    "",
                                    Span {
                                        lo: 101,
                                        hi: 102,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn redundant_semicolons() {
    check(
        &wrap_in_callable("let x = 2;;;;;", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: ";;;;",
                    level: Warn,
                    message: "redundant semicolons",
                    help: "remove the redundant semicolons",
                    code_action: Some(
                        CodeAction {
                            title: "Remove redundant semicolons",
                            edits: [
                                (
                                    "",
                                    Span {
                                        lo: 81,
                                        hi: 85,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn needless_operation_no_lint_for_lambda_operations() {
    check(
        &wrap_in_callable("let a = (a) => a + 1;", CallableKind::Function),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn needless_operation_no_lint_for_valid_lambda_operations() {
    check(
        &wrap_in_callable("let op = (q) => H(q);", CallableKind::Function),
        &expect![[r"
            []
        "]],
    );
}

#[test]
fn needless_operation_non_empty_op_and_no_specialization() {
    check(
        &wrap_in_callable("let x = 2;", CallableKind::Operation),
        &expect![[r#"
            [
                SrcLint {
                    source: "RunProgram",
                    level: Allow,
                    message: "operation does not contain any quantum operations",
                    help: "this callable can be declared as a function instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn needless_operation_non_empty_op_and_specialization() {
    check(
        indoc! {"
        operation Run(target : Qubit) : Unit is Adj {
            body ... {
                Message(\"hi\");
            }
            adjoint self;
        }
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "Run",
                    level: Allow,
                    message: "operation does not contain any quantum operations",
                    help: "this callable can be declared as a function instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn needless_operation_no_lint_for_empty_op_explicit_specialization() {
    check(
        indoc! {"
        operation I(target : Qubit) : Unit {
            body ... {}
            adjoint self;
        }

    "},
        &expect![[r"
            []
        "]],
    );
}

#[test]
fn needless_operation_no_lint_for_empty_op_implicit_specialization() {
    check(
        indoc! {"
        operation DoNothing() : Unit is Adj + Ctl {}
    "},
        &expect![[r"
            []
        "]],
    );
}

#[test]
fn needless_operation_partial_application() {
    check(
        indoc! {"
        operation PrepareBellState(q1 : Qubit, q2 : Qubit) : Unit {
            H(q1);
            CNOT(q1, q2);
        }

        operation PartialApplication(q1 : Qubit) : Qubit => Unit {
            return PrepareBellState(q1, _);
        }
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "PartialApplication",
                    level: Allow,
                    message: "operation does not contain any quantum operations",
                    help: "this callable can be declared as a function instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_newtype_usage() {
    check(
        indoc! {"
        newtype Foo = ();
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "newtype Foo = ();",
                    level: Allow,
                    message: "deprecated `newtype` declarations",
                    help: "`newtype` declarations are deprecated, use `struct` instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_function_cons() {
    check(
        indoc! {"
        struct Foo {}
        function Bar() : Foo { Foo() }
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "Foo",
                    level: Allow,
                    message: "deprecated function constructors",
                    help: "function constructors for struct types are deprecated, use `new` instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_with_op_for_structs() {
    check(
        indoc! {"
        struct Foo { x : Int }
        function Bar() : Foo {
            let foo = new Foo { x = 2 };
            foo w/ x <- 3
        }
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "foo w/ x <- 3",
                    level: Allow,
                    message: "deprecated `w/` and `w/=` operators for structs",
                    help: "`w/` and `w/=` operators for structs are deprecated, use `new` instead",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with struct constructor",
                            edits: [
                                (
                                    "new Foo {\n        ...foo,\n        x = 3,\n    }",
                                    Span {
                                        lo: 111,
                                        hi: 124,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_with_eq_op_for_structs() {
    check(
        indoc! {"
        struct Foo { x : Int }
        function Bar() : Foo {
            mutable foo = new Foo { x = 2 };
            foo w/= x <- 3;
            foo
        }
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "foo w/= x <- 3",
                    level: Allow,
                    message: "deprecated `w/` and `w/=` operators for structs",
                    help: "`w/` and `w/=` operators for structs are deprecated, use `new` instead",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with struct constructor",
                            edits: [
                                (
                                    "foo = new Foo {\n        ...foo,\n        x = 3,\n    }",
                                    Span {
                                        lo: 115,
                                        hi: 129,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_double_colon_op() {
    check(
        indoc! {"
        struct A { b : B }
        struct B { c : C }
        struct C { i : Int }
        function Bar(a : A) : Unit {
            a::b.c::i
        }
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "a::b.c::i",
                    level: Allow,
                    message: "deprecated `::` for field access",
                    help: "`::` operator is deprecated, use `.` instead",
                    code_action: Some(
                        CodeAction {
                            title: "Replace `::` with `.` for field access",
                            edits: [
                                (
                                    ".",
                                    Span {
                                        lo: 126,
                                        hi: 128,
                                    },
                                ),
                                (
                                    ".",
                                    Span {
                                        lo: 121,
                                        hi: 123,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_double_colon_op_with_spacing() {
    check(
        indoc! {"
        struct A { b : B }
        struct B { c : C }
        struct C { i : Int }
        function Bar(a : A) : Unit {
            a  ::  b.c
            ::
            i
        }
    "},
        &expect![[r#"
            [
                SrcLint {
                    source: "a  ::  b.c\n    ::\n    i",
                    level: Allow,
                    message: "deprecated `::` for field access",
                    help: "`::` operator is deprecated, use `.` instead",
                    code_action: Some(
                        CodeAction {
                            title: "Replace `::` with `.` for field access",
                            edits: [
                                (
                                    ".",
                                    Span {
                                        lo: 135,
                                        hi: 137,
                                    },
                                ),
                                (
                                    ".",
                                    Span {
                                        lo: 123,
                                        hi: 125,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn needless_operation_inside_function_call() {
    check(
        indoc! {"
    operation Main() : Unit {
        Wrapper(A());
    }

    function Wrapper(_: Unit) : Unit {}

    operation A() : Unit {
        use q = Qubit();
        M(q);
    }
    "},
        &expect![[r"
            []
        "]],
    );
}

#[test]
fn deprecated_update_expr_lint() {
    check(
        &wrap_in_callable(
            "mutable arr = []; arr w/ idx <- 42;",
            CallableKind::Function,
        ),
        &expect![[r#"
            [
                SrcLint {
                    source: "arr w/ idx <- 42",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_assign_update_expr_code_action() {
    check(
        &wrap_in_callable(
            "mutable arr = []; arr w/= idx <- 42;",
            CallableKind::Function,
        ),
        &expect![[r#"
            [
                SrcLint {
                    source: "arr w/= idx <- 42",
                    level: Allow,
                    message: "deprecated use of update assignment expressions",
                    help: "update assignment expressions \"a w/= b <- c\" are deprecated; consider using explicit assignment instead \"a[b] = c\"",
                    code_action: Some(
                        CodeAction {
                            title: "Replace update assignment expression with explicit assignment",
                            edits: [
                                (
                                    "arr[idx] = 42",
                                    Span {
                                        lo: 89,
                                        hi: 106,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_assign_update_expr_nested_code_action() {
    check(
        &wrap_in_callable(
            "mutable a = [[1, 2], [3, 4]]; a w/= 1 <- (a[1] w/ 1 <- 7);",
            CallableKind::Function,
        ),
        &expect![[r#"
            [
                SrcLint {
                    source: "a w/= 1 <- (a[1] w/ 1 <- 7)",
                    level: Allow,
                    message: "deprecated use of update assignment expressions",
                    help: "update assignment expressions \"a w/= b <- c\" are deprecated; consider using explicit assignment instead \"a[b] = c\"",
                    code_action: Some(
                        CodeAction {
                            title: "Replace update assignment expression with explicit assignment",
                            edits: [
                                (
                                    "a[1][1] = 7",
                                    Span {
                                        lo: 101,
                                        hi: 128,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "a[1] w/ 1 <- 7",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_assign_update_expr_deep_nested_code_action() {
    check(
        &wrap_in_callable(
            "mutable a = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]; a w/= 0 <- (a[0] w/ 1 <- (a[0][1] w/ 0 <- 9));",
            CallableKind::Function,
        ),
        &expect![[r#"
            [
                SrcLint {
                    source: "a w/= 0 <- (a[0] w/ 1 <- (a[0][1] w/ 0 <- 9))",
                    level: Allow,
                    message: "deprecated use of update assignment expressions",
                    help: "update assignment expressions \"a w/= b <- c\" are deprecated; consider using explicit assignment instead \"a[b] = c\"",
                    code_action: Some(
                        CodeAction {
                            title: "Replace update assignment expression with explicit assignment",
                            edits: [
                                (
                                    "a[0][1][0] = 9",
                                    Span {
                                        lo: 121,
                                        hi: 166,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "a[0] w/ 1 <- (a[0][1] w/ 0 <- 9)",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
                SrcLint {
                    source: "a[0][1] w/ 0 <- 9",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_assign_update_expr_mismatched_code_action() {
    check(
        &wrap_in_callable(
            "mutable a = [[1, 2], [3, 4]]; a w/= 0 <- (a[1] w/ 1 <- 7);",
            CallableKind::Function,
        ),
        &expect![[r#"
            [
                SrcLint {
                    source: "a w/= 0 <- (a[1] w/ 1 <- 7)",
                    level: Allow,
                    message: "deprecated use of update assignment expressions",
                    help: "update assignment expressions \"a w/= b <- c\" are deprecated; consider using explicit assignment instead \"a[b] = c\"",
                    code_action: Some(
                        CodeAction {
                            title: "Replace update assignment expression with explicit assignment",
                            edits: [
                                (
                                    "a[0] = a[1] w/ 1 <- 7",
                                    Span {
                                        lo: 101,
                                        hi: 128,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "a[1] w/ 1 <- 7",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_assign_update_expr_impure_index_single_level_code_action() {
    check(
        "function NextIndex() : Int { 0 }
        function RunProgram() : Unit {
            mutable a = [[1, 2], [3, 4]];
            a w/= NextIndex() <- (a[NextIndex()] w/ 1 <- 7);
        }",
        &expect![[r#"
            [
                SrcLint {
                    source: "a w/= NextIndex() <- (a[NextIndex()] w/ 1 <- 7)",
                    level: Allow,
                    message: "deprecated use of update assignment expressions",
                    help: "update assignment expressions \"a w/= b <- c\" are deprecated; consider using explicit assignment instead \"a[b] = c\"",
                    code_action: Some(
                        CodeAction {
                            title: "Replace update assignment expression with explicit assignment",
                            edits: [
                                (
                                    "a[NextIndex()] = a[NextIndex()] w/ 1 <- 7",
                                    Span {
                                        lo: 154,
                                        hi: 201,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "a[NextIndex()] w/ 1 <- 7",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_assign_update_expr_impure_value_pure_indices_code_action() {
    check(
        "function NextValue() : Int { 0 }
        function RunProgram() : Unit {
            mutable a = [[1, 2], [3, 4]];
            a w/= 0 <- (a[0] w/ 1 <- NextValue());
        }",
        &expect![[r#"
            [
                SrcLint {
                    source: "a w/= 0 <- (a[0] w/ 1 <- NextValue())",
                    level: Allow,
                    message: "deprecated use of update assignment expressions",
                    help: "update assignment expressions \"a w/= b <- c\" are deprecated; consider using explicit assignment instead \"a[b] = c\"",
                    code_action: Some(
                        CodeAction {
                            title: "Replace update assignment expression with explicit assignment",
                            edits: [
                                (
                                    "a[0][1] = NextValue()",
                                    Span {
                                        lo: 154,
                                        hi: 191,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "a[0] w/ 1 <- NextValue()",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn deprecated_assign_update_expr_impure_middle_index_partial_merge_code_action() {
    check(
        "function NextIndex() : Int { 0 }
        function RunProgram() : Unit {
            mutable a = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]];
            a w/= 0 <- (a[0] w/ NextIndex() <- (a[0][NextIndex()] w/ 1 <- 7));
        }",
        &expect![[r#"
            [
                SrcLint {
                    source: "a w/= 0 <- (a[0] w/ NextIndex() <- (a[0][NextIndex()] w/ 1 <- 7))",
                    level: Allow,
                    message: "deprecated use of update assignment expressions",
                    help: "update assignment expressions \"a w/= b <- c\" are deprecated; consider using explicit assignment instead \"a[b] = c\"",
                    code_action: Some(
                        CodeAction {
                            title: "Replace update assignment expression with explicit assignment",
                            edits: [
                                (
                                    "a[0][NextIndex()] = a[0][NextIndex()] w/ 1 <- 7",
                                    Span {
                                        lo: 174,
                                        hi: 239,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "a[0] w/ NextIndex() <- (a[0][NextIndex()] w/ 1 <- 7)",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
                SrcLint {
                    source: "a[0][NextIndex()] w/ 1 <- 7",
                    level: Allow,
                    message: "deprecated use of update expressions",
                    help: "update expressions \"a w/ b <- c\" are deprecated; consider using explicit assignment instead",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if() {
    check(
        &wrap_in_callable("if true { 42 } else { 0 } - 1", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "- 1",
                    level: Warn,
                    message: "ambiguous unary operator after if-expression",
                    help: "consider wrapping the if-expression in parentheses or using a semicolon to clarify the intended use of the operator",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if_fixed_by_parens_around_if() {
    check(
        &wrap_in_callable("(if true { 42 } else { 0 }) - 1", CallableKind::Function),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if_fixed_by_parens_around_all() {
    check(
        &wrap_in_callable("(if true { 42 } else { 0 } - 1)", CallableKind::Function),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if_fixed_by_semicolon() {
    check(
        &wrap_in_callable("if true { 42 } else { 0 }; - 1", CallableKind::Function),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if_does_trigger_warning_on_different_non_unit_types() {
    check(
        &wrap_in_callable("if true { 42.0 } else { 0.0 } - 1", CallableKind::Function),
        &expect![[r#"
            [
                SrcLint {
                    source: "- 1",
                    level: Warn,
                    message: "ambiguous unary operator after if-expression",
                    help: "consider wrapping the if-expression in parentheses or using a semicolon to clarify the intended use of the operator",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if_does_not_trigger_on_unit_type() {
    check(
        &wrap_in_callable("if true { 42; } else { 0; } - 1", CallableKind::Function),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if_does_not_trigger_for_unary_ops_besides_minus_plus() {
    check(
        &wrap_in_callable("if true { 42 } else { 0 } !1", CallableKind::Function),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn deprecated_borrow_keyword() {
    check(
        &wrap_in_callable("borrow qs = Qubit[2];", CallableKind::Operation),
        &expect![[r#"
            [
                SrcLint {
                    source: "borrow",
                    level: Warn,
                    message: "deprecated `borrow` qubit allocation",
                    help: "the `borrow` keyword for qubit allocation is deprecated, use `use` instead",
                    code_action: Some(
                        CodeAction {
                            title: "Replace `borrow` with `use`",
                            edits: [
                                (
                                    "use",
                                    Span {
                                        lo: 72,
                                        hi: 78,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn check_that_hir_lints_are_deduplicated_in_operations_with_multiple_specializations() {
    check_with_deduplication(
        "
        operation Main() : Unit {}
        operation LintProblem() : Unit is Adj + Ctl {
            use q = Qubit();
            0.0 == 0.0;
        }",
        &expect![[r#"
            [
                SrcLint {
                    source: "0.0 == 0.0",
                    level: Warn,
                    message: "strict comparison of doubles",
                    help: "consider comparing them with some margin of error",
                    code_action: None,
                },
            ]
        "#]],
    );
}

fn compile_and_collect_lints(source: &str, config: Option<&[LintOrGroupConfig]>) -> Vec<Lint> {
    let (lints, _) = compile_for_target_and_collect_lints(
        &[("source.qs", source)],
        TargetCapabilityFlags::all(),
        config,
    );
    lints
}

/// Compiles `sources`, given as `(name, contents)` pairs, for a target with the given
/// `capabilities`. Returns the lints along with any compilation errors.
fn compile_for_target_and_collect_lints(
    sources: &[(&str, &str)],
    capabilities: TargetCapabilityFlags,
    config: Option<&[LintOrGroupConfig]>,
) -> (Vec<Lint>, Vec<qsc::compile::Error>) {
    let mut store = PackageStore::new(compile::core());
    let std = store.insert(compile::std(&store, capabilities));
    let sources = SourceMap::new(
        sources
            .iter()
            .map(|&(name, contents)| (name.into(), contents.into())),
        None,
    );
    let (unit, errors) = qsc::compile::compile(
        &store,
        &[(std, None)],
        sources,
        PackageType::Exe,
        capabilities,
        LanguageFeatures::default(),
    );

    let id = store.insert(unit);
    let unit = store.get(id).expect("user package should exist");

    let mut actual_config = vec![LintOrGroupConfig::Lint(LintConfig {
        kind: LintKind::Ast(crate::AstLint::AvoidNamespaceBlock),
        level: LintLevel::Allow,
    })];

    if let Some(c) = config {
        actual_config.extend_from_slice(c);
    }

    let lints = run_lints_without_deduplication(&store, unit, capabilities, Some(&actual_config));
    (lints, errors)
}

#[test]
fn avoid_namespace_block_explicit() {
    let source = "namespace Foo { operation Main() : Unit {} }";
    let config: &[LintOrGroupConfig] = &[LintOrGroupConfig::Lint(LintConfig {
        kind: LintKind::Ast(crate::AstLint::AvoidNamespaceBlock),
        level: LintLevel::Warn,
    })];
    let lints = compile_and_collect_lints(source, Some(config));
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, source))
        .collect();
    expect![[r#"
        [
            SrcLint {
                source: "Foo",
                level: Warn,
                message: "avoid using explicit namespace blocks",
                help: "Q# best practice is to not use namespace blocks to enclose code; the namespace is inferred from the file path",
                code_action: None,
            },
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn avoid_namespace_block_implicit() {
    let source = "operation Main() : Unit {}"; // Implicit namespace, no block
    let config: &[LintOrGroupConfig] = &[LintOrGroupConfig::Lint(LintConfig {
        kind: LintKind::Ast(crate::AstLint::AvoidNamespaceBlock),
        level: LintLevel::Warn,
    })];
    let lints = compile_and_collect_lints(source, Some(config));
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, source))
        .collect();
    expect![[r#"
        []
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn avoid_namespace_block_multiple() {
    let source = indoc! {"
        namespace Foo { operation A() : Unit {} }
        namespace Bar { operation B() : Unit {} }
    "};
    let config: &[LintOrGroupConfig] = &[LintOrGroupConfig::Lint(LintConfig {
        kind: LintKind::Ast(crate::AstLint::AvoidNamespaceBlock),
        level: LintLevel::Warn,
    })];
    let lints = compile_and_collect_lints(source, Some(config));
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, source))
        .collect();
    expect![[r#"
        [
            SrcLint {
                source: "Foo",
                level: Warn,
                message: "avoid using explicit namespace blocks",
                help: "Q# best practice is to not use namespace blocks to enclose code; the namespace is inferred from the file path",
                code_action: None,
            },
            SrcLint {
                source: "Bar",
                level: Warn,
                message: "avoid using explicit namespace blocks",
                help: "Q# best practice is to not use namespace blocks to enclose code; the namespace is inferred from the file path",
                code_action: None,
            },
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn avoid_namespace_block_with_comments() {
    let source = indoc! {"
        // # Sample
        // Getting started
        namespace Foo {
            operation Main() : Unit {}
        }
    "};
    let config: &[LintOrGroupConfig] = &[LintOrGroupConfig::Lint(LintConfig {
        kind: LintKind::Ast(crate::AstLint::AvoidNamespaceBlock),
        level: LintLevel::Warn,
    })];
    let lints = compile_and_collect_lints(source, Some(config));
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, source))
        .collect();
    expect![[r#"
        [
            SrcLint {
                source: "Foo",
                level: Warn,
                message: "avoid using explicit namespace blocks",
                help: "Q# best practice is to not use namespace blocks to enclose code; the namespace is inferred from the file path",
                code_action: None,
            },
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn avoid_namespace_block_with_doc_comments() {
    let source = indoc! {"
        /// This is a doc comment
        namespace Foo {
            operation Main() : Unit {}
        }
    "};
    let config: &[LintOrGroupConfig] = &[LintOrGroupConfig::Lint(LintConfig {
        kind: LintKind::Ast(crate::AstLint::AvoidNamespaceBlock),
        level: LintLevel::Warn,
    })];
    let lints = compile_and_collect_lints(source, Some(config));
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, source))
        .collect();
    expect![[r#"
        [
            SrcLint {
                source: "Foo",
                level: Warn,
                message: "avoid using explicit namespace blocks",
                help: "Q# best practice is to not use namespace blocks to enclose code; the namespace is inferred from the file path",
                code_action: None,
            },
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn prefer_mresetz_for_m_on_base_profile() {
    check_for_target(
        indoc! {"
            operation Main() : Result {
                use q = Qubit();
                H(q);
                let r = M(q);
                Reset(q);
                r
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetZ",
                                    Span {
                                        lo: 71,
                                        hi: 72,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_for_measure_each_z_on_base_profile() {
    check_for_target(
        indoc! {"
            operation Main() : Result[] {
                use qs = Qubit[2];
                H(qs[0]);
                let rs = MeasureEachZ(qs);
                ResetAll(qs);
                rs
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "MeasureEachZ",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetEachZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetEachZ",
                                    Span {
                                        lo: 80,
                                        hi: 92,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_for_m_passed_as_value_on_base_profile() {
    check_for_target(
        indoc! {"
            import Std.Arrays.ForEach;

            operation Main() : Result[] {
                use qs = Qubit[2];
                let rs = ForEach(M, qs);
                ResetAll(qs);
                rs
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetZ",
                                    Span {
                                        lo: 102,
                                        hi: 103,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_for_qualified_m_on_base_profile() {
    check_for_target(
        indoc! {"
            operation Main() : Result[] {
                use q = Qubit();
                let a = Std.Intrinsic.M(q);
                let b = Microsoft.Quantum.Intrinsic.M(q);
                Reset(q);
                [a, b]
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "Std.Intrinsic.M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetZ",
                                    Span {
                                        lo: 63,
                                        hi: 78,
                                    },
                                ),
                            ],
                        },
                    ),
                },
                SrcLint {
                    source: "Microsoft.Quantum.Intrinsic.M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetZ",
                                    Span {
                                        lo: 95,
                                        hi: 124,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_not_reported_on_targets_with_qubit_reuse() {
    let source = indoc! {"
        operation Main() : Result {
            use q = Qubit();
            H(q);
            let r = M(q);
            Reset(q);
            r
        }
    "};
    check_for_target(
        source,
        Profile::AdaptiveRI.into(),
        &expect![[r#"
        []
    "#]],
    );
    check_for_target(
        source,
        Profile::Unrestricted.into(),
        &expect![[r#"
        []
    "#]],
    );
}

#[test]
fn prefer_mresetz_not_reported_for_measurements_with_reset() {
    check_for_target(
        indoc! {"
            operation Main() : Result[] {
                use q = Qubit();
                use qs = Qubit[2];
                let r = MResetZ(q);
                let rs = MResetEachZ(qs);
                [r] + rs
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn prefer_mresetz_not_reported_for_user_defined_m() {
    check_for_target(
        indoc! {"
            operation M(q : Qubit) : Result {
                MResetZ(q)
            }

            operation Main() : Result {
                use q = Qubit();
                M(q)
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn prefer_mresetz_not_reported_in_test_callables() {
    check_for_target(
        indoc! {r#"
            import Std.Diagnostics.Fact;

            operation Main() : Result {
                use q = Qubit();
                let r = M(q);
                Reset(q);
                r
            }

            @Test()
            operation MeasurementKeepsState() : Unit {
                use q = Qubit();
                X(q);
                let measure = qubit => M(qubit);
                let results = [M(q), measure(q)];
                Fact(results == [One, One], "`M` should not reset the qubit");
                Reset(q);
            }
        "#},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetZ",
                                    Span {
                                        lo: 91,
                                        hi: 92,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_not_reported_in_simulatable_intrinsics() {
    check_for_target(
        indoc! {"
            @SimulatableIntrinsic()
            operation Dephase(q : Qubit) : Unit {
                M(q);
            }

            operation Main() : Result {
                use q = Qubit();
                Dephase(q);
                let r = M(q);
                Reset(q);
                r
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetZ",
                                    Span {
                                        lo: 152,
                                        hi: 153,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_quick_fix_omitted_when_std_is_a_local() {
    check_for_target(
        indoc! {"
            operation Main() : Result {
                use q = Qubit();
                let Std = 5;
                let r = M(q);
                Reset(q);
                r
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_quick_fix_omitted_when_std_is_declared_again_later() {
    check_for_target(
        indoc! {"
            operation Main() : Result {
                use q = Qubit();
                let Std = 5;
                let r = M(q);
                let Std = 6;
                Reset(q);
                r
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_quick_fix_omitted_with_nested_std_namespace() {
    check_for_target(
        indoc! {"
            namespace App.Std.Measurement {
                operation MResetZ(q : Qubit) : Result {
                    Reset(q);
                    Zero
                }
            }

            namespace App {
                operation Main() : Result {
                    use q = Qubit();
                    let r = M(q);
                    Reset(q);
                    r
                }
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_quick_fix_omitted_with_std_namespace_export() {
    check_for_target(
        indoc! {"
            namespace Replacement {
                operation MResetZ(q : Qubit) : Result {
                    Reset(q);
                    Zero
                }
            }

            namespace App.Std {
                export Replacement as Measurement;
            }

            namespace App {
                operation Main() : Result {
                    use q = Qubit();
                    let r = M(q);
                    Reset(q);
                    r
                }
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_quick_fix_omitted_with_std_alias() {
    check_for_target(
        indoc! {"
            namespace App.Lib.Measurement {
                operation MResetZ(q : Qubit) : Result {
                    Reset(q);
                    Zero
                }
            }

            namespace App {
                import App.Lib as Std;

                operation Main() : Result {
                    use q = Qubit();
                    let r = M(q);
                    Reset(q);
                    r
                }
            }
        "},
        Profile::Base.into(),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Warn,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: None,
                },
            ]
        "#]],
    );
}

#[test]
fn prefer_mresetz_quick_fix_omitted_with_std_namespace_in_dependency() {
    // A dependency only registers the last segment of each namespace as importable,
    // so `Lib.X.Std` must be found through the namespace path.
    let capabilities = Profile::Base.into();
    let mut store = PackageStore::new(compile::core());
    let std = store.insert(compile::std(&store, capabilities));
    let lib_source = indoc! {"
        namespace X.Std.Measurement {
            operation MResetZ(q : Qubit) : Result {
                Reset(q);
                Zero
            }
            export MResetZ;
        }
    "};
    let (lib, lib_errors) = qsc::compile::compile(
        &store,
        &[(std, None)],
        SourceMap::new([("Lib.qs".into(), lib_source.into())], None),
        PackageType::Lib,
        capabilities,
        LanguageFeatures::default(),
    );
    assert!(
        lib_errors.is_empty(),
        "dependency should compile: {lib_errors:?}"
    );
    let lib = store.insert(lib);

    let source = indoc! {"
        import Lib.X.*;

        operation Main() : Result {
            use q = Qubit();
            let r = M(q);
            Reset(q);
            r
        }
    "};
    let (unit, errors) = qsc::compile::compile(
        &store,
        &[(std, None), (lib, Some("Lib".into()))],
        SourceMap::new([("source.qs".into(), source.into())], None),
        PackageType::Exe,
        capabilities,
        LanguageFeatures::default(),
    );
    assert!(errors.is_empty(), "source should compile: {errors:?}");
    let id = store.insert(unit);
    let unit = store.get(id).expect("user package should exist");

    let actual: Vec<_> = run_lints_without_deduplication(&store, unit, capabilities, None)
        .into_iter()
        .map(|lint| SrcLint::from(&lint, source))
        .collect();
    expect![[r#"
        [
            SrcLint {
                source: "M",
                level: Warn,
                message: "measurement without reset on a target that does not support qubit reuse",
                help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                code_action: None,
            },
        ]
    "#]].assert_debug_eq(&actual);
}

#[test]
fn prefer_mresetz_not_reported_in_circuit_files() {
    let main = indoc! {"
        operation Main() : Result {
            use q = Qubit();
            let r = M(q);
            Reset(q);
            r
        }
    "};
    // Circuit files (`.qsc`) are converted to generated Q# like this before compiling.
    let circuit = indoc! {"
        operation Circuit(q : Qubit) : Result {
            H(q);
            M(q)
        }
    "};
    let (lints, errors) = compile_for_target_and_collect_lints(
        &[("Main.qs", main), ("Circuit.qsc", circuit)],
        Profile::Base.into(),
        None,
    );
    assert!(errors.is_empty(), "sources should compile: {errors:?}");
    // Only the `M` in `Main.qs`, the first source, is reported, so its span maps onto `main`.
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, main))
        .collect();
    expect![[r#"
        [
            SrcLint {
                source: "M",
                level: Warn,
                message: "measurement without reset on a target that does not support qubit reuse",
                help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                code_action: Some(
                    CodeAction {
                        title: "Replace with `MResetZ` (resets after measuring)",
                        edits: [
                            (
                                "Std.Measurement.MResetZ",
                                Span {
                                    lo: 61,
                                    hi: 62,
                                },
                            ),
                        ],
                    },
                ),
            },
        ]
    "#]].assert_debug_eq(&actual);
}

#[test]
fn prefer_mresetz_level_can_be_configured() {
    check_for_target_with_config(
        indoc! {"
            operation Main() : Result {
                use q = Qubit();
                M(q)
            }
        "},
        Profile::Base.into(),
        Some(&[LintOrGroupConfig::Lint(LintConfig {
            kind: LintKind::Hir(crate::HirLint::PreferMResetZ),
            level: LintLevel::Allow,
        })]),
        &expect![[r#"
            [
                SrcLint {
                    source: "M",
                    level: Allow,
                    message: "measurement without reset on a target that does not support qubit reuse",
                    help: "if the qubit's state after measurement is not needed, use `MResetZ` or `MResetEachZ` instead; on this target, any later use of a qubit measured without reset, even `Reset`, costs an extra CNOT gate and often an extra qubit",
                    code_action: Some(
                        CodeAction {
                            title: "Replace with `MResetZ` (resets after measuring)",
                            edits: [
                                (
                                    "Std.Measurement.MResetZ",
                                    Span {
                                        lo: 53,
                                        hi: 54,
                                    },
                                ),
                            ],
                        },
                    ),
                },
            ]
        "#]],
    );
}

fn check(source: &str, expected: &Expect) {
    let source = wrap_in_namespace(source);
    let actual: Vec<_> = compile_and_collect_lints(&source, None)
        .into_iter()
        .map(|lint| SrcLint::from(&lint, &source))
        .collect();
    expected.assert_debug_eq(&actual);
}

fn check_with_config(source: &str, config: Option<&[LintOrGroupConfig]>, expected: &Expect) {
    let source = wrap_in_namespace(source);
    let actual: Vec<_> = compile_and_collect_lints(&source, config)
        .into_iter()
        .map(|lint| SrcLint::from(&lint, &source))
        .collect();
    expected.assert_debug_eq(&actual);
}

fn check_with_deduplication(source: &str, expected: &Expect) {
    let source = wrap_in_namespace(source);
    let mut lints = compile_and_collect_lints(&source, None);
    remove_duplicates(&mut lints);
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, &source))
        .collect();
    expected.assert_debug_eq(&actual);
}

/// Like [`check`], but compiles `source` for a target with the given `capabilities`,
/// and requires it to compile without errors.
fn check_for_target(source: &str, capabilities: TargetCapabilityFlags, expected: &Expect) {
    check_for_target_with_config(source, capabilities, None, expected);
}

/// Like [`check_with_config`], but compiles `source` for a target with the given
/// `capabilities`, and requires it to compile without errors.
fn check_for_target_with_config(
    source: &str,
    capabilities: TargetCapabilityFlags,
    config: Option<&[LintOrGroupConfig]>,
    expected: &Expect,
) {
    let (lints, errors) =
        compile_for_target_and_collect_lints(&[("source.qs", source)], capabilities, config);
    assert!(errors.is_empty(), "source should compile: {errors:?}");
    let actual: Vec<_> = lints
        .into_iter()
        .map(|lint| SrcLint::from(&lint, source))
        .collect();
    expected.assert_debug_eq(&actual);
}

/// Wraps some source code into a namespace, to make testing easier.
fn wrap_in_namespace(source: &str) -> String {
    format!(
        "namespace Foo {{
            {source}
        }}"
    )
}

fn wrap_in_callable(source: &str, callable_type: CallableKind) -> String {
    format!(
        "{callable_type} RunProgram() : Unit {{
            {source}
        }}"
    )
}

/// A version of Lint that replaces the span by source code
/// to make unit tests easier to write and verify.
#[allow(dead_code)]
#[derive(Debug)]
struct SrcLint {
    source: String,
    level: LintLevel,
    message: &'static str,
    help: &'static str,
    code_action: Option<CodeAction>,
}

impl SrcLint {
    fn from(lint: &Lint, source: &str) -> Self {
        Self {
            source: source[lint.span].into(),
            level: lint.level,
            message: lint.message,
            help: lint.help,
            code_action: lint.code_action.clone(),
        }
    }
}
