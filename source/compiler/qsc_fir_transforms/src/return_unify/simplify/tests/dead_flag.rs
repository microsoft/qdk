// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::dead_flag`].
//!
//! Two test flavors share this file:
//!
//! * Q#-driven `check_simplify_rule_q` tests in the [`q_driven`]
//!   submodule snapshot the pre/post-rule FIR around a single
//!   `dead_flag::apply` invocation. These tests pin the rule's
//!   behavior against representative Q# bodies; the snapshot header
//!   records `fired=<bool>` so each case witnesses whether the
//!   single-rule pass mutated the block.
//!
//!   `dead_flag` runs last in `super::run_to_fixpoint` after the
//!   structural rules have collapsed the trailing merge that
//!   consumes the flag. On canonical pre-simplify shapes the merge
//!   still reads `__has_returned`, so the single-rule pass records
//!   `fired=false` — the rule is correctly refusing to drop a live
//!   setter. The full fixpoint behavior is covered in
//!   `fixpoint::tests`.
//!
//! * Direct-FIR construction tests (marked MANUAL-FIR) in the outer
//!   module build minimal post-merge-collapse shapes to pin the
//!   downstream-reader walker and the closure-capture safety net.
//!   These shapes are not reachable from user-written Q# via a single
//!   `dead_flag::apply` because the merge has not yet been collapsed
//!   at that point in the pipeline. The end-to-end Q# →
//!   return-unified flag-lowering output is covered by the larger
//!   [`crate::return_unify::tests::flag_lowering`] suite.
//!
//! MANUAL-FIR positive cases (rule must fire):
//!
//! 1. Canonical post-merge-collapse: a `mutable __has_returned : Bool`
//!    binding plus a single `__has_returned = true;` setter, no
//!    downstream reader. The fallback name-scan recovers the flag id
//!    (no trailing merge survives), the setter is dropped.
//! 2. Multiple consecutive flag setters at the top level, all dead.
//!    All are dropped in a single `apply` call.
//! 3. Cross-block dead flag: a flag setter at the top level whose only
//!    "downstream reader" candidate sits inside a nested block whose
//!    walker confirms there is no actual read.
//!
//! MANUAL-FIR negative cases (rule must not fire):
//!
//! 1. The canonical trailing merge survives and its condition reads
//!    `__has_returned`. The downstream walker sees the read and the
//!    rule refuses.
//!
//! MANUAL-FIR closure regression case (rule must still fire):
//!
//! 1. A downstream `Closure` expression with a non-empty capture list.
//!    `return_unify` synthesizes `__has_returned` after closure lifting,
//!    so the synthesized flag cannot appear in any closure's captures
//!    by construction. This test pins the rule's behavior against an
//!    earlier draft that bailed on *any* downstream closure regardless
//!    of captures, leaving setters live whenever the user happened to
//!    bind a closure later in the block.

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{BlockId, ExprKind, Lit, LocalVarId, Mutability, Package, PackageLookup, Res, StmtKind},
    ty::{Prim, Ty},
};

use crate::fir_builder::{
    alloc_assign_expr, alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr, alloc_expr_stmt,
    alloc_if_expr, alloc_local_var, alloc_local_var_expr, alloc_semi_stmt,
};
use crate::return_unify::simplify::dead_flag;
use crate::return_unify::symbols;

/// Allocate a `mutable __has_returned : Bool = false;` binding and
/// return the local id plus its declaration statement.
fn alloc_has_returned_binding(
    package: &mut Package,
    assigner: &mut Assigner,
) -> (LocalVarId, qsc_fir::fir::StmtId) {
    let init = alloc_bool_lit(package, assigner, false, Span::default());
    alloc_local_var(
        package,
        assigner,
        symbols::HAS_RETURNED,
        &Ty::Prim(Prim::Bool),
        init,
        Mutability::Mutable,
    )
}

/// Build a `__has_returned = true;` `Semi` statement.
fn build_flag_set_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    flag_id: LocalVarId,
) -> qsc_fir::fir::StmtId {
    let lhs = alloc_local_var_expr(
        package,
        assigner,
        flag_id,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let rhs = alloc_bool_lit(package, assigner, true, Span::default());
    let assign = alloc_assign_expr(package, assigner, lhs, rhs, Span::default());
    alloc_semi_stmt(package, assigner, assign, Span::default())
}

/// Build a trailing `Expr(Int)` literal statement of the given value.
fn build_trailing_int(
    package: &mut Package,
    assigner: &mut Assigner,
    value: i64,
) -> qsc_fir::fir::StmtId {
    let lit = alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::Int),
        ExprKind::Lit(Lit::Int(value)),
        Span::default(),
    );
    alloc_expr_stmt(package, assigner, lit, Span::default())
}

/// Count the number of flag-set statements (`Semi(Assign(Var(flag), _))`)
/// in `block_id`.
fn count_flag_sets(package: &Package, block_id: BlockId, flag_id: LocalVarId) -> usize {
    package
        .get_block(block_id)
        .stmts
        .iter()
        .filter(|&&sid| {
            let StmtKind::Semi(expr_id) = package.get_stmt(sid).kind else {
                return false;
            };
            let ExprKind::Assign(lhs_id, _) = &package.get_expr(expr_id).kind else {
                return false;
            };
            matches!(
                &package.get_expr(*lhs_id).kind,
                ExprKind::Var(Res::Local(id), _) if *id == flag_id,
            )
        })
        .count()
}

#[test]
fn single_dead_setter_is_dropped() {
    // Block shape:
    //   mutable __has_returned = false;
    //   __has_returned = true;
    //   42
    // The flag is identified via the fallback name-scan (no trailing
    // merge). Downstream of the setter is only an Int literal — no
    // flag read, no closure — so the setter is dead.
    let mut package = Package::default();
    let mut assigner = Assigner::default();

    let (flag_id, decl_stmt) = alloc_has_returned_binding(&mut package, &mut assigner);
    let set_stmt = build_flag_set_stmt(&mut package, &mut assigner, flag_id);
    let tail_stmt = build_trailing_int(&mut package, &mut assigner, 42);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![decl_stmt, set_stmt, tail_stmt],
        Ty::Prim(Prim::Int),
        Span::default(),
    );

    let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block_id);
    let fired = dead_flag::apply(&mut package, &mut assigner, block_id, &synth_slots);
    assert!(
        fired,
        "dead_flag must drop the lone unread `__has_returned = true;` setter",
    );

    let stmts = &package.get_block(block_id).stmts;
    assert_eq!(
        stmts.len(),
        2,
        "block should retain the binding and the trailing literal after dropping the setter",
    );
    assert_eq!(
        count_flag_sets(&package, block_id, flag_id),
        0,
        "no flag-set statements should remain after the rule fires",
    );
    // The trailing statement is preserved.
    let StmtKind::Expr(tail_id) = package.get_stmt(stmts[1]).kind else {
        panic!("trailing stmt should be an Expr stmt");
    };
    assert!(
        matches!(&package.get_expr(tail_id).kind, ExprKind::Lit(Lit::Int(42))),
        "trailing literal value should be preserved",
    );
}

#[test]
fn multiple_dead_setters_are_all_dropped() {
    // Block shape:
    //   mutable __has_returned = false;
    //   __has_returned = true;
    //   __has_returned = true;
    //   __has_returned = true;
    //   7
    // None of the setters are observed downstream and no closure
    // appears. All three setters must be dropped in a single call.
    let mut package = Package::default();
    let mut assigner = Assigner::default();

    let (flag_id, decl_stmt) = alloc_has_returned_binding(&mut package, &mut assigner);
    let set_a = build_flag_set_stmt(&mut package, &mut assigner, flag_id);
    let set_b = build_flag_set_stmt(&mut package, &mut assigner, flag_id);
    let set_c = build_flag_set_stmt(&mut package, &mut assigner, flag_id);
    let tail_stmt = build_trailing_int(&mut package, &mut assigner, 7);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![decl_stmt, set_a, set_b, set_c, tail_stmt],
        Ty::Prim(Prim::Int),
        Span::default(),
    );

    let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block_id);
    let fired = dead_flag::apply(&mut package, &mut assigner, block_id, &synth_slots);
    assert!(
        fired,
        "dead_flag must drop every unread `__has_returned = true;` setter in one pass",
    );

    let stmts = &package.get_block(block_id).stmts;
    assert_eq!(
        stmts.len(),
        2,
        "block should retain only the binding and the trailing literal",
    );
    assert_eq!(
        count_flag_sets(&package, block_id, flag_id),
        0,
        "all flag-set statements should be removed",
    );
}

#[test]
fn dead_setter_with_nested_block_downstream_is_dropped() {
    // Block shape:
    //   mutable __has_returned = false;
    //   __has_returned = true;
    //   { let unrelated = 1; 2 };     // nested block stmt — no flag read
    //   3
    // The downstream walker descends into the nested block via
    // `push_children` and confirms no `__has_returned` read appears
    // anywhere below the setter. The setter is dropped.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let int_ty = Ty::Prim(Prim::Int);

    let (flag_id, decl_stmt) = alloc_has_returned_binding(&mut package, &mut assigner);
    let set_stmt = build_flag_set_stmt(&mut package, &mut assigner, flag_id);

    // Nested block: `{ let unrelated = 1; 2 }`. We bind a fresh local
    // (the binding itself is unrelated to `__has_returned`) and end the
    // inner block with an Int literal so the walker descends through
    // both the `Local` init and the trailing `Expr`.
    let unrelated_init = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(1)),
        Span::default(),
    );
    let (_unrelated_local, unrelated_decl) = alloc_local_var(
        &mut package,
        &mut assigner,
        "unrelated",
        &int_ty,
        unrelated_init,
        Mutability::Immutable,
    );
    let inner_tail_value = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(2)),
        Span::default(),
    );
    let inner_tail_stmt = alloc_expr_stmt(
        &mut package,
        &mut assigner,
        inner_tail_value,
        Span::default(),
    );
    let inner_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![unrelated_decl, inner_tail_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let inner_block_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        inner_bid,
        int_ty.clone(),
        Span::default(),
    );
    let inner_block_stmt = alloc_semi_stmt(
        &mut package,
        &mut assigner,
        inner_block_expr,
        Span::default(),
    );

    let tail_stmt = build_trailing_int(&mut package, &mut assigner, 3);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![decl_stmt, set_stmt, inner_block_stmt, tail_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block_id);
    let fired = dead_flag::apply(&mut package, &mut assigner, block_id, &synth_slots);
    assert!(
        fired,
        "dead_flag must drop the setter when the nested block downstream contains no flag read",
    );

    let stmts = &package.get_block(block_id).stmts;
    assert_eq!(
        stmts.len(),
        3,
        "block should retain binding, nested block stmt, and trailing literal",
    );
    assert_eq!(
        count_flag_sets(&package, block_id, flag_id),
        0,
        "no flag-set statements should remain after the rule fires",
    );
}

#[test]
fn surviving_trailing_merge_blocks_the_drop() {
    // Block shape:
    //   mutable __has_returned = false;
    //   mutable __ret_val = 0;
    //   __has_returned = true;
    //   if __has_returned { __ret_val } else { 0 }
    // The merge's `cond` reads `__has_returned` — the rule's downstream
    // walker hits the read and refuses to drop the setter. The merge
    // also serves as the primary signal that recovers the flag id (no
    // fallback name scan is needed).
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let int_ty = Ty::Prim(Prim::Int);
    let bool_ty = Ty::Prim(Prim::Bool);

    let (flag_id, decl_stmt) = alloc_has_returned_binding(&mut package, &mut assigner);

    // mutable __ret_val = 0;
    let ret_init = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(0)),
        Span::default(),
    );
    let (ret_local, ret_decl) = alloc_local_var(
        &mut package,
        &mut assigner,
        symbols::RET_VAL,
        &int_ty,
        ret_init,
        Mutability::Mutable,
    );

    let set_stmt = build_flag_set_stmt(&mut package, &mut assigner, flag_id);

    // if __has_returned { __ret_val } else { 0 }
    let cond = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        flag_id,
        bool_ty.clone(),
        Span::default(),
    );
    let then_var = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        ret_local,
        int_ty.clone(),
        Span::default(),
    );
    let then_stmt = alloc_expr_stmt(&mut package, &mut assigner, then_var, Span::default());
    let then_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![then_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let then_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        then_bid,
        int_ty.clone(),
        Span::default(),
    );
    let else_arm = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(0)),
        Span::default(),
    );
    let merge = alloc_if_expr(
        &mut package,
        &mut assigner,
        cond,
        then_expr,
        Some(else_arm),
        int_ty.clone(),
        Span::default(),
    );
    let merge_stmt = alloc_expr_stmt(&mut package, &mut assigner, merge, Span::default());

    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![decl_stmt, ret_decl, set_stmt, merge_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let before = package.get_block(block_id).stmts.clone();
    let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block_id);
    let fired = dead_flag::apply(&mut package, &mut assigner, block_id, &synth_slots);
    assert!(
        !fired,
        "dead_flag must refuse when the trailing merge reads `__has_returned`",
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when a downstream reader is live",
    );
}

#[test]
fn downstream_closure_does_not_block_drop() {
    // Block shape:
    //   mutable __has_returned = false;
    //   __has_returned = true;
    //   { let f = || -> () { () }; 5 };
    //   7
    // The nested block contains a `Closure` expression bound to `f`.
    // Closure capture lists were finalized during HIR -> FIR lowering,
    // before `return_unify` synthesized `__has_returned`, so the
    // closure cannot possibly capture the flag id. The walker sees no
    // explicit `Var(flag_id)` read downstream, the setter is dead, and
    // the rule drops it. This pins the rule against an earlier draft
    // that bailed on *any* downstream closure.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let int_ty = Ty::Prim(Prim::Int);

    let (flag_id, decl_stmt) = alloc_has_returned_binding(&mut package, &mut assigner);
    let set_stmt = build_flag_set_stmt(&mut package, &mut assigner, flag_id);

    let item_id = qsc_fir::fir::LocalItemId::from(0_usize);
    let closure_expr = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Err,
        ExprKind::Closure(Vec::new(), item_id),
        Span::default(),
    );
    let (_f_local, f_decl) = alloc_local_var(
        &mut package,
        &mut assigner,
        "f",
        &Ty::Err,
        closure_expr,
        Mutability::Immutable,
    );
    let inner_tail = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(5)),
        Span::default(),
    );
    let inner_tail_stmt = alloc_expr_stmt(&mut package, &mut assigner, inner_tail, Span::default());
    let inner_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![f_decl, inner_tail_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let inner_block_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        inner_bid,
        int_ty.clone(),
        Span::default(),
    );
    let inner_block_stmt = alloc_semi_stmt(
        &mut package,
        &mut assigner,
        inner_block_expr,
        Span::default(),
    );

    let tail_stmt = build_trailing_int(&mut package, &mut assigner, 7);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![decl_stmt, set_stmt, inner_block_stmt, tail_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block_id);
    let fired = dead_flag::apply(&mut package, &mut assigner, block_id, &synth_slots);
    assert!(
        fired,
        "dead_flag must drop the setter -- a downstream closure cannot capture a slot synthesized after HIR -> FIR lowering",
    );
    assert_eq!(
        package.get_block(block_id).stmts.len(),
        3,
        "declaration, closure-bearing block stmt, and trailing int must remain after dropping the setter",
    );
    assert_eq!(
        count_flag_sets(&package, block_id, flag_id),
        0,
        "no flag-set statements should remain after the rule fires",
    );
}

/// Q#-driven `check_simplify_rule_q` tests. These pin the rule's
/// behavior against representative Q# bodies. On canonical
/// pre-simplify shapes the trailing merge still reads
/// `__has_returned`, so the single-rule pass records `fired=false`;
/// the rule fires only after the structural rules collapse the merge
/// (see `fixpoint::tests`).
mod q_driven {
    use expect_test::expect;
    use indoc::indoc;

    use crate::return_unify::simplify::dead_flag;
    use crate::return_unify::tests::check_simplify_rule_q;

    #[test]
    fn guard_clause_shape_keeps_flag_live() {
        // `if c { return v; } rest` lowers to the guard-clause flag-
        // strategy shape whose trailing merge cond reads
        // `__has_returned`. The single-rule pass sees the live reader
        // and records `fired=false`. The full fixpoint behavior
        // (where `guard_clause` collapses the merge first, after
        // which `dead_flag` drops the setter) is exercised in
        // `fixpoint::tests`.
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
            "dead_flag",
            dead_flag::apply,
            &expect![[r#"
                // before dead_flag (fired=false)
                function Main() : Int {
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

                // after dead_flag
                function Main() : Int {
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
            "#]],
        );
    }

    #[test]
    fn both_arms_return_shape_keeps_flag_live() {
        // `if c { return a; } else { return b; }` lowers to the
        // both-arms-return flag-lowering shape whose trailing merge
        // cond reads `__has_returned`. Same reasoning as above —
        // `fired=false`.
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
            "dead_flag",
            dead_flag::apply,
            &expect![[r#"
                // before dead_flag (fired=false)
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

                // after dead_flag
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
            "#]],
        );
    }

    #[test]
    fn bare_return_only_body_keeps_flag_live() {
        // `return v;` lowers to the bare-return terminal-pair shape
        // followed by a trailing merge that reads `__has_returned`.
        // `fired=false` for the same reason; `bare_return` is what
        // collapses this in the fixpoint.
        check_simplify_rule_q(
            indoc! {r#"
            namespace Test {
                function Main() : Int {
                    return 42;
                }
            }
            "#},
            "Main",
            "dead_flag",
            dead_flag::apply,
            &expect![[r#"
                // before dead_flag (fired=false)
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

                // after dead_flag
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
            "#]],
        );
    }
}
