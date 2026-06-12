// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use proptest::prelude::*;

/// Generates syntactically valid Q# programs exercising defunctionalization's
/// key code paths: lambda arguments, partial application, and direct callable
/// references passed to higher-order functions.
fn defunc_pattern_strategy() -> impl Strategy<Value = String> {
    let val = || 0..50i64;

    prop_oneof![
        // 1. Lambda passed as argument to a higher-order function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(x -> x + {a}, {b})
                }}
            }}
        "}),
        // 2. Partial application of a two-argument function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Add(x : Int, y : Int) : Int {{ x + y }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Add({a}, _), {b})
                }}
            }}
        "}),
        // 3. Direct callable reference as argument.
        val().prop_map(|a| formatdoc! {"
            namespace Test {{
                function Double(x : Int) : Int {{ x * 2 }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Double, {a})
                }}
            }}
        "}),
        // 4. Nested higher-order calls: function returning a lambda.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function MakeAdder(n : Int) : Int -> Int {{ x -> x + n }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(MakeAdder({a}), {b})
                }}
            }}
        "}),
    ]
}

/// Generates programs with multi-capture closures where the captures have
/// distinct values and are used in non-commutative operations, ensuring
/// capture ordering is exercised.
fn multi_capture_strategy() -> impl Strategy<Value = String> {
    // Use distinct non-zero values so swapped captures produce a different result.
    (2..20i64, 1..10i64)
        .prop_filter("a must differ from b", |(a, b)| a != b && *b != 0)
        .prop_flat_map(|(a, b)| {
            prop_oneof![
                // Two captures used in non-commutative subtraction.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a - b + x, 0)
                        }}
                    }}
                "}),
                // Two captures used in non-commutative division.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a / b + x, 0)
                        }}
                    }}
                "}),
                // Three captures in position-sensitive expression.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            let c = 1;
                            Apply(x -> (a - b) * c + x, 0)
                        }}
                    }}
                "}),
            ]
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn differential_defunctionalize(source in defunc_pattern_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]
    #[test]
    fn differential_multi_capture_ordering(source in multi_capture_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

/// Regression for the `Multi ⊔ Multi` (nested dispatch on both sides) join: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are themselves dynamic conditionals, and the *same* callable (`X`)
/// reaches the local from both branches under different guards.
///
/// The lattice merge must not deduplicate the false-branch occurrence of `X`
/// by callable identity — doing so drops the `!outer && rb` dispatch arm and
/// makes that path fall through to the outer default (`Z`) instead of applying
/// `X`. The fixture pins `outer == false` (`a` stays |0>) and the false-branch
/// inner guard `rb == One` (`b` is |1>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// applies `Z(q)` (measuring `Zero`), diverging in both return value and effect
/// trace. The guards are pure reads of pre-measured `Result` locals so the
/// fixture isolates the lattice merge from condition-hoisting concerns.
#[test]
fn multi_multi_shared_callable_across_branches_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                // a stays |0> so the outer guard is false; b is |1> so the
                // false-branch inner guard is true — the dispatch arm the
                // identity-dedup would drop.
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             if rb == One { X } else { Y }
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}
