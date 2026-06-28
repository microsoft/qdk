// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `Return` detection for the return-unification pass.
//!
//! Delegates to the shared pre-order walker in [`crate::walk_utils`] so
//! that `ExprKind` variant coverage is maintained in a single location.
//!
//! `ExprKind::Closure` is treated as a leaf by [`crate::walk_utils::for_each_expr`]:
//! closure captures are [`qsc_fir::fir::LocalVarId`]s rather than
//! expressions, and the closure body lives in a separate callable that
//! `return_unify` visits independently.

#[cfg(test)]
mod tests;

use crate::walk_utils;
use qsc_fir::fir::{BlockId, ExprId, ExprKind, Package, PackageLookup, StmtId, StmtKind};

/// Returns `true` when `block_id` contains any `ExprKind::Return` at any depth.
pub(super) fn contains_return_in_block(package: &Package, block_id: BlockId) -> bool {
    let mut found = false;
    walk_utils::for_each_expr_in_block(package, block_id, &mut |_id, expr| {
        if matches!(expr.kind, ExprKind::Return(_)) {
            found = true;
        }
    });
    found
}

/// Returns `true` when the statement's initializer/expression contains any
/// `ExprKind::Return`.
pub(super) fn contains_return_in_stmt(package: &Package, stmt_id: StmtId) -> bool {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => {
            contains_return_in_expr(package, *expr_id)
        }
        StmtKind::Local(_, _, expr_id) => contains_return_in_expr(package, *expr_id),
        StmtKind::Item(_) => false,
    }
}

/// Return `true` when any sub-expression of `expr_id` is an `ExprKind::Return`.
///
/// Delegates to [`walk_utils::for_each_expr`], which walks every
/// sub-expression in pre-order and treats [`ExprKind::Closure`] as a leaf
/// (closure bodies live in separate callables). Does not short-circuit.
pub(super) fn contains_return_in_expr(package: &Package, expr_id: ExprId) -> bool {
    let mut found = false;
    walk_utils::for_each_expr(package, expr_id, &mut |_id, expr| {
        if matches!(expr.kind, ExprKind::Return(_)) {
            found = true;
        }
    });
    found
}
