// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Both-branches-return collapse simplifier rule.
//!
//! Recognizes the canonical flag-strategy output for an `if` whose arms
//! both unconditionally set the return slot:
//!
//! ```text
//! {
//!     ...
//!     if c {
//!         __ret_val = v1;
//!         __has_returned = true;
//!     } else {
//!         __ret_val = v2;
//!         __has_returned = true;
//!     }
//!     if __has_returned { __ret_val } else { /* unit or fallback */ }
//! }
//! ```
//!
//! and folds it to:
//!
//! ```text
//! {
//!     ...
//!     if c { v1 } else { v2 }
//! }
//! ```
//!
//! Provides both-branches structured recovery for shapes lowered through the
//! flag pipeline.
//!
//! # Distinctions from [`super::guard_clause`]
//!
//! * The outer `if` here always carries an `else` arm whose body also
//!   matches the slot-set sequence. Asymmetric shapes (only one arm
//!   sets the flag) belong to the [`super::guard_clause`] rule and are
//!   refused by this rule.
//! * The flag transform does not emit a lazy `if not __has_returned`
//!   continuation between the guard set and the merge when both arms
//!   set the flag — that statement would be statically dead. The rule
//!   therefore matches on exactly two trailing statements (the guard
//!   set and the merge), not three.
//!
//! # Qubit-safety bailout
//!
//! The collapse moves the slot-write RHS into the value position of a
//! structured `if`. To stay safe against direct-IR consumers, the rule
//! refuses to fire when either `v1` or `v2` mentions a sub-expression
//! whose type contains [`qsc_fir::ty::Prim::Qubit`]. The check uses the
//! shared
//! [`super::expr_tree_contains_qubit_type`] walker — option (a) in the
//! step plan — because the alternative ("no `Allocate` between slot
//! decl and merge") would have required a Q#-stdlib-aware intrinsic
//! lookup, which is heavier and offers no extra protection for typed
//! Q# (where you cannot return qubits in the first place).

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{BlockId, ExprId, ExprKind, Package, PackageLookup, StmtId, StmtKind},
    ty::Ty,
};

use crate::fir_builder::{alloc_block, alloc_block_expr, alloc_expr_stmt, alloc_if_expr};

use super::{expr_tree_contains_qubit_type, identify_merge_or_trailing_slot, match_slot_set_arm};
use crate::return_unify::lower::SynthSlots;

/// Apply the both-branches-return collapse rule to `block_id`.
///
/// Iterates the rewrite to fixpoint within `block_id`. Each successful
/// rewrite shortens the block by exactly one statement (the merge is
/// dropped, the guard-set `if` is replaced with the new value-producing
/// `if`), so termination is guaranteed without an explicit bound.
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
    let guard_idx = merge_idx - 1;

    let block_ty = package.get_block(block_id).ty.clone();

    let Some((has_returned, return_slot)) =
        identify_merge_or_trailing_slot(package, block_id, stmt_ids[merge_idx], &block_ty, slots)
    else {
        return false;
    };
    let Some((cond_id, v1_id, v2_id)) = identify_both_branches_set(
        package,
        stmt_ids[guard_idx],
        has_returned,
        return_slot,
        &block_ty,
    ) else {
        return false;
    };

    // Conservative bailout: refuse to lift a qubit-typed sub-expression
    // out of the slot-write position. See the module-level docs.
    if expr_tree_contains_qubit_type(package, v1_id)
        || expr_tree_contains_qubit_type(package, v2_id)
    {
        return false;
    }

    let new_if = build_replacement_if(package, assigner, cond_id, v1_id, v2_id, &block_ty);
    let new_stmt = alloc_expr_stmt(package, assigner, new_if, Span::default());

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.truncate(guard_idx);
    block.stmts.push(new_stmt);
    true
}

/// Identifies an `if c { ... slot-set ... } else { ... slot-set ... }`
/// statement.
///
/// Returns `(cond_expr_id, then_rhs_id, else_rhs_id)` on match. Refuses
/// when either arm fails to match the canonical slot-set sequence, or
/// when the outer `if` carries no `else` arm (the guard-clause shape).
fn identify_both_branches_set(
    package: &Package,
    stmt_id: StmtId,
    has_returned: qsc_fir::fir::LocalVarId,
    return_slot: qsc_fir::fir::LocalVarId,
    return_ty: &Ty,
) -> Option<(ExprId, ExprId, ExprId)> {
    let (StmtKind::Expr(if_expr_id) | StmtKind::Semi(if_expr_id)) = package.get_stmt(stmt_id).kind
    else {
        return None;
    };
    let if_expr = package.get_expr(if_expr_id);
    let ExprKind::If(cond_id, then_id, Some(else_id)) = &if_expr.kind else {
        return None;
    };
    let v1 = match_slot_set_arm(package, *then_id, has_returned, return_slot, return_ty)?;
    let v2 = match_slot_set_arm(package, *else_id, has_returned, return_slot, return_ty)?;
    Some((*cond_id, v1, v2))
}

/// Build `if cond { v1 } else { v2 }` and return its `ExprId`. Wraps
/// `v1`/`v2` in single-statement blocks so the new `if` is syntactically
/// well-formed and snapshots stay stable.
fn build_replacement_if(
    package: &mut Package,
    assigner: &mut Assigner,
    cond_id: ExprId,
    v1_id: ExprId,
    v2_id: ExprId,
    block_ty: &Ty,
) -> ExprId {
    let v1_stmt = alloc_expr_stmt(package, assigner, v1_id, Span::default());
    let then_bid = alloc_block(
        package,
        assigner,
        vec![v1_stmt],
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

    let v2_stmt = alloc_expr_stmt(package, assigner, v2_id, Span::default());
    let else_bid = alloc_block(
        package,
        assigner,
        vec![v2_stmt],
        block_ty.clone(),
        Span::default(),
    );
    let else_expr = alloc_block_expr(
        package,
        assigner,
        else_bid,
        block_ty.clone(),
        Span::default(),
    );

    alloc_if_expr(
        package,
        assigner,
        cond_id,
        then_expr,
        Some(else_expr),
        block_ty.clone(),
        Span::default(),
    )
}
