// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use proptest::prelude::*;

/// Regression for controlled dispatch of a *capturing* closure passed to a
/// higher-order operation whose callable parameter is **not** the first
/// argument. The HOF applies `Controlled op(ctls, q)`, so rewrite must nest the
/// closure's captures inside the base input tuple beneath the control register
/// (`([ctls], (q, capture0, capture1))`) rather than appending them as trailing
/// top-level siblings of `([ctls], q)`. A mis-placed capture would either crash
/// downstream control/input splitting or diverge from the original semantics.
///
/// The control qubit is prepared |1> so the controlled rotation actually fires;
/// the captured angles are threaded through a partial application so the closure
/// carries two ordered captures across the control boundary (exercising the
/// multi-capture nesting order, not just placement).
#[test]
fn controlled_capturing_closure_nonzero_param_slot_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation RotOp(a : Double, b : Double, q : Qubit) : Unit is Adj + Ctl {
                Rx(a, q);
                Rz(b, q);
            }
            operation ApplyCtl(ctls : Qubit[], op : Qubit => Unit is Ctl, q : Qubit) : Unit {
                Controlled op(ctls, q);
            }
            @EntryPoint()
            operation Main() : Result {
                use ctl = Qubit();
                use q = Qubit();
                X(ctl);
                let a = 3.141592653589793;
                let b = 1.5707963267948966;
                let op = RotOp(a, b, _);
                ApplyCtl([ctl], op, q);
                return MResetZ(q);
            }
        }
    "#});
}

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
    fn proptest_defunctionalize_preserves_semantics(source in defunc_pattern_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]
    #[test]
    fn proptest_multi_capture_ordering_preserves_semantics(source in multi_capture_strategy()) {
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

/// Regression for the `Single ⊔ Multi` join: a callable-valued local is
/// selected by an outer dynamic `if` whose *true* branch is a single concrete
/// callable (`X`) and whose *false* branch is itself a dynamic conditional that
/// can also yield `X` (under its own guard).
///
/// The lattice merge must not deduplicate the true-branch `X` against the
/// occurrence already present in the false-branch `Multi` — doing so drops the
/// `outer` dispatch arm and reroutes the `outer == true` path through the
/// false-branch's inner guards instead of unconditionally applying `X`. The
/// fixture pins `outer == true` (`a` is |1>) and the false-branch inner guard
/// `rb == One` false (`b` stays |0>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// falls through to the false-branch default `Z(q)` (measuring `Zero`),
/// diverging in both return value and effect trace. The guards are pure reads
/// of pre-measured `Result` locals so the fixture isolates the lattice merge
/// from condition-hoisting concerns.
#[test]
fn single_multi_shared_callable_across_branches_is_equivalent() {
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
                // a is |1> so the outer guard is true — op must be the
                // true-branch `X`; b stays |0> so the false-branch inner guard
                // is false, the arm the identity-dedup would route through.
                X(a);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             X
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for the `Multi ⊔ Multi` join's "unmodified variable" fast path: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are dynamic conditionals that yield the *same set of callables*
/// (`X`/`Z`) but under *different* inner guards (`rb` in the true branch, `rc`
/// in the false branch).
///
/// The merge must not treat the two branches as an unmodified variable just
/// because the callable identities coincide — the guards differ, so keeping the
/// true-branch chain drops the outer condition and reroutes the `outer == false`
/// path through the true branch's `rb` guard instead of the false branch's `rc`
/// guard. The fixture pins `outer == false` (`a` stays |0>), `rb == One`
/// (`b` is |1>), and `rc == Zero` (`c` stays |0>): the original applies `Z(q)`
/// (measuring `Zero`) while the buggy rewrite applies `X(q)` (measuring `One`),
/// diverging in both return value and effect trace.
#[test]
fn multi_multi_same_callables_different_guards_is_equivalent() {
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
                use c = Qubit();
                // a stays |0> (outer guard false); b is |1> (rb == One);
                // c stays |0> (rc == Zero).
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let rc = MResetZ(c);
                let op = if ra == One {
                             if rb == One { X } else { Z }
                         } else {
                             if rc == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Probe: a conditional callable is bound from a guard variable that is then
/// mutated before the callable is applied. The original captures the callable
/// value at binding time (guard true -> `X`); a defunctionalization that
/// re-evaluates the guard at the apply site would read the mutated guard
/// (now false -> `Z`) and diverge.
///
/// The safe-degradation regression asserting the pipeline rejects this rather
/// than silently miscompiling lives in
/// `defunctionalize::tests::guard_var_reassigned_after_binding_degrades_to_dynamic`.
#[test]
fn guard_var_never_reassigned_after_binding_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                X(a);
                let ra = MResetZ(a);
                // `flag` is mutable but never reassigned after the binding, so
                // hoisting its read to the apply site is safe and dispatch is
                // preserved.
                mutable flag = ra == One;
                let op = if flag { X } else { Z };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}
