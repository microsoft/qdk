// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Single-branch-return collapse simplifier rule.
//!
//! Recognizes the canonical flag-strategy output for a trailing `if` whose
//! only one arm sets the return slot:
//!
//! ```text
//! {
//!     ...
//!     let __trailing_result : T = if cond {
//!         value
//!     } else {
//!         { __ret_val = v; __has_returned = true; };
//!     };
//!     if __has_returned { __ret_val } else { __trailing_result }
//! }
//! ```
//!
//! and folds it to:
//!
//! ```text
//! {
//!     ...
//!     if cond { value } else { v }
//! }
//! ```
//!
//! The symmetric case (slot-set in the then-arm, value in the else-arm)
//! is handled identically.
//!
//! # Distinctions from other rules
//!
//! * [`super::both_branches`] requires both arms to set the return slot.
//!   This rule handles the asymmetric case where exactly one arm sets
//!   the flag.
//! * [`super::guard_clause`] recognizes a standalone `if` statement that
//!   sets the flag followed by a lazy continuation. This rule recognizes
//!   the same pattern when it appears inside a `let __trailing_result`
//!   binding — the shape emitted when the `if` is the block's trailing
//!   expression and only one branch has a `return`.
//! * [`super::let_folding`] would handle the `let __trailing_result`
//!   binding if the initializer did not write the merge slots. This
//!   rule takes over in the case `let_folding` refuses.
//!
//! # Qubit-safety bailout
//!
//! The collapse moves the slot-write RHS into the value position of a
//! structured `if`. The rule refuses to fire when the slot RHS mentions
//! a sub-expression whose type contains [`qsc_fir::ty::Prim::Qubit`],
//! matching the conservative policy of [`super::both_branches`].

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{BlockId, ExprId, ExprKind, LocalVarId, Package, PackageLookup, PatKind, StmtKind},
};

use crate::fir_builder::{alloc_block, alloc_block_expr, alloc_expr_stmt, alloc_if_expr};

use super::{
    expr_tree_contains_qubit_type, extract_root_local, identify_merge_or_trailing_slot,
    match_slot_set_arm, push_children,
};
use crate::return_unify::symbols::TRAILING_RESULT as TRAILING_RESULT_NAME;

/// Apply the single-branch-return collapse rule to `block_id`.
///
/// Iterates the rewrite to fixpoint within `block_id`. Each successful
/// rewrite shortens the block by exactly one statement, so termination
/// is guaranteed without an explicit bound.
pub(super) fn apply(package: &mut Package, assigner: &mut Assigner, block_id: BlockId) -> bool {
    let mut changed = false;
    while try_apply_once(package, assigner, block_id) {
        changed = true;
    }
    changed
}

/// Performs at most one rewrite. Returns `true` when the pattern matched.
fn try_apply_once(package: &mut Package, assigner: &mut Assigner, block_id: BlockId) -> bool {
    let stmt_ids = package.get_block(block_id).stmts.clone();
    if stmt_ids.len() < 2 {
        return false;
    }
    let merge_idx = stmt_ids.len() - 1;
    let let_idx = merge_idx - 1;

    let block_ty = package.get_block(block_id).ty.clone();

    // Identify the merge expression and recover slot locals.
    let Some((has_returned, return_slot)) =
        identify_merge_or_trailing_slot(package, block_id, stmt_ids[merge_idx], &block_ty)
    else {
        return false;
    };

    // Statement before the merge must be `let __trailing_result : T = if cond { A } else { B }`.
    let StmtKind::Local(_, pat_id, init_expr_id) = package.get_stmt(stmt_ids[let_idx]).kind else {
        return false;
    };
    let pat = package.get_pat(pat_id);
    let PatKind::Bind(ident) = &pat.kind else {
        return false;
    };
    if ident.name.as_ref() != TRAILING_RESULT_NAME {
        return false;
    }

    let init = package.get_expr(init_expr_id);
    let ExprKind::If(cond_id, then_id, Some(else_id)) = init.kind else {
        return false;
    };

    // Try to match each arm as a slot-set sequence.
    let then_slot_rhs = match_slot_set_arm(package, then_id, has_returned, return_slot, &block_ty);
    let else_slot_rhs = match_slot_set_arm(package, else_id, has_returned, return_slot, &block_ty);

    match (then_slot_rhs, else_slot_rhs) {
        (None, Some(v)) => {
            // Else-branch sets slots, then-branch is a value.
            // Original: `if cond { value } else { return v; }`
            // Fold to: `if cond { value } else { v }`
            if expr_tree_contains_qubit_type(package, v) {
                return false;
            }
            // Refuse when the value arm writes to the merge slots — the
            // fold would discard the trailing merge that reads those slots.
            if arm_writes_to_merge_slots(package, then_id, has_returned, return_slot) {
                return false;
            }
            let else_arm = wrap_in_block_expr(package, assigner, v, &block_ty);
            let new_if = alloc_if_expr(
                package,
                assigner,
                cond_id,
                then_id,
                Some(else_arm),
                block_ty,
                Span::default(),
            );
            let new_stmt = alloc_expr_stmt(package, assigner, new_if, Span::default());
            let block = package.blocks.get_mut(block_id).expect("block not found");
            block.stmts.truncate(let_idx);
            block.stmts.push(new_stmt);
            true
        }
        (Some(v), None) => {
            // Then-branch sets slots, else-branch is a value.
            // Original: `if cond { return v; } else { value }`
            // Fold to: `if cond { v } else { value }`
            if expr_tree_contains_qubit_type(package, v) {
                return false;
            }
            // Refuse when the value arm writes to the merge slots.
            if arm_writes_to_merge_slots(package, else_id, has_returned, return_slot) {
                return false;
            }
            let then_arm = wrap_in_block_expr(package, assigner, v, &block_ty);
            let new_if = alloc_if_expr(
                package,
                assigner,
                cond_id,
                then_arm,
                Some(else_id),
                block_ty,
                Span::default(),
            );
            let new_stmt = alloc_expr_stmt(package, assigner, new_if, Span::default());
            let block = package.blocks.get_mut(block_id).expect("block not found");
            block.stmts.truncate(let_idx);
            block.stmts.push(new_stmt);
            true
        }
        // Both arms set slots → handled by `both_branches`.
        // Neither arm sets slots → not our pattern.
        _ => false,
    }
}

/// Wrap an expression in a single-statement block expression for use as
/// an `if` arm. Matches the shape emitted by [`super::both_branches`].
fn wrap_in_block_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: qsc_fir::fir::ExprId,
    block_ty: &qsc_fir::ty::Ty,
) -> qsc_fir::fir::ExprId {
    let stmt = alloc_expr_stmt(package, assigner, expr_id, Span::default());
    let bid = alloc_block(
        package,
        assigner,
        vec![stmt],
        block_ty.clone(),
        Span::default(),
    );
    alloc_block_expr(package, assigner, bid, block_ty.clone(), Span::default())
}

/// Returns `true` when `arm_expr_id` contains any assignment whose LHS
/// root resolves to `has_returned` or `return_slot`.
///
/// Mirrors [`super::let_folding::init_writes_to_merge_slots`]. The fold
/// is only valid when the value arm does not write the merge's slots;
/// otherwise the trailing merge (which we remove) would have read the
/// post-write slot values and produced a different result than the
/// folded `if`.
fn arm_writes_to_merge_slots(
    package: &Package,
    arm_expr_id: ExprId,
    has_returned: LocalVarId,
    return_slot: LocalVarId,
) -> bool {
    let mut stack = vec![arm_expr_id];
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
            && (root_local == has_returned || root_local == return_slot)
        {
            return true;
        }
        push_children(package, id, &mut stack);
    }
    false
}
