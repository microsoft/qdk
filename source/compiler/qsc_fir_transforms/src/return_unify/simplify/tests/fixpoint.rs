// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Integration tests for [`crate::return_unify::simplify::run_to_fixpoint`].
//!
//! Most tests in this suite are **Q#-driven** via [`check_simplify_rule_q`]:
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
//! One direct-FIR test ([`guard_clause_plus_dead_flag_via_run_to_fixpoint`])
//! is kept hand-built because the orphan `__has_returned = true;`
//! setter it pins is not reliably emitted by normalize +
//! `transform_block_with_flags` on its own — it only appears
//! mid-fixpoint after a sibling rule strips the last downstream flag
//! reader. Direct construction is the only way to exercise this
//! multi-rule chain (`guard_clause` → `dead_flag` → `dead_local`) on
//! the canonical orphan-setter shape.
//!
//! For Q#-driven tests, [`run_to_fixpoint`] is wrapped in
//! [`run_to_fixpoint_bool`] because [`check_simplify_rule_q`] expects
//! the rule callback to return `bool` (whether anything was rewritten).
//! The driver always returns `()` — every fixpoint sequence either
//! converges silently or rewrites in place — so the shim
//! unconditionally returns `true` and the snapshot header always reads
//! `// before run_to_fixpoint (fired=true)`.

use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, ExprId, ExprKind, Lit, LocalVarId, Mutability, Package, PackageLookup, Res,
        StmtId, StmtKind,
    },
    ty::{Prim, Ty},
};

use crate::fir_builder::{
    alloc_assign_expr, alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr, alloc_expr_stmt,
    alloc_if_expr, alloc_local_var, alloc_local_var_expr, alloc_not_expr, alloc_semi_stmt,
};
use crate::return_unify::simplify;
use crate::return_unify::tests::check_simplify_rule_q;

/// Adapt [`simplify::run_to_fixpoint`] (which returns `()`) to the
/// `FnOnce(_, _, _) -> bool` contract that
/// [`check_simplify_rule_q`] requires. The driver always advances to a
/// fixpoint, so the shim unconditionally returns `true`.
fn run_to_fixpoint_bool(pkg: &mut Package, asgn: &mut Assigner, bid: BlockId) -> bool {
    let mut errors = Vec::new();
    simplify::run_to_fixpoint(pkg, asgn, bid, &mut errors);
    assert!(errors.is_empty(), "unexpected fixpoint errors: {errors:?}");
    true
}

/// Slot identities shared by every direct-FIR fixture in this module.
struct Slots {
    has_returned: LocalVarId,
    ret_val: LocalVarId,
}

/// Allocate the canonical `mutable __has_returned : Bool = false;` and
/// `mutable __ret_val : Int = 0;` decls and return their statement ids
/// plus the recovered slot locals.
fn alloc_slot_decls(package: &mut Package, assigner: &mut Assigner) -> (Slots, StmtId, StmtId) {
    let bool_ty = Ty::Prim(Prim::Bool);
    let int_ty = Ty::Prim(Prim::Int);

    let hr_init = alloc_bool_lit(package, assigner, false, Span::default());
    let (hr_local, hr_decl) = alloc_local_var(
        package,
        assigner,
        "__has_returned",
        &bool_ty,
        hr_init,
        Mutability::Mutable,
    );

    let rv_init = alloc_expr(
        package,
        assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(0)),
        Span::default(),
    );
    let (rv_local, rv_decl) = alloc_local_var(
        package,
        assigner,
        "__ret_val",
        &int_ty,
        rv_init,
        Mutability::Mutable,
    );

    (
        Slots {
            has_returned: hr_local,
            ret_val: rv_local,
        },
        hr_decl,
        rv_decl,
    )
}

/// Build a `__ret_val = v;` Semi statement.
fn build_slot_assign_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    v_id: ExprId,
) -> StmtId {
    let int_ty = Ty::Prim(Prim::Int);
    let lhs = alloc_local_var_expr(package, assigner, slots.ret_val, int_ty, Span::default());
    let assign = alloc_assign_expr(package, assigner, lhs, v_id, Span::default());
    alloc_semi_stmt(package, assigner, assign, Span::default())
}

/// Build a `__has_returned = true;` Semi statement.
fn build_flag_set_stmt(package: &mut Package, assigner: &mut Assigner, slots: &Slots) -> StmtId {
    let bool_ty = Ty::Prim(Prim::Bool);
    let lhs = alloc_local_var_expr(
        package,
        assigner,
        slots.has_returned,
        bool_ty,
        Span::default(),
    );
    let rhs = alloc_bool_lit(package, assigner, true, Span::default());
    let assign = alloc_assign_expr(package, assigner, lhs, rhs, Span::default());
    alloc_semi_stmt(package, assigner, assign, Span::default())
}

/// Build a Unit-typed block expression carrying the flat slot/flag
/// assign pair: `{ __ret_val = v; __has_returned = true; }`.
fn build_slot_set_arm_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    v_id: ExprId,
) -> ExprId {
    let slot_stmt = build_slot_assign_stmt(package, assigner, slots, v_id);
    let flag_stmt = build_flag_set_stmt(package, assigner, slots);
    let arm_bid = alloc_block(
        package,
        assigner,
        vec![slot_stmt, flag_stmt],
        Ty::UNIT,
        Span::default(),
    );
    alloc_block_expr(package, assigner, arm_bid, Ty::UNIT, Span::default())
}

/// Build the canonical trailing merge
/// `if __has_returned { __ret_val } else { __ret_val }` whose then arm
/// is the Block-wrapped Var that [`identify_merge`] requires. The else
/// arm reads `__ret_val` directly; the rule never inspects the else's
/// value (it replaces the entire merge), so the choice is unconstrained
/// — using `__ret_val` keeps the fixture self-contained without
/// introducing a `__trailing_result` binding (which would activate the
/// independent `let_folding` rule).
fn build_merge_stmt(package: &mut Package, assigner: &mut Assigner, slots: &Slots) -> StmtId {
    let bool_ty = Ty::Prim(Prim::Bool);
    let int_ty = Ty::Prim(Prim::Int);

    let cond = alloc_local_var_expr(
        package,
        assigner,
        slots.has_returned,
        bool_ty,
        Span::default(),
    );
    let then_var = alloc_local_var_expr(
        package,
        assigner,
        slots.ret_val,
        int_ty.clone(),
        Span::default(),
    );
    let then_stmt = alloc_expr_stmt(package, assigner, then_var, Span::default());
    let then_bid = alloc_block(
        package,
        assigner,
        vec![then_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let then_expr = alloc_block_expr(package, assigner, then_bid, int_ty.clone(), Span::default());
    let else_arm = alloc_local_var_expr(
        package,
        assigner,
        slots.ret_val,
        int_ty.clone(),
        Span::default(),
    );
    let merge = alloc_if_expr(
        package,
        assigner,
        cond,
        then_expr,
        Some(else_arm),
        int_ty,
        Span::default(),
    );
    alloc_expr_stmt(package, assigner, merge, Span::default())
}

/// Count `Semi(Assign(Var(has_returned), _))` statements in `block_id`.
/// Mirrors the per-rule helper in [`super::dead_flag`] tests.
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

/// Extract the inner `If(cond, then, Some(else))` from a single
/// trailing `Expr` statement. Panics if the shape does not match.
fn unwrap_trailing_if(package: &Package, stmt_id: StmtId) -> (ExprId, ExprId, ExprId) {
    let StmtKind::Expr(if_id) = package.get_stmt(stmt_id).kind else {
        panic!("trailing stmt should be an Expr stmt");
    };
    let ExprKind::If(cond_id, then_id, Some(else_id)) = &package.get_expr(if_id).kind else {
        panic!("trailing stmt should hold an If(_, _, Some(_))");
    };
    (*cond_id, *then_id, *else_id)
}

/// Return the single trailing-`Expr` value of `block_expr_id` when it
/// is `Block({ <e> })`. Panics otherwise.
fn unwrap_single_block_value(package: &Package, block_expr_id: ExprId) -> ExprId {
    let ExprKind::Block(bid) = &package.get_expr(block_expr_id).kind else {
        panic!(
            "expected Block expression, got {:?}",
            package.get_expr(block_expr_id).kind
        );
    };
    let blk = package.get_block(*bid);
    assert_eq!(blk.stmts.len(), 1, "expected single-stmt block");
    let StmtKind::Expr(e) = package.get_stmt(blk.stmts[0]).kind else {
        panic!("expected Expr stmt in single-stmt block");
    };
    e
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
                        2
                    } else __ret_val;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            // namespace Test
            function Main() : Int {
                body {
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

            // after run_to_fixpoint
            // namespace Test
            function Main() : Int {
                body {
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
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    {
                        __ret_val = 42;
                        __has_returned = true;
                    };
                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            // namespace Test
            function Main() : Int {
                body {
                    42
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn guard_clause_plus_dead_flag_via_run_to_fixpoint() {
    // MANUAL-FIR: this fixture pins a multi-rule chain where an extra
    // orphan `__has_returned = true;` setter sits between the slot
    // decls and the guard set. Normalize + `transform_block_with_flags`
    // does not emit this orphan shape on its own — it only arises
    // mid-fixpoint after a sibling rule has stripped the last
    // downstream flag reader — so direct construction is the only way
    // to exercise the rule chain (guard_clause → dead_flag →
    // dead_local) against the canonical orphan-setter shape.
    //
    // Pre-fixpoint:
    //   mutable __has_returned : Bool = false;
    //   mutable __ret_val : Int = 0;
    //   __has_returned = true;                                 (orphan setter)
    //   if true { __ret_val = 5; __has_returned = true; }      (guard set)
    //   if not __has_returned { 8 }                            (lazy continuation)
    //   if __has_returned { __ret_val } else { __ret_val }     (trailing merge)
    //
    // Post-fixpoint:
    //   if true { 5 } else { { 8 } }
    //
    // The guard_clause rule rewrites the trailing guard/cont/merge
    // into a single `if` expression; the orphan setter then has no
    // downstream flag reader (the rewritten if's cond is a literal,
    // and neither arm reads the flag), so dead_flag drops it in the
    // same fixpoint iteration. dead_local then drops the now-unused
    // slot decls.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let int_ty = Ty::Prim(Prim::Int);
    let bool_ty = Ty::Prim(Prim::Bool);

    let (slots, hr_decl, rv_decl) = alloc_slot_decls(&mut package, &mut assigner);

    // Orphan `__has_returned = true;` setter (will be dead after guard_clause fires).
    let orphan_stmt = build_flag_set_stmt(&mut package, &mut assigner, &slots);

    // Guard set: `if true { __ret_val = 5; __has_returned = true; }`.
    let v_expr = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(5)),
        Span::default(),
    );
    let guard_then = build_slot_set_arm_expr(&mut package, &mut assigner, &slots, v_expr);
    let guard_cond = alloc_bool_lit(&mut package, &mut assigner, true, Span::default());
    let guard_if = alloc_if_expr(
        &mut package,
        &mut assigner,
        guard_cond,
        guard_then,
        None,
        Ty::UNIT,
        Span::default(),
    );
    let guard_stmt = alloc_semi_stmt(&mut package, &mut assigner, guard_if, Span::default());

    // Continuation: `if not __has_returned { 8 }`.
    let rest_value = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(8)),
        Span::default(),
    );
    let rest_value_stmt = alloc_expr_stmt(&mut package, &mut assigner, rest_value, Span::default());
    let rest_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![rest_value_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let rest_block_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        rest_bid,
        int_ty.clone(),
        Span::default(),
    );
    let flag_read = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        slots.has_returned,
        bool_ty,
        Span::default(),
    );
    let not_flag = alloc_not_expr(&mut package, &mut assigner, flag_read, Span::default());
    let cont_if = alloc_if_expr(
        &mut package,
        &mut assigner,
        not_flag,
        rest_block_expr,
        None,
        int_ty.clone(),
        Span::default(),
    );
    let cont_stmt = alloc_semi_stmt(&mut package, &mut assigner, cont_if, Span::default());

    let merge_stmt = build_merge_stmt(&mut package, &mut assigner, &slots);

    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![
            hr_decl,
            rv_decl,
            orphan_stmt,
            guard_stmt,
            cont_stmt,
            merge_stmt,
        ],
        int_ty,
        Span::default(),
    );

    simplify::run_to_fixpoint(&mut package, &mut assigner, block_id, &mut Vec::new());

    let stmts = package.get_block(block_id).stmts.clone();
    assert_eq!(
        stmts.len(),
        1,
        "guard_clause should collapse last 3 stmts, dead_flag should drop the orphan setter, and dead_local should drop the now-unused __has_returned/__ret_val decls",
    );

    let (cond_id, then_id, else_id) = unwrap_trailing_if(&package, stmts[0]);
    assert!(
        matches!(
            package.get_expr(cond_id).kind,
            ExprKind::Lit(Lit::Bool(true))
        ),
        "rewritten if's condition should be the guard's cond literal",
    );
    let then_value = unwrap_single_block_value(&package, then_id);
    assert!(
        matches!(
            package.get_expr(then_value).kind,
            ExprKind::Lit(Lit::Int(5))
        ),
        "then-arm should reuse the slot RHS (5)",
    );
    let else_value = unwrap_single_block_value(&package, else_id);
    assert!(
        matches!(
            package.get_expr(else_value).kind,
            ExprKind::Lit(Lit::Int(8))
        ),
        "else-arm should carry the continuation's rest block trailing value (8)",
    );

    // Critical multi-rule witness: the orphan setter is gone.
    assert_eq!(
        count_flag_sets(&package, block_id, slots.has_returned),
        0,
        "dead_flag should drop the orphan __has_returned setter once guard_clause removes its only downstream reader",
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
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    {
                        __ret_val = 17;
                        __has_returned = true;
                    };
                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after run_to_fixpoint
            // namespace Test
            function Main() : Int {
                body {
                    17
                }
            }
            // entry
            Main()
        "#]],
    );
}
