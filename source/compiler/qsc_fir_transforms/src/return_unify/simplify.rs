// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Post-flag-transform simplifier catalogue.
//!
//! After [`super::transform_block_with_flags`] lowers a return-bearing
//! block through the flag/slot model, this module folds the canonical
//! flag-output shapes back into structured form with named,
//! individually-tested rewrite rules. This mirrors the structural recovery
//! LLVM's `SimplifyCFG` performs after `mergereturn` and the
//! Erosa-Hendren named-rewrite-catalogue pattern.
//!
//! # Rule signature convention
//!
//! Each rule is a free function `apply(package, assigner, block_id, slots)
//! -> bool` that mutates `block_id` in place and returns `true` iff it
//! fired. Rules rewrite whole `Block.stmts` sequences rather than single
//! expressions, so an `Option<ExprId>` return could not express their
//! stmt-list rewrites; mutating in place also reuses the `alloc_*`
//! builders in [`super`] directly.
//!
//! # Fixpoint driver
//!
//! [`run_to_fixpoint`] iterates the catalogue until no rule fires, using a
//! measure-based divergence detector (statement count + identical-branch
//! count) with a per-block hard cap to surface divergent rules without
//! panicking.
//!
//! # Rule ordering
//!
//! [`try_fold_identical_branches`] runs first, then the structural rules
//! ([`guard_clause`], [`both_branches`], [`bare_return`]) against the
//! pre-fold shape, then [`let_folding`] inlines any remaining
//! `__trailing_result` binding the structural rules did not consume.
//! [`dead_flag`] then drops flag-set assignments with no downstream
//! reader, and [`dead_local`] runs last to remove the now-unused
//! `__has_returned` / `__ret_val` declarations.
//!
//! The structural rules run before [`let_folding`] because their patterns
//! include the lazy `if not __has_returned` continuation as a separate
//! statement between the guard set and the merge; folding it into the
//! merge's else-arm first would prevent [`guard_clause`] from matching.

mod bare_return;
mod both_branches;
mod dead_flag;
mod dead_local;

pub(super) use dead_local::init_is_side_effect_free;
mod guard_clause;
mod let_folding;
mod single_branch;

#[cfg(test)]
mod tests;

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, ExprId, ExprKind, Lit, LocalVarId, Mutability, Package, PackageLookup, PatKind,
        Res, StmtId, StmtKind,
    },
    ty::{Prim, Ty},
};

use super::lower::SynthSlots;
use crate::walk_utils;

/// Run the simplifier catalogue to fixpoint on `block_id`.
///
/// Iterates the rule catalogue until no rule fires. Uses a measure-based
/// divergence detector: the tuple `(block.stmts.len(),
/// count_identical_branch_heads)` must strictly decrease across
/// consecutive `changed = true` iterations. A hard cap of
/// `stmts.len() * 4 + 16` guards against unbounded looping.
///
/// On divergence or hard-cap exhaustion, pushes
/// [`super::Error::FixpointNotReached`] and returns without panicking.
pub(super) fn run_to_fixpoint(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    errors: &mut Vec<super::Error>,
    slots: &SynthSlots,
) {
    let hard_cap = {
        let block = package.get_block(block_id);
        block.stmts.len() * 4 + 16
    };
    let mut prev_measure: Option<(usize, usize)> = None;
    for _ in 0..hard_cap {
        let changed = apply_all_rules(package, assigner, block_id, slots);
        if !changed {
            return;
        }
        let block = package.get_block(block_id);
        let measure = (
            block.stmts.len(),
            count_identical_branch_heads(package, &block.stmts),
        );
        if matches!(prev_measure, Some(prev) if measure >= prev) {
            errors.push(super::Error::FixpointNotReached {
                phase: "simplify",
                block: block_id,
            });
            return;
        }
        prev_measure = Some(measure);
    }
    // Hard cap reached without convergence.
    errors.push(super::Error::FixpointNotReached {
        phase: "simplify",
        block: block_id,
    });
}

/// Run all simplifier rules once and return whether any rule fired.
fn apply_all_rules(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    slots: &SynthSlots,
) -> bool {
    let mut changed = false;
    changed |= guard_clause::apply(package, assigner, block_id, slots);
    changed |= run_identical_branches(package, block_id);
    changed |= both_branches::apply(package, assigner, block_id, slots);
    changed |= single_branch::apply(package, assigner, block_id, slots);
    changed |= bare_return::apply(package, assigner, block_id, slots);
    changed |= let_folding::apply(package, assigner, block_id, slots);
    changed |= dead_flag::apply(package, assigner, block_id, slots);
    changed |= dead_local::apply(package, assigner, block_id);
    changed
}

/// Count how many top-level `If` statements in `stmts` have structurally
/// equal then/else branches — the pattern [`run_identical_branches`] folds.
fn count_identical_branch_heads(package: &Package, stmts: &[StmtId]) -> usize {
    stmts
        .iter()
        .filter(|&&stmt_id| {
            let expr_id = match &package.get_stmt(stmt_id).kind {
                StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
                StmtKind::Item(_) => return false,
            };
            try_fold_identical_branches(package, expr_id).is_some()
        })
        .count()
}

/// Drive the legacy identical-branches fold across every statement of
/// `block_id`.
///
/// Equivalent to the pre-refactor inline `simplify_flag_patterns` body:
/// walk each top-level statement and fold its initializer/trailing
/// expression when the expression is `If(_, then, Some(else))` with
/// structurally identical arms.
fn run_identical_branches(package: &mut Package, block_id: BlockId) -> bool {
    let stmts = package.get_block(block_id).stmts.clone();
    let mut changed = false;
    for stmt_id in stmts {
        changed |= fold_identical_branches_in_stmt(package, stmt_id);
    }
    changed
}

/// Fold `If(c, x, Some(x))` → `x` for the expression at the head of
/// `stmt_id`. Returns `true` when the fold fires.
fn fold_identical_branches_in_stmt(package: &mut Package, stmt_id: StmtId) -> bool {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
        StmtKind::Item(_) => return false,
    };
    let Some(replacement) = try_fold_identical_branches(package, expr_id) else {
        return false;
    };
    let stmt = package.stmts.get_mut(stmt_id).expect("stmt not found");
    match &mut stmt.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
            *e = replacement;
        }
        StmtKind::Item(_) => return false,
    }
    true
}

/// If `expr_id` is an `If(cond, then_expr, Some(else_expr))` where the
/// then and else branches are structurally identical, return the branch
/// expression id to replace the if with. Returns `None` otherwise.
pub(super) fn try_fold_identical_branches(package: &Package, expr_id: ExprId) -> Option<ExprId> {
    let expr = package.get_expr(expr_id);
    let ExprKind::If(_, then_id, Some(else_id)) = &expr.kind else {
        return None;
    };
    if exprs_structurally_equal(package, *then_id, *else_id) {
        Some(*then_id)
    } else {
        None
    }
}

/// Peels a trivial `Block([Expr(x)])` wrapper, returning the inner `x`.
/// Returns the original `ExprId` unchanged for all other expression kinds.
fn peel_single_expr_block(package: &Package, expr_id: ExprId) -> ExprId {
    if let ExprKind::Block(block_id) = package.get_expr(expr_id).kind {
        let block = package.get_block(block_id);
        if let [single_stmt_id] = block.stmts.as_slice()
            && let StmtKind::Expr(inner_id) = package.get_stmt(*single_stmt_id).kind
        {
            return inner_id;
        }
    }
    expr_id
}

/// Recursively compare two expression trees for structural equality.
///
/// Two expressions are structurally equal when their `ExprKind` variants
/// match and all recursive children are structurally equal. Span and
/// exec-graph metadata are ignored; only the semantic shape matters.
///
/// This is intentionally conservative: any unknown or complex pattern
/// returns `false` to avoid incorrect folding.
///
/// Trivial single-expression blocks (`Block([Expr(x)])`) are peeled before
/// comparison so they match the bare expression `x`. This lets
/// `identical_branches` fold degenerate merges whose arms differ only in
/// block wrapping.
pub(super) fn exprs_structurally_equal(package: &Package, a: ExprId, b: ExprId) -> bool {
    // Peel trivial single-expression blocks so `Block([Expr(x)])` matches `x`.
    let a = peel_single_expr_block(package, a);
    let b = peel_single_expr_block(package, b);
    if a == b {
        return true;
    }
    let ea = package.get_expr(a);
    let eb = package.get_expr(b);
    if ea.ty != eb.ty {
        return false;
    }
    match (&ea.kind, &eb.kind) {
        (ExprKind::Var(res_a, args_a), ExprKind::Var(res_b, args_b)) => {
            res_a == res_b && args_a == args_b
        }
        (ExprKind::Lit(lit_a), ExprKind::Lit(lit_b)) => lit_a == lit_b,
        (ExprKind::Tuple(elems_a), ExprKind::Tuple(elems_b)) => {
            elems_a.len() == elems_b.len()
                && elems_a
                    .iter()
                    .zip(elems_b.iter())
                    .all(|(&a, &b)| exprs_structurally_equal(package, a, b))
        }
        (ExprKind::Block(bid_a), ExprKind::Block(bid_b)) => {
            blocks_structurally_equal(package, *bid_a, *bid_b)
        }
        (ExprKind::UnOp(op_a, operand_a), ExprKind::UnOp(op_b, operand_b)) => {
            op_a == op_b && exprs_structurally_equal(package, *operand_a, *operand_b)
        }
        (ExprKind::BinOp(op_a, l_a, r_a), ExprKind::BinOp(op_b, l_b, r_b)) => {
            op_a == op_b
                && exprs_structurally_equal(package, *l_a, *l_b)
                && exprs_structurally_equal(package, *r_a, *r_b)
        }
        (ExprKind::If(c_a, t_a, e_a), ExprKind::If(c_b, t_b, e_b)) => {
            exprs_structurally_equal(package, *c_a, *c_b)
                && exprs_structurally_equal(package, *t_a, *t_b)
                && match (e_a, e_b) {
                    (Some(ea), Some(eb)) => exprs_structurally_equal(package, *ea, *eb),
                    (None, None) => true,
                    _ => false,
                }
        }
        (ExprKind::Array(a_elems), ExprKind::Array(b_elems))
        | (ExprKind::ArrayLit(a_elems), ExprKind::ArrayLit(b_elems)) => {
            a_elems.len() == b_elems.len()
                && a_elems
                    .iter()
                    .zip(b_elems.iter())
                    .all(|(&a, &b)| exprs_structurally_equal(package, a, b))
        }
        // Conservative: anything else is considered non-equal.
        _ => false,
    }
}

/// Recursively compare two blocks for structural equality.
pub(super) fn blocks_structurally_equal(package: &Package, a: BlockId, b: BlockId) -> bool {
    if a == b {
        return true;
    }
    let ba = package.get_block(a);
    let bb = package.get_block(b);
    if ba.ty != bb.ty || ba.stmts.len() != bb.stmts.len() {
        return false;
    }
    ba.stmts
        .iter()
        .zip(bb.stmts.iter())
        .all(|(&sa, &sb)| stmts_structurally_equal(package, sa, sb))
}

/// Recursively compare two statements for structural equality.
pub(super) fn stmts_structurally_equal(package: &Package, a: StmtId, b: StmtId) -> bool {
    if a == b {
        return true;
    }
    let sa = package.get_stmt(a);
    let sb = package.get_stmt(b);
    match (&sa.kind, &sb.kind) {
        (StmtKind::Expr(ea), StmtKind::Expr(eb)) | (StmtKind::Semi(ea), StmtKind::Semi(eb)) => {
            exprs_structurally_equal(package, *ea, *eb)
        }
        (StmtKind::Local(m_a, p_a, e_a), StmtKind::Local(m_b, p_b, e_b)) => {
            m_a == m_b && p_a == p_b && exprs_structurally_equal(package, *e_a, *e_b)
        }
        _ => false,
    }
}

/// Discard import to silence unused warning until span-using rules land.
#[allow(dead_code)]
const _: Option<Span> = None;

// ---------------------------------------------------------------------------
// Shared slot/flag identification helpers used by the per-rule modules.
//
// Each anchors on the canonical trailing merge expression and the
// `__ret_val = v; __has_returned = true;` slot-set sequence. They stay
// narrow: each returns `Option<_>` and never mutates the IR.
// ---------------------------------------------------------------------------

/// Slot identities extracted from a trailing
/// `if __has_returned { __ret_val } else { ... }` merge.
pub(super) struct MergeInfo {
    pub(super) has_returned: LocalVarId,
    pub(super) return_slot: LocalVarId,
}

/// Identify the trailing merge expression and recover the slot
/// [`LocalVarId`]s. Returns `None` if `stmt_id` is not the canonical
/// merge shape or the slot types do not match `block_ty`.
pub(super) fn identify_merge(
    package: &Package,
    stmt_id: StmtId,
    block_ty: &Ty,
) -> Option<MergeInfo> {
    let StmtKind::Expr(expr_id) = package.get_stmt(stmt_id).kind else {
        return None;
    };
    let merge_expr = package.get_expr(expr_id);
    if merge_expr.ty != *block_ty {
        return None;
    }
    let ExprKind::If(cond_id, then_id, Some(else_id)) = &merge_expr.kind else {
        return None;
    };
    // Both arms must have the block's value type, so the rewrite preserves typing.
    if package.get_expr(*then_id).ty != *block_ty || package.get_expr(*else_id).ty != *block_ty {
        return None;
    }
    let has_returned = extract_local_read(package, *cond_id, Some(&Ty::Prim(Prim::Bool)))?;
    let return_slot = extract_then_arm_slot_read(package, *then_id, block_ty)?;
    Some(MergeInfo {
        has_returned,
        return_slot,
    })
}

/// Identify the slot/flag locals from the trailing statement of
/// `block_id`, preferring the canonical [`identify_merge`] shape and
/// falling back to a bare `Expr(Var(__ret_val))` trailing read, recovering
/// the `__has_returned` flag by [`SynthSlots`] id from the block's
/// `mutable` declarations.
///
/// The bare-trailing path fires when the flag-strategy lowering emitted
/// no merge expression — typically when the return is the entire body and
/// no fallthrough value exists to merge with.
///
/// Returns `(has_returned, return_slot)`.
pub(super) fn identify_merge_or_trailing_slot(
    package: &Package,
    block_id: BlockId,
    tail_stmt: StmtId,
    block_ty: &Ty,
    slots: &SynthSlots,
) -> Option<(LocalVarId, LocalVarId)> {
    if let Some(merge) = identify_merge(package, tail_stmt, block_ty) {
        return Some((merge.has_returned, merge.return_slot));
    }
    identify_trailing_slot_read(package, block_id, tail_stmt, slots)
}

/// Recognizes a bare trailing `Expr(Var(__ret_val))` final statement and
/// recovers the slot/flag [`LocalVarId`]s by matching the block's
/// `mutable` Local declarations against the [`SynthSlots`] ids.
///
/// Used by [`identify_merge_or_trailing_slot`] as the fallback for
/// shapes that lack the canonical merge expression.
pub(super) fn identify_trailing_slot_read(
    package: &Package,
    block_id: BlockId,
    tail_stmt: StmtId,
    slots: &SynthSlots,
) -> Option<(LocalVarId, LocalVarId)> {
    let StmtKind::Expr(expr_id) = package.get_stmt(tail_stmt).kind else {
        return None;
    };
    let ExprKind::Var(Res::Local(slot_id), _) = &package.get_expr(expr_id).kind else {
        return None;
    };
    let slot_id = *slot_id;

    let mut slot_matches = false;
    let mut flag_id = None;
    for &sid in &package.get_block(block_id).stmts {
        let StmtKind::Local(Mutability::Mutable, pat_id, _) = package.get_stmt(sid).kind else {
            continue;
        };
        let pat = package.get_pat(pat_id);
        let PatKind::Bind(ident) = &pat.kind else {
            continue;
        };
        if ident.id == slot_id && ident.id == slots.return_slot.var_id {
            slot_matches = true;
        } else if pat.ty == Ty::Prim(Prim::Bool) && ident.id == slots.has_returned {
            flag_id = Some(ident.id);
        }
    }
    if !slot_matches {
        return None;
    }
    Some((flag_id?, slot_id))
}

/// Returns `Some(local_id)` when `expr_id` reads a single `Local` whose
/// type matches `expected_ty` (when provided).
pub(super) fn extract_local_read(
    package: &Package,
    expr_id: ExprId,
    expected_ty: Option<&Ty>,
) -> Option<LocalVarId> {
    let e = package.get_expr(expr_id);
    if let Some(ty) = expected_ty
        && e.ty != *ty
    {
        return None;
    }
    let ExprKind::Var(Res::Local(id), _) = &e.kind else {
        return None;
    };
    Some(*id)
}

/// Returns the `LocalVarId` read by the merge-then arm of the canonical
/// flag-strategy merge expression.
///
/// Accepts two equivalent shapes:
///
/// * A `Block` containing exactly one `Expr(Var(Res::Local(_), _))`
///   statement of type `return_ty` (the legacy "split" shape).
/// * A bare `Var(Res::Local(_), _)` expression of type `return_ty` (the
///   shape emitted by `create_flag_trailing_expr_for_slot`, which
///   wraps the slot read directly inside the merge `If`).
pub(super) fn extract_then_arm_slot_read(
    package: &Package,
    then_expr_id: ExprId,
    return_ty: &Ty,
) -> Option<LocalVarId> {
    let then_expr = package.get_expr(then_expr_id);
    match &then_expr.kind {
        ExprKind::Block(bid) => {
            let blk = package.get_block(*bid);
            if blk.stmts.len() != 1 {
                return None;
            }
            let StmtKind::Expr(inner_expr_id) = package.get_stmt(blk.stmts[0]).kind else {
                return None;
            };
            extract_local_read(package, inner_expr_id, Some(return_ty))
        }
        ExprKind::Var(_, _) => extract_local_read(package, then_expr_id, Some(return_ty)),
        _ => None,
    }
}

/// Returns `Some(rhs_id)` when `assign_expr_id` is `Assign(Var(slot), rhs)`
/// where `rhs` has type `return_ty`.
pub(super) fn match_slot_assign(
    package: &Package,
    assign_expr_id: ExprId,
    return_slot: LocalVarId,
    return_ty: &Ty,
) -> Option<ExprId> {
    let e = package.get_expr(assign_expr_id);
    let ExprKind::Assign(lhs_id, rhs_id) = &e.kind else {
        return None;
    };
    let lhs_local = extract_local_read(package, *lhs_id, None)?;
    if lhs_local != return_slot {
        return None;
    }
    if package.get_expr(*rhs_id).ty != *return_ty {
        return None;
    }
    Some(*rhs_id)
}

/// Returns `true` when `assign_expr_id` is `Assign(Var(has_returned), true)`.
pub(super) fn match_flag_set(
    package: &Package,
    assign_expr_id: ExprId,
    has_returned: LocalVarId,
) -> bool {
    let e = package.get_expr(assign_expr_id);
    let ExprKind::Assign(lhs_id, rhs_id) = &e.kind else {
        return false;
    };
    let Some(lhs_local) = extract_local_read(package, *lhs_id, Some(&Ty::Prim(Prim::Bool))) else {
        return false;
    };
    if lhs_local != has_returned {
        return false;
    }
    matches!(
        &package.get_expr(*rhs_id).kind,
        ExprKind::Lit(Lit::Bool(true))
    )
}

/// Inspect an `if`-arm expression body and return the slot-write RHS
/// when the body matches the canonical
/// `{ __ret_val = v; __has_returned = true; }` slot-set sequence.
///
/// Accepts two equivalent shapes produced by the flag transform:
///
/// * `[Semi(Block([Semi(slot_assign), Semi(flag_assign)]))]` — the
///   nested in-place `Return(v)` rewrite, where the original `Return`
///   expression became a Unit-typed block.
/// * `[Semi(slot_assign), Semi(flag_assign)]` — the flat form, accepted
///   for robustness against pretty-printer-equivalent shape drift.
///
/// Returns `None` when `arm_expr_id` is not a `Block` carrying one of
/// those shapes, or when the slot/flag references don't match the
/// supplied identities.
pub(super) fn match_slot_set_arm(
    package: &Package,
    arm_expr_id: ExprId,
    has_returned: LocalVarId,
    return_slot: LocalVarId,
    return_ty: &Ty,
) -> Option<ExprId> {
    let arm_expr = package.get_expr(arm_expr_id);
    let ExprKind::Block(outer_bid) = &arm_expr.kind else {
        return None;
    };
    let outer_stmts = package.get_block(*outer_bid).stmts.clone();

    let assign_stmts: Vec<StmtId> = if outer_stmts.len() == 1 {
        let StmtKind::Semi(inner_expr_id) = package.get_stmt(outer_stmts[0]).kind else {
            return None;
        };
        let ExprKind::Block(inner_bid) = &package.get_expr(inner_expr_id).kind else {
            return None;
        };
        package.get_block(*inner_bid).stmts.clone()
    } else if outer_stmts.len() == 2 {
        outer_stmts
    } else {
        return None;
    };

    if assign_stmts.len() != 2 {
        return None;
    }
    let StmtKind::Semi(slot_assign_id) = package.get_stmt(assign_stmts[0]).kind else {
        return None;
    };
    let StmtKind::Semi(flag_assign_id) = package.get_stmt(assign_stmts[1]).kind else {
        return None;
    };

    let v_id = match_slot_assign(package, slot_assign_id, return_slot, return_ty)?;
    if !match_flag_set(package, flag_assign_id, has_returned) {
        return None;
    }
    Some(v_id)
}

/// Returns `true` when any sub-expression reachable from `expr_id` has a
/// type that mentions `Ty::Prim(Prim::Qubit)` (directly or under
/// `Array`/`Tuple` wrappers).
///
/// Delegates to [`walk_utils::for_each_expr`] for the tree traversal.
/// Does not short-circuit: the walker visits every reachable node even
/// after a qubit-typed expression is found.
///
/// Used as a conservative bailout: the `both_branches` rule moves the
/// slot-write RHS into the value position of a structured `if`, and we
/// refuse to do so if the value can carry a qubit reference. In
/// practice user-written Q# can never return qubits, so this walker
/// almost never fires; it exists to keep direct-IR consumers safe.
pub(super) fn expr_tree_contains_qubit_type(package: &Package, expr_id: ExprId) -> bool {
    let mut found = false;
    walk_utils::for_each_expr(package, expr_id, &mut |_id, expr| {
        if ty_contains_qubit(&expr.ty) {
            found = true;
        }
    });
    found
}

/// Walk `bid`'s statements and push every reachable expression onto `stack`.
pub(super) fn push_block_exprs(package: &Package, bid: BlockId, stack: &mut Vec<ExprId>) {
    let blk = package.get_block(bid);
    for &sid in &blk.stmts {
        match &package.get_stmt(sid).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => stack.push(*e),
            StmtKind::Item(_) => {}
        }
    }
}

/// Peel `Index`/`Field` projection wrappers and return the underlying
/// root local id when `expr_id` resolves to a local read.
///
/// Shared across the simplifier rules that need to recognize slot
/// reads or writes through the array-backed and UDT slot strategies:
/// `__ret_val`, `__ret_val[0]`, and `__ret_val::field` all peel to the
/// same root local id.
pub(super) fn extract_root_local(package: &Package, expr_id: ExprId) -> Option<LocalVarId> {
    let mut current = expr_id;
    loop {
        match &package.get_expr(current).kind {
            ExprKind::Var(Res::Local(id), _) => return Some(*id),
            ExprKind::Index(inner, _) | ExprKind::Field(inner, _) => current = *inner,
            _ => return None,
        }
    }
}

/// Push every immediate child expression of `expr_id` onto `stack`,
/// including statement initializers reachable through inner blocks.
///
/// Companion to [`push_block_exprs`]: shared by the simplifier rules
/// whose bailout scanners walk an expression tree exhaustively (e.g.
/// [`let_folding`]'s slot-write detector and [`dead_flag`]'s flag-read
/// detector). Closures contribute no children here; rules that need to
/// treat closure presence as a bailout must check `ExprKind::Closure`
/// at the visit step, not at the child-push step.
pub(super) fn push_children(package: &Package, expr_id: ExprId, stack: &mut Vec<ExprId>) {
    match &package.get_expr(expr_id).kind {
        ExprKind::Array(elems) | ExprKind::ArrayLit(elems) | ExprKind::Tuple(elems) => {
            stack.extend(elems.iter().copied());
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Index(a, b)
        | ExprKind::Call(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            stack.push(*a);
            stack.push(*b);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            stack.push(*a);
            stack.push(*b);
            stack.push(*c);
        }
        ExprKind::UnOp(_, inner)
        | ExprKind::Field(inner, _)
        | ExprKind::Return(inner)
        | ExprKind::Fail(inner) => stack.push(*inner),
        ExprKind::If(c, t, e) => {
            stack.push(*c);
            stack.push(*t);
            if let Some(e) = e {
                stack.push(*e);
            }
        }
        ExprKind::Block(bid) => push_block_exprs(package, *bid, stack),
        ExprKind::While(cond, bid) => {
            stack.push(*cond);
            push_block_exprs(package, *bid, stack);
        }
        ExprKind::Range(a, b, c) => {
            if let Some(e) = a {
                stack.push(*e);
            }
            if let Some(e) = b {
                stack.push(*e);
            }
            if let Some(e) = c {
                stack.push(*e);
            }
        }
        ExprKind::String(parts) => {
            for p in parts {
                if let qsc_fir::fir::StringComponent::Expr(e) = p {
                    stack.push(*e);
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                stack.push(*c);
            }
            stack.extend(fields.iter().map(|f| f.value));
        }
        ExprKind::Closure(_, _) | ExprKind::Var(_, _) | ExprKind::Lit(_) | ExprKind::Hole => {}
    }
}

/// Returns `true` when `ty` mentions `Ty::Prim(Prim::Qubit)` anywhere in
/// its structure.
fn ty_contains_qubit(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(Prim::Qubit) => true,
        Ty::Array(inner) => ty_contains_qubit(inner),
        Ty::Tuple(items) => items.iter().any(ty_contains_qubit),
        Ty::Prim(_) | Ty::Arrow(_) | Ty::Infer(_) | Ty::Param(_) | Ty::Udt(_) | Ty::Err => false,
    }
}

/// Count the number of references to `target` reachable from `root`.
///
/// Counts each `ExprKind::Var(Res::Local(target), _)` occurrence and
/// each entry in a `ExprKind::Closure` capture list whose value equals
/// `target`. Recurses through every reachable sub-expression and
/// statement initializer, mirroring [`expr_tree_contains_qubit_type`]'s
/// walk order. Used by [`let_folding`] to confirm a let-bound local has
/// exactly one downstream use before inlining its initializer.
pub(super) fn local_use_count(package: &Package, root: ExprId, target: LocalVarId) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let expr = package.get_expr(id);
        match &expr.kind {
            ExprKind::Var(Res::Local(local), _) => {
                if *local == target {
                    count += 1;
                }
            }
            ExprKind::Closure(captures, _) => {
                count += captures.iter().filter(|&&id| id == target).count();
            }
            ExprKind::Array(elems) | ExprKind::ArrayLit(elems) | ExprKind::Tuple(elems) => {
                stack.extend(elems.iter().copied());
            }
            ExprKind::ArrayRepeat(a, b)
            | ExprKind::Assign(a, b)
            | ExprKind::AssignOp(_, a, b)
            | ExprKind::BinOp(_, a, b)
            | ExprKind::Index(a, b)
            | ExprKind::Call(a, b)
            | ExprKind::AssignField(a, _, b)
            | ExprKind::UpdateField(a, _, b) => {
                stack.push(*a);
                stack.push(*b);
            }
            ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
                stack.push(*a);
                stack.push(*b);
                stack.push(*c);
            }
            ExprKind::UnOp(_, inner)
            | ExprKind::Field(inner, _)
            | ExprKind::Return(inner)
            | ExprKind::Fail(inner) => {
                stack.push(*inner);
            }
            ExprKind::If(c, t, e) => {
                stack.push(*c);
                stack.push(*t);
                if let Some(e) = e {
                    stack.push(*e);
                }
            }
            ExprKind::Block(bid) => push_block_exprs(package, *bid, &mut stack),
            ExprKind::While(cond, bid) => {
                stack.push(*cond);
                push_block_exprs(package, *bid, &mut stack);
            }
            ExprKind::Range(a, b, c) => {
                if let Some(e) = a {
                    stack.push(*e);
                }
                if let Some(e) = b {
                    stack.push(*e);
                }
                if let Some(e) = c {
                    stack.push(*e);
                }
            }
            ExprKind::String(parts) => {
                for p in parts {
                    if let qsc_fir::fir::StringComponent::Expr(e) = p {
                        stack.push(*e);
                    }
                }
            }
            ExprKind::Struct(_, copy, fields) => {
                if let Some(c) = copy {
                    stack.push(*c);
                }
                stack.extend(fields.iter().map(|f| f.value));
            }
            ExprKind::Lit(_) | ExprKind::Hole | ExprKind::Var(_, _) => {}
        }
    }
    count
}
