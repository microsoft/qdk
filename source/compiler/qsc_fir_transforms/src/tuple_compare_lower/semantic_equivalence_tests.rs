// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(feature = "slow-proptest-tests")]
use indoc::formatdoc;
use indoc::indoc;
#[cfg(feature = "slow-proptest-tests")]
use proptest::prelude::*;

#[test]
fn tuple_comparison_evaluates_calls_once_and_does_not_skip_measurements() {
    // Calling Bump twice flips the qubit back to Zero. Skipping the second
    // reset after a mismatched first element leaves the other qubit at One.
    for operator in ["==", "!="] {
        for (body, expected) in [
            (
                format!("use q = Qubit(); let _ = Bump(q) {operator} (One, 5); MResetZ(q)"),
                true,
            ),
            (
                format!(
                    "use a = Qubit(); use b = Qubit(); X(b); let _ = (MResetZ(a), MResetZ(b)) {operator} (One, Zero); MResetZ(b)"
                ),
                false,
            ),
        ] {
            let source = format!(
                r#"
                operation Bump(q : Qubit) : (Result, Int) {{ X(q); (M(q), 5) }}
                @EntryPoint() operation Main() : Result {{ {body} }}
                "#
            );
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(expected)),
            );
        }
    }
}

#[test]
fn tuple_comparison_preserves_earlier_reads_and_operand_failure_order() {
    for operator in ["==", "!="] {
        for body in [
            format!("mutable x = 1; (x, {{ set x = 2; 3 }}) {operator} (1, 3)"),
            format!("mutable x = 1; (x, 3) {operator} ({{ set x = 2; 1 }}, 3)"),
            format!("mutable pair = (1, 3); pair {operator} {{ set pair = (2, 4); (1, 3) }}"),
            format!("mutable x = 1; (x, 3) {operator} ({{ set x += 1; 1 }}, 3)"),
            format!(
                "mutable x = 1; mutable y = 3; (x, y) {operator} {{ set (x, y) = (2, 4); (1, 3) }}"
            ),
            format!("mutable x = 1; (((x, 3) == ({{ set x = 2; 1 }}, 3)), x) {operator} (true, 2)"),
            format!(
                "mutable x = 1; mutable count = 0; for i in 1..3 {{ if (x, 3) == ({{ set x += 1; i }}, 3) {{ set count += 1; }} }} (count, x) {operator} (3, 4)"
            ),
            format!("Make(\"left\", 1) {operator} Make(\"right\", 1)"),
        ] {
            let source = format!(
                r#"
                function Make(label : String, x : Int) : (Int, (Int, Int)) {{
                    Message(label);
                    (x, (2, 3))
                }}
                @EntryPoint() operation Main() : Bool {{ {body} }}
                "#
            );
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Bool(operator == "=="),
            );
        }
        let source = format!(
            r#"
            function Log(label : String, value : Int) : Int {{ Message(label); value }}
            @EntryPoint() operation Main() : Bool {{
                mutable pair = (1, (2, 3));
                pair {operator} {{
                    set pair = (9, (8, 7));
                    (Log("right first", 1), (Log("right second", 2), Log("right third", 3)))
                }}
            }}
            "#
        );
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Bool(operator == "=="),
        );
        let failure = format!(
            r#"
            function Fail(label : String) : Int {{ Message(label); fail label }}
            @EntryPoint() operation Main() : Bool {{
                (0, Fail("left")) {operator} (1, Fail("right"))
            }}
            "#
        );
        assert!(
            crate::test_utils::eval_qsharp_original(&failure)
                .expect_err("left operand must fail")
                .contains("left")
        );
        crate::test_utils::check_semantic_equivalence(&failure);
        let indexing_failure =
            format!("@EntryPoint() operation Main() : Bool {{ (0, [7][2]) {operator} (1, 7) }}");
        let error = crate::test_utils::eval_qsharp_original(&indexing_failure)
            .expect_err("the out-of-range access must run even after a mismatched element");
        assert!(error.contains("IndexOutOfRange(2,"), "{error}");
        crate::test_utils::check_semantic_equivalence(&indexing_failure);

        let lazy = format!(
            r#"
            function Fail() : Int {{ fail "untaken operand"; }}
            @EntryPoint() operation Main() : Bool {{
                false and ((0, Fail()) {operator} (1, 7))
            }}
            "#
        );
        crate::test_utils::check_semantic_equivalence_with_expected(
            &lazy,
            qsc_eval::val::Value::Bool(false),
        );
        for (left, right, equal) in [
            ("((), 1)", "((), 1)", true),
            ("(((), ()), 1)", "(((), ()), 2)", false),
            ("(empty, ((), 1))", "(empty, ((), 1))", true),
        ] {
            let source = format!(
                "@EntryPoint() operation Main() : Bool {{ let empty : Int[] = []; {left} {operator} {right} }}"
            );
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Bool(if operator == "==" { equal } else { !equal }),
            );
        }
    }
}

#[test]
fn tuple_eq_comparison_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Bool {
                let a = (1, 2);
                let b = (1, 2);
                a == b
            }
        }
    "#});
}

#[test]
fn tuple_neq_comparison_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Bool {
                let a = (1, 2);
                let b = (3, 4);
                a != b
            }
        }
    "#});
}

#[test]
fn nested_tuple_eq_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Bool {
                let a = ((1, 2), 3);
                let b = ((1, 2), 3);
                a == b
            }
        }
    "#});
}

#[cfg(feature = "slow-proptest-tests")]
fn flat_int_tuple_comparison_pattern() -> impl Strategy<Value = String> {
    (
        2usize..=4,
        prop::bool::ANY,
        prop::collection::vec(-20i64..=20, 4),
        prop::collection::vec(-20i64..=20, 4),
    )
        .prop_map(|(width, use_not_equal, left_values, right_values)| {
            let left_tuple = left_values
                .into_iter()
                .take(width)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let right_tuple = right_values
                .into_iter()
                .take(width)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let operator = if use_not_equal { "!=" } else { "==" };

            formatdoc! {r#"
                    namespace Test {{
                        @EntryPoint()
                        function Main() : Bool {{
                            let left = ({left_tuple});
                            let right = ({right_tuple});
                            left {operator} right
                        }}
                    }}
                "#}
        })
}

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn flat_int_tuple_comparison_preserves_semantics(source in flat_int_tuple_comparison_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[cfg(feature = "slow-proptest-tests")]
fn qsharp_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[cfg(feature = "slow-proptest-tests")]
fn nested_mixed_tuple_comparison_strategy() -> impl Strategy<Value = String> {
    (
        prop::bool::ANY,
        -16i64..=16,
        prop::bool::ANY,
        -16i64..=16,
        prop::bool::ANY,
        -16i64..=16,
        -16i64..=16,
        prop::bool::ANY,
        -16i64..=16,
        prop::bool::ANY,
        -16i64..=16,
    )
        .prop_map(
            |(
                use_not_equal,
                left_a,
                left_flag_a,
                left_double,
                left_flag_b,
                left_c,
                right_a,
                right_flag_a,
                right_double,
                right_flag_b,
                right_c,
            )| {
                let operator = if use_not_equal { "!=" } else { "==" };
                let left_flag_a = qsharp_bool(left_flag_a);
                let left_flag_b = qsharp_bool(left_flag_b);
                let right_flag_a = qsharp_bool(right_flag_a);
                let right_flag_b = qsharp_bool(right_flag_b);

                formatdoc! {r#"
                    namespace Test {{
                        @EntryPoint()
                        function Main() : Bool {{
                            let left = (({left_a}, {left_flag_a}), ({left_double}.0, ({left_flag_b}, {left_c})));
                            let right = (({right_a}, {right_flag_a}), ({right_double}.0, ({right_flag_b}, {right_c})));
                            left {operator} right
                        }}
                    }}
                "#}
            },
        )
}

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn nested_mixed_tuple_comparison_preserves_semantics(
        source in nested_mixed_tuple_comparison_strategy()
    ) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
