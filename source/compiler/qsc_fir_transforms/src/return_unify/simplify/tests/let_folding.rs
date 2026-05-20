// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::let_folding`].
//!
//! Two test flavors share this file:
//!
//! * Q#-driven `check_simplify_rule_q` tests in the [`q_driven`]
//!   submodule snapshot the pre/post-rule FIR around a single
//!   `let_folding::apply` invocation. These tests pin the rule's effect
//!   against what the lowerer actually emits for representative Q#
//!   bodies, and witness `fired=<bool>` in the snapshot header.
//!
//! * Direct-FIR construction tests (marked MANUAL-FIR) in the outer
//!   module pin matcher discipline against shapes that user-written Q#
//!   cannot express — wrong binding names, multiple uses inside the
//!   merge, init expressions that write a merge slot. These exist so
//!   future lowering bugs that emit malformed FIR are still rejected
//!   by the rule, and so the rule's safety nets are exercised
//!   independent of the dispatch oracle. The end-to-end Q# →
//!   return-unified output for the flag strategy is covered by the
//!   larger [`crate::return_unify::tests::flag_strategy`] suite.
//!
//! MANUAL-FIR positive cases (rule must fire):
//!
//! 1. Canonical literal initializer — the merge's else arm becomes the
//!    literal expression id.
//! 2. Block-expression initializer with a side-effecting inner stmt —
//!    the rule still fires; the side-effect block is reused as-is.
//! 3. Call-expression initializer — the rule still fires.
//!
//! MANUAL-FIR negative cases (rule must not fire):
//!
//! 1. Let-bound local name is not `__trailing_result`.
//! 2. The `__trailing_result` local appears twice inside the trailing
//!    merge (e.g. once in the cond and once in the else arm).
//! 3. The init expression writes one of the merge slots (e.g. the
//!    flag-strategy's both-branches-return shape, where each arm sets
//!    `__has_returned`). Folding would let the merge read the slot
//!    before the init's writes commit.

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, ExprId, ExprKind, Lit, LocalVarId, Mutability, Package, PackageLookup, Res,
        StmtKind,
    },
    ty::{Prim, Ty},
};

use crate::fir_builder::{
    alloc_block, alloc_expr, alloc_expr_stmt, alloc_if_expr, alloc_local_var, alloc_local_var_expr,
    alloc_semi_stmt,
};
use crate::return_unify::simplify::let_folding;

/// Slot identities shared by every test fixture.
struct Slots {
    has_returned: LocalVarId,
    ret_val: LocalVarId,
}

/// Allocate `__has_returned : Bool` and `__ret_val : T` local var ids.
fn alloc_slots(assigner: &mut Assigner) -> Slots {
    Slots {
        has_returned: assigner.next_local(),
        ret_val: assigner.next_local(),
    }
}

/// Build a trailing-merge expression
/// `if has_returned __ret_val else trailing_var` whose else arm reads
/// `else_local` and whose then arm reads `slots.ret_val`. Returns the
/// merge's `ExprId`.
fn build_merge(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    else_local: LocalVarId,
    return_ty: &Ty,
) -> ExprId {
    let cond = alloc_local_var_expr(
        package,
        assigner,
        slots.has_returned,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let then_arm = alloc_local_var_expr(
        package,
        assigner,
        slots.ret_val,
        return_ty.clone(),
        Span::default(),
    );
    let else_arm = alloc_local_var_expr(
        package,
        assigner,
        else_local,
        return_ty.clone(),
        Span::default(),
    );
    alloc_if_expr(
        package,
        assigner,
        cond,
        then_arm,
        Some(else_arm),
        return_ty.clone(),
        Span::default(),
    )
}

/// Build the canonical `let __trailing_result : T = init; if ... else __trailing_result`
/// pattern, returning the enclosing block id along with the local id of
/// the bound trailing result.
fn build_canonical_pattern(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    init_expr_id: ExprId,
    return_ty: &Ty,
    binding_name: &str,
) -> (BlockId, LocalVarId, ExprId) {
    let (trailing_local, let_stmt) = alloc_local_var(
        package,
        assigner,
        binding_name,
        return_ty,
        init_expr_id,
        Mutability::Immutable,
    );
    let merge_expr_id = build_merge(package, assigner, slots, trailing_local, return_ty);
    let merge_stmt = alloc_expr_stmt(package, assigner, merge_expr_id, Span::default());
    let block_id = alloc_block(
        package,
        assigner,
        vec![let_stmt, merge_stmt],
        return_ty.clone(),
        Span::default(),
    );
    (block_id, trailing_local, merge_expr_id)
}

#[test]
fn canonical_literal_init_folds_into_merge_else() {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    let init = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(42)),
        Span::default(),
    );
    let (block_id, _, merge_expr_id) = build_canonical_pattern(
        &mut package,
        &mut assigner,
        &slots,
        init,
        &int_ty,
        "__trailing_result",
    );

    let fired = let_folding::apply(&mut package, &mut assigner, block_id);
    assert!(fired, "let_folding must fold the canonical pattern");

    // The block should now have exactly one statement: the merge.
    let stmts = &package.get_block(block_id).stmts;
    assert_eq!(stmts.len(), 1, "let stmt should be dropped");
    assert!(
        matches!(package.get_stmt(stmts[0]).kind, StmtKind::Expr(e) if e == merge_expr_id),
        "remaining stmt should be the original merge"
    );

    // The merge's else arm should now point at the let init.
    let merge = package.get_expr(merge_expr_id);
    let ExprKind::If(_, _, Some(else_id)) = merge.kind else {
        panic!("merge should remain an If with an else arm");
    };
    assert_eq!(
        else_id, init,
        "merge else arm should be redirected to the let init expression"
    );
}

#[test]
fn block_init_with_side_effect_folds() {
    // The init is a block expression carrying a side-effecting `Semi`
    // followed by a literal trailing expression. Folding the let moves
    // the block into the merge's else arm as-is — no node reallocation.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    // Side effect: a synthetic mutable assign of an unrelated local.
    // The expression's semantics don't matter for the rule's match;
    // only that it is a non-trivial sub-expression to verify the
    // walker traverses without panicking.
    let sink_local = assigner.next_local();
    let sink_lhs = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Var(Res::Local(sink_local), Vec::new()),
        Span::default(),
    );
    let sink_rhs = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(1)),
        Span::default(),
    );
    let side_effect = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::Assign(sink_lhs, sink_rhs),
        Span::default(),
    );
    let side_effect_stmt =
        alloc_semi_stmt(&mut package, &mut assigner, side_effect, Span::default());

    let tail_value = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(7)),
        Span::default(),
    );
    let tail_stmt = alloc_expr_stmt(&mut package, &mut assigner, tail_value, Span::default());

    let inner_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![side_effect_stmt, tail_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let init = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Block(inner_bid),
        Span::default(),
    );

    let (block_id, _, merge_expr_id) = build_canonical_pattern(
        &mut package,
        &mut assigner,
        &slots,
        init,
        &int_ty,
        "__trailing_result",
    );

    let fired = let_folding::apply(&mut package, &mut assigner, block_id);
    assert!(fired, "let_folding must fold block-typed initializers");

    let stmts = &package.get_block(block_id).stmts;
    assert_eq!(stmts.len(), 1, "let stmt should be dropped");
    let merge = package.get_expr(merge_expr_id);
    let ExprKind::If(_, _, Some(else_id)) = merge.kind else {
        panic!("merge should remain an If with an else arm");
    };
    assert_eq!(
        else_id, init,
        "merge else arm should now reference the let init block expression"
    );
}

#[test]
fn call_init_folds() {
    // The init is a Call(callable_var, arg) expression. The rule must
    // fold the let regardless of the init expression kind, as long as
    // the trailing-result name and use-count constraints hold.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    let callable_local = assigner.next_local();
    // The callable's exact arrow type is irrelevant to the rule; the
    // walker only inspects `ExprKind` shape. Use `Ty::Err` to avoid
    // constructing a full `Arrow` value here.
    let callable_expr = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::Err,
        ExprKind::Var(Res::Local(callable_local), Vec::new()),
        Span::default(),
    );
    let arg_expr = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::Tuple(Vec::new()),
        Span::default(),
    );
    let init = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Call(callable_expr, arg_expr),
        Span::default(),
    );

    let (block_id, _, merge_expr_id) = build_canonical_pattern(
        &mut package,
        &mut assigner,
        &slots,
        init,
        &int_ty,
        "__trailing_result",
    );

    let fired = let_folding::apply(&mut package, &mut assigner, block_id);
    assert!(fired, "let_folding must fold call-typed initializers");

    let stmts = &package.get_block(block_id).stmts;
    assert_eq!(stmts.len(), 1, "let stmt should be dropped");
    let merge = package.get_expr(merge_expr_id);
    let ExprKind::If(_, _, Some(else_id)) = merge.kind else {
        panic!("merge should remain an If with an else arm");
    };
    assert_eq!(
        else_id, init,
        "merge else arm should now reference the let init call expression"
    );
}

#[test]
fn wrong_name_refuses_to_fold() {
    // The let binds a local whose name is not `__trailing_result`. The
    // rule must refuse to fire even though every other shape detail
    // matches the canonical pattern.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    let init = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(42)),
        Span::default(),
    );
    let (block_id, _, _) = build_canonical_pattern(
        &mut package,
        &mut assigner,
        &slots,
        init,
        &int_ty,
        "some_other_name",
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = let_folding::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "let_folding must refuse non-canonical binding names"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the rule refuses to fire"
    );
}

#[test]
fn multiple_uses_in_merge_refuse_to_fold() {
    // Build a pattern where `__trailing_result` appears twice in the
    // trailing merge: once in the cond (artificial — typed as Bool
    // here to keep the merge well-formed) and once in the else arm.
    // The use-count guard must refuse the fold.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let bool_ty = Ty::Prim(Prim::Bool);

    // Init is a Bool literal because the merge's value type and the
    // local's type must match for the IR to be well-formed.
    let init = alloc_expr(
        &mut package,
        &mut assigner,
        bool_ty.clone(),
        ExprKind::Lit(Lit::Bool(false)),
        Span::default(),
    );

    // Build the let first so we know `trailing_local`.
    let (trailing_local, let_stmt) = alloc_local_var(
        &mut package,
        &mut assigner,
        "__trailing_result",
        &bool_ty,
        init,
        Mutability::Immutable,
    );

    // Cond reads `__trailing_result` (the second, disqualifying use).
    let cond = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        trailing_local,
        bool_ty.clone(),
        Span::default(),
    );
    let then_arm = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        slots.ret_val,
        bool_ty.clone(),
        Span::default(),
    );
    let else_arm = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        trailing_local,
        bool_ty.clone(),
        Span::default(),
    );
    let merge_expr_id = alloc_if_expr(
        &mut package,
        &mut assigner,
        cond,
        then_arm,
        Some(else_arm),
        bool_ty.clone(),
        Span::default(),
    );
    let merge_stmt = alloc_expr_stmt(&mut package, &mut assigner, merge_expr_id, Span::default());
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![let_stmt, merge_stmt],
        bool_ty.clone(),
        Span::default(),
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = let_folding::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "let_folding must refuse when the trailing local is used more than once"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the use-count guard fires"
    );
    // Drop `slots.has_returned` warning suppression — referenced via
    // construction implicitly.
    let _ = slots.has_returned;
}

#[test]
fn init_that_writes_merge_slot_refuses_to_fold() {
    // The init expression contains an assignment to `__has_returned`.
    // Folding would let the merge read the flag before the assignment
    // commits, breaking semantic equivalence. The bailout must trip.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);
    let bool_ty = Ty::Prim(Prim::Bool);

    // Build an init block whose first statement assigns
    // `__has_returned = true` and whose trailing expression is a literal.
    let flag_lhs = alloc_expr(
        &mut package,
        &mut assigner,
        bool_ty.clone(),
        ExprKind::Var(Res::Local(slots.has_returned), Vec::new()),
        Span::default(),
    );
    let flag_rhs = alloc_expr(
        &mut package,
        &mut assigner,
        bool_ty.clone(),
        ExprKind::Lit(Lit::Bool(true)),
        Span::default(),
    );
    let flag_assign = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::Assign(flag_lhs, flag_rhs),
        Span::default(),
    );
    let flag_assign_stmt =
        alloc_semi_stmt(&mut package, &mut assigner, flag_assign, Span::default());

    let tail_value = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(7)),
        Span::default(),
    );
    let tail_stmt = alloc_expr_stmt(&mut package, &mut assigner, tail_value, Span::default());

    let inner_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![flag_assign_stmt, tail_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let init = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Block(inner_bid),
        Span::default(),
    );

    let (block_id, _, _) = build_canonical_pattern(
        &mut package,
        &mut assigner,
        &slots,
        init,
        &int_ty,
        "__trailing_result",
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = let_folding::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "let_folding must refuse when the init writes a merge slot"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the slot-write bailout fires"
    );
}

/// Q#-driven `check_simplify_rule_q` tests. These pin the rule's
/// effect against what the lowerer actually emits for representative
/// Q# bodies; the snapshot header records `fired=<bool>` so each case
/// witnesses whether the single-rule pass mutated the block.
mod q_driven {
    use expect_test::expect;
    use indoc::indoc;

    use crate::return_unify::simplify::let_folding;
    use crate::return_unify::tests::check_simplify_rule_q;

    #[test]
    fn guard_clause_shape_let_trailing_folds() {
        // `if c { return v; } rest` lowers to the guard-clause flag-
        // strategy shape, which carries a `let __trailing_result : T =
        // <init>;` binding followed by the canonical trailing merge.
        // The `<init>` reads `__has_returned` and `__ret_val` but does
        // not write them, so the slot-write bailout does not trip and
        // the rule folds the let into the merge's else arm.
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
            "let_folding",
            let_folding::apply,
            &expect![[r#"
                // before let_folding (fired=true)
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

                // after let_folding
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

                        if __has_returned __ret_val else {
                            if not __has_returned {
                                0
                            } else __ret_val
                        }

                    }
                }
                // entry
                Main()
            "#]],
        );
    }

    #[test]
    fn both_arms_return_shape_has_no_let_trailing() {
        // `if c { return a; } else { return b; }` lowers to the
        // flag-strategy shape *without* a `let __trailing_result`
        // binding — the trailing merge directly reads `__ret_val` on
        // both arms. `let_folding` records `fired=false` because the
        // canonical `let __trailing_result` anchor is absent.
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
            "let_folding",
            let_folding::apply,
            &expect![[r#"
                // before let_folding (fired=false)
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

                // after let_folding
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
            "#]],
        );
    }

    #[test]
    fn bare_return_only_body_has_no_let_trailing() {
        // A body that is just `return v;` lowers to the bare-return
        // terminal-pair shape with no `let __trailing_result`. The
        // rule records `fired=false`; the `bare_return` rule (not
        // under test here) collapses this shape.
        check_simplify_rule_q(
            indoc! {r#"
            namespace Test {
                function Main() : Int {
                    return 42;
                }
            }
            "#},
            "Main",
            "let_folding",
            let_folding::apply,
            &expect![[r#"
                // before let_folding (fired=false)
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

                // after let_folding
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
            "#]],
        );
    }
}
