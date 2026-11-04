// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Integration tests for [`crate::return_unify::simplify::run_to_fixpoint`].
//!
//! Tests in this suite are **Q#-driven** via [`check_simplify_rule_q`]:
//! a Q# snippet is compiled, the pipeline runs through mono +
//! return-unify-without-simplify, the pre-simplify FIR is snapshotted,
//! `run_to_fixpoint` is applied to `Main`'s body block, and the
//! post-rule FIR is snapshotted. The before/after snapshots pin the
//! driver's end-to-end effect against what the lowerer actually emits,
//! so the test inputs cannot drift from the canonical user shapes.
//! These tests lock in **rule integration** — a regression in the
//! simplifier driver's ordering, fixpoint termination, or rule
//! activation would only surface as drift in the broader snapshot
//! suites, but it would show here as a direct snapshot mismatch.
//!
//! For Q#-driven tests, [`run_to_fixpoint`] is wrapped in
//! [`run_to_fixpoint_bool`] because [`check_simplify_rule_q`] expects
//! the rule callback to return `bool` (whether anything was rewritten).
//! The driver returns `()` and reports convergence failures through its
//! error vector. The shim asserts that vector is empty and returns `true`,
//! so `// before run_to_fixpoint (fired=true)` records a successful run,
//! not whether any statement changed.

use expect_test::expect;
use indoc::indoc;
use qsc_fir::{
    assigner::Assigner,
    fir::{BlockId, Package, PackageId},
};

use crate::return_unify::simplify;
use crate::return_unify::tests::check_simplify_rule_q;

/// Adapt [`simplify::run_to_fixpoint`] (which returns `()`) to the
/// `FnOnce(_, _, _, _, _) -> bool` contract that
/// [`check_simplify_rule_q`] requires. Assert convergence before returning
/// `true`; this marker does not indicate whether the FIR changed.
fn run_to_fixpoint_bool(
    pkg: &mut Package,
    asgn: &mut Assigner,
    pkg_id: PackageId,
    bid: BlockId,
    slots: &crate::return_unify::lower::SynthSlots,
) -> bool {
    let mut errors = Vec::new();
    simplify::run_to_fixpoint(pkg, asgn, pkg_id, bid, &mut errors, slots);
    assert!(errors.is_empty(), "unexpected fixpoint errors: {errors:?}");
    true
}

#[test]
fn guard_clause_via_run_to_fixpoint() {
    // Q# input: `if true { return 1; } 2`. The lowerer emits the
    // canonical guard-clause flag-lowering shape (guard set + lazy
    // continuation + trailing merge). After `run_to_fixpoint`,
    // `guard_clause` collapses the guard/cont/merge into a single
    // trailing `if` expression and `dead_local` drops the now-unused
    // slot decls.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                2
            }
        }
        "#},
        "Main",
        "run_to_fixpoint",
        run_to_fixpoint_bool,
        &expect![[r#"
            // before run_to_fixpoint (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let __trailing_result : Int = if (not __has_returned) {
                    2
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            function Main() : Int {
                if true {
                    1
                } else {
                    2
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn both_branches_via_run_to_fixpoint() {
    // Q# input: both arms `return`. The lowerer emits the canonical
    // both-branches flag-lowering shape (if/else slot-set + trailing
    // merge). After `run_to_fixpoint`, `both_branches` collapses the
    // pair into a single trailing `if` expression and `dead_local`
    // drops the now-unused slot decls.
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
        "run_to_fixpoint",
        run_to_fixpoint_bool,
        &expect![[r#"
            // before run_to_fixpoint (fired=true)
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

            // after run_to_fixpoint
            function Main() : Int {
                if true {
                    1
                } else {
                    2
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_branch_via_run_to_fixpoint() {
    // Q# input: an asymmetric `if true { return 1; } else { 2 }` whose
    // then-arm returns and whose else-arm yields a value. The lowerer
    // emits the canonical single-branch flag-lowering shape (a `let
    // __trailing_result` binding wrapping the asymmetric `if` plus a
    // trailing merge). After `run_to_fixpoint`, `single_branch` folds
    // the asymmetric `if` into an `if c { v } else { rest }` value
    // expression and `dead_local` drops the now-unused slot decls.
    //
    // This is the fixpoint-level companion to the single-pass
    // `single_branch::then_arm_return_collapses_to_if_else` test; the
    // both-arms-return and guard-clause shapes are covered separately by
    // `both_branches_via_run_to_fixpoint` and `guard_clause_via_run_to_fixpoint`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    2
                }
            }
        }
        "#},
        "Main",
        "run_to_fixpoint",
        run_to_fixpoint_bool,
        &expect![[r#"
            // before run_to_fixpoint (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __trailing_result : Int = if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                    __ret_val
                } else {
                    2
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            function Main() : Int {
                if true {
                    1
                } else {
                    2
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn bare_return_via_run_to_fixpoint() {
    // Q# input: a single `return 42;` body. The lowerer emits the
    // canonical bare-return flag-lowering shape (nested-block terminal
    // pair + trailing merge). After `run_to_fixpoint`, `bare_return`
    // collapses the pair + merge into the lone slot RHS value and
    // `dead_local` drops the now-unused slot decls.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                return 42;
            }
        }
        "#},
        "Main",
        "run_to_fixpoint",
        run_to_fixpoint_bool,
        &expect![[r#"
            // before run_to_fixpoint (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                {
                    __ret_val = 42;
                    __has_returned = true;
                };
                if __has_returned {
                    __ret_val
                } else {
                    __ret_val
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            function Main() : Int {
                42
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn fixpoint_retains_merge_after_an_earlier_return() {
    check_simplify_rule_q(
        indoc! {r#"
        function Main() : Int {
            if true { return 1; }
            return 2;
            0
        }
        "#},
        "Main",
        "run_to_fixpoint",
        run_to_fixpoint_bool,
        &expect![[r#"
            // before run_to_fixpoint (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                if (not __has_returned) {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                };
                let __trailing_result : Int = if (not __has_returned) {
                    0
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                if (not __has_returned) {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        0
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn fixpoint_does_not_replace_tail_with_user_binding() {
    check_simplify_rule_q(
        indoc! {r#"
        function Main() : Int {
            if false { return 1; }
            let y = 2;
            y + 100
        }
        "#},
        "Main",
        "run_to_fixpoint",
        run_to_fixpoint_bool,
        &expect![[r#"
            // before run_to_fixpoint (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if false {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let y : Int = if (not __has_returned) {
                    2
                } else {
                    0
                };
                let __trailing_result : Int = if (not __has_returned) {
                    y + 100
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if false {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let y : Int = if (not __has_returned) {
                    2
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        y + 100
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_body_emit_shape_collapses_to_value() {
    // Diagnostic: pin the post-`run_to_fixpoint` shape for the
    // canonical single-body `return v` emit produced by
    // `super::super::create_flag_trailing_expr_for_slot` on its
    // **no-trailing-expression** path.
    //
    // Q# input is a single `return 17;` body. The lowerer emits the
    // canonical single-body flag-lowering shape:
    //   * `Local(Mut, __has_returned : Bool = false)`
    //   * `Local(Mut, __ret_val : Int = 0)`
    //   * `Semi(Block([Semi(__ret_val = 17), Semi(__has_returned = true)]))`
    //   * `Expr(if __has_returned { __ret_val } else { __ret_val })`
    //     (the merge's else-arm carries a `__ret_val` read, matching
    //     what the lowerer emits when no original trailing expression
    //     exists to fall through to; the rule never inspects the else
    //     because it replaces the whole merge)
    //
    // Expected post-`run_to_fixpoint` shape: a single trailing
    // `Expr(Lit(Int(17)))` — `bare_return` collapses the terminal
    // pair + merge to `v`, then `dead_local` drops the unused
    // `__has_returned` and `__ret_val` declarations whose initializers
    // are side-effect-free literals.
    //
    // If this snapshot ever drifts, the documented single-body shape
    // has changed and the `bare_return` matcher's preconditions need
    // re-examination. The current pass is the regression witness that
    // the existing `bare_return` rule already handles the no-trailing
    // single-body shape.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                return 17;
            }
        }
        "#},
        "Main",
        "run_to_fixpoint",
        run_to_fixpoint_bool,
        &expect![[r#"
            // before run_to_fixpoint (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                {
                    __ret_val = 17;
                    __has_returned = true;
                };
                if __has_returned {
                    __ret_val
                } else {
                    __ret_val
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            function Main() : Int {
                17
            }
            // entry
            Main()
        "#]],
    );
}
