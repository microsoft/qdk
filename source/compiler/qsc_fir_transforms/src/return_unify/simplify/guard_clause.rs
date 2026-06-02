// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Guard-clause collapse simplifier rule.
//!
//! Recognizes the canonical flag-strategy output for a guard clause:
//!
//! ```text
//! {
//!     ...
//!     if c { __ret_val = v; __has_returned = true; }
//!     if not __has_returned { rest_stmts; rest_value }
//!     if __has_returned { __ret_val } else { rest_value_or_fallback }
//! }
//! ```
//!
//! and folds it to:
//!
//! ```text
//! {
//!     ...
//!     if c { v } else { rest_stmts; rest_value }
//! }
//! ```
//!
//! # Slot identification
//!
//! The slot [`LocalVarId`]s for `__has_returned` and `__ret_val` are
//! recovered from the trailing merge expression:
//!
//! * its `cond` is `Var(Res::Local(has_returned), _)` of type `Bool`;
//! * its `then` is a `Block` with a single trailing
//!   `Expr(Var(Res::Local(return_slot), _))` of the merge's type `T`.
//!
//! The guard-set and lazy continuation must then reference exactly those
//! locals, or the rule refuses to fire.

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{BlockId, ExprId, ExprKind, LocalVarId, Package, PackageLookup, StmtId, StmtKind, UnOp},
    ty::{Prim, Ty},
};

use crate::fir_builder::{
    alloc_block, alloc_block_expr, alloc_expr_stmt, alloc_if_expr, alloc_not_expr,
};

use super::{extract_local_read, identify_merge_or_trailing_slot, match_slot_set_arm};
use crate::return_unify::lower::SynthSlots;

/// Apply the guard-clause collapse rule to `block_id`.
///
/// Iterates the rewrite to fixpoint within `block_id`. Each successful
/// rewrite shortens the block by exactly two statements, so termination
/// is guaranteed without an explicit bound.
pub(super) fn apply(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    slots: &SynthSlots,
) -> bool {
    let mut changed = false;
    while try_apply_once(package, assigner, block_id, slots) {
        changed = true;
    }
    changed
}

/// Performs at most one rewrite. Returns `true` when the pattern matched.
fn try_apply_once(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    slots: &SynthSlots,
) -> bool {
    let stmt_ids = package.get_block(block_id).stmts.clone();
    if stmt_ids.len() < 3 {
        return false;
    }
    let merge_idx = stmt_ids.len() - 1;
    let cont_idx = merge_idx - 1;
    let guard_idx = merge_idx - 2;

    let block_ty = package.get_block(block_id).ty.clone();

    let Some((has_returned, return_slot)) =
        identify_merge_or_trailing_slot(package, block_id, stmt_ids[merge_idx], &block_ty, slots)
    else {
        return false;
    };
    let (cond_id, v_id) = if let Some(pair) = identify_guard_set(
        package,
        stmt_ids[guard_idx],
        has_returned,
        return_slot,
        &block_ty,
    ) {
        pair
    } else if let Some((cond, v)) = identify_guard_else_arm(
        package,
        stmt_ids[guard_idx],
        has_returned,
        return_slot,
        &block_ty,
    ) {
        let not_cond = alloc_not_expr(package, assigner, cond, Span::default());
        (not_cond, v)
    } else {
        return false;
    };
    let Some(rest_block_id) =
        identify_continuation(package, stmt_ids[cont_idx], has_returned, &block_ty)
    else {
        return false;
    };

    // Build the replacement: `if c { v } else { rest_block }`.
    let v_stmt = alloc_expr_stmt(package, assigner, v_id, Span::default());
    let then_bid = alloc_block(
        package,
        assigner,
        vec![v_stmt],
        block_ty.clone(),
        Span::default(),
    );
    let then_expr = alloc_block_expr(
        package,
        assigner,
        then_bid,
        block_ty.clone(),
        Span::default(),
    );
    let else_expr = alloc_block_expr(
        package,
        assigner,
        rest_block_id,
        block_ty.clone(),
        Span::default(),
    );
    let new_if = alloc_if_expr(
        package,
        assigner,
        cond_id,
        then_expr,
        Some(else_expr),
        block_ty.clone(),
        Span::default(),
    );
    let new_stmt = alloc_expr_stmt(package, assigner, new_if, Span::default());

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.truncate(guard_idx);
    block.stmts.push(new_stmt);
    true
}

/// Identifies an `if c { __ret_val = v; __has_returned = true; }` statement.
///
/// Accepts the canonical flag-strategy shape produced by
/// `replace_returns_in_expr`, where the original
/// `Return(v)` is mutated to a Unit-typed block whose body contains the
/// two slot/flag assignments. The wrapping `if`'s then-arm is therefore a
/// `Block` containing a single `Semi(Block(...))` statement. The
/// flatter `Block` containing the two assigns directly is accepted as
/// well to make the rule robust against minor pretty-printer-equivalent
/// shape drift.
///
/// Returns `Some((cond_expr_id, v_expr_id))` on match. Refuses to fire
/// when the `if` carries an `else` arm: such a shape is the
/// both-branches-return pattern handled by the `both_branches` rule.
fn identify_guard_set(
    package: &Package,
    stmt_id: StmtId,
    has_returned: LocalVarId,
    return_slot: LocalVarId,
    return_ty: &Ty,
) -> Option<(ExprId, ExprId)> {
    let (StmtKind::Expr(if_expr_id) | StmtKind::Semi(if_expr_id)) = package.get_stmt(stmt_id).kind
    else {
        return None;
    };
    let if_expr = package.get_expr(if_expr_id);
    let ExprKind::If(cond_id, then_id, None) = &if_expr.kind else {
        return None;
    };
    let v_id = match_slot_set_arm(package, *then_id, has_returned, return_slot, return_ty)?;
    Some((*cond_id, v_id))
}

/// Identifies the inverted-orientation guard
/// `if c { /* empty Unit */ } else { __ret_val = v; __has_returned = true; }`
/// where the slot-set sequence lives in the else-arm and the then-arm is
/// a no-op fall-through.
///
/// Returns `Some((cond_expr_id, v_expr_id))` on match. The caller wraps
/// `cond_expr_id` in `UnOp::NotL` and feeds the result into the same
/// rewriter the then-arm matcher uses; the resulting shape is the
/// canonical `if not c { v } else { rest_block }` post-rewrite form.
///
/// The matcher requires the then-arm to be an empty Unit block so the
/// `not`-wrap rewrite preserves semantics without composing the
/// original then-arm content into the continuation. Non-trivial
/// then-arms (e.g. `if c { x(); } else { return v }`) are out of scope:
/// the simpler rewrite cannot express the required composition without a
/// dedicated continuation-splicing rule.
fn identify_guard_else_arm(
    package: &Package,
    stmt_id: StmtId,
    has_returned: LocalVarId,
    return_slot: LocalVarId,
    return_ty: &Ty,
) -> Option<(ExprId, ExprId)> {
    let (StmtKind::Expr(if_expr_id) | StmtKind::Semi(if_expr_id)) = package.get_stmt(stmt_id).kind
    else {
        return None;
    };
    let if_expr = package.get_expr(if_expr_id);
    let ExprKind::If(cond_id, then_id, Some(else_id)) = &if_expr.kind else {
        return None;
    };
    if !then_arm_is_unit_noop(package, *then_id) {
        return None;
    }
    let v_id = match_slot_set_arm(package, *else_id, has_returned, return_slot, return_ty)?;
    Some((*cond_id, v_id))
}

/// Returns `true` when `then_expr_id` is an empty Unit-typed block —
/// the only then-arm shape [`identify_guard_else_arm`] accepts. The
/// constraint keeps the inverted rewrite a pure `not`-wrap and rules out
/// non-trivial then-arms whose content would otherwise be silently
/// dropped from the rewrite output.
fn then_arm_is_unit_noop(package: &Package, then_expr_id: ExprId) -> bool {
    let then_expr = package.get_expr(then_expr_id);
    if then_expr.ty != Ty::UNIT {
        return false;
    }
    let ExprKind::Block(bid) = &then_expr.kind else {
        return false;
    };
    package.get_block(*bid).stmts.is_empty()
}

/// Identifies the `if not __has_returned { rest_block }` continuation
/// statement and returns the underlying `rest_block` id. The continuation
/// may carry an else arm (e.g. the canonical lazy-continuation `else
/// __ret_val`); the else arm is dropped along with the merge.
///
/// Accepts two shapes:
///
/// * The bare `Semi(If(not __has_returned, rest_block, _))` statement.
/// * The `let __trailing_result : T = if not __has_returned { ... } else __ret_val;`
///   binding emitted by `create_flag_trailing_expr_for_slot`,
///   where the lazy continuation is the let-bound initializer. The bound
///   local is read by the trailing merge in the canonical shape, so
///   discarding the binding alongside the merge is safe.
fn identify_continuation(
    package: &Package,
    stmt_id: StmtId,
    has_returned: LocalVarId,
    return_ty: &Ty,
) -> Option<BlockId> {
    let if_expr_id = match package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => e,
        StmtKind::Local(_, _, init) => init,
        StmtKind::Item(_) => return None,
    };
    let if_expr = package.get_expr(if_expr_id);
    let ExprKind::If(cond_id, then_id, _) = &if_expr.kind else {
        return None;
    };
    let cond = package.get_expr(*cond_id);
    let ExprKind::UnOp(UnOp::NotL, inner_id) = &cond.kind else {
        return None;
    };
    let flag_id = extract_local_read(package, *inner_id, Some(&Ty::Prim(Prim::Bool)))?;
    if flag_id != has_returned {
        return None;
    }
    let then_expr = package.get_expr(*then_id);
    let ExprKind::Block(bid) = &then_expr.kind else {
        return None;
    };
    if package.get_block(*bid).ty != *return_ty {
        return None;
    }
    Some(*bid)
}
