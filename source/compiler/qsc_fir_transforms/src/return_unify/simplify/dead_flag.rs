// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dead-flag elimination simplifier rule.
//!
//! Drops `__has_returned = true;` assignment statements whose value is
//! never read on any downstream path within the simplifier's block of
//! operation. The rule runs last in [`super::run_to_fixpoint`] so the
//! structural rules ([`super::guard_clause`], [`super::both_branches`],
//! [`super::bare_return`]) have already collapsed the trailing merge
//! that originally consumed the flag — the leftover setter writes are
//! then statically dead.
//!
//! # Recognized shape
//!
//! Each candidate statement is `Semi(Assign(<flag_lhs>, _))` where
//! `<flag_lhs>` peels (via [`super::extract_root_local`]) to the
//! block's `__has_returned` local. The RHS is intentionally not
//! constrained: any value written to the flag whose result is unread
//! downstream is dead.
//!
//! # Recovering the flag local
//!
//! The flag local id is discovered by [`identify_has_returned_local`]:
//!
//! 1. **Primary**: if the block's trailing statement is the canonical
//!    merge `Expr(If(cond, _, Some(_)))`, recover the flag from
//!    `cond`'s `Var(Res::Local(_))` (mirrors
//!    [`super::let_folding`]'s use of [`super::extract_local_read`]).
//! 2. **Fallback**: scan the block's statements for a
//!    `mutable __has_returned : Bool` binding by name. This path is
//!    needed when the structural rules have already collapsed the
//!    merge; the binding remains in the block as long as a slot setter
//!    is still emitted upstream of the collapsed merge.
//!
//! The fallback uses the `"__has_returned"` name only as a last resort,
//! never as the primary signal: the flag transform always emits both
//! shapes, so the merge-cond path catches the common case without
//! relying on the synthesized name.
//!
//! # Downstream reader detection
//!
//! For each candidate at index `i`, [`downstream_has_flag_read`] walks
//! the statements at indices `i+1..end` of the block, descending into
//! every nested block reachable through `If`, `While`, `Block`, etc. A
//! `Var(Res::Local(flag_id))` read anywhere in that subtree marks the
//! candidate live. The LHS of any further `Assign(Var(flag_id), _)`
//! statement is *not* counted as a read: a flag write's LHS is the
//! target of the store, not a value read, and treating it as a read
//! would falsely keep one dead setter alive whenever multiple dead
//! setters cluster together.
//!
//! # Closures need no special handling
//!
//! `return_unify` synthesizes `__has_returned` after HIR -> FIR
//! lowering, but FIR lowering is also where closures are lifted and
//! their capture lists are finalized. The flag's `LocalVarId` did not
//! exist when those captures were computed, so no closure observed by
//! this rule can possibly carry the flag in its `ExprKind::Closure`
//! capture list. The lifted callable body referenced by a closure is a
//! separate top-level item that cannot reach the enclosing block's
//! locals except through its captures, so the flag is unreachable
//! through that path too. The walker therefore treats closures as
//! opaque leaves via [`super::push_children`], which neither recurses
//! into the closure body nor inspects the captures.
//!
//! # Safety
//!
//! `dead_flag` runs last in [`super::run_to_fixpoint`]'s iteration so
//! no upstream rule can introduce a new flag reader after `dead_flag`
//! has scanned for them in the same pass. The driver re-runs the
//! catalogue from the top whenever any rule fires; if a future rule
//! ordering change introduces a reader-inserting rule after
//! `dead_flag`, the next iteration's `dead_flag` pass will re-scan and
//! correctly refuse to drop the now-live setter.

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
/// Returns `true` when at least one flag-set statement was removed.
/// All eligible setters in the block are dropped in a single call;
/// the driver does not need to re-invoke the rule for fixpoint on this
/// rule alone, but the driver's outer loop guarantees a re-scan after
/// any other rule that may have reshaped the block.
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

    // Fallback: a `mutable __has_returned : Bool` binding in the block.
    // Only used when the merge has already been collapsed by the
    // structural rules.
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
