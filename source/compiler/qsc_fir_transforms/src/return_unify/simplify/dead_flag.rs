// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dead-flag elimination simplifier rule.
//!
//! Drops `__has_returned = true;` assignments whose value is never read on
//! any downstream path within the block. The rule runs last in
//! [`super::run_to_fixpoint`], after the structural rules
//! ([`super::guard_clause`], [`super::both_branches`],
//! [`super::bare_return`]) have collapsed the trailing merge that consumed
//! the flag, leaving the setter writes statically dead.
//!
//! # Recognized shape
//!
//! Each candidate is `Semi(Assign(<flag_lhs>, _))` where `<flag_lhs>`
//! peels (via [`super::extract_root_local`]) to the block's
//! `__has_returned` local. The RHS is unconstrained: any write whose
//! result is unread downstream is dead.
//!
//! # Recovering the flag local
//!
//! [`identify_has_returned_local`] uses two tiers:
//!
//! 1. **Primary**: recover the flag from the `Var(Res::Local(_))`
//!    condition of the canonical trailing merge
//!    `Expr(If(cond, _, Some(_)))`.
//! 2. **Fallback**: match a `mutable __has_returned : Bool` binding by
//!    [`SynthSlots`] id, for when the structural rules have already
//!    collapsed the merge.
//!
//! # Downstream reader detection
//!
//! For each candidate at index `i`, [`downstream_has_flag_read`] walks
//! statements `i+1..end`, descending into nested blocks. The LHS of a
//! later `Assign(Var(flag_id), _)` is *not* counted as a read — it is a
//! store target — so clustered dead setters do not keep each other alive.
//!
//! # Closures need no special handling
//!
//! The flag's `LocalVarId` is minted after FIR lowering finalizes closure
//! capture lists, so no closure can carry it in its capture list, and a
//! closure's lifted body reaches enclosing locals only through its
//! captures. The walker treats closures as opaque leaves via
//! [`super::push_children`].
//!
//! # Safety
//!
//! Running last means no upstream rule can introduce a new flag reader
//! after `dead_flag` has scanned in the same pass. The driver re-runs the
//! catalogue from the top whenever any rule fires, so a future
//! reader-inserting rule placed after `dead_flag` would still be caught on
//! the next iteration.

use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, ExprId, ExprKind, LocalVarId, Mutability, Package, PackageLookup, PatKind, Res,
        StmtId, StmtKind,
    },
    ty::{Prim, Ty},
};

use super::{extract_local_read, extract_root_local, push_children};
use crate::return_unify::lower::SynthSlots;

/// Apply the dead-flag elimination rule to `block_id`.
///
/// Returns `true` when at least one flag-set statement was removed. All
/// eligible setters are dropped in a single call; the driver's outer loop
/// re-scans after any other rule reshapes the block.
pub(super) fn apply(
    package: &mut Package,
    _assigner: &mut Assigner,
    block_id: BlockId,
    slots: &SynthSlots,
) -> bool {
    let Some(flag_id) = identify_has_returned_local(package, block_id, slots) else {
        return false;
    };

    let stmt_ids = package.get_block(block_id).stmts.clone();
    if stmt_ids.is_empty() {
        return false;
    }

    let mut to_drop: Vec<usize> = Vec::new();
    for i in 0..stmt_ids.len() {
        if !is_flag_set_stmt(package, stmt_ids[i], flag_id) {
            continue;
        }
        if downstream_has_flag_read(package, &stmt_ids[i + 1..], flag_id) {
            continue;
        }
        to_drop.push(i);
    }

    if to_drop.is_empty() {
        return false;
    }

    let block = package.blocks.get_mut(block_id).expect("block not found");
    // Remove in reverse order so earlier indices remain valid.
    for &i in to_drop.iter().rev() {
        block.stmts.remove(i);
    }
    true
}

/// Recover the `__has_returned` flag's [`LocalVarId`] for `block_id`.
///
/// See the module-level docs for the two-tier strategy. Returns `None`
/// when neither signal is available — in that case the rule cannot
/// safely identify the flag and refuses to fire.
fn identify_has_returned_local(
    package: &Package,
    block_id: BlockId,
    slots: &SynthSlots,
) -> Option<LocalVarId> {
    let stmts = &package.get_block(block_id).stmts;

    // Primary: trailing merge `Expr(If(cond, _, Some(_)))` where `cond`
    // reads a Bool-typed local. Mirrors `let_folding`'s use of the merge
    // condition to recover slot identities.
    if let Some(&last_id) = stmts.last()
        && let StmtKind::Expr(expr_id) = package.get_stmt(last_id).kind
        && let ExprKind::If(cond_id, _, Some(_)) = &package.get_expr(expr_id).kind
        && let Some(local) = extract_local_read(package, *cond_id, Some(&Ty::Prim(Prim::Bool)))
    {
        return Some(local);
    }

    // Fallback: a `mutable __has_returned : Bool` binding in the block,
    // matched by `SynthSlots` id. Only used when the merge has already
    // been collapsed by the structural rules.
    for &sid in stmts {
        let StmtKind::Local(Mutability::Mutable, pat_id, _) = package.get_stmt(sid).kind else {
            continue;
        };
        let pat = package.get_pat(pat_id);
        if pat.ty != Ty::Prim(Prim::Bool) {
            continue;
        }
        if let PatKind::Bind(ident) = &pat.kind
            && ident.id == slots.has_returned
        {
            return Some(ident.id);
        }
    }
    None
}

/// Returns `true` when `stmt_id` is `Semi(Assign(lhs, _))` whose LHS
/// root local is `flag_id`.
fn is_flag_set_stmt(package: &Package, stmt_id: StmtId, flag_id: LocalVarId) -> bool {
    let StmtKind::Semi(expr_id) = package.get_stmt(stmt_id).kind else {
        return false;
    };
    let ExprKind::Assign(lhs_id, _) = &package.get_expr(expr_id).kind else {
        return false;
    };
    extract_root_local(package, *lhs_id) == Some(flag_id)
}

/// Walk every expression reachable from `downstream_stmts` and return
/// `true` if any subexpression reads `flag_id`.
///
/// The LHS of `Assign(Var(flag_id), _)` (and its projection-wrapped
/// variants) is *not* counted as a read: it is a write target. This
/// distinction lets the rule drop a sequence of consecutive dead
/// setters in one pass without the earlier setters being held live by
/// the LHS of the later ones.
///
/// Closures are opaque leaves: see the module-level docs for why a
/// downstream closure cannot observe the synthesized flag through its
/// captures or its lifted body.
fn downstream_has_flag_read(
    package: &Package,
    downstream_stmts: &[StmtId],
    flag_id: LocalVarId,
) -> bool {
    let mut stack: Vec<ExprId> = Vec::new();
    for &sid in downstream_stmts {
        match &package.get_stmt(sid).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => stack.push(*e),
            StmtKind::Item(_) => {}
        }
    }

    while let Some(id) = stack.pop() {
        let expr = package.get_expr(id);
        match &expr.kind {
            ExprKind::Var(Res::Local(local), _) if *local == flag_id => return true,
            ExprKind::Assign(lhs, rhs) if extract_root_local(package, *lhs) == Some(flag_id) => {
                // Flag write: the LHS `Var(flag)` is a write target,
                // not a read. Recurse only into the RHS to catch any
                // flag reads embedded in the value being written.
                stack.push(*rhs);
                continue;
            }
            _ => {}
        }
        push_children(package, id, &mut stack);
    }
    false
}
