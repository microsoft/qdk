// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

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

/// Evaluates the entry exec graph of the given FIR store with a fixed
/// simulator seed for determinism. Returns `Ok(value)` on success, or
/// `Err(error_string)` on evaluation failure.

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

// All 4 specializations with flag strategy (for-loop desugar)

#[test]
fn qubit_alloc_scope_with_flag_strategy_semantic() {
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

// Qubit alloc scope + flag strategy — release continuations are guarded

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
fn if_expr_init_with_while_return_uses_flag_strategy_semantic() {
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
fn simple_if_expr_init_with_return_stays_structured_semantic() {
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
fn flag_strategy_guards_local_after_return_semantic() {
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
//   - guard_stmt_with_flag_rejects_non_unit_expr_stmt (#[should_panic])
//   - flag_trailing_without_trailing_expr_rejects_non_unit_contract (#[should_panic])
//   - unsupported_return_slot_default_in_flag_strategy_produces_error (expects error list)
//   - unsupported_guarded_local_default_in_flag_strategy_is_explicit_contract (#[should_panic])
//   - qubit_return_in_while_produces_error (expects error list)
//
// Specialization tests (Adj/Ctl, no single entry point output):
//   - explicit_specialization_bodies_are_return_unified
//   - simulatable_intrinsic_body_is_return_unified
//   - all_four_specializations_with_return_in_loop
//
// Arrow-typed return tests (blocked by defunctionalization limitation):
//   - arrow_typed_return_in_structured_path
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
fn single_trailing_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                return 42;
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
fn multiple_guard_clauses_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                if false {
                    return 2;
                }
                if true {
                    return 3;
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
fn return_in_nested_block_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 10;
                }
                5
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
fn while_return_tuple_value_uses_flag_fallback_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                mutable i = 0;
                while i < 3 {
                    if i == 1 {
                        return (i, true);
                    }
                    i += 1;
                }
                (-1, false)
            }
        }
    "#});
}

#[test]
fn while_return_array_value_uses_flag_fallback_semantic() {
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
fn while_local_initializer_if_return_is_rewritten_by_flag_strategy_semantic() {
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
fn unit_returning_with_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Unit {
                if true {
                    return ();
                }
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
fn return_in_else_branch_only_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    1
                } else {
                    return 2;
                }
            }
        }
    "#});
}

#[test]
fn range_return_default_in_flag_strategy_is_supported_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Range {
                mutable i = 0;
                while i < 1 {
                    return 0..1;
                }
                2..3
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

// Idempotency tests
//
// Verify that running `unify_returns` a second time on already-transformed
// FIR is a no-op: no new arena entries (blocks, stmts, exprs, pats) are
// allocated, and no errors are produced.

/// Helper: compile through `return_unify`, then run `unify_returns` again and
/// assert that the package arenas are unchanged (no new IDs allocated).

#[test]
fn triple_nested_if_return_with_else_return_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if 0 > 0 {
                    if 0 > 0 {
                        if 0 > 0 { return 0; }
                        return 0;
                    }
                    0
                } else {
                    return 0;
                }
            }
        }
    "#});
}
