// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A buried `return` short-circuits before the surrounding work runs.
//!
//! Before the operand lift, a `return` buried in an eagerly-evaluated operand
//! still let the enclosing operator, access, or call execute first — dividing
//! by zero, indexing out of range, or running a sibling operand's quantum
//! effects. These value and trace locks assert the lifted program matches the
//! untransformed one, which short-circuits at the buried `return`.

use super::*;

#[test]
fn operand_return_in_divisor_short_circuits_before_division() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 10 / { return 7; 0 };
                x
            }
        }
    "#});
}

#[test]
fn operand_return_in_index_short_circuits_before_access() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20];
                let x = arr[{ return 1; 5 }];
                x
            }
        }
    "#});
}

#[test]
fn operand_return_in_call_arg_short_circuits_before_call() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Divide(a : Int, b : Int) : Int { a / b }
            function Main() : Int {
                Divide(10, { return 7; 0 })
            }
        }
    "#});
}

#[test]
fn operand_return_skips_sibling_operand_quantum_effects() {
    // `{ return 5; 0 } + { X(q); Reset(q); 0 }`: the left operand returns
    // before the right operand is evaluated, so the `X`/`Reset` on the right
    // must never run. The return value is `5` whether or not those effects
    // execute, so this is invisible to value comparison; the trace lock
    // asserts the ordered quantum operations match the untransformed program
    // (which short-circuits the right operand) rather than running the extra
    // gate and reset.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let x = { return 5; 0 } + { X(q); Reset(q); 0 };
                x
            }
        }
    "#});
}

#[test]
fn operand_return_in_short_circuit_and_lhs_short_circuits_before_rhs() {
    // `{ return 7; true } and (1 / 0 == 0)`: the `and` left operand is the only
    // operand the lift rewrites — its RHS is deliberately excluded because the
    // RHS is conditionally evaluated (only when the LHS is true). Here the LHS
    // block fires a `return` before yielding `true`, so the RHS must never run.
    // The return is load-bearing: without it the LHS is `true`, the RHS *would*
    // be evaluated, and `1 / 0` would fault. A lift that wrongly evaluates the
    // RHS eagerly (or fails to short-circuit the return) faults instead of
    // returning `7`, so the value lock catches the RHS-exclusion regression.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let b = { return 7; true } and (1 / 0 == 0);
                if b { 1 } else { 2 }
            }
        }
    "#});
}

#[test]
fn operand_return_in_short_circuit_or_lhs_short_circuits_before_rhs() {
    // `{ return 7; false } or (1 / 0 == 0)`: mirror of the `and` case. The `or`
    // left operand is lifted, the RHS excluded. The LHS block fires a `return`
    // before yielding `false`, so the RHS must never run. The return is load-
    // bearing: without it the LHS is `false`, the RHS *would* be evaluated, and
    // `1 / 0` would fault. A lift that evaluates the RHS eagerly faults instead
    // of returning `7`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let b = { return 7; false } or (1 / 0 == 0);
                if b { 1 } else { 2 }
            }
        }
    "#});
}
