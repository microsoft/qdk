// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify`]'s `identical_branches`
//! rule (driven by `run_identical_branches`).
//!
//! These tests use [`check_simplify_rule_q`]: a Q# snippet is compiled,
//! the pipeline runs through mono + return-unify-without-simplify, the
//! pre-simplify FIR is snapshotted, the rule is invoked on the named
//! callable's body block, and the post-rule FIR is snapshotted. The
//! before/after snapshots pin the rule's effect against what the
//! lowerer actually emits.
//!
//! `run_identical_branches(package, block_id) -> bool` has a different
//! signature than the standard rule `apply` (it takes no assigner or
//! synth slots), so the harness — which expects
//! `FnOnce(&mut Package, &mut Assigner, BlockId, &SynthSlots) -> bool` —
//! is fed an adapter closure that ignores those arguments. The test
//! module is a descendant of `simplify`, so calling the private
//! `super::super::run_identical_branches` is allowed.
//!
//! The snapshot header records `fired=<bool>` so each case witnesses
//! whether the single-rule pass mutated the block. The impure-condition
//! decline test is the rule-level witness of the H-1 fix: an
//! identical-arm `if` whose condition is not side-effect-free must not
//! be folded (dropping the condition would drop its side effects).

use expect_test::expect;
use indoc::indoc;

use crate::return_unify::tests::check_simplify_rule_q;

#[test]
fn pure_flag_merge_collapses_to_slot_read() {
    // Canonical `if c { return a; } else { return b; }`. The lowerer
    // emits both-arm slot writes plus a trailing pure-flag merge
    // `if __has_returned { __ret_val } else { __ret_val }`. The
    // `identical_branches` rule folds that trailing merge — its
    // condition is a bare `Var(__has_returned)` read (pure) and its
    // arms are structurally identical — down to the bare slot read
    // `__ret_val`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    return 2;
                }
            }
        }
        "#},
        "Main",
        "identical_branches",
        |package, _assigner, package_id, block_id, _slots| {
            super::super::run_identical_branches(package, package_id, block_id)
        },
        &expect![[r#"
            // before identical_branches (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                } else {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                }

                if __has_returned {
                    __ret_val
                } else {
                    __ret_val
                }
            }
            // entry
            Main()

            // after identical_branches
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                } else {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                }

                __ret_val
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn impure_condition_refuses_to_collapse() {
    // H-1 regression witness at the rule level. The user `if`'s
    // condition `{ set c += 1; true }` is not side-effect-free, so the
    // `identical_branches` rule must refuse to fold it even though its
    // arms are structurally identical — folding would drop the
    // condition block and its `set c += 1` side effect. With no
    // early returns there is no pure-flag merge in the block, so the
    // impure `if` is the only fold candidate and `fired=false` is a
    // clean witness that the impure `if` is preserved.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Unit {
                mutable c = 0;
                if { c += 1; true } {
                    c += 2;
                } else {
                    c += 2;
                }
            }
        }
        "#},
        "Main",
        "identical_branches",
        |package, _assigner, package_id, block_id, _slots| {
            super::super::run_identical_branches(package, package_id, block_id)
        },
        &expect![[r#"
            // before identical_branches (fired=false)
            operation Main() : Unit {
                mutable c : Int = 0;
                if {
                    c += 1;
                    true
                }
                {
                    c += 2;
                } else {
                    c += 2;
                }

            }
            // entry
            Main()

            // after identical_branches
            operation Main() : Unit {
                mutable c : Int = 0;
                if {
                    c += 1;
                    true
                }
                {
                    c += 2;
                } else {
                    c += 2;
                }

            }
            // entry
            Main()
        "#]],
    );
}
