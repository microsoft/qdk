// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tuple comparison lowering pass — runs after UDT erasure, before
//! tuple-decompose.
//!
//! Rewrites `BinOp(Eq/Neq)` on non-empty tuple-typed operands into
//! element-wise scalar comparisons joined by `AndL`/`OrL`. Literal tuples
//! are flattened without materializing aggregates. Operand effects and mutable
//! reads are snapshotted in source order before any comparison, so short-circuiting
//! cannot skip or replay their evaluation.
//! A balanced boolean tree keeps comparison depth logarithmic while retaining
//! left-to-right short-circuit order.
//!
//! # What to know before diving in
//!
//! - **Establishes [`crate::invariants::InvariantLevel::PostTupleCompLower`]:**
//!   no `BinOp(Eq/Neq)` on tuple operands remains in reachable code.
//! - **Ordering is load-bearing.** It must run before tuple-decompose, which
//!   cannot decompose a binding that has a whole-value use such as tuple
//!   equality; this pass removes those uses first.
//! - **Empty tuples (Unit) are excluded** — no elements means no element-wise
//!   comparison and no identity to seed the join, so `lower_single_cmp`
//!   returns early. Whole-Unit equality is left for downstream passes.
//! - **Aliased `ExprId`s by design (cross-pass contract).** Field projections
//!   share references to stable operand values, so a local-read `ExprId`
//!   can appear under multiple parent edges. The
//!   immediately-following [`crate::tuple_decompose`] `replace_expr_references`
//!   walk must tolerate this: redirecting one occurrence must not break the
//!   others, and the original aggregate may become dead once all parents are
//!   redirected. See the mirror note in [`crate::tuple_decompose`].
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds exec graphs (including the
//!   synthesized `AndL`/`OrL` and `Field(..)` nodes) later.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod semantic_equivalence_tests;

use crate::fir_builder::{
    alloc_bin_op_expr, alloc_block, alloc_expr_stmt, alloc_field_expr, alloc_local_var,
    alloc_local_var_expr, reachable_local_callables,
};
use crate::package_assigners::PackageAssigners;
use crate::reachability::{
    collect_reachable_from_entry, collect_reachable_package_closure, collect_reachable_with_seeds,
};
use crate::walk_utils::{
    assignment_written_locals, collect_expr_ids_in_entry_and_local_callables,
    collect_expr_ids_in_local_callables,
};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::PackageSpan;
use qsc_fir::fir::{
    BinOp, ExprId, ExprKind, LocalVarId, Mutability, Package, PackageId, PackageLookup,
    PackageStore, Res, StmtId, StoreItemId,
};
use qsc_fir::ty::{Prim, Ty};
use rustc_hash::FxHashSet;

/// Rewrites `BinOp(Eq/Neq)` on non-empty tuple-typed operands into
/// element-wise comparisons across the entry-reachable package closure.
///
/// Scope and idempotence:
///
/// - Walks every reachable callable in its owning package, minting any fresh
///   nodes from that package's own assigner.
/// - Returns early without modification when the entry package has no
///   entry expression, since nothing is reachable to rewrite.
/// - Rewrites each matched expression **in place**, preserving its
///   original `ExprId` so downstream references (including
///   execution-graph re-linking) stay stable.
pub fn lower_tuple_comparisons(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) {
    // Nothing is reachable to rewrite when the entry package has no entry
    // expression. `collect_reachable_from_entry` asserts an entry exists, so
    // guard before rooting reachability.
    if store.get(package_id).entry.is_none() {
        return;
    }
    let reachable = collect_reachable_from_entry(store, package_id);
    lower_tuple_comparisons_in_reachable(store, package_id, assigners, &reachable);
}

/// Seed-rooted variant of [`lower_tuple_comparisons`] for the
/// signature-preserving sub-pipeline.
///
/// Lowers tuple comparisons in entry-reachable code and in the `seeds` roots
/// (pinned target bodies and their transitive callees), each in its owning
/// package via [`PackageAssigners`]. Entry-reachable code already processed by
/// this pass holds no tuple comparisons, so the re-walk is a no-op there; only
/// the seeded bodies contribute fresh rewrites.
pub fn lower_tuple_comparisons_with_seeds(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    seeds: &[StoreItemId],
) {
    // With an empty seed set this mirrors the entry-only behavior: nothing is
    // reachable to rewrite when the package has no entry. A seed-only invocation
    // (codegen always supplies an entry) still proceeds.
    if store.get(package_id).entry.is_none() && seeds.is_empty() {
        return;
    }
    let reachable = collect_reachable_with_seeds(store, package_id, seeds);
    lower_tuple_comparisons_in_reachable(store, package_id, assigners, &reachable);
}

/// Lowers tuple comparisons across every package in `reachable`, minting fresh
/// nodes from each package's own assigner. Shared by the entry-rooted and
/// seed-rooted entry points.
fn lower_tuple_comparisons_in_reachable(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    reachable: &FxHashSet<StoreItemId>,
) {
    let pkg_ids: Vec<PackageId> = collect_reachable_package_closure(package_id, reachable)
        .into_iter()
        .collect();
    for pkg in pkg_ids {
        let assigner = assigners.get_mut(store, pkg);
        let package = store.get(pkg);

        // Collect reachable local callable item IDs for this package.
        let local_item_ids: Vec<_> = reachable_local_callables(package, pkg, reachable)
            .map(|(item_id, _)| item_id)
            .collect();

        // The entry expression lives only in the entry package; foreign
        // (library) packages have none, so only their callable bodies are
        // walked.
        let expr_ids = if pkg == package_id {
            collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids)
        } else {
            collect_expr_ids_in_local_callables(package, &local_item_ids)
        };

        let written_locals = expr_ids
            .iter()
            .flat_map(|id| assignment_written_locals(package, package.get_expr(*id)))
            .collect();
        let package = store.get_mut(pkg);
        for expr_id in expr_ids {
            lower_single_cmp(package, assigner, expr_id, &written_locals);
        }
    }
}

/// Lowers tuple equality or inequality without changing operand evaluation order.
///
/// For example, `(a(), b()) == (c(), d())` becomes:
/// ```text
/// let lhs_0 = a();
/// let lhs_1 = b();
/// let rhs_0 = c();
/// let rhs_1 = d();
/// (lhs_0 == rhs_0) and (lhs_1 == rhs_1)
/// ```
///
/// All four calls run before comparison can short-circuit. Constants and
/// unwritten locals need no temporary. The original comparison's ID is retained.
fn lower_single_cmp(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    written_locals: &FxHashSet<LocalVarId>,
) {
    let expr = package.get_expr(expr_id);
    let (op, lhs_id, rhs_id) = match &expr.kind {
        ExprKind::BinOp(op @ (BinOp::Eq | BinOp::Neq), lhs, rhs) => (*op, *lhs, *rhs),
        _ => return,
    };
    let span = expr.span;

    if !matches!(&package.get_expr(lhs_id).ty, Ty::Tuple(elems) if !elems.is_empty()) {
        return;
    }

    let joiner = match op {
        BinOp::Eq => BinOp::AndL,
        BinOp::Neq => BinOp::OrL,
        // Guarded by the outer `matches!(op, BinOp::Eq | BinOp::Neq)`
        // discriminant above; any other operator exits at the `match
        // &expr.kind` early-return.
        _ => unreachable!(),
    };

    let mut stmts = Vec::new();
    let mut lhs_elems = Vec::new();
    snapshot_operand(
        package,
        assigner,
        lhs_id,
        "_.cmp_lhs",
        written_locals,
        &mut stmts,
        &mut lhs_elems,
    );
    let mut rhs_elems = Vec::new();
    snapshot_operand(
        package,
        assigner,
        rhs_id,
        "_.cmp_rhs",
        written_locals,
        &mut stmts,
        &mut rhs_elems,
    );
    assert_eq!(
        lhs_elems.len(),
        rhs_elems.len(),
        "comparison tuple shapes must match"
    );

    // Build element-wise comparisons.
    let mut cmp_ids = Vec::with_capacity(lhs_elems.len());
    for (lhs, rhs) in lhs_elems.into_iter().zip(rhs_elems) {
        let elem_cmp =
            alloc_bin_op_expr(package, assigner, op, lhs, rhs, Ty::Prim(Prim::Bool), span);
        cmp_ids.push(elem_cmp);
    }

    let result_id = join_comparisons(package, assigner, &cmp_ids, joiner, span);
    let kind = if stmts.is_empty() {
        package.get_expr(result_id).kind.clone()
    } else {
        stmts.push(alloc_expr_stmt(package, assigner, result_id, span));
        let block = alloc_block(package, assigner, stmts, Ty::Prim(Prim::Bool), span);
        ExprKind::Block(block)
    };

    // Rewrite the original expression in-place.
    let target = package.exprs.get_mut(expr_id).expect("expr exists");
    target.kind = kind;
    target.ty = Ty::Prim(Prim::Bool);
}

/// Flattens literal tuples, eagerly saving any leaves whose evaluation cannot
/// move past later operands. Computed tuples are saved once before projection.
fn snapshot_operand(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    name: &str,
    written_locals: &FxHashSet<LocalVarId>,
    stmts: &mut Vec<StmtId>,
    leaves: &mut Vec<ExprId>,
) {
    let expr = package.get_expr(expr_id).clone();
    let elements = if let ExprKind::Tuple(elements) = expr.kind
        && !elements.is_empty()
    {
        elements
    } else {
        // Purity alone is insufficient: reads may observe later writes, and
        // side-effect-free expressions may fail on a short-circuited path.
        let mut base = expr_id;
        while let ExprKind::Field(target, _) = package.get_expr(base).kind {
            base = target;
        }
        let stable = match package.get_expr(base).kind {
            ExprKind::Lit(_) => true,
            ExprKind::Var(Res::Local(local), _) => !written_locals.contains(&local),
            ExprKind::Tuple(ref elements) => elements.is_empty(),
            _ => false,
        };
        let value = if stable {
            expr_id
        } else {
            let (local, binding) = alloc_local_var(
                package,
                assigner,
                name,
                &expr.ty,
                expr_id,
                Mutability::Immutable,
            );
            stmts.push(binding);
            alloc_local_var_expr(package, assigner, local, expr.ty.clone(), expr.span)
        };

        if let Ty::Tuple(types) = &expr.ty
            && !types.is_empty()
        {
            types
                .iter()
                .enumerate()
                .map(|(index, ty)| {
                    alloc_field_expr(package, assigner, value, index, ty.clone(), expr.span)
                })
                .collect()
        } else {
            leaves.push(value);
            return;
        }
    };
    for (index, element) in elements.into_iter().enumerate() {
        snapshot_operand(
            package,
            assigner,
            element,
            &format!("{name}_{index}"),
            written_locals,
            stmts,
            leaves,
        );
    }
}

/// Balances the boolean tree without reordering comparisons or changing
/// short-circuit behavior. A linear fold would turn shallow, wide tuples into
/// deeply nested FIR for recursive downstream consumers.
fn join_comparisons(
    package: &mut Package,
    assigner: &mut Assigner,
    exprs: &[ExprId],
    joiner: BinOp,
    span: PackageSpan,
) -> ExprId {
    assert!(
        !exprs.is_empty(),
        "comparison must have at least one element"
    );
    if let [expr] = exprs {
        return *expr;
    }
    let (left, right) = exprs.split_at(exprs.len() / 2);
    let left = join_comparisons(package, assigner, left, joiner, span);
    let right = join_comparisons(package, assigner, right, joiner, span);
    alloc_bin_op_expr(
        package,
        assigner,
        joiner,
        left,
        right,
        Ty::Prim(Prim::Bool),
        span,
    )
}
