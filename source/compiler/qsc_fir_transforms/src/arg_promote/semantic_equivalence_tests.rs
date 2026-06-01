// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(feature = "slow-proptest-tests")]
use indoc::formatdoc;
use indoc::indoc;
#[cfg(feature = "slow-proptest-tests")]
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
fn tuple_param_variable_call_site_flattened_preserves_semantics() {
    // The argument is a variable bound to a tuple (`let x = (10, 20); Add(x)`)
    // rather than a tuple literal, exercising the call-site projection rewrite.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Add(pair : (Int, Int)) : Int {
                let (a, b) = pair;
                a + b
            }

            @EntryPoint()
            function Main() : Int {
                let x = (10, 20);
                Add(x)
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

#[test]
fn depth3_nested_tuple_param_flattened_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Sum(x : (Int, (Int, (Int, Int)))) : Int {
                let (a, (b, (c, d))) = x;
                a + b + c + d
            }

            @EntryPoint()
            function Main() : Int {
                Sum((10, (20, (30, 40))))
            }
        }
    "#});
}

#[test]
fn nested_param_controlled_call_site_flattened_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Foo(p : (Int, (Int, Int)), target : Qubit) : Unit is Ctl + Adj {
                body ... {
                    let (a, (b, c)) = p;
                    if (a + b + c) % 2 == 1 {
                        X(target);
                    }
                }
                adjoint self;
            }

            @EntryPoint()
            operation Main() : Result {
                use ctl = Qubit();
                use target = Qubit();
                X(ctl);
                Controlled Foo([ctl], ((1, (2, 2)), target));
                Adjoint Foo((1, (2, 3)), target);
                let r = MResetZ(target);
                Reset(ctl);
                r
            }
        }
    "#});
}

#[test]
fn nested_single_field_struct_param_arity_one_edge_preserves_semantics() {
    // A nested single-field struct erases to a 1-tuple leaf, exercising the
    // arity-1 leaf-projection path while the outer parameter is flattened.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Wrap { V : Int }

            function Foo(x : (Int, Wrap)) : Int {
                let (a, w) = x;
                a + w.V
            }

            @EntryPoint()
            function Main() : Int {
                Foo((1, new Wrap { V = 2 }))
            }
        }
    "#});
}

#[test]
fn mixed_field_and_whole_use_preserves_semantics() {
    // The parameter is read by field (`p.X`) and also returned as a whole value,
    // exercising aggregate reconstruction at the whole-value tail read.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            function Mixed(p : Pair) : Pair {
                let _ = p.X;
                p
            }

            @EntryPoint()
            function Main() : Int {
                let r = Mixed(new Pair { X = 3, Y = 4 });
                r.X + r.Y
            }
        }
    "#});
}

#[test]
fn recursive_self_call_promotion_preserves_semantics() {
    // The recursive self-call forwards `p` as a whole value while the base case
    // reads it by field, so promotion must reconstruct the argument at the
    // self-call site and still converge.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            function Loop(p : Pair, n : Int) : Int {
                if n <= 0 {
                    p.X + p.Y
                } else {
                    Loop(p, n - 1)
                }
            }

            @EntryPoint()
            function Main() : Int {
                Loop(new Pair { X = 1, Y = 2 }, 3)
            }
        }
    "#});
}

#[test]
fn whole_value_forward_call_preserves_semantics() {
    // A single-package end-to-end check: `Forward` reads `p.X` and forwards `p`
    // as a whole value to `Consume`, so both callables are promoted and the
    // forwarded argument is reconstructed.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            function Consume(p : Pair) : Int {
                p.X + p.Y
            }

            function Forward(p : Pair) : Int {
                let _ = p.X;
                Consume(p)
            }

            @EntryPoint()
            function Main() : Int {
                Forward(new Pair { X = 5, Y = 7 })
            }
        }
    "#});
}

#[test]
fn controllable_whole_value_use_preserves_semantics() {
    // A controllable callable reads `p` by field and forwards it as a whole
    // value, exercising reconstruction at the controlled call site.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            operation Apply(p : Pair, target : Qubit) : Unit is Ctl + Adj {
                body ... {
                    if (p.X + p.Y) % 2 == 1 {
                        X(target);
                    }
                }
                adjoint self;
            }

            operation Forward(p : Pair, target : Qubit) : Unit is Ctl + Adj {
                body ... {
                    let _ = p.X;
                    Apply(p, target);
                }
                adjoint self;
            }

            @EntryPoint()
            operation Main() : Result {
                use ctl = Qubit();
                use target = Qubit();
                X(ctl);
                Controlled Forward([ctl], (new Pair { X = 1, Y = 2 }, target));
                let r = MResetZ(target);
                Reset(ctl);
                r
            }
        }
    "#});
}

#[test]
fn adjointable_whole_value_use_preserves_semantics() {
    // An adjointable callable forwards `p` as a whole value; the adjoint
    // specialization must reconstruct the argument so body and adjoint cancel.
    crate::test_utils::check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { X : Int, Y : Int }

            operation Apply(p : Pair, target : Qubit) : Unit is Adj {
                body ... {
                    if (p.X + p.Y) % 2 == 1 {
                        X(target);
                    }
                }
                adjoint self;
            }

            operation Forward(p : Pair, target : Qubit) : Unit is Adj {
                body ... {
                    let _ = p.X;
                    Apply(p, target);
                }
                adjoint self;
            }

            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                Forward(new Pair { X = 1, Y = 2 }, target);
                Adjoint Forward(new Pair { X = 1, Y = 2 }, target);
                MResetZ(target)
            }
        }
    "#});
}

#[cfg(feature = "slow-proptest-tests")]
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

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn tuple_parameter_argument_promotion_preserves_semantics(source in tuple_parameter_argument_pattern()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[cfg(feature = "slow-proptest-tests")]
fn qsharp_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[cfg(feature = "slow-proptest-tests")]
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

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn nested_mixed_struct_callable_arg_promotion_preserves_semantics(
        source in nested_mixed_struct_callable_strategy()
    ) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}
