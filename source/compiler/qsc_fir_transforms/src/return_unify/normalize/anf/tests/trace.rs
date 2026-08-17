// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The lifted spine performs the same ordered quantum effects as the original.
//!
//! When a `return` is buried in one operand, the sibling operands (and, for a
//! lifted `if` condition, both branches) are dead on the return path and must
//! not run. Their quantum effects are invisible to the return value, so these
//! locks compare the ordered `TraceOp` sequence of the transformed program to
//! the untransformed one, which short-circuits at the buried `return`.

use super::*;

#[test]
fn range_struct_and_string_prefixes_preserve_order() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            @EntryPoint()
            operation Main() : Int {
                use rangePrefix = Qubit();
                use structPrefix = Qubit();
                let range = {
                    X(rangePrefix);
                    Reset(rangePrefix);
                    1
                }..{ return 7; 2 }..3;
                let pair = new Pair {
                    First = {
                        X(structPrefix);
                        Reset(structPrefix);
                        4
                    },
                    Second = { return 8; 5 }
                };
                range.Start + pair.First
            }
        }
    "#});
}

#[test]
fn controlled_adjoint_nondefaultable_prefix_preserves_shape() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Target(q : Qubit) : Unit is Adj + Ctl {
                body ... { X(q); }
                adjoint auto;
                controlled (controls, ...) { Controlled X(controls, q); }
                controlled adjoint auto;
            }
            @EntryPoint()
            operation Main() : Int {
                use control = Qubit();
                use target = Qubit();
                X(control);
                (Controlled Adjoint Target)([control], { return 7; target });
                Reset(control);
                Reset(target);
                0
            }
        }
    "#});
}

#[test]
fn return_in_call_arg_skips_sibling_arg_quantum_effects() {
    // `Add({ return 5; 0 }, { X(q); Reset(q); 0 })`: the first argument returns
    // before the second is evaluated, so the `X`/`Reset` on the right must
    // never run. The return value is `5` either way, so the trace lock — not a
    // value comparison — catches a lift that fails to short-circuit the sibling
    // argument's effects.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let x = Add({ return 5; 0 }, { X(q); Reset(q); 0 });
                x
            }
        }
    "#});
}

#[test]
fn return_in_first_tuple_element_skips_sibling_element_quantum_effects() {
    // `({ return 5; 0 }, { X(q); Reset(q); 0 })`: the first element returns
    // before the second is evaluated, so the right element's `X`/`Reset` must
    // never run. The trace lock asserts the ordered quantum operations match
    // the untransformed program rather than running the extra gate and reset.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let (a, b) = ({ return 5; 0 }, { X(q); Reset(q); 0 });
                a + b
            }
        }
    "#});
}

#[test]
fn return_in_if_condition_skips_both_branch_quantum_effects() {
    // `if { return 5; true } { X(q); 1 } else { Reset(q); 2 }`: the condition
    // block returns before either branch is selected, so neither the `X` in the
    // then-branch nor the `Reset` in the else-branch must run. The condition is
    // an unconditional operand position, so the lift binds it to a spine temp;
    // the trace lock confirms both branch effects are short-circuited.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let x = if { return 5; true } { X(q); 1 } else { Reset(q); 2 };
                x
            }
        }
    "#});
}

#[test]
fn return_in_range_start_skips_sibling_endpoint_quantum_effects() {
    // `{ return 5; 1 }..{ X(q); Reset(q); 2 }`: the range start returns before
    // the end endpoint is evaluated, so the `X`/`Reset` building the end must
    // never run. The trace lock confirms the sibling endpoint's effects are
    // short-circuited.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let r = { return 5; 1 }..{ X(q); Reset(q); 2 };
                mutable total = 0;
                for i in r {
                    set total += i;
                }
                total
            }
        }
    "#});
}

#[test]
fn return_in_struct_field_skips_sibling_field_quantum_effects() {
    // `new Pair { First = { return 5; 0 }, Second = { X(q); Reset(q); 0 } }`:
    // the first field returns before the second is evaluated, so the second
    // field's `X`/`Reset` must never run. The trace lock confirms the sibling
    // field's effects are short-circuited.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let p = new Pair { First = { return 5; 0 }, Second = { X(q); Reset(q); 0 } };
                p.First + p.Second
            }
        }
    "#});
}

#[test]
fn return_in_qubit_array_element_skips_sibling_element_quantum_effects() {
    // `[{ return 5; q }, { X(q2); Reset(q2); q2 }]`: the first array element
    // returns before the second is evaluated, so the second element's
    // `X`/`Reset` must never run. The first element's value type is `Qubit`, so
    // its lifted operand temp is array-backed; the trace lock confirms the
    // sibling element's effects are short-circuited.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                use q2 = Qubit();
                let xs = [{ return 5; q }, { X(q2); Reset(q2); q2 }];
                Length(xs)
            }
        }
    "#});
}

#[test]
fn earlier_sibling_operands_evaluate_in_source_order_before_lift() {
    // `Pick({ X(q1); 1 }, { Reset(q2); 2 }, { return 5; 3 })`: the buried
    // `return` sits in the *last* operand, so the two *earlier* operands `A` and
    // `B` are live and must both run, in source order, before the lift
    // short-circuits. Lifting the last operand pins each earlier sibling to its
    // own spine temp first, so this lock proves the pinning preserves
    // left-to-right evaluation: the only effects are `A = X(q1)` then
    // `B = Reset(q2)`, and the trace must record `[X(q1), Reset(q2)]` in that
    // order. A lift that reordered or dropped a pinned sibling would diverge
    // from the untransformed trace, which runs `A` then `B` then returns `5`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Pick(a : Int, b : Int, c : Int) : Int { c }
            @EntryPoint()
            operation Main() : Int {
                use q1 = Qubit();
                use q2 = Qubit();
                let x = Pick({ X(q1); 1 }, { Reset(q2); 2 }, { return 5; 3 });
                x
            }
        }
    "#});
}

#[test]
fn nested_return_preserves_effectful_outer_callee() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Apply(pair : (Int, Int)) : Unit {}

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                ({ X(q); Reset(q); Apply })((0, { return 5; 1 }));
                0
            }
        }
    "#});
}

#[test]
fn update_field_evaluates_replacement_before_record() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }

            @EntryPoint()
            operation Main() : Int {
                use recordQubit = Qubit();
                use replacementQubit = Qubit();
                let pair = new Pair { First = 1, Second = 2 };
                let updated = {
                    X(recordQubit);
                    Reset(recordQubit);
                    pair
                } w/ First <- {
                    X(replacementQubit);
                    Reset(replacementQubit);
                    return 7;
                    9
                };
                updated.First
            }
        }
    "#});
}

#[test]
fn update_index_evaluates_index_and_replacement_before_container() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use indexQubit = Qubit();
                use replacementQubit = Qubit();
                use containerQubit = Qubit();
                let updated = {
                    X(containerQubit);
                    Reset(containerQubit);
                    [1, 2]
                } w/ {
                    X(indexQubit);
                    Reset(indexQubit);
                    0
                } <- {
                    X(replacementQubit);
                    Reset(replacementQubit);
                    return 7;
                    9
                };
                updated[0]
            }
        }
    "#});
}

#[test]
fn return_in_short_circuit_and_lhs_skips_rhs_quantum_effects() {
    // `{ return 5; true } and { X(q); Reset(q); true }`: the `and` left operand
    // fires a `return` before yielding `true`, so the conditionally-evaluated
    // RHS — which the lift deliberately leaves inline rather than hoisting —
    // must never run. The return value is `1` either way, so a lift that
    // evaluated the RHS eagerly is invisible to value comparison; the trace lock
    // catches the stray `X`/`Reset`. The return is load-bearing: without it the
    // LHS is `true`, so the RHS *would* run under normal short-circuit
    // semantics, isolating this from the boolean short-circuit itself.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let b = { return 5; true } and { X(q); Reset(q); true };
                if b { 1 } else { 2 }
            }
        }
    "#});
}
