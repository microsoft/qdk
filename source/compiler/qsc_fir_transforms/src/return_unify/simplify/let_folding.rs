// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Trailing-result let-folding simplifier rule.
//!
//! Folds the canonical `__trailing_result` binding emitted by
//! `create_flag_trailing_expr_for_slot` into the
//! immediately following merge expression:
//!
//! ```text
//! {
//!     ... pre-stmts ...
//!     let __trailing_result : T = E;
//!     if __has_returned { __ret_val } else { __trailing_result }
//! }
//! ```
//!
//! becomes
//!
//! ```text
//! {
//!     ... pre-stmts ...
//!     if __has_returned { __ret_val } else { E }
//! }
//! ```
//!
//! The flag transform interposes the `let __trailing_result` binding
//! between the structural rules' anchor shapes and the trailing merge.
//! Inlining the binding restores contiguity so
//! [`super::guard_clause`] and [`super::both_branches`] can recognize
//! the flag-lowering output on a subsequent fixpoint pass. This rule runs
//! at position 6 in [`super::run_to_fixpoint`], **after** the structural
//! rules have had their chance to consume the binding directly; it cleans
//! up any `__trailing_result` binding they left behind so the next pass
//! can match.
//!
//! # Why this rewrite is safe
//!
//! Pre-fold, `E` evaluates unconditionally at the let-init position
//! before the merge inspects `__has_returned`. Post-fold, `E` evaluates
//! only when `__has_returned` is `false`. The change is semantics
//! preserving **only when `E` does not write the merge's slots**:
//!
//! * If `E` writes `__has_returned` or `__ret_val`, then the pre-fold
//!   merge reads the post-`E` slot values, but the post-fold merge
//!   reads the pre-`E` values and may take the wrong arm. The rule
//!   therefore refuses to fire when `E` contains any assignment whose
//!   LHS root resolves to either slot — see
//!   [`init_writes_to_merge_slots`].
//! * Otherwise, `E`'s value is the merge result on the
//!   `__has_returned == false` path in both shapes, and on the
//!   `__has_returned == true` path the pre-fold evaluation of `E` was
//!   value-discarded (the merge took the `then` arm), so skipping `E`
//!   post-fold preserves observable semantics. Function calls inside
//!   `E` are safe because Q# locals are not aliasable across calls.
//!
//! # Recognized shape
//!
//! The rule matches only the exact `__trailing_result` shape produced by
//! the flag transform:
//!
//! * Statement `i` is `Local(_, Bind(ident), init)` whose `ident.id` is
//!   the [`SynthSlots`] `trailing_result` local.
//! * Statement `i+1` is the block's last statement,
//!   `Expr(If(cond, then, Some(else)))`.
//! * `cond` is `Var(Res::Local(has_returned))`.
//! * `then` reduces to a root local read of `ret_val` (direct slot) or
//!   `ret_val[0]` (array-backed slot).
//! * `else` is exactly `Var(Res::Local(ident.id))`.
//! * The let-bound local appears nowhere else in the merge (verified via
//!   [`super::local_use_count`]).
//! * `init` does not write either slot's root local.
//!
//! Generalizing to arbitrary single-use let-elimination is future work.

use qsc_fir::{
    assigner::Assigner,
    fir::{BlockId, ExprId, ExprKind, LocalVarId, Package, PackageLookup, PatKind, Res, StmtKind},
};

use crate::fir_builder::{alloc_block, alloc_block_expr, alloc_expr_stmt};

use super::{extract_root_local, local_use_count, push_children};
use crate::return_unify::lower::SynthSlots;

/// Apply the let-folding rule to `block_id`.
///
/// Iterates to fixpoint within `block_id`. A block carries at most one
/// `__trailing_result` binding today, so the loop usually runs once; it
/// mirrors the other rules' shape and stays correct if multiple bindings
/// ever appear.
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
    if stmt_ids.len() < 2 {
        return false;
    }
    let merge_idx = stmt_ids.len() - 1;
    let let_idx = merge_idx - 1;

    // Statement `i` must be a `let __trailing_result : T = E` binding.
    let StmtKind::Local(_, pat_id, init_expr_id) = package.get_stmt(stmt_ids[let_idx]).kind else {
        return false;
    };
    let pat = package.get_pat(pat_id);
    let PatKind::Bind(ident) = &pat.kind else {
        return false;
    };
    let Some(trailing) = slots.trailing_result else {
        return false;
    };
    if ident.id != trailing {
        return false;
    }
    let trailing_local = ident.id;

    // Statement `i+1` must be the trailing merge expression.
    let StmtKind::Expr(merge_expr_id) = package.get_stmt(stmt_ids[merge_idx]).kind else {
        return false;
    };
    let merge = package.get_expr(merge_expr_id);
    let ExprKind::If(cond_id, then_id, Some(else_id)) = merge.kind else {
        return false;
    };

    // Recover the merge's slot identities so we can refuse to fold when
    // the let-init writes either slot. `cond` must read a single local;
    // `then` must reduce to a root local (direct or array-backed slot).
    let Some(has_returned_local) = extract_var_local(package, cond_id) else {
        return false;
    };
    let Some(ret_val_local) = extract_root_local(package, then_id) else {
        return false;
    };

    // The merge's else arm must be exactly `Var(Res::Local(trailing_local))`.
    let Some(else_local) = extract_var_local(package, else_id) else {
        return false;
    };
    if else_local != trailing_local {
        return false;
    }

    // The let-bound local must not appear anywhere else in the merge.
    if local_use_count(package, merge_expr_id, trailing_local) != 1 {
        return false;
    }

    // Refuse when the init expression writes either merge slot.
    if init_writes_to_merge_slots(package, init_expr_id, has_returned_local, ret_val_local) {
        return false;
    }

    // Recover the init expression's type and span before mutating, so the
    // wrap-in-block branch below can synthesize a block with matching shape.
    let init_expr = package.get_expr(init_expr_id);
    let init_ty = init_expr.ty.clone();
    let init_span = init_expr.span;
    let init_is_if = matches!(init_expr.kind, ExprKind::If(..));

    // Determine the else-arm payload. We need to wrap the init in a Block
    // exactly when it is a nested `If`. The Q# pretty printer renders an
    // `If` directly in else position using `elif`, which the Q# parser
    // only accepts when the chain's bodies are all blocks. The folded
    // form mixes the outer merge's expression-style arms (`if X Y else
    // Z`) with the inlined block-bodied init, producing an unparsable
    // mix. Forcing a block around an `If` init keeps the rendered form
    // `else { if ... }`, which round-trips. Other init shapes (literals,
    // calls, vars, blocks) are unaffected by the `elif` rendering rule
    // and inline directly. See
    // `tests/normalize/flag_strategy::{while_body_with_call_arg_return,
    // nested_block_middle_of_block_fix}` for round-trip witnesses.
    let new_else_id = if init_is_if {
        let wrap_stmt_id = alloc_expr_stmt(package, assigner, init_expr_id, init_span);
        let wrap_block_id = alloc_block(
            package,
            assigner,
            vec![wrap_stmt_id],
            init_ty.clone(),
            init_span,
        );
        alloc_block_expr(package, assigner, wrap_block_id, init_ty, init_span)
    } else {
        init_expr_id
    };

    // Mutate the merge: redirect the else arm to point at the inlined
    // payload. The original let stmt is about to be removed, so any reuse
    // of `init_expr_id` is safe.
    let merge_mut = package.exprs.get_mut(merge_expr_id).expect("merge expr");
    if let ExprKind::If(_, _, slot) = &mut merge_mut.kind {
        *slot = Some(new_else_id);
    } else {
        unreachable!("merge expr kind changed between read and mutate");
    }

    // Drop the let stmt from the block.
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.remove(let_idx);
    true
}

/// Returns `Some(id)` when `expr_id` is exactly `Var(Res::Local(id))`.
fn extract_var_local(package: &Package, expr_id: ExprId) -> Option<LocalVarId> {
    if let ExprKind::Var(Res::Local(id), _) = &package.get_expr(expr_id).kind {
        Some(*id)
    } else {
        None
    }
}

/// Returns `true` when `root` contains any assignment whose LHS root
/// local matches `has_returned` or `ret_val`.
///
/// Walks the expression tree exhaustively. Calls within `root` are
/// assumed safe: Q# locals are not aliasable across calls, so a callee
/// cannot mutate the caller's slot locals.
fn init_writes_to_merge_slots(
    package: &Package,
    root: ExprId,
    has_returned: LocalVarId,
    ret_val: LocalVarId,
) -> bool {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let expr = package.get_expr(id);
        let lhs = match &expr.kind {
            ExprKind::Assign(lhs, _)
            | ExprKind::AssignOp(_, lhs, _)
            | ExprKind::AssignField(lhs, _, _)
            | ExprKind::AssignIndex(lhs, _, _) => Some(*lhs),
            _ => None,
        };
        if let Some(lhs_id) = lhs
            && let Some(root_local) = extract_root_local(package, lhs_id)
            && (root_local == has_returned || root_local == ret_val)
        {
            return true;
        }
        push_children(package, id, &mut stack);
    }
    false
}
