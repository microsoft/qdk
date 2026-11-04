// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Guard-clause collapse simplifier rule.
//!
//! Recognizes the canonical flag-strategy output for a guard clause:
//!
//! ```text
//! {
//!     mutable __has_returned = false;
//!     ... // no writes to the return flag or slot
//!     if c { __ret_val = v; __has_returned = true; }
//!     let __trailing_result = if not __has_returned { rest } else __ret_val;
//!     if __has_returned { __ret_val } else { __trailing_result }
//! }
//! ```
//!
//! and folds it to:
//!
//! ```text
//! {
//!     ...
//!     if c { v } else { rest }
//! }
//! ```
//!
//! # Rewrite preconditions
//!
//! The flag, return slot, and trailing-result binding must match the
//! [`SynthSlots`] identities. The merge must read that exact trailing result,
//! not merely an expression with the same type. Bare continuations and merges
//! whose trailing-result binding has already been folded are not accepted.
//!
//! The prefix must declare the flag as false and must not write either slot.
//! Neither the guard condition nor the continuation may write either slot.
//! Otherwise the final merge may be needed to select an earlier or nested
//! return. The skipped continuation arm and merge arms must be pure local reads.

use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, ExprId, ExprKind, Lit, LocalVarId, Mutability, Package, PackageLookup, PatKind,
        StmtId, StmtKind, UnOp,
    },
    ty::{Prim, Ty},
};

use crate::fir_builder::{
    alloc_block, alloc_block_expr, alloc_expr_stmt, alloc_if_expr, alloc_not_expr,
};

use super::{
    expr_writes_to_merge_slots, extract_local_read, extract_then_arm_slot_read, identify_merge,
    match_slot_set_arm,
};
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

    let Some(rest_expr_id) = identify_continuation(
        package,
        stmt_ids[cont_idx],
        stmt_ids[merge_idx],
        slots,
        &block_ty,
    ) else {
        return false;
    };
    if !prefix_keeps_flag_clear(package, &stmt_ids[..guard_idx], slots) {
        return false;
    }

    let has_returned = slots.has_returned;
    let return_slot = slots.return_slot.var_id;
    let (cond_id, v_id, inverted) = if let Some((cond, v)) = identify_guard_set(
        package,
        stmt_ids[guard_idx],
        has_returned,
        return_slot,
        &block_ty,
    ) {
        (cond, v, false)
    } else if let Some((cond, v)) = identify_guard_else_arm(
        package,
        stmt_ids[guard_idx],
        has_returned,
        return_slot,
        &block_ty,
    ) {
        (cond, v, true)
    } else {
        return false;
    };
    if expr_writes_to_merge_slots(package, cond_id, has_returned, return_slot)
        || expr_writes_to_merge_slots(package, rest_expr_id, has_returned, return_slot)
    {
        return false;
    }

    // Build the replacement: `if c { v } else { rest_block }`.
    let cond_id = if inverted {
        alloc_not_expr(package, assigner, cond_id, package.synthetic_span())
    } else {
        cond_id
    };
    let v_stmt = alloc_expr_stmt(package, assigner, v_id, package.synthetic_span());
    let then_bid = alloc_block(
        package,
        assigner,
        vec![v_stmt],
        block_ty.clone(),
        package.synthetic_span(),
    );
    let then_expr = alloc_block_expr(
        package,
        assigner,
        then_bid,
        block_ty.clone(),
        package.synthetic_span(),
    );
    let new_if = alloc_if_expr(
        package,
        assigner,
        cond_id,
        then_expr,
        Some(rest_expr_id),
        block_ty.clone(),
        package.synthetic_span(),
    );
    let new_stmt = alloc_expr_stmt(package, assigner, new_if, package.synthetic_span());

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.truncate(guard_idx);
    block.stmts.push(new_stmt);
    true
}

/// Proves that no earlier statement could have selected a return value.
fn prefix_keeps_flag_clear(package: &Package, stmts: &[StmtId], slots: &SynthSlots) -> bool {
    let mut initialized_false = false;
    for &stmt_id in stmts {
        let expr_id = match package.get_stmt(stmt_id).kind {
            StmtKind::Local(mutability, pat_id, init) => {
                let pat = package.get_pat(pat_id);
                if let PatKind::Bind(ident) = &pat.kind
                    && ident.id == slots.has_returned
                {
                    if mutability != Mutability::Mutable
                        || pat.ty != Ty::Prim(Prim::Bool)
                        || !matches!(package.get_expr(init).kind, ExprKind::Lit(Lit::Bool(false)))
                    {
                        return false;
                    }
                    initialized_false = true;
                }
                init
            }
            StmtKind::Expr(expr) | StmtKind::Semi(expr) => expr,
            StmtKind::Item(_) => continue,
        };
        if expr_writes_to_merge_slots(
            package,
            expr_id,
            slots.has_returned,
            slots.return_slot.var_id,
        ) {
            return false;
        }
    }
    initialized_false
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
/// Returns `Some((cond_expr_id, v_expr_id))` on match. This matcher
/// rejects every `else` arm; [`identify_guard_else_arm`] separately
/// checks the inverted guard shape.
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

/// Matches the synthesized trailing-result binding and the merge that reads it.
/// Returns the continuation body only when all discarded arms are pure reads
/// of the expected locals. Slot writes are checked by the caller.
fn identify_continuation(
    package: &Package,
    stmt_id: StmtId,
    merge_stmt_id: StmtId,
    slots: &SynthSlots,
    return_ty: &Ty,
) -> Option<ExprId> {
    let merge = identify_merge(package, merge_stmt_id, return_ty)?;
    if merge.has_returned != slots.has_returned || merge.return_slot != slots.return_slot.var_id {
        return None;
    }
    let trailing_result = slots.trailing_result?;
    let StmtKind::Expr(merge_expr_id) = package.get_stmt(merge_stmt_id).kind else {
        return None;
    };
    let ExprKind::If(_, _, Some(fallthrough)) = package.get_expr(merge_expr_id).kind else {
        return None;
    };
    if extract_then_arm_slot_read(package, fallthrough, return_ty) != Some(trailing_result) {
        return None;
    }
    let StmtKind::Local(Mutability::Immutable, pat_id, if_expr_id) = package.get_stmt(stmt_id).kind
    else {
        return None;
    };
    let pat = package.get_pat(pat_id);
    let PatKind::Bind(ident) = &pat.kind else {
        return None;
    };
    if ident.id != trailing_result || pat.ty != *return_ty {
        return None;
    }
    let if_expr = package.get_expr(if_expr_id);
    let ExprKind::If(cond_id, then_id, Some(else_id)) = &if_expr.kind else {
        return None;
    };
    if if_expr.ty != *return_ty
        || extract_then_arm_slot_read(package, *else_id, return_ty) != Some(merge.return_slot)
    {
        return None;
    }
    let cond = package.get_expr(*cond_id);
    let ExprKind::UnOp(UnOp::NotL, inner_id) = &cond.kind else {
        return None;
    };
    let flag_id = extract_local_read(package, *inner_id, Some(&Ty::Prim(Prim::Bool)))?;
    if flag_id != merge.has_returned {
        return None;
    }
    let then_expr = package.get_expr(*then_id);
    let ExprKind::Block(bid) = &then_expr.kind else {
        return None;
    };
    if package.get_block(*bid).ty != *return_ty {
        return None;
    }
    Some(*then_id)
}
