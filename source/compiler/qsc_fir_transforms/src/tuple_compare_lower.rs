// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tuple comparison lowering pass.
//!
//! Rewrites `BinOp(Eq/Neq)` on non-empty tuple-typed operands into
//! element-wise scalar comparisons joined by `AndL`/`OrL`.
//!
//! Establishes [`crate::invariants::InvariantLevel::PostTupleCompLower`]:
//! no `BinOp(Eq/Neq)` remains on tuple-typed operands in reachable code.
//!
//! # Pipeline position
//!
//! Runs after UDT erasure (which converts structs to tuples) and before
//! SROA (which decomposes tuple-typed locals into scalars). This ordering
//! is critical: SROA cannot decompose bindings that have whole-value uses
//! such as tuple equality, so this pass eliminates those uses first.
//!
//! # Input patterns
//!
//! - `BinOp(Eq | Neq, lhs, rhs)` where both operands are non-empty
//!   `Ty::Tuple`.
//!
//! # Rewrites
//!
//! ```text
//! // Before
//! BinOp(Eq, (a, b, c), (x, y, z))
//!
//! // After
//! AndL(AndL(Eq(a, x), Eq(b, y)), Eq(c, z))
//! ```
//!
//! Nested tuple operands recurse through `lower_single_cmp` so element
//! comparisons are themselves lowered before being folded.
//!
//! # Notes
//!
//! - Synthesized expressions use `EMPTY_EXEC_RANGE` (zero-length exec
//!   graph range). The [`crate::exec_graph_rebuild`] pass runs afterward
//!   and rebuilds correct exec graphs for the entire package, including
//!   the synthesized `AndL`/`OrL` nodes **and** any synthesized
//!   `Field(..)` accesses produced by `extract_or_field`.

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::EMPTY_EXEC_RANGE;
use crate::reachability::collect_reachable_from_entry;
use crate::walk_utils::for_each_expr_in_callable_impl;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, Expr, ExprId, ExprKind, Field, FieldPath, ItemKind, Package, PackageId, PackageLookup,
    PackageStore,
};
use qsc_fir::ty::{Prim, Ty};

/// Rewrites `BinOp(Eq/Neq)` on non-empty tuple-typed operands into
/// element-wise comparisons in the entry-reachable portion of a package.
///
/// Scope and idempotence:
///
/// - Scans only callables whose item reference lives in the target
///   package; cross-package items stay untouched.
/// - Returns early without modification when the target package has no
///   entry expression, since nothing is reachable to rewrite.
/// - Rewrites each matched expression **in place**, preserving its
///   original `ExprId` so downstream references (including
///   execution-graph re-linking) stay stable.
pub fn lower_tuple_comparisons(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) {
    let package = store.get(package_id);
    if package.entry.is_none() {
        return;
    }

    let reachable = collect_reachable_from_entry(store, package_id);
    let package = store.get(package_id);

    // Collect all ExprIds in reachable callables.
    let mut expr_ids: Vec<ExprId> = Vec::new();
    for item_id in &reachable {
        if item_id.package != package_id {
            continue;
        }
        let item = package.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            for_each_expr_in_callable_impl(package, &decl.implementation, &mut |id, _| {
                expr_ids.push(id);
            });
        }
    }

    let package = store.get_mut(package_id);
    for expr_id in expr_ids {
        lower_single_cmp(package, assigner, expr_id);
    }
}

/// Rewrites a single `BinOp(Eq/Neq)` expression with tuple-typed operands
/// into element-wise comparisons.
fn lower_single_cmp(package: &mut Package, assigner: &mut Assigner, expr_id: ExprId) {
    let expr = package.get_expr(expr_id);
    let (op, lhs_id, rhs_id) = match &expr.kind {
        ExprKind::BinOp(op @ (BinOp::Eq | BinOp::Neq), lhs, rhs) => (*op, *lhs, *rhs),
        _ => return,
    };
    let span = expr.span;

    let lhs_ty = package.get_expr(lhs_id).ty.clone();
    let elem_tys = match &lhs_ty {
        Ty::Tuple(elems) if !elems.is_empty() => elems.clone(),
        _ => return,
    };

    let joiner = match op {
        BinOp::Eq => BinOp::AndL,
        BinOp::Neq => BinOp::OrL,
        // Guarded by the outer `matches!(op, BinOp::Eq | BinOp::Neq)`
        // discriminant above; any other operator exits at the `match
        // &expr.kind` early-return.
        _ => unreachable!(),
    };

    // Extract element ExprIds: use existing Tuple element IDs when available,
    // otherwise synthesize Field accesses. This avoids creating Field
    // expressions with empty exec graph ranges on static tuple literals,
    // which would cause issues in the partial evaluator's static-classical
    // entry-eval path
    let lhs_elems = extract_or_field(package, assigner, lhs_id, &elem_tys, span);
    let rhs_elems = extract_or_field(package, assigner, rhs_id, &elem_tys, span);

    // Build element-wise comparisons.
    let mut cmp_ids: Vec<ExprId> = Vec::with_capacity(elem_tys.len());
    for i in 0..elem_tys.len() {
        let elem_cmp = make_bin_op(
            package,
            assigner,
            op,
            lhs_elems[i],
            rhs_elems[i],
            Ty::Prim(Prim::Bool),
            span,
        );
        // Recursively lower nested tuple comparisons.
        lower_single_cmp(package, assigner, elem_cmp);
        cmp_ids.push(elem_cmp);
    }

    // Fold element comparisons left-to-right with the joiner.
    let result_id = fold_left(package, assigner, &cmp_ids, joiner, span);

    // Rewrite the original expression in-place.
    let result_expr = package.get_expr(result_id);
    let result_kind = result_expr.kind.clone();
    let target = package.exprs.get_mut(expr_id).expect("expr exists");
    target.kind = result_kind;
    target.ty = Ty::Prim(Prim::Bool);
}

/// Extracts element `ExprId`s from a tuple-typed expression.
///
/// If the expression is `ExprKind::Tuple(es)`, returns the element IDs
/// directly. Otherwise, synthesizes `Field(expr, Path([i]))` for each
/// element.
fn extract_or_field(
    package: &mut Package,
    assigner: &mut Assigner,
    tuple_expr_id: ExprId,
    elem_tys: &[Ty],
    span: qsc_data_structures::span::Span,
) -> Vec<ExprId> {
    let expr = package.get_expr(tuple_expr_id);
    if let ExprKind::Tuple(es) = &expr.kind {
        assert_eq!(
            es.len(),
            elem_tys.len(),
            "tuple expression arity must match type arity"
        );
        return es.clone();
    }
    elem_tys
        .iter()
        .enumerate()
        .map(|(i, ty)| make_field_access(package, assigner, tuple_expr_id, i, ty.clone(), span))
        .collect()
}

/// Allocates a `Field(tuple_expr, Path([index]))` expression.
fn make_field_access(
    package: &mut Package,
    assigner: &mut Assigner,
    tuple_expr_id: ExprId,
    index: usize,
    elem_ty: Ty,
    span: qsc_data_structures::span::Span,
) -> ExprId {
    let id = assigner.next_expr();
    package.exprs.insert(
        id,
        Expr {
            id,
            span,
            ty: elem_ty,
            kind: ExprKind::Field(
                tuple_expr_id,
                Field::Path(FieldPath {
                    indices: vec![index],
                }),
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    id
}

/// Allocates a `BinOp(op, lhs, rhs)` expression.
fn make_bin_op(
    package: &mut Package,
    assigner: &mut Assigner,
    op: BinOp,
    lhs: ExprId,
    rhs: ExprId,
    ty: Ty,
    span: qsc_data_structures::span::Span,
) -> ExprId {
    let id = assigner.next_expr();
    package.exprs.insert(
        id,
        Expr {
            id,
            span,
            ty,
            kind: ExprKind::BinOp(op, lhs, rhs),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    id
}

/// Folds expressions left-to-right with a joiner operator.
///
/// `[a, b, c]` with `AndL` becomes `AndL(AndL(a, b), c)`.
fn fold_left(
    package: &mut Package,
    assigner: &mut Assigner,
    exprs: &[ExprId],
    joiner: BinOp,
    span: qsc_data_structures::span::Span,
) -> ExprId {
    assert!(!exprs.is_empty(), "fold_left requires at least one expr");
    let mut acc = exprs[0];
    for &e in &exprs[1..] {
        acc = make_bin_op(
            package,
            assigner,
            joiner,
            acc,
            e,
            Ty::Prim(Prim::Bool),
            span,
        );
    }
    acc
}
