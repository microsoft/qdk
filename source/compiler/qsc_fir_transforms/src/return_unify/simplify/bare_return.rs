// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Bare-return collapse simplifier rule.
//!
//! Recognizes the canonical flag-strategy output for an unconditional
//! trailing assignment to the return slot whose flag is forced `true`
//! immediately before the merge, and rewrites it to a plain
//! value-producing tail:
//!
//! ```text
//! {
//!     ... pre-stmts ...
//!     { __ret_val = v; __has_returned = true; }   // nested-block form
//!     if __has_returned { __ret_val } else { /* fallthrough */ }
//! }
//! ```
//!
//! becomes
//!
//! ```text
//! {
//!     ... pre-stmts ...
//!     v
//! }
//! ```
//!
//! The flat shape is also accepted, where the two assignments appear as
//! contiguous `Semi` statements rather than wrapped in a Unit block:
//!
//! ```text
//! {
//!     ... pre-stmts ...
//!     __ret_val = v;
//!     __has_returned = true;
//!     if __has_returned { __ret_val } else { /* fallthrough */ }
//! }
//! ```
//!
//! A third shape is recognized when the flag-strategy lowering emitted
//! no trailing merge at all — which happens when the return is the
//! entire body and no fallthrough value exists to merge with:
//!
//! ```text
//! {
//!     mutable __has_returned : Bool = false;
//!     mutable __ret_val : T = <default>;
//!     { __ret_val = v; __has_returned = true; }
//!     __ret_val
//! }
//! ```
//!
//! In this no-merge shape the slot/flag locals are identified by name
//! through their `mutable __has_returned : Bool` and
//! `mutable __ret_val : T` declarations elsewhere in the block, the
//! same fallback strategy [`super::dead_flag`] uses.
//!
//! Provides bare-return structured recovery for shapes lowered through the
//! flag pipeline.
//!
//! # Why this rewrite is safe
//!
//! After the terminal pair, `__has_returned == true`, so the merge takes
//! its `then` arm and reads `__ret_val`. The slot assignment immediately
//! before the merge gives `__ret_val == v`, so replacing the merge with
//! `v` preserves the merge's value. The fallthrough else arm is dropped
//! because it is statically unreachable on this path.
//!
//! # Conservative bailouts
//!
//! The rule refuses to fire when any pre-stmt:
//!
//! * Writes either slot (`__ret_val` or `__has_returned`). Reusing slots
//!   earlier in the block is legal, but folding the merge tail in that
//!   case crosses paths that the per-block rule cannot reason about
//!   without a full data-flow analysis. The `dead_flag` rule handles
//!   leftover writes downstream.
//! * Reads `__has_returned`. A pre-stmt read may participate in earlier
//!   control flow that the rule does not analyze; refusing here keeps
//!   the safety net narrow.
//!
//! Closures need no special handling. `return_unify` synthesizes the
//! slot locals after HIR -> FIR lowering, but FIR lowering is also
//! where closures are lifted and their capture lists are finalized. So
//! no closure observed by this rule can possibly capture `__has_returned`
//! or `__ret_val`: those `LocalVarId`s did not exist when the captures
//! were computed. The lifted callable body referenced by a closure is
//! a separate top-level item that cannot reach the enclosing block's
//! locals except through its captures, so a downstream closure cannot
//! observe the slots through any other path either. The walker treats
//! closures as opaque leaves via [`super::push_children`] and that is
//! sound.

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{BlockId, ExprId, ExprKind, LocalVarId, Package, PackageLookup, Res, StmtId, StmtKind},
    ty::Ty,
};

use crate::fir_builder::alloc_expr_stmt;

use super::{
    extract_root_local, identify_merge_or_trailing_slot, match_flag_set, match_slot_assign,
    push_children,
};
use crate::return_unify::lower::SynthSlots;

/// Apply the bare-return collapse rule to `block_id`.
///
/// Iterates the rewrite to fixpoint within `block_id`. Each successful
/// rewrite shortens the block by at least one statement (the merge plus
/// the terminal pair collapses to a single trailing `Expr(v)`), so
/// termination is guaranteed without an explicit bound.
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
    let tail_idx = stmt_ids.len() - 1;
    let block_ty = package.get_block(block_id).ty.clone();

    // Identify the slot/flag locals via either:
    // 1. The canonical trailing merge `if __has_returned { __ret_val } else { ... }`.
    // 2. A bare trailing slot read `__ret_val`, used when the merge has
    //    already been eliminated (e.g. when the return is the entire body
    //    so the flag-strategy lowering emitted no merge in the first place).
    let Some((has_returned, return_slot)) =
        identify_merge_or_trailing_slot(package, block_id, stmt_ids[tail_idx], &block_ty, slots)
    else {
        return false;
    };

    // Try the nested-block form first (the canonical shape emitted by
    // `replace_returns_in_expr` for `Semi(Return(v))`), then fall back
    // to the flat two-semi form.
    let (terminal_start_idx, v_id) = if let Some(v) = identify_nested_pair_stmt(
        package,
        stmt_ids[tail_idx - 1],
        has_returned,
        return_slot,
        &block_ty,
    ) {
        (tail_idx - 1, v)
    } else if stmt_ids.len() >= 3
        && let Some(v) = identify_flat_pair_stmts(
            package,
            stmt_ids[tail_idx - 2],
            stmt_ids[tail_idx - 1],
            has_returned,
            return_slot,
            &block_ty,
        )
    {
        (tail_idx - 2, v)
    } else {
        return false;
    };

    // Conservative bailout: refuse when any pre-stmt writes either slot
    // or reads the flag. See the module-level docs.
    if !pre_stmts_safe(
        package,
        &stmt_ids[..terminal_start_idx],
        has_returned,
        return_slot,
    ) {
        return false;
    }

    let new_stmt = alloc_expr_stmt(package, assigner, v_id, Span::default());
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.truncate(terminal_start_idx);
    block.stmts.push(new_stmt);
    true
}

/// Recognizes the nested-block terminal pair shape:
/// `Semi(Block([Semi(slot_assign), Semi(flag_assign)]))`.
///
/// Returns the slot-assign RHS expression id on match.
fn identify_nested_pair_stmt(
    package: &Package,
    stmt_id: StmtId,
    has_returned: LocalVarId,
    return_slot: LocalVarId,
    return_ty: &Ty,
) -> Option<ExprId> {
    // Accept either `Semi(block)` or `Expr(block)` because a Unit-typed
    // block wrapper is semantically identical in both positions (the
    // value is unit and discarded either way). The flag-strategy emits
    // the trailing slot-read shape with the slot-set block as an
    // `Expr` stmt followed by an `Expr(Var(__ret_val))` tail.
    let (StmtKind::Semi(block_expr_id) | StmtKind::Expr(block_expr_id)) =
        package.get_stmt(stmt_id).kind
    else {
        return None;
    };
    let ExprKind::Block(inner_bid) = &package.get_expr(block_expr_id).kind else {
        return None;
    };
    let stmts = package.get_block(*inner_bid).stmts.clone();
    if stmts.len() != 2 {
        return None;
    }
    let StmtKind::Semi(slot_assign_id) = package.get_stmt(stmts[0]).kind else {
        return None;
    };
    let StmtKind::Semi(flag_assign_id) = package.get_stmt(stmts[1]).kind else {
        return None;
    };
    let v_id = match_slot_assign(package, slot_assign_id, return_slot, return_ty)?;
    if !match_flag_set(package, flag_assign_id, has_returned) {
        return None;
    }
    Some(v_id)
}

/// Recognizes the flat terminal pair shape:
/// `[Semi(slot_assign), Semi(flag_assign)]` as two contiguous statements.
///
/// Returns the slot-assign RHS expression id on match.
fn identify_flat_pair_stmts(
    package: &Package,
    slot_stmt: StmtId,
    flag_stmt: StmtId,
    has_returned: LocalVarId,
    return_slot: LocalVarId,
    return_ty: &Ty,
) -> Option<ExprId> {
    let StmtKind::Semi(slot_assign_id) = package.get_stmt(slot_stmt).kind else {
        return None;
    };
    let StmtKind::Semi(flag_assign_id) = package.get_stmt(flag_stmt).kind else {
        return None;
    };
    let v_id = match_slot_assign(package, slot_assign_id, return_slot, return_ty)?;
    if !match_flag_set(package, flag_assign_id, has_returned) {
        return None;
    }
    Some(v_id)
}

/// Returns `true` when every statement in `pre_stmts` is safe to keep
/// in place under the collapse: no writes to either slot, no reads of
/// the flag.
fn pre_stmts_safe(
    package: &Package,
    pre_stmts: &[StmtId],
    has_returned: LocalVarId,
    return_slot: LocalVarId,
) -> bool {
    for &sid in pre_stmts {
        let expr_id = match &package.get_stmt(sid).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
            StmtKind::Item(_) => continue,
        };
        if expr_tree_writes_or_reads_slots(package, expr_id, has_returned, return_slot) {
            return false;
        }
    }
    true
}

/// Walks the expression tree rooted at `root` and returns `true` when it
/// contains either:
///
/// * An assignment whose LHS root local is `has_returned` or `return_slot`.
/// * A `Var(Res::Local(has_returned), _)` read.
fn expr_tree_writes_or_reads_slots(
    package: &Package,
    root: ExprId,
    has_returned: LocalVarId,
    return_slot: LocalVarId,
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
            && (root_local == has_returned || root_local == return_slot)
        {
            return true;
        }
        if let ExprKind::Var(Res::Local(local), _) = &expr.kind
            && *local == has_returned
        {
            return true;
        }
        // Closures are opaque leaves: see the module-level docs for
        // why a downstream closure cannot observe the synthesized
        // slots through its captures.
        push_children(package, id, &mut stack);
    }
    false
}
