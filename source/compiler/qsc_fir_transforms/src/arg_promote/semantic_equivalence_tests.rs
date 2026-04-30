// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use indoc::indoc;
use proptest::prelude::*;

#[test]
fn tuple_param_flattened_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Add(pair : (Int, Int)) : Int {
                let (a, b) = pair;
                a + b
            }

            @EntryPoint()
            function Main() : Int {
                Add((3, 4))
            }
        }
    "#});
}

#[test]
fn nested_tuple_param_flattened_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Sum(args : ((Int, Int), Int)) : Int {
                let ((a, b), c) = args;
                a + b + c
            }

            @EntryPoint()
            function Main() : Int {
                Sum(((1, 2), 3))
            }
        }
    "#});
}

#[test]
fn mixed_scalar_and_tuple_params_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Weighted(scale : Int, pair : (Int, Int)) : Int {
                let (x, y) = pair;
                scale * (x + y)
            }

            @EntryPoint()
            function Main() : Int {
                Weighted(2, (5, 7))
            }
        }
    "#});
}

fn tuple_parameter_argument_pattern() -> impl Strategy<Value = String> {
    (2usize..=4, prop::collection::vec(-20i64..=20, 4)).prop_map(|(width, argument_values)| {
        let parameter_type = (0..width).map(|_| "Int").collect::<Vec<_>>().join(", ");
        let field_bindings = (0..width)
            .map(|index| format!("field{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = argument_values
            .into_iter()
            .take(width)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        formatdoc! {r#"
                namespace Test {{
                    function ProjectFirst(parameter : ({parameter_type})) : Int {{
                        let ({field_bindings}) = parameter;
                        field0
                    }}

                    @EntryPoint()
                    function Main() : Int {{
                        ProjectFirst(({arguments}))
                    }}
                }}
            "#}
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn tuple_parameter_argument_promotion_preserves_semantics(source in tuple_parameter_argument_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

fn qsharp_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn nested_mixed_struct_callable_strategy() -> impl Strategy<Value = String> {
    (
        -20i64..=20,
        prop::bool::ANY,
        -20i64..=20,
        prop::bool::ANY,
        prop::bool::ANY,
    )
        .prop_map(|(value, flag, bonus, enabled, prefer_alias)| {
            let flag = qsharp_bool(flag);
            let enabled = qsharp_bool(enabled);
            let selector = qsharp_bool(prefer_alias);

            formatdoc! {r#"
                namespace Test {{
                    struct Inner {{ Value : Int, Flag : Bool }}
                    struct Outer {{ Left : Inner, Bonus : Int, Enabled : Bool }}

                    function Sum(input : Outer) : Int {{
                        let signed = if input.Left.Flag {{ input.Left.Value }} else {{ -input.Left.Value }};
                        if input.Enabled {{ signed + input.Bonus }} else {{ signed - input.Bonus }}
                    }}

                    @EntryPoint()
                    function Main() : Int {{
                        let input = new Outer {{
                            Left = new Inner {{ Value = {value}, Flag = {flag} }},
                            Bonus = {bonus},
                            Enabled = {enabled}
                        }};
                        let f = Sum;
                        let viaAlias = f(input);
                        let direct = Sum(input);
                        if {selector} {{ viaAlias }} else {{ direct }}
                    }}
                }}
            "#}
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn nested_mixed_struct_callable_arg_promotion_preserves_semantics(
        source in nested_mixed_struct_callable_strategy()
    ) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
