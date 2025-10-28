// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Lint, LintLevel, LintOrGroupConfig,
    lint_groups::LintGroup,
    linter::{remove_duplicates, run_lints_without_deduplication},
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, span::Span, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, SourceMap};
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
                    code_action_edits: [],
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
                    code_action_edits: [],
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
                    code_action_edits: [],
                },
                SrcLint {
                    source: "a = b = c",
                    level: Warn,
                    message: "discouraged use of chain assignment",
                    help: "assignment expressions always return `Unit`, so chaining them may not be useful",
                    code_action_edits: [],
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
                    code_action_edits: [
                        (
                            "",
                            Span {
                                lo: 71,
                                hi: 74,
                            },
                        ),
                    ],
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
                    code_action_edits: [],
                },
                SrcLint {
                    source: "RunProgram",
                    level: Allow,
                    message: "operation does not contain any quantum operations",
                    help: "this callable can be declared as a function instead",
                    code_action_edits: [],
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
                    code_action_edits: [
                        (
                            "",
                            Span {
                                lo: 94,
                                hi: 97,
                            },
                        ),
                    ],
                },
                SrcLint {
                    source: "((1 + 2)) / 0",
                    level: Error,
                    message: "attempt to divide by zero",
                    help: "division by zero will fail at runtime",
                    code_action_edits: [],
                },
                SrcLint {
                    source: "((1 + 2))",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action_edits: [
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
                SrcLint {
                    source: "RunProgram",
                    level: Allow,
                    message: "operation does not contain any quantum operations",
                    help: "this callable can be declared as a function instead",
                    code_action_edits: [],
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
                    code_action_edits: [
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
                    code_action_edits: [],
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
                    code_action_edits: [],
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
                    code_action_edits: [],
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
                    code_action_edits: [
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
                    code_action_edits: [
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
                SrcLint {
                    source: "(5 * 4 * (2 ^ 10))",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action_edits: [
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
                SrcLint {
                    source: "(2 ^ 10)",
                    level: Allow,
                    message: "unnecessary parentheses",
                    help: "remove the extra parentheses for clarity",
                    code_action_edits: [
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
                    code_action_edits: [
                        (
                            "",
                            Span {
                                lo: 81,
                                hi: 85,
                            },
                        ),
                    ],
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
                    code_action_edits: [],
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
                    code_action_edits: [],
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
                    code_action_edits: [],
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
                    code_action_edits: [],
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
                    code_action_edits: [],
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
                    code_action_edits: [
                        (
                            "new Foo {\n        ...foo,\n        x = 3,\n    }",
                            Span {
                                lo: 111,
                                hi: 124,
                            },
                        ),
                    ],
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
                    code_action_edits: [
                        (
                            "foo = new Foo {\n        ...foo,\n        x = 3,\n    }",
                            Span {
                                lo: 115,
                                hi: 129,
                            },
                        ),
                    ],
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
                    code_action_edits: [
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
                    code_action_edits: [
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
                    code_action_edits: [],
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
                    code_action_edits: [
                        (
                            "arr[idx] = 42",
                            Span {
                                lo: 89,
                                hi: 106,
                            },
                        ),
                    ],
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
                    code_action_edits: [],
                },
            ]
        "#]],
    );
}

#[test]
fn ambiguous_unary_operator_after_if_fixed_by_parens() {
    check(
        &wrap_in_callable("(if true { 42 } else { 0 }) - 1", CallableKind::Function),
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
                    code_action_edits: [],
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
                    code_action_edits: [],
                },
            ]
        "#]],
    );
}

fn compile_and_collect_lints(source: &str, config: Option<&[LintOrGroupConfig]>) -> Vec<Lint> {
    let mut store = PackageStore::new(compile::core());
    let std = store.insert(compile::std(&store, TargetCapabilityFlags::all()));
    let sources = SourceMap::new([("source.qs".into(), source.into())], None);
    let (unit, _) = qsc::compile::compile(
        &store,
        &[(std, None)],
        sources,
        PackageType::Exe,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );

    let id = store.insert(unit);
    let unit = store.get(id).expect("user package should exist");
    run_lints_without_deduplication(&store, unit, config)
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
    code_action_edits: Vec<(String, Span)>,
}

impl SrcLint {
    fn from(lint: &Lint, source: &str) -> Self {
        Self {
            source: source[lint.span].into(),
            level: lint.level,
            message: lint.message,
            help: lint.help,
            code_action_edits: lint
                .code_action_edits
                .iter()
                .map(|(edit, span)| (edit.clone(), *span))
                .collect(),
        }
    }
}
