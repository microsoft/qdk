// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared expression-tree walkers for FIR transform passes.
//!
//! Provides [`for_each_expr`], a closure-based pre-order walker that
//! eliminates duplicated `ExprKind` matching across transform modules.
//!
//! # Helper surface
//!
//! The module exposes three families of helpers:
//!
//! - **Closure-based pre-order walkers.** [`for_each_expr`] visits a single
//!   expression and its descendants; [`for_each_expr_in_block`] visits every
//!   expression within a block; [`for_each_expr_in_callable_impl`] visits
//!   every expression across all specializations of a [`CallableImpl`]. None
//!   of these recurse into closure bodies — [`ExprKind::Closure`] is treated
//!   as a leaf, so callables reached only through a closure capture are not
//!   visited transitively.
//! - **Local-variable use classification.** [`collect_uses_in_block`] and
//!   [`collect_uses_in_expr`] record every occurrence of a [`LocalVarId`],
//!   classifying each as either a *field-only* use or a *whole-value* use.
//!   See [`# Use classification`](#use-classification) below for the rules.
//! - **Reachable-`ExprId` collectors.** [`collect_expr_ids_in_entry`],
//!   [`collect_expr_ids_in_local_callables`], and
//!   [`collect_expr_ids_in_entry_and_local_callables`] return every
//!   [`ExprId`] reachable from the given roots, deduplicated.
//!   [`extend_expr_ids_in_local_callables`] is the in-place variant used to
//!   accumulate IDs across roots while sharing a single dedup set.
//!
//! # Use classification
//!
//! The [`collect_uses_in_block`] and [`collect_uses_in_expr`] helpers
//! classify every occurrence of a [`LocalVarId`] as either a *field-only*
//! use or a *whole-value* use. Tuple-decomposing passes rely on that
//! distinction to decide whether a local can be scalarized safely.
//!
//! - A **"use"** is any expression that mentions the local: a
//!   `Var(Res::Local(local))` read, a [`Closure`](ExprKind::Closure)
//!   capture, or an assignment whose left-hand side resolves to the local.
//! - **Decomposable assignment.** When the right-hand side of an
//!   `Assign(Var(local), Tuple(..))` is a tuple literal, the classifier
//!   treats it as a field-only use: each tuple element flows into a
//!   separate field so the local's whole value is not reconstituted.
//! - **Closure captures are whole-value.** [`ExprKind::Closure`] captures
//!   carry the local by value, so the walkers never attempt to split them
//!   even when the captured type is a tuple.
//! - **Non-`Path` `Field` access is whole-value.** A [`Field`] projection
//!   that is not a `Field::Path` keeps the record value materialized and is
//!   classified as a whole-value use.

#[cfg(test)]
mod tests;

use crate::fir_builder::functored_specs;
use qsc_fir::fir::{
    BlockId, CallableImpl, Expr, ExprId, ExprKind, Field, ItemKind, LocalItemId, LocalVarId,
    Package, PackageLookup, Res, SpecDecl, SpecImpl, StmtKind, StringComponent,
};
use rustc_hash::FxHashSet;

/// Walks an expression tree in pre-order, invoking `visit` for each expression.
///
/// Does not recurse into closure bodies: [`ExprKind::Closure`] is a leaf from
/// the walker's perspective, so a callable reached only through a closure
/// capture will not appear in the traversal.
pub fn for_each_expr<F>(pkg: &Package, expr_id: ExprId, visit: &mut F)
where
    F: FnMut(ExprId, &Expr),
{
    let expr = pkg.get_expr(expr_id);
    visit(expr_id, expr);
    walk_children(pkg, &expr.kind, visit);
}

/// Walks all expressions within a block.
///
/// Does not recurse into closure bodies; see [`for_each_expr`].
pub fn for_each_expr_in_block<F>(pkg: &Package, block_id: BlockId, visit: &mut F)
where
    F: FnMut(ExprId, &Expr),
{
    let block = pkg.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = pkg.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                for_each_expr(pkg, *e, visit);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Walks expressions in a callable implementation.
///
/// Does not recurse into closure bodies; see [`for_each_expr`].
pub fn for_each_expr_in_callable_impl<F>(pkg: &Package, callable_impl: &CallableImpl, visit: &mut F)
where
    F: FnMut(ExprId, &Expr),
{
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            for_each_expr_in_spec_impl(pkg, spec_impl, visit);
        }
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            for_each_expr_in_spec_decl(pkg, spec_decl, visit);
        }
    }
}

fn for_each_expr_in_spec_impl<F>(pkg: &Package, spec_impl: &SpecImpl, visit: &mut F)
where
    F: FnMut(ExprId, &Expr),
{
    for_each_expr_in_spec_decl(pkg, &spec_impl.body, visit);
    for spec in functored_specs(spec_impl) {
        for_each_expr_in_spec_decl(pkg, spec, visit);
    }
}

fn for_each_expr_in_spec_decl<F>(pkg: &Package, spec_decl: &SpecDecl, visit: &mut F)
where
    F: FnMut(ExprId, &Expr),
{
    for_each_expr_in_block(pkg, spec_decl.block, visit);
}

/// Exhaustive match over all `ExprKind` variants. No wildcard arm — adding a
/// new variant to `ExprKind` will produce a compile error here.
///
/// Does not recurse into closure bodies: `ExprKind::Closure` is matched as a
/// leaf alongside `Hole`, `Lit`, and `Var`.
fn walk_children<F>(pkg: &Package, kind: &ExprKind, visit: &mut F)
where
    F: FnMut(ExprId, &Expr),
{
    match kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for &e in exprs {
                for_each_expr(pkg, e, visit);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            for_each_expr(pkg, *a, visit);
            for_each_expr(pkg, *b, visit);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            for_each_expr(pkg, *a, visit);
            for_each_expr(pkg, *b, visit);
            for_each_expr(pkg, *c, visit);
        }
        ExprKind::Block(block_id) => {
            for_each_expr_in_block(pkg, *block_id, visit);
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            for_each_expr(pkg, *e, visit);
        }
        ExprKind::If(cond, body, otherwise) => {
            for_each_expr(pkg, *cond, visit);
            for_each_expr(pkg, *body, visit);
            if let Some(e) = otherwise {
                for_each_expr(pkg, *e, visit);
            }
        }
        ExprKind::Range(start, step, end) => {
            for e in [start, step, end].into_iter().flatten() {
                for_each_expr(pkg, *e, visit);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                for_each_expr(pkg, *c, visit);
            }
            for fa in fields {
                for_each_expr(pkg, fa.value, visit);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let StringComponent::Expr(e) = component {
                    for_each_expr(pkg, *e, visit);
                }
            }
        }
        ExprKind::While(cond, block) => {
            for_each_expr(pkg, *cond, visit);
            for_each_expr_in_block(pkg, *block, visit);
        }
    }
}

/// Classifies uses of `local_id` in a block.
///
/// Pushes `true` for field-only uses, `false` for whole-value uses.
pub(crate) fn collect_uses_in_block(
    package: &Package,
    block_id: BlockId,
    local_id: LocalVarId,
    uses: &mut Vec<bool>,
) {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                collect_uses_in_expr(package, *e, local_id, uses, false);
            }
            StmtKind::Local(_, _, expr) => {
                collect_uses_in_expr(package, *expr, local_id, uses, false);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Recursively classifies uses of `local_id` in an expression.
///
/// `inside_field` is true when `expr_id` is the direct child of a
/// `Field(_, Path(_))` or non-empty `AssignField(_, Path(_), _)` — meaning the
/// variable reference is being used for field access.
pub(crate) fn collect_uses_in_expr(
    package: &Package,
    expr_id: ExprId,
    local_id: LocalVarId,
    uses: &mut Vec<bool>,
    inside_field: bool,
) {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(var_id), _) if *var_id == local_id => {
            uses.push(inside_field);
        }
        ExprKind::Field(inner, Field::Path(_)) => {
            collect_uses_in_expr(package, *inner, local_id, uses, true);
        }
        ExprKind::AssignField(record, Field::Path(path), value) if !path.indices.is_empty() => {
            collect_uses_in_expr(package, *record, local_id, uses, true);
            collect_uses_in_expr(package, *value, local_id, uses, false);
        }
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                collect_uses_in_expr(package, e, local_id, uses, false);
            }
        }
        ExprKind::Assign(a, b) => {
            let lhs_expr = package.get_expr(*a);
            let rhs_expr = package.get_expr(*b);
            if let ExprKind::Var(Res::Local(var_id), _) = &lhs_expr.kind
                && *var_id == local_id
                && matches!(rhs_expr.kind, ExprKind::Tuple(_))
            {
                // Whole-tuple assignment with tuple literal RHS: treat as decomposable.
                uses.push(true);
                // Walk RHS elements for any uses of local_id.
                if let ExprKind::Tuple(elements) = &rhs_expr.kind {
                    for &e in elements {
                        collect_uses_in_expr(package, e, local_id, uses, false);
                    }
                }
            } else {
                collect_uses_in_expr(package, *a, local_id, uses, false);
                collect_uses_in_expr(package, *b, local_id, uses, false);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            collect_uses_in_expr(package, *a, local_id, uses, false);
            collect_uses_in_expr(package, *b, local_id, uses, false);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            collect_uses_in_expr(package, *a, local_id, uses, false);
            collect_uses_in_expr(package, *b, local_id, uses, false);
            collect_uses_in_expr(package, *c, local_id, uses, false);
        }
        ExprKind::Block(block_id) => {
            collect_uses_in_block(package, *block_id, local_id, uses);
        }
        ExprKind::Fail(e) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            collect_uses_in_expr(package, *e, local_id, uses, false);
        }
        ExprKind::Field(inner, _) => {
            collect_uses_in_expr(package, *inner, local_id, uses, false);
        }
        ExprKind::If(cond, body, otherwise) => {
            collect_uses_in_expr(package, *cond, local_id, uses, false);
            collect_uses_in_expr(package, *body, local_id, uses, false);
            if let Some(e) = otherwise {
                collect_uses_in_expr(package, *e, local_id, uses, false);
            }
        }
        ExprKind::Range(s, st, e) => {
            for x in [s, st, e].into_iter().flatten() {
                collect_uses_in_expr(package, *x, local_id, uses, false);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    collect_uses_in_expr(package, *e, local_id, uses, false);
                }
            }
        }
        ExprKind::While(cond, block_id) => {
            collect_uses_in_expr(package, *cond, local_id, uses, false);
            collect_uses_in_block(package, *block_id, local_id, uses);
        }
        ExprKind::Closure(vars, _) => {
            if vars.contains(&local_id) {
                uses.push(false);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                collect_uses_in_expr(package, *c, local_id, uses, false);
            }
            for fa in fields {
                collect_uses_in_expr(package, fa.value, local_id, uses, false);
            }
        }
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Classification of a single use of a local variable.
///
/// Unlike the boolean [`collect_uses_in_block`] classifier, this variant
/// records the [`ExprId`] of every whole-value read so a later pass can
/// rewrite those sites in place rather than disqualifying the local outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamUse {
    /// A `Field::Path` or `Field::Prim` projection over the local
    /// (for example `p.0` or `p::Item`).
    FieldAccess,
    /// A whole-tuple assignment whose right-hand side is a tuple literal;
    /// each element flows to a separate field, so the local is decomposable.
    Decomposable,
    /// A bare `Var(Res::Local(local))` read at the given expression.
    WholeValueRead(ExprId),
    /// A use that prevents promotion: a whole-value reassignment from a
    /// non-tuple right-hand side, a closure capture, or a non-`Path`/`Prim`
    /// field projection.
    HardBlock,
}

/// Classifies uses of `local_id` in a block, recording each as a [`ParamUse`].
///
/// This is the [`ParamUse`] counterpart of [`collect_uses_in_block`]: it
/// preserves the whole-value read sites (as [`ParamUse::WholeValueRead`])
/// instead of collapsing them to a single boolean.
pub(crate) fn classify_uses_in_block(
    package: &Package,
    block_id: BlockId,
    local_id: LocalVarId,
    out: &mut Vec<ParamUse>,
) {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                classify_uses_in_expr(package, *e, local_id, out, false);
            }
            StmtKind::Local(_, _, expr) => {
                classify_uses_in_expr(package, *expr, local_id, out, false);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Recursively classifies uses of `local_id` in an expression.
///
/// `inside_field` is true when `expr_id` is the direct child of a
/// `Field(_, Path(_) | Prim(_))` or non-empty `AssignField(_, Path(_), _)` —
/// meaning the variable reference is being used for field access.
#[allow(clippy::too_many_lines)] // Exhaustive `ExprKind` match mirrors `collect_uses_in_expr`.
fn classify_uses_in_expr(
    package: &Package,
    expr_id: ExprId,
    local_id: LocalVarId,
    out: &mut Vec<ParamUse>,
    inside_field: bool,
) {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(var_id), _) if *var_id == local_id => {
            if inside_field {
                out.push(ParamUse::FieldAccess);
            } else {
                out.push(ParamUse::WholeValueRead(expr_id));
            }
        }
        ExprKind::Field(inner, Field::Path(_) | Field::Prim(_)) => {
            classify_uses_in_expr(package, *inner, local_id, out, true);
        }
        ExprKind::AssignField(record, Field::Path(path), value) if !path.indices.is_empty() => {
            classify_uses_in_expr(package, *record, local_id, out, true);
            classify_uses_in_expr(package, *value, local_id, out, false);
        }
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                classify_uses_in_expr(package, e, local_id, out, false);
            }
        }
        ExprKind::Assign(a, b) => {
            let lhs_expr = package.get_expr(*a);
            let rhs_expr = package.get_expr(*b);
            if let ExprKind::Var(Res::Local(var_id), _) = &lhs_expr.kind
                && *var_id == local_id
            {
                if let ExprKind::Tuple(elements) = &rhs_expr.kind {
                    // Tuple-literal RHS: each element flows to its own field.
                    out.push(ParamUse::Decomposable);
                    for &e in elements {
                        classify_uses_in_expr(package, e, local_id, out, false);
                    }
                } else {
                    // Non-tuple whole-value reassignment: block.
                    out.push(ParamUse::HardBlock);
                    classify_uses_in_expr(package, *b, local_id, out, false);
                }
            } else {
                classify_uses_in_expr(package, *a, local_id, out, false);
                classify_uses_in_expr(package, *b, local_id, out, false);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            classify_uses_in_expr(package, *a, local_id, out, false);
            classify_uses_in_expr(package, *b, local_id, out, false);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            classify_uses_in_expr(package, *a, local_id, out, false);
            classify_uses_in_expr(package, *b, local_id, out, false);
            classify_uses_in_expr(package, *c, local_id, out, false);
        }
        ExprKind::Block(block_id) => {
            classify_uses_in_block(package, *block_id, local_id, out);
        }
        ExprKind::Fail(e) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            classify_uses_in_expr(package, *e, local_id, out, false);
        }
        ExprKind::Field(inner, _) => {
            // Non-`Path`/`Prim` field projection keeps the whole value live.
            let inner_expr = package.get_expr(*inner);
            if let ExprKind::Var(Res::Local(var_id), _) = &inner_expr.kind
                && *var_id == local_id
            {
                out.push(ParamUse::HardBlock);
            } else {
                classify_uses_in_expr(package, *inner, local_id, out, false);
            }
        }
        ExprKind::If(cond, body, otherwise) => {
            classify_uses_in_expr(package, *cond, local_id, out, false);
            classify_uses_in_expr(package, *body, local_id, out, false);
            if let Some(e) = otherwise {
                classify_uses_in_expr(package, *e, local_id, out, false);
            }
        }
        ExprKind::Range(s, st, e) => {
            for x in [s, st, e].into_iter().flatten() {
                classify_uses_in_expr(package, *x, local_id, out, false);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    classify_uses_in_expr(package, *e, local_id, out, false);
                }
            }
        }
        ExprKind::While(cond, block_id) => {
            classify_uses_in_expr(package, *cond, local_id, out, false);
            classify_uses_in_block(package, *block_id, local_id, out);
        }
        ExprKind::Closure(vars, _) => {
            if vars.contains(&local_id) {
                out.push(ParamUse::HardBlock);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                classify_uses_in_expr(package, *c, local_id, out, false);
            }
            for fa in fields {
                classify_uses_in_expr(package, fa.value, local_id, out, false);
            }
        }
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Collects all expression IDs reachable from the package entry expression.
///
/// Returns an empty vector when the package has no entry.
pub(crate) fn collect_expr_ids_in_entry(package: &Package) -> Vec<ExprId> {
    let mut ids = Vec::new();
    let mut seen = FxHashSet::default();
    if let Some(entry_id) = package.entry {
        for_each_expr(package, entry_id, &mut |expr_id, _| {
            if seen.insert(expr_id) {
                ids.push(expr_id);
            }
        });
    }
    ids
}

/// Collects all expression IDs from the specialization bodies of the given
/// local callables.
pub(crate) fn collect_expr_ids_in_local_callables(
    package: &Package,
    local_item_ids: &[LocalItemId],
) -> Vec<ExprId> {
    let mut ids = Vec::new();
    let mut seen = FxHashSet::default();
    extend_expr_ids_in_local_callables(package, local_item_ids, &mut ids, &mut seen);
    ids
}

/// Collects all expression IDs from the entry expression and the specialization
/// bodies of the given local callables.
pub(crate) fn collect_expr_ids_in_entry_and_local_callables(
    package: &Package,
    local_item_ids: &[LocalItemId],
) -> Vec<ExprId> {
    let mut ids = collect_expr_ids_in_entry(package);
    let mut seen: FxHashSet<ExprId> = ids.iter().copied().collect();
    extend_expr_ids_in_local_callables(package, local_item_ids, &mut ids, &mut seen);
    ids
}

/// Extends an existing expression ID collection with IDs from the given local
/// callable bodies. Skips IDs already in `seen`.
pub(crate) fn extend_expr_ids_in_local_callables(
    package: &Package,
    local_item_ids: &[LocalItemId],
    ids: &mut Vec<ExprId>,
    seen: &mut FxHashSet<ExprId>,
) {
    for &local_item_id in local_item_ids {
        let Some(item) = package.items.get(local_item_id) else {
            continue;
        };
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        for_each_expr_in_callable_impl(package, &decl.implementation, &mut |expr_id, _| {
            if seen.insert(expr_id) {
                ids.push(expr_id);
            }
        });
    }
}
