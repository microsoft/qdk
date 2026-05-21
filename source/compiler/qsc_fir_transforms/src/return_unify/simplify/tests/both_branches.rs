// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::both_branches`].
//!
//! Most tests use [`check_simplify_rule_q`]: a Q# snippet is compiled,
//! the pipeline runs through mono + return-unify-without-simplify, the
//! pre-simplify FIR is snapshotted, [`both_branches::apply`] is invoked
//! on the named callable's body block, and the post-rule FIR is
//! snapshotted. The before/after snapshots pin the rule's effect
//! against what the lowerer actually emits, so the test inputs cannot
//! drift from the canonical flag-strategy output shape.
//!
//! The snapshot header records `fired=<bool>` so each case witnesses
//! whether the single-rule pass mutated the block. `fired=false`
//! appears for shapes the rule must refuse:
//!   * the guard-clause shape (only one arm sets the flag — the
//!     `guard_clause` rule's domain);
//!   * shapes the single-rule pass cannot reach without sibling rules
//!     collapsing intermediate stmts first; the fixpoint driver
//!     bridges these gaps (see `fixpoint::tests`).
//!
//! The qubit-typed slot RHS contract stays as direct-FIR construction
//! (marked MANUAL-FIR) because user-written Q# cannot express a
//! qubit-typed slot RHS — qubits cannot appear in callable return
//! types — but direct-IR consumers can, and the rule's safety net
//! exists exactly for them.

use expect_test::expect;
use indoc::indoc;

use crate::return_unify::simplify::both_branches;
use crate::return_unify::tests::check_simplify_rule_q;

#[test]
fn simple_both_branches_collapses_to_if_else() {
    // Canonical `if c { return a; } else { return b; }`. The lowerer
    // emits the flag-strategy shape with terminal slot-writes in both
    // arms; the single-pass `both_branches` rule collapses the outer
    // if into an `if c { a } else { b }` value expression.
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
        "both_branches",
        both_branches::apply,
        &expect![[r#"
            // before both_branches (fired=true)
            // namespace Test
            function Main() : Int {
                body {
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

                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after both_branches
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if true {
                        1
                    } else {
                        2
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_both_branches_collapses_recursively() {
    // Both-arms-return nested inside one arm of an outer both-arms-
    // return. The single-pass `both_branches` rule records
    // `fired=false` on this canonical pre-simplify shape: the outer
    // then-arm's block holds a `Semi(If(...))` rather than the
    // canonical `{ slot_write; flag_set }` terminal pair, so the
    // matcher refuses. The fixpoint driver bridges the gap by
    // collapsing the inner if first, after which the outer shape
    // becomes recognizable — see `fixpoint::tests` for the converged
    // behavior.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    if false {
                        return 1;
                    } else {
                        return 2;
                    }
                } else {
                    return 3;
                }
            }
        }
        "#},
        "Main",
        "both_branches",
        both_branches::apply,
        &expect![[r#"
            // before both_branches (fired=false)
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if true {
                        if false {
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

                    } else {
                        {
                            __ret_val = 3;
                            __has_returned = true;
                        };
                    }

                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after both_branches
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if true {
                        if false {
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

                    } else {
                        {
                            __ret_val = 3;
                            __has_returned = true;
                        };
                    }

                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn both_branches_with_complex_arm_expressions() {
    // Arms return non-trivial call expressions; the rule must lift
    // those expressions intact into the value position of the new
    // `if`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function F(x : Int) : Int { x + 1 }
            function G(y : Int) : Int { y * 2 }
            function Main() : Int {
                let x = 3;
                let y = 4;
                if true {
                    return F(x);
                } else {
                    return G(y);
                }
            }
        }
        "#},
        "Main",
        "both_branches",
        both_branches::apply,
        &expect![[r#"
            // before both_branches (fired=true)
            // namespace Test
            function F(x : Int) : Int {
                body {
                    x + 1
                }
            }
            function G(y : Int) : Int {
                body {
                    y * 2
                }
            }
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let x : Int = 3;
                    let y : Int = 4;
                    if true {
                        {
                            __ret_val = F(x);
                            __has_returned = true;
                        };
                    } else {
                        {
                            __ret_val = G(y);
                            __has_returned = true;
                        };
                    }

                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after both_branches
            // namespace Test
            function F(x : Int) : Int {
                body {
                    x + 1
                }
            }
            function G(y : Int) : Int {
                body {
                    y * 2
                }
            }
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let x : Int = 3;
                    let y : Int = 4;
                    if true {
                        F(x)
                    } else {
                        G(y)
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn only_one_arm_returns_is_guard_clause_shape_not_both_branches() {
    // Negative: `if c { return v; } rest` is the guard-clause shape
    // (the if-else's else arm is missing). The `both_branches` rule
    // must refuse to fire on this shape — `fired=false`. The
    // `guard_clause` rule (not under test here) is what collapses
    // this pattern; see `guard_clause::tests` for that contract.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                0
            }
        }
        "#},
        "Main",
        "both_branches",
        both_branches::apply,
        &expect![[r#"
            // before both_branches (fired=false)
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if true {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                    }

                    let __trailing_result : Int = if not __has_returned {
                        0
                    } else __ret_val;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()

            // after both_branches
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if true {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                    }

                    let __trailing_result : Int = if not __has_returned {
                        0
                    } else __ret_val;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn qubit_typed_rhs_refuses_to_collapse() {
    // MANUAL-FIR: direct construction of a minimal both-branches
    // pattern whose slot-write RHS contains a `Var` of
    // `Ty::Prim(Prim::Qubit)`. The conservative qubit walker must
    // trip and `apply` must return `false`, leaving the block
    // unchanged. User-written Q# cannot reach this shape (qubits
    // cannot appear in callable return types), but direct-IR
    // consumers can, and the rule's safety net exists exactly for
    // them.
    use crate::fir_builder::{
        alloc_assign_expr, alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr,
        alloc_expr_stmt, alloc_if_expr, alloc_semi_stmt,
    };
    use qsc_data_structures::span::Span;
    use qsc_fir::{
        assigner::Assigner,
        fir::{ExprKind, LocalVarId, Package, Res},
        ty::{Prim, Ty},
    };

    let mut package = Package::default();
    let mut assigner = Assigner::default();

    // Allocate fresh local var ids for the slot, the flag, and the
    // qubit-typed local referenced from the RHS.
    let slot_local: LocalVarId = assigner.next_local();
    let flag_local: LocalVarId = assigner.next_local();
    let qubit_local: LocalVarId = assigner.next_local();

    let qubit_ty = Ty::Prim(Prim::Qubit);
    let bool_ty = Ty::Prim(Prim::Bool);
    let return_ty = qubit_ty.clone();

    // Helper: allocate a `Var(Res::Local(id))` expression of `ty`.
    let make_var = |pkg: &mut Package, asn: &mut Assigner, id: LocalVarId, ty: Ty| -> _ {
        alloc_expr(
            pkg,
            asn,
            ty,
            ExprKind::Var(Res::Local(id), Vec::new()),
            Span::default(),
        )
    };

    // Build slot-set sequences:
    //   then arm: { __ret_val = qubit_local; __has_returned = true; }
    //   else arm: { __ret_val = qubit_local; __has_returned = true; }
    // Both arms reference the same qubit-typed local on the RHS, which is
    // exactly the shape the bailout refuses.
    let mk_arm = |pkg: &mut Package, asn: &mut Assigner| {
        let slot_lhs = alloc_expr(
            pkg,
            asn,
            return_ty.clone(),
            ExprKind::Var(Res::Local(slot_local), Vec::new()),
            Span::default(),
        );
        let slot_rhs = alloc_expr(
            pkg,
            asn,
            return_ty.clone(),
            ExprKind::Var(Res::Local(qubit_local), Vec::new()),
            Span::default(),
        );
        let slot_assign = alloc_assign_expr(pkg, asn, slot_lhs, slot_rhs, Span::default());
        let slot_stmt = alloc_semi_stmt(pkg, asn, slot_assign, Span::default());

        let flag_lhs = alloc_expr(
            pkg,
            asn,
            bool_ty.clone(),
            ExprKind::Var(Res::Local(flag_local), Vec::new()),
            Span::default(),
        );
        let flag_rhs = alloc_bool_lit(pkg, asn, true, Span::default());
        let flag_assign = alloc_assign_expr(pkg, asn, flag_lhs, flag_rhs, Span::default());
        let flag_stmt = alloc_semi_stmt(pkg, asn, flag_assign, Span::default());

        let arm_bid = alloc_block(
            pkg,
            asn,
            vec![slot_stmt, flag_stmt],
            Ty::UNIT,
            Span::default(),
        );
        alloc_block_expr(pkg, asn, arm_bid, Ty::UNIT, Span::default())
    };

    let then_arm = mk_arm(&mut package, &mut assigner);
    let else_arm = mk_arm(&mut package, &mut assigner);

    let cond = alloc_bool_lit(&mut package, &mut assigner, true, Span::default());
    let outer_if = alloc_if_expr(
        &mut package,
        &mut assigner,
        cond,
        then_arm,
        Some(else_arm),
        Ty::UNIT,
        Span::default(),
    );
    let guard_set_stmt = alloc_semi_stmt(&mut package, &mut assigner, outer_if, Span::default());

    // Build the merge `if __has_returned { __ret_val } else { __ret_val }`.
    let merge_cond = make_var(&mut package, &mut assigner, flag_local, bool_ty.clone());
    let then_slot_var = make_var(&mut package, &mut assigner, slot_local, return_ty.clone());
    let then_slot_stmt =
        alloc_expr_stmt(&mut package, &mut assigner, then_slot_var, Span::default());
    let then_blk = alloc_block(
        &mut package,
        &mut assigner,
        vec![then_slot_stmt],
        return_ty.clone(),
        Span::default(),
    );
    let then_blk_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        then_blk,
        return_ty.clone(),
        Span::default(),
    );
    let else_slot_var = make_var(&mut package, &mut assigner, slot_local, return_ty.clone());
    let else_blk_stmt =
        alloc_expr_stmt(&mut package, &mut assigner, else_slot_var, Span::default());
    let else_blk = alloc_block(
        &mut package,
        &mut assigner,
        vec![else_blk_stmt],
        return_ty.clone(),
        Span::default(),
    );
    let else_blk_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        else_blk,
        return_ty.clone(),
        Span::default(),
    );
    let merge_if = alloc_if_expr(
        &mut package,
        &mut assigner,
        merge_cond,
        then_blk_expr,
        Some(else_blk_expr),
        return_ty.clone(),
        Span::default(),
    );
    let merge_stmt = alloc_expr_stmt(&mut package, &mut assigner, merge_if, Span::default());

    let outer_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![guard_set_stmt, merge_stmt],
        return_ty.clone(),
        Span::default(),
    );

    // Snapshot the block contents before applying the rule.
    let before = package.blocks.get(outer_bid).expect("block").stmts.clone();

    // Sanity: without the qubit-typed RHS, the rule would fold; with it,
    // the bailout must fire and `apply` must report no change.
    let changed = both_branches::apply(&mut package, &mut assigner, outer_bid);
    assert!(
        !changed,
        "both_branches rule must refuse to collapse a qubit-typed slot RHS"
    );

    let after = package.blocks.get(outer_bid).expect("block").stmts.clone();
    assert_eq!(
        before, after,
        "block statements must be unchanged when the bailout fires"
    );
}
