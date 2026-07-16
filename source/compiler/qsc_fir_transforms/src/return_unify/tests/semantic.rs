// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Runtime semantic-equivalence locks for return unification.
//!
//! Several fixtures here intentionally re-use the same Q# sources as the
//! structural-snapshot tests in [`super::general`] and
//! [`super::flag_lowering`] (named with a `_semantic` suffix). They are **not**
//! redundant: the snapshot tests pin the *post-transform FIR shape*, while these
//! drive the program through `check_semantic_equivalence`, evaluating both the
//! original and unified programs and asserting they produce the same value or
//! error. A transform that preserved structure but changed runtime behavior
//! (or vice versa) would fail only one of the two suites, so both are kept.

use super::*;

#[test]
fn outer_return_wrapping_if_with_stmt_return_in_else_does_not_loop_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return if M(q) == One {
                    1
                } else {
                    return M(q) == One ? 0 | 1;
                };
            }
        }
    "#});
}

#[test]
fn while_divzero_condition_short_circuits_after_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while 10 / (3 - i) > 0 {
                    i += 1;
                    if i == 3 {
                        return i;
                    }
                }
                -1
            }
        }
    "#});
}

#[test]
fn while_mixed_condition_and_body_returns_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while (if i > 5 { return 99; } else { true }) {
                    i += 1;
                    if i == 3 {
                        return i;
                    }
                }
                -1
            }
        }
    "#});
}

#[test]
fn bare_return_with_dead_code_semantic() {
    // Classical version: exercises the same bare-return + dead-code
    // truncation path without qubit scope asymmetry.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1;
                return 42;
                let y = x + 1;
                y + 2
            }
        }
    "#});
}

#[test]
fn return_after_dynamic_branch_with_dead_code_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Unit {
                use q = Qubit();
                if M(q) == One {
                    X(q);
                } else {
                    H(q);
                }
                H(q);
                return ();
                Y(q);
            }
        }
    "#});
}

#[test]
fn nested_if_with_returns_at_different_levels_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    if false {
                        return 1;
                    }
                    return 2;
                }
                3
            }
        }
    "#});
}

#[test]
fn nested_block_middle_of_block_fix_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let c = true;
                let _unused = {
                    if c { return 1; }
                    2
                };
                let y = 3;
                y
            }
        }
    "#});
}

#[test]
fn hoist_return_in_range_endpoint_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable sum = 0;
                for i in 0..(return 5) {
                    sum += i;
                }
                sum
            }
        }
    "#});
}

#[test]
fn return_bool_in_dynamic_branch_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Bool {
                use q = Qubit();
                if M(q) == One {
                    return true;
                }
                false
            }
        }
    "#});
}

#[test]
fn return_unit_after_side_effects_semantic() {
    // Classical version: exercises the same early-return-unit + remaining
    // side-effects path without qubit scope asymmetry.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Unit {
                mutable x = 0;
                if x == 0 {
                    x = 1;
                    return ();
                }
                x = 2;
            }
        }
    "#});
}

#[test]
fn both_branches_return_with_qubit_scope_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Bool {
                use q = Qubit();
                let r = M(q);
                Reset(q);
                if r == One {
                    return true;
                } else {
                    return false;
                }
            }
        }
    "#});
}

#[test]
fn for_loop_with_early_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                for i in 0..10 {
                    if i == 5 {
                        return i;
                    }
                }
                -1
            }
        }
    "#});
}

#[test]
fn deeply_nested_block_with_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = {
                    if true {
                        return 10;
                    }
                    5
                };
                x
            }
        }
    "#});
}

#[test]
fn multiple_returns_in_helper_function_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Classify(x : Int) : Int {
                if x > 0 {
                    return 1;
                }
                if x < 0 {
                    return -1;
                }
                0
            }
            function Main() : Int {
                Classify(5)
            }
        }
    "#});
}

#[test]
fn guard_clause_with_existing_else_and_remaining_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    let _ = 0;
                }
                2
            }
        }
    "#});
}

#[test]
fn return_tuple_value_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                if true {
                    return (1, true);
                }
                (0, false)
            }
        }
    "#});
}

// Recursive function with early return

#[test]
fn recursive_function_with_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Factorial(n : Int) : Int {
                if n <= 1 {
                    return 1;
                }
                n * Factorial(n - 1)
            }
            function Main() : Int {
                Factorial(5)
            }
        }
    "#});
}

// Tuple return + while + nested if

#[test]
fn tuple_return_in_while_with_nested_if_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                mutable i = 0;
                while i < 10 {
                    if i > 5 {
                        if i == 7 {
                            return (i, true);
                        }
                    }
                    i += 1;
                }
                (-1, false)
            }
        }
    "#});
}

// All 4 specializations with flag lowering (for-loop desugar)

#[test]
fn qubit_alloc_scope_with_flag_lowering_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 5 {
                    use q = Qubit();
                    if i == 3 {
                        return i;
                    }
                    i += 1;
                }
                -1
            }
        }
    "#});
}

// repeat-until + return (desugared to while at HIR)

#[test]
fn repeat_until_with_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable result = 0;
                mutable attempt = 0;
                repeat {
                    if attempt > 3 {
                        return -1;
                    }
                    attempt += 1;
                    result = attempt * 2;
                } until result > 5;
                result
            }
        }
    "#});
}

// fail + return in same control flow

#[test]
fn while_body_side_effect_guarded_after_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable sum = 0;
                mutable i = 0;
                while i < 10 {
                    if i == 3 {
                        return sum;
                    }
                    sum += i;
                    i += 1;
                }
                sum
            }
        }
    "#});
}

// Qubit alloc scope + flag lowering — release continuations are guarded

#[test]
fn qubit_release_guarded_in_for_loop_with_early_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable result = 0;
                for i in 0..4 {
                    use q = Qubit();
                    if i == 3 {
                        result = i;
                        return result;
                    }
                }
                result
            }
        }
    "#});
}

#[test]
fn body_level_qubit_release_guarded_with_while_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                while i < 10 {
                    if i == 3 {
                        Reset(q);
                        return i;
                    }
                    i += 1;
                }
                Reset(q);
                0
            }
        }
    "#});
}

#[test]
fn post_loop_qubit_allocation_skipped_after_early_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Foo(early : Bool) : Int {
                mutable i = 0;
                while i < 1 {
                    if early {
                        return 7;
                    }
                    i += 1;
                }
                use q = Qubit();
                if early {
                    fail "post-loop qubit path should be skipped";
                }
                Reset(q);
                11
            }

            @EntryPoint()
            operation Main() : (Int, Int) {
                (Foo(true), Foo(false))
            }
        }
    "#});
}

#[test]
fn recursive_while_body_qubit_suffix_skipped_after_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation MustNotRun() : Unit {
                fail "recursive while suffix executed";
            }

            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                    use q = Qubit();
                    MustNotRun();
                    Reset(q);
                }
                0
            }
        }
    "#});
}

#[test]
fn recursive_nested_block_qubit_suffix_skipped_after_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation MustNotRun() : Unit {
                fail "recursive nested suffix executed";
            }

            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    {
                        return 1;
                        use q = Qubit();
                        MustNotRun();
                        Reset(q);
                    };
                    i += 1;
                }
                0
            }
        }
    "#});
}

#[test]
fn final_trailing_side_effect_skipped_after_flag_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation MustNotRun() : Int {
                fail "final trailing expression executed";
                0
            }

            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                MustNotRun()
            }
        }
    "#});
}

#[test]
fn array_of_udt_wrapping_qubit_side_effecting_tail_skipped_after_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            newtype Wrapped = Qubit;

            operation Observe(values : Wrapped[]) : Int {
                fail "tail should not run after return";
                0
            }

            operation Foo(q : Qubit) : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                let values = [Wrapped(q)];
                Observe(values)
            }

            operation Main() : Int {
                use q = Qubit();
                Foo(q)
            }
        }
    "#});
}

#[test]
fn qubit_return_in_while_uses_array_backed_return_slot_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation Pick(q : Qubit) : Qubit {
                mutable i = 0;
                while i < 1 {
                    return q;
                }
                q
            }

            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let returned = Pick(q);
                X(returned);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn tuple_with_qubit_return_in_while_uses_array_backed_return_slot_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation Pick(q : Qubit) : (Qubit, Int) {
                mutable i = 0;
                while i < 1 {
                    return (q, 7);
                }
                (q, 0)
            }

            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let (returned, tag) = Pick(q);
                if tag == 7 {
                    X(returned);
                }
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn udt_wrapping_qubit_return_in_while_uses_array_backed_return_slot_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            newtype Wrapped = Qubit;

            operation Pick(q : Qubit) : Wrapped {
                mutable i = 0;
                while i < 1 {
                    return Wrapped(q);
                }
                Wrapped(q)
            }

            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let returned = Pick(q)!;
                X(returned);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn if_expr_init_with_while_return_uses_flag_lowering_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = if true {
                    mutable i = 0;
                    while i < 5 {
                        if i == 3 {
                            return 42;
                        }
                        i += 1;
                    }
                    0
                } else {
                    1
                };
                x
            }
        }
    "#});
}

#[test]
fn simple_if_expr_init_with_return_recovers_structured_branch_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 10;
                }
                let x = if false { return 20; } else { 30 };
                x
            }
        }
    "#});
}

#[test]
fn flag_lowering_guards_local_after_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while i < 5 {
                    if i == 3 {
                        return i;
                    }
                    let y = i * 2;
                    i += 1;
                }
                -1
            }
        }
    "#});
}

// Tests excluded from semantic comparison (no `_semantic` companion):
//
// Error-contract tests (test panics/errors, not values):
//   - guard_stmt_with_flag_panics_on_non_unit_expr_stmt (asserts panic)
//   - flag_trailing_without_trailing_expr_rejects_non_unit_contract (asserts panic)
//   - recursive_udt_early_return_fails_before_return_unify (expects error list)
//
// Specialization tests (Adj/Ctl, no single entry point output):
//   - explicit_specialization_bodies_are_return_unified
//   - simulatable_intrinsic_body_is_return_unified
//   - all_four_specializations_with_return_in_loop
//
// No-return or identity tests (no transform to validate):
//   - no_op_function_without_returns
//   - already_normalized_idempotency
//   - lowered_reachable_callables_do_not_emit_while_local_initializers (no returns in source)
//
// Non-standard compilation flow (synthetic FIR or direct transform call):
//   - synthetic_while_local_initializer_shape_still_eliminates_returns
//   - nested_block_with_while_return_not_transformable_by_if_else
//
// Structural comparison only (compares two sources, not runtime values):
//   - classify_semi_return_and_expr_return_produce_same_shape

#[test]
fn arrow_typed_return_simplifies_to_if_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Choose(flag : Bool) : (Int -> Int) {
                if flag {
                    return x -> x + 1;
                }
                x -> x * 2
            }

            function Main() : Int {
                let f = Choose(true);
                f(10)
            }
        }
    "#});
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Choose(flag : Bool) : (Int -> Int) {
                if flag {
                    return x -> x + 1;
                }
                x -> x * 2
            }

            function Main() : Int {
                let f = Choose(false);
                f(10)
            }
        }
    "#});
}

#[test]
fn aggregate_arrow_typed_return_simplifies_to_if_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Choose(flag : Bool) : ((Int -> Int), Int) {
                if flag {
                    return (x -> x + 1, 100);
                }
                (x -> x * 2, 7)
            }

            function Main() : (Int, Int) {
                let (trueF, trueOffset) = Choose(true);
                let (falseF, falseOffset) = Choose(false);
                (trueF(10) + trueOffset, falseF(10) + falseOffset)
            }
        }
    "#});
}

#[test]
fn aggregate_arrow_typed_return_udt_field_access_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            newtype Choice = (F : Int -> Int, Offset : Int);
            function Choose(flag : Bool) : Choice {
                if flag { return Choice(x -> x + 1, 100); }
                Choice(x -> x * 2, 7)
            }
            function Main() : Int {
                let selected = Choose(true);
                let f = selected::F;
                f(10) + selected::Offset
            }
        }
    "#});
}

#[test]
fn guard_clause_pattern_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                0
            }
        }
    "#});
}

#[test]
fn both_branches_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    return 2;
                }
            }
        }
    "#});
}

#[test]
fn return_inside_while_loop_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while i < 10 {
                    if i == 5 {
                        return i;
                    }
                    i += 1;
                }
                -1
            }
        }
    "#});
}

#[test]
fn while_return_array_value_via_flag_transform_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int[] {
                mutable i = 0;
                while i < 3 {
                    if i == 1 {
                        return [i, i + 1];
                    }
                    i += 1;
                }
                []
            }
        }
    "#});
}

#[test]
fn while_local_initializer_if_return_is_rewritten_by_flag_lowering_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                mutable i = 0;
                while i < 3 {
                    let _ = if i == 1 {
                        Add((return 42), i)
                    };
                    i += 1;
                }
                i + 5
            }
        }
    "#});
}

#[test]
fn while_local_initializer_if_else_return_preserves_fallthrough_tail_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                mutable i = 0;
                while i < 3 {
                    let x = if i == 1 {
                        Add((return 7), i)
                    } else {
                        i + 10
                    };
                    i += x;
                }
                let tail = i + 5;
                tail
            }
        }
    "#});
}

#[test]
fn nested_loop_exit_convergence_is_guarded_by_flag_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable outer = 0;
                mutable inner = 0;
                while outer < 2 {
                    while inner < 2 {
                        if inner == 1 {
                            return outer + inner;
                        }
                        inner += 1;
                    }
                    outer += 1;
                    inner = 0;
                }
                -1
            }
        }
    "#});
}

#[test]
fn while_body_call_arg_return_keeps_loop_before_trailing_merge_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }

            function Main() : Int {
                mutable i = 0;
                while i < 3 {
                    let _ = Add((return 42), 2);
                    i += 1;
                }
                -1
            }
        }
    "#});
}

#[test]
fn return_value_is_complex_expression_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                if true {
                    return Add(1, 2) + Add(3, 4);
                }
                0
            }
        }
    "#});
}

#[test]
fn fail_and_return_in_same_control_flow_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let c = true;
                if c {
                    return 42;
                } else {
                    fail "unreachable";
                }
            }
        }
    "#});
}

// Quantum semantic companions (added after qubit-scope semantic fix)

#[test]
fn nested_qubit_scope_return_updates_outer_block_type_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation Main() : Result {
                use outer = Qubit() {
                    use qubit = Qubit() {
                        let result = MResetZ(qubit);
                        Reset(outer);
                        return result;
                    }
                }
            }
        }
    "#});
}

#[test]
fn early_return_in_qubit_array_scope_preserves_release_order_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use qs = Qubit[2];
                if flag {
                    return 1;
                }
                0
            }

            operation Main() : Int {
                Foo(true)
            }
        }
    "#});
}

#[test]
fn impure_if_condition_preserved_when_branches_identical_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable c = 0;
                if { c += 1; true } { return c; } else { return c; }
            }
        }
    "#});
}
