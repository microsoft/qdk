// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use indoc::indoc;
use proptest::prelude::*;

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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn flat_int_tuple_comparison_preserves_semantics(source in flat_int_tuple_comparison_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

fn qsharp_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn nested_mixed_tuple_comparison_preserves_semantics(
        source in nested_mixed_tuple_comparison_strategy()
    ) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
