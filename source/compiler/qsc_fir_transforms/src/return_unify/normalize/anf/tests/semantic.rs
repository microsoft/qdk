// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The lifted spine returns the same value as the untransformed program.
//!
//! A `return` buried in an eagerly-evaluated operand short-circuits before the
//! surrounding operator, access, or sibling operand runs. Each fixture pairs
//! the buried `return` with a sibling that would fault (index out of range,
//! array-repeat with an invalid size, divide by zero) if the lift failed to
//! short-circuit, so an equal Ok/Err result witnesses that the lifted spine
//! preserves the original early-return value behavior.

use super::*;

#[test]
fn return_in_first_tuple_element_short_circuits_before_sibling_out_of_range() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20];
                let (a, b) = ({ return 7; 0 }, arr[5]);
                a + b
            }
        }
    "#});
}

#[test]
fn dead_return_in_tuple_operand_of_controlled_call_preserves_effect() {
    // `Controlled Foo([c], ({ if false { return One; } 0 }, q))` — the operand
    // is a functor-applied (`Controlled`) call whose tuple argument has a
    // statement-carrying Block element burying a DEAD return. The block must be
    // lifted out of the tuple operand without disturbing the functor
    // application or the qubit sibling. With `c` prepared to |1>, the
    // `Controlled X` inside `Foo` fires on `q`, so a correct lift yields
    // `M(q) == One`; a mishandled operand would drop the call and diverge.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Foo(pair : (Int, Qubit)) : Unit is Ctl {
                body ... {
                    let (_, qq) = pair;
                    X(qq);
                }
                controlled (ctls, ...) {
                    let (_, qq) = pair;
                    Controlled X(ctls, qq);
                }
            }
            operation Main() : Result {
                use (c, q) = (Qubit(), Qubit());
                X(c);
                Controlled Foo([c], ({ if false { return One; } 0 }, q));
                M(q)
            }
        }
    "#});
}

#[test]
fn firing_return_in_tuple_operand_of_controlled_call_short_circuits() {
    // Same shape, but the buried return FIRES before the `Controlled Foo` call
    // runs, so `Foo` is never invoked and `q` stays |0>. The early-return value
    // `One` is observed (the `(Ok, Ok)` arm) iff the lift short-circuits the
    // whole functor-applied call exactly as the untransformed program does.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Foo(pair : (Int, Qubit)) : Unit is Ctl {
                body ... {
                    let (_, qq) = pair;
                    X(qq);
                }
                controlled (ctls, ...) {
                    let (_, qq) = pair;
                    Controlled X(ctls, qq);
                }
            }
            operation Main() : Result {
                use (c, q) = (Qubit(), Qubit());
                X(c);
                Controlled Foo([c], ({ if true { return One; } 0 }, q));
                M(q)
            }
        }
    "#});
}

#[test]
fn return_in_first_array_element_short_circuits_before_sibling_out_of_range() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20];
                let xs = [{ return 7; 0 }, arr[5]];
                xs[0]
            }
        }
    "#});
}

#[test]
fn return_in_array_repeat_value_short_circuits_before_size_divide_by_zero() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let xs = [{ return 7; 0 }, size = 10 / 0];
                xs[0]
            }
        }
    "#});
}

#[test]
fn return_in_binop_lhs_short_circuits_before_rhs_divide_by_zero() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = { return 7; 0 } + 10 / 0;
                x
            }
        }
    "#});
}

#[test]
fn return_in_unop_operand_short_circuits_before_negation() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = -{ return 7; 3 };
                x
            }
        }
    "#});
}

#[test]
fn return_in_range_start_short_circuits_before_sibling_out_of_range() {
    // `{ return 7; 1 }..arr[5]` — the range start returns before the end
    // operand `arr[5]` (out of range) is evaluated, so the lifted spine yields
    // the early-return value rather than faulting on the sibling index.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20];
                let r = { return 7; 1 }..arr[5];
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
fn return_in_struct_field_short_circuits_before_sibling_out_of_range() {
    // `new Pair { First = { return 7; 0 }, Second = arr[5] }` — the first field
    // returns before the sibling field `arr[5]` (out of range) is evaluated, so
    // the lifted spine yields the early-return value rather than faulting.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let arr = [10, 20];
                let p = new Pair { First = { return 7; 0 }, Second = arr[5] };
                p.First + p.Second
            }
        }
    "#});
}

#[test]
fn return_in_struct_copy_receiver_short_circuits_before_sibling_out_of_range() {
    // `new Pair { ...{ return 7; base }, First = arr[5] }` — the copy receiver
    // returns before the field value `arr[5]` (out of range) is evaluated, so
    // the lifted spine yields the early-return value rather than faulting.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let arr = [10, 20];
                let base = new Pair { First = 1, Second = 2 };
                let p = new Pair { ...{ return 7; base }, First = arr[5] };
                p.First
            }
        }
    "#});
}

#[test]
fn return_in_qubit_array_element_short_circuits_before_sibling_out_of_range() {
    // `[{ return 5; q }, [q][9]]` — the first array element returns before the
    // sibling element `[q][9]` (out of range) is evaluated. The lifted operand
    // block has value type `Qubit`, so its temp is array-backed; the early
    // return must still yield `5` rather than faulting on the sibling index.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                use q = Qubit();
                let xs = [{ return 5; q }, [q][9]];
                Length(xs)
            }
        }
    "#});
}

#[test]
fn return_in_tuple_qubit_temp_short_circuits_before_sibling_divide_by_zero() {
    // `[{ return 5; (q, 0) }, (q, 10 / 0)]` — the first array element (a tuple
    // `(Qubit, Int)`) returns before the sibling tuple's `10 / 0` is evaluated.
    // The lifted operand temp has value type `(Qubit, Int)` and is array-backed;
    // the early return must yield `5` rather than faulting on the divide.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                use q = Qubit();
                let xs = [{ return 5; (q, 0) }, (q, 10 / 0)];
                Length(xs)
            }
        }
    "#});
}

// The following fixtures pin operand-position lifts that mutate or read through
// an assignment/receiver slot. Each buries a `return` in the slot under test
// and follows the mutated/read value with a trailing `… + 100`. The buried
// `return` fires, so the early-return value (not the `… + 100` fall-through) is
// the observable result: an equal value witnesses that the lifted spine
// short-circuits before the surrounding assignment or access runs, exactly as
// the untransformed program does.

#[test]
fn return_in_assign_rhs_short_circuits_before_assignment() {
    // `set x = { return 1; 5 }` — the Assign RHS returns before `x` is written,
    // so the early-return value `1` is observed rather than the `x + 100`
    // fall-through.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 0;
                set x = { return 1; 5 };
                x + 100
            }
        }
    "#});
}

#[test]
fn return_in_assignop_rhs_short_circuits_before_compound_assignment() {
    // `set x += { return 2; 5 }` — the AssignOp RHS returns before the compound
    // update runs, so the early-return value `2` is observed.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 10;
                set x += { return 2; 5 };
                x + 100
            }
        }
    "#});
}

#[test]
fn return_in_assignindex_replacement_short_circuits_before_index_assignment() {
    // `set arr w/= 0 <- { return 3; 5 }` — the AssignIndex replacement slot
    // returns before the element is written, so the early-return value `3` is
    // observed.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable arr = [1, 2, 3];
                set arr w/= 0 <- { return 3; 5 };
                arr[0] + 100
            }
        }
    "#});
}

#[test]
fn return_in_assignindex_index_short_circuits_before_index_assignment() {
    // `set arr w/= { return 4; 0 } <- 5` — the AssignIndex index slot returns
    // before the element is written, so the early-return value `4` is observed.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable arr = [1, 2, 3];
                set arr w/= { return 4; 0 } <- 5;
                arr[0] + 100
            }
        }
    "#});
}

#[test]
fn return_in_field_receiver_short_circuits_before_field_access() {
    // `{ return 6; p }.First` — the field-access receiver returns before the
    // field is read, so the early-return value `6` is observed.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let p = new Pair { First = 1, Second = 2 };
                let y = { return 6; p }.First;
                y + 100
            }
        }
    "#});
}

#[test]
fn return_in_update_index_value_short_circuits_before_copy_and_update() {
    // `arr w/ 0 <- { return 7; 5 }` — the UpdateIndex value slot returns before
    // the copy-and-update runs, so the early-return value `7` is observed.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [1, 2, 3];
                let arr2 = arr w/ 0 <- { return 7; 5 };
                arr2[0] + 100
            }
        }
    "#});
}

// The following assign-family fixtures bury a NON-FIRING `return` in the value
// (and AssignIndex index) operand, guarded by a runtime-false flag so the
// assignment write must actually land. Each then reads the mutated binding,
// asserting the surviving write is observed. The buried operand stays a lift
// candidate (it still contains a `return`), so the ANF pass must not pin the
// mutable lvalue place to an immutable by-value copy: doing so would silently
// drop the write and surface the stale pre-assignment value.

#[test]
fn nonfiring_return_in_assign_rhs_preserves_assignment_write() {
    // `set x = { if go { return 7; } 5 }` with `go` false — the write must land,
    // so the trailing read observes `5`, not the stale `0`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 0;
                let go = false;
                set x = { if go { return 7; } 5 };
                x
            }
        }
    "#});
}

#[test]
fn nonfiring_return_in_assignop_rhs_preserves_compound_assignment_write() {
    // `set x += { if go { return 7; } 5 }` with `go` false — the compound update
    // must land, so the trailing read observes `15`, not the stale `10`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable x = 10;
                let go = false;
                set x += { if go { return 7; } 5 };
                x
            }
        }
    "#});
}

#[test]
fn nonfiring_return_in_assignfield_value_preserves_field_write() {
    // `set p w/= First <- { if go { return 7; } 9 }` with `go` false — the field
    // update must land, so the trailing read observes `9`, not the stale `1`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                mutable p = new Pair { First = 1, Second = 2 };
                let go = false;
                set p w/= First <- { if go { return 7; } 9 };
                p::First
            }
        }
    "#});
}

#[test]
fn nonfiring_return_in_assignindex_value_preserves_element_write() {
    // `set arr w/= 0 <- { if go { return 7; } 9 }` with `go` false — the element
    // write must land, so the trailing read observes `9`, not the stale `1`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable arr = [1, 2, 3];
                let go = false;
                set arr w/= 0 <- { if go { return 7; } 9 };
                arr[0]
            }
        }
    "#});
}

#[test]
fn nonfiring_return_in_assignindex_index_preserves_element_write() {
    // `set arr w/= { if go { return 7; } 0 } <- 9` with `go` false — the index
    // operand buries a non-firing return while the place stays untouched, so the
    // element write must land and the trailing read observes `9`, not `1`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable arr = [1, 2, 3];
                let go = false;
                set arr w/= { if go { return 7; } 0 } <- 9;
                arr[0]
            }
        }
    "#});
}

// The following fixtures pin qubit-handle IDENTITY through an array-backed
// operand value: the buried `return` is DEAD (`if false { return … }`), so the
// operand stays a lift candidate and is array-backed (its value type is a
// qubit-bearing tuple/UDT, hence non-defaultable), yet the qubit handle flows
// through the `[t][0]` backing to a downstream gate. Identity is asserted
// observationally (Q# has no surface `===` for qubits): gate through the
// operand value, then measure the ORIGINAL qubit. A lost handle would land the
// gate elsewhere and diverge the value/effect-trace, failing the assert.

#[test]
fn dead_return_in_tuple_qubit_operand_preserves_qubit_identity() {
    // `xs = [{ if false { return Zero; } (q, 0) }, (q, 1)]` — the first array
    // element is a `(Qubit, Int)` Block operand burying a dead return, forcing
    // the array-backed lift. `X(qq)` (with `let (qq, _) = xs[0]`) gates through
    // the array-backed handle; `M(q)` on the ORIGINAL qubit is `One` iff the
    // handle identity survives.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Result {
                use q = Qubit();
                let xs = [{ if false { return Zero; } (q, 0) }, (q, 1)];
                let (qq, _) = xs[0];
                X(qq);
                M(q)
            }
        }
    "#});
}

#[test]
fn dead_return_in_udt_qubit_operand_preserves_qubit_identity() {
    // `ws = [{ if false { return Zero; } new Holder { Q = q, Tag = 0 } }, …]` —
    // the first array element is a `Holder` (pure type `(Qubit, Int)`) Block
    // operand burying a dead return, forcing the array-backed lift. `X(ws[0].Q)`
    // gates through the array-backed handle; `M(q)` is `One` iff identity holds.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Holder { Q : Qubit, Tag : Int }
            operation Main() : Result {
                use q = Qubit();
                let ws = [{ if false { return Zero; } new Holder { Q = q, Tag = 0 } }, new Holder { Q = q, Tag = 1 }];
                X(ws[0].Q);
                M(q)
            }
        }
    "#});
}

// The following fixtures pin `Result` seed-UNOBSERVABILITY. `Result` is
// defaultable (seed `Zero`), so a `Result` operand temp takes the direct Var
// branch seeded by `if not __has_returned { init } else { Zero }`. These pin
// that the `Zero` seed is bound only on the dead path and is never observed on
// a live path.

#[test]
fn dead_return_in_result_operand_does_not_leak_seed_on_live_path() {
    // `X(q)` prepares |1>, so a correct measurement yields `One` (distinct from
    // the seed `Zero`). The first array element buries a DEAD return, so the
    // live path binds `rs[0] = M(q) = One`; if the `Zero` seed ever leaked onto
    // the live path, `rs[0]` would be `Zero`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Result {
                use q = Qubit();
                X(q);
                let rs = [{ if false { return Zero; } M(q) }, M(q)];
                rs[0]
            }
        }
    "#});
}

#[test]
fn firing_return_in_sibling_result_operand_leaves_seed_unobserved() {
    // `rs = [M(q), { return One; M(q) }]` — the first operand `M(q)` is a
    // `Result` temp whose post-return seed (`Zero`) is bound, but the SIBLING
    // fires `return One`, so the function returns `One` before the seed can be
    // observed.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Result {
                use q = Qubit();
                X(q);
                let rs = [M(q), { return One; M(q) }];
                rs[0]
            }
        }
    "#});
}

// The following P1 fixtures bury a `return` in an operand slot that no other
// fixture exercises: the Index TARGET, the UpdateField RECEIVER and VALUE, the
// AssignField VALUE, and the Fail operand. Each buries the `return` ahead of a
// sibling or surrounding operation that would fault or diverge (out-of-range
// index, copy-and-update, field assignment, `fail`) if the lift failed to
// short-circuit, so an equal result witnesses that the lifted spine preserves
// the original early-return value. UpdateField/AssignField use modern `struct`
// syntax; ANF runs before UdtErase, so the field updates are intact at lift
// time.

#[test]
fn return_in_index_target_short_circuits_before_out_of_range_access() {
    // `({ return 1; arr })[5]` — the Index TARGET operand returns before the
    // out-of-range `[5]` access runs, so the early-return value `1` is observed
    // rather than faulting on the sibling index.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20, 30];
                ({ return 1; arr })[5]
            }
        }
    "#});
}

#[test]
fn return_in_update_field_receiver_short_circuits_before_copy_and_update() {
    // `({ return 7; p }) w/ First <- 9` — the UpdateField RECEIVER operand
    // returns before the copy-and-update runs, so the early-return value `7` is
    // observed rather than the updated record.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let p = new Pair { First = 1, Second = 2 };
                let q = ({ return 7; p }) w/ First <- 9;
                q::First
            }
        }
    "#});
}

#[test]
fn return_in_update_field_value_short_circuits_before_copy_and_update() {
    // `p w/ First <- { return 7; 9 }` — the UpdateField VALUE operand returns
    // before the copy-and-update runs, so the early-return value `7` is observed
    // rather than the updated record.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let p = new Pair { First = 1, Second = 2 };
                let q = p w/ First <- { return 7; 9 };
                q::First
            }
        }
    "#});
}

#[test]
fn return_in_assign_field_value_short_circuits_before_field_assignment() {
    // `set p w/= First <- { return 7; 9 }` — the AssignField VALUE operand
    // returns before the field is written, so the early-return value `7` is
    // observed rather than the mutated field.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                mutable p = new Pair { First = 1, Second = 2 };
                set p w/= First <- { return 7; 9 };
                p::First
            }
        }
    "#});
}

#[test]
fn return_in_fail_operand_short_circuits_before_fail() {
    // `fail { return 1; "msg" }` — the Fail operand returns before `fail` runs,
    // so the early-return value `1` is observed (the `(Ok, Ok)` arm) rather than
    // the program diverging.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                fail { return 1; "msg" }
            }
        }
    "#});
}

#[test]
fn bare_fail_diverges_identically() {
    // `fail "msg"` with no buried return — both the original and transformed
    // programs fail identically, exercising the `(Err, Err)` panic-parity arm of
    // `check_semantic_equivalence`. The `fail` runs as a statement ahead of an
    // `Int`-typed `0` tail so the program type-checks without a bare-`fail` body
    // (the buried-return short-circuit form is covered above).
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                fail "msg";
                0
            }
        }
    "#});
}

// The following P2 fixtures complete the operand-slot coverage for the three
// eagerly-evaluated slots that previously had only a spine-shape snapshot
// (isolation.rs `isolated_anf_lifts_array_repeat_size_block`,
// `isolated_anf_lifts_range_end_block`, and the Range STEP slot): the
// ArrayRepeat SIZE, the Range END, and the Range STEP operand. Each buries a
// NON-FIRING `return` (guarded by a runtime-false `go` flag) in the targeted
// slot so the operand stays a lift candidate yet the value is actually
// consumed; the trailing read then observes the consumed size/iteration. An
// equal value and effect-trace witness that the lifted spine feeds the original
// operand value into the surrounding ArrayRepeat/Range exactly as the
// untransformed program does.

#[test]
fn nonfiring_return_in_array_repeat_size_preserves_array_length() {
    // `[0, size = { if go { return 7; } 3 }]` with `go` false — the SIZE operand
    // buries a non-firing return, so the array is built with size `3` and the
    // trailing `Length` observes `3`, not the early-return `7`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let go = false;
                let a = [0, size = { if go { return 7; } 3 }];
                Length(a)
            }
        }
    "#});
}

#[test]
fn nonfiring_return_in_range_end_preserves_iteration() {
    // `0..{ if go { return 7; } 5 }` with `go` false — the END operand buries a
    // non-firing return, so the range `0..5` is iterated in full and the trailing
    // sum observes `0+1+2+3+4+5 = 15`, not the early-return `7`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let go = false;
                let r = 0..{ if go { return 7; } 5 };
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
fn nonfiring_return_in_range_step_preserves_iteration() {
    // `0..{ if go { return 7; } 2 }..10` with `go` false — the STEP operand
    // buries a non-firing return, so the range `0..2..10` is iterated in full and
    // the trailing sum observes `0+2+4+6+8+10 = 30`, not the early-return `7`.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let go = false;
                let r = 0..{ if go { return 7; } 2 }..10;
                mutable total = 0;
                for i in r {
                    set total += i;
                }
                total
            }
        }
    "#});
}
