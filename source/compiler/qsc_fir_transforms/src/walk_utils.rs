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
//! - **Structural per-callable walker.** [`for_each_node_in_callable`] yields
//!   every structural node — [`CallableNode::Block`], [`CallableNode::Stmt`],
//!   [`CallableNode::Expr`], and [`CallableNode::Pat`] — of one callable: the
//!   callable input pattern, each present specialization's input pattern, and
//!   every block, statement, expression, and pattern of every specialization
//!   body. [`for_each_node_from_expr_root`] drives the same expression/block
//!   recursion from a bare root [`ExprId`] (for example, a package entry
//!   expression). Both share the single [`for_each_direct_child`] enumeration,
//!   so they descend nested blocks without a parallel `ExprKind` match.
//! - **Local-variable use classification.** [`for_each_use_event`] emits a
//!   [`UseEvent`] for every occurrence of a [`LocalVarId`], classifying each
//!   as either a *field-only* use or a *whole-value* use.
//!   [`classify_uses_in_block`] collects those events into a per-site
//!   [`ParamUse`] vector, while [`classify_block_use`] folds them into a
//!   single [`UseClass`] aggregate. See
//!   [`# Use classification`](#use-classification) below for the rules.
//! - **Reachable-`ExprId` collectors.** [`collect_expr_ids_in_entry`],
//!   [`collect_expr_ids_in_local_callables`], and
//!   [`collect_expr_ids_in_entry_and_local_callables`] return every
//!   [`ExprId`] reachable from the given roots, deduplicated.
//!   [`extend_expr_ids_in_local_callables`] is the in-place variant used to
//!   accumulate IDs across roots while sharing a single dedup set.
//!
//! # Use classification
//!
//! Tuple-decomposing passes rely on the *field-only* vs. *whole-value*
//! distinction recorded by [`for_each_use_event`] (surfaced through
//! [`classify_uses_in_block`] and [`classify_block_use`]) to decide whether a
//! local can be scalarized safely. The rules are:
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
    BinOp, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, ItemKind,
    LocalItemId, LocalVarId, Package, PackageLookup, PatId, PatKind, Res, SpecDecl, SpecImpl,
    StmtId, StmtKind, StringComponent, UnOp,
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

/// Pre-order walker child step: recurse into each direct child expression and
/// descend through each direct child block.
///
/// Does not recurse into closure bodies; see [`for_each_direct_child`], which
/// supplies the single exhaustive `ExprKind` enumeration this builds on.
fn walk_children<F>(pkg: &Package, kind: &ExprKind, visit: &mut F)
where
    F: FnMut(ExprId, &Expr),
{
    for_each_direct_child(kind, |child| match child {
        DirectChild::Expr(e) => for_each_expr(pkg, e, visit),
        DirectChild::Block(block_id) => for_each_expr_in_block(pkg, block_id, visit),
    });
}

/// A direct child of an expression, as yielded by [`for_each_direct_child`]:
/// either a child expression reachable without crossing a block boundary, or
/// an immediately-nested block.
pub(crate) enum DirectChild {
    /// A child expression in the same block scope as its parent.
    Expr(ExprId),
    /// An immediately-nested block — an [`ExprKind::Block`] body or a
    /// [`ExprKind::While`] loop body.
    Block(BlockId),
}

/// Invokes `visit` for each *direct* child of `kind`: every child expression
/// reachable without crossing a block boundary ([`DirectChild::Expr`]) and
/// every immediately-nested block ([`DirectChild::Block`]).
///
/// This is the single exhaustive `ExprKind` enumeration that block-aware
/// walkers build on; each chooses its own block policy inside `visit`. Closure
/// bodies are leaves (alongside `Hole`, `Lit`, `Var`), consistent with
/// [`for_each_expr`]. No wildcard arm: a new `ExprKind` variant breaks the
/// build here, forcing every walker to be reconsidered in one place.
pub(crate) fn for_each_direct_child<F: FnMut(DirectChild)>(kind: &ExprKind, mut visit: F) {
    match kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for &e in exprs {
                visit(DirectChild::Expr(e));
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
            visit(DirectChild::Expr(*a));
            visit(DirectChild::Expr(*b));
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            visit(DirectChild::Expr(*a));
            visit(DirectChild::Expr(*b));
            visit(DirectChild::Expr(*c));
        }
        ExprKind::Block(block_id) => {
            visit(DirectChild::Block(*block_id));
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            visit(DirectChild::Expr(*e));
        }
        ExprKind::If(cond, body, otherwise) => {
            visit(DirectChild::Expr(*cond));
            visit(DirectChild::Expr(*body));
            if let Some(e) = otherwise {
                visit(DirectChild::Expr(*e));
            }
        }
        ExprKind::Range(start, step, end) => {
            for e in [start, step, end].into_iter().flatten() {
                visit(DirectChild::Expr(*e));
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                visit(DirectChild::Expr(*c));
            }
            for fa in fields {
                visit(DirectChild::Expr(fa.value));
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let StringComponent::Expr(e) = component {
                    visit(DirectChild::Expr(*e));
                }
            }
        }
        ExprKind::While(cond, block) => {
            visit(DirectChild::Expr(*cond));
            visit(DirectChild::Block(*block));
        }
    }
}

/// A structural node of a callable, as yielded by
/// [`for_each_node_in_callable`] and [`for_each_node_from_expr_root`].
///
/// Unlike the expr-only walkers, the structural walker visits every node
/// kind that carries a `.ty` — blocks, statements, expressions, and
/// patterns — so a checker can assert a whole-tree invariant from a single
/// traversal.
pub enum CallableNode {
    /// A reachable block: a specialization body or a nested
    /// [`ExprKind::Block`] / [`ExprKind::While`] body.
    Block(BlockId),
    /// A statement within a reachable block.
    Stmt(StmtId),
    /// An expression reachable from a specialization body or expr root.
    Expr(ExprId),
    /// A pattern: a callable or specialization input, a
    /// [`StmtKind::Local`] binding, or a nested tuple element of either.
    Pat(PatId),
}

/// Walks every structural node of `decl`, invoking `visit` for each
/// [`CallableNode`].
///
/// Coverage is complete for the callable's reachable tree:
/// - **Patterns.** The callable input ([`CallableDecl::input`]), each present
///   specialization input ([`SpecDecl::input`], including the control-register
///   inputs carried by the `ctl` / `ctl_adj` specs and the single
///   [`CallableImpl::SimulatableIntrinsic`] spec), and every
///   [`StmtKind::Local`] binding — each walked recursively through
///   [`PatKind::Tuple`] elements.
/// - **Blocks / statements / expressions.** Every specialization body block,
///   every nested block, every statement, and every expression of every
///   specialization, via the shared [`for_each_direct_child`] descent.
///
/// Does not recurse into closure bodies; see [`for_each_expr`]. The yield
/// order is pre-order within each subtree but is otherwise unspecified;
/// callers must not depend on the relative order of nodes from different
/// specializations.
pub fn for_each_node_in_callable<F>(pkg: &Package, decl: &CallableDecl, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    for_each_node_in_pat(pkg, decl.input, visit);
    for_each_node_in_callable_impl(pkg, &decl.implementation, visit);
}

/// Walks every structural node reachable from a bare root expression,
/// invoking `visit` for each [`CallableNode`].
///
/// Drives the same expression/block recursion as
/// [`for_each_node_in_callable`], so a nested [`ExprKind::Block`] or
/// [`ExprKind::While`] body contributes its blocks, statements, expressions,
/// and [`StmtKind::Local`] patterns. Use this for roots that are not anchored
/// to a callable, such as a package entry expression.
///
/// Does not recurse into closure bodies; see [`for_each_expr`].
pub fn for_each_node_from_expr_root<F>(pkg: &Package, expr_id: ExprId, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    for_each_node_in_expr(pkg, expr_id, visit);
}

fn for_each_node_in_callable_impl<F>(pkg: &Package, callable_impl: &CallableImpl, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    match callable_impl {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            for_each_node_in_spec_impl(pkg, spec_impl, visit);
        }
    }
}

fn for_each_node_in_spec_impl<F>(pkg: &Package, spec_impl: &SpecImpl, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    for_each_node_in_spec_decl(pkg, &spec_impl.body, visit);
    for spec in functored_specs(spec_impl) {
        for_each_node_in_spec_decl(pkg, spec, visit);
    }
}

fn for_each_node_in_spec_decl<F>(pkg: &Package, spec_decl: &SpecDecl, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    if let Some(input) = spec_decl.input {
        for_each_node_in_pat(pkg, input, visit);
    }
    for_each_node_in_block(pkg, spec_decl.block, visit);
}

fn for_each_node_in_block<F>(pkg: &Package, block_id: BlockId, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    visit(CallableNode::Block(block_id));
    let block = pkg.get_block(block_id);
    for &stmt_id in &block.stmts {
        visit(CallableNode::Stmt(stmt_id));
        let stmt = pkg.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                for_each_node_in_expr(pkg, *e, visit);
            }
            StmtKind::Local(_, pat, e) => {
                for_each_node_in_pat(pkg, *pat, visit);
                for_each_node_in_expr(pkg, *e, visit);
            }
            StmtKind::Item(_) => {}
        }
    }
}

fn for_each_node_in_expr<F>(pkg: &Package, expr_id: ExprId, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    visit(CallableNode::Expr(expr_id));
    let expr = pkg.get_expr(expr_id);
    for_each_direct_child(&expr.kind, |child| match child {
        DirectChild::Expr(e) => for_each_node_in_expr(pkg, e, visit),
        DirectChild::Block(block_id) => for_each_node_in_block(pkg, block_id, visit),
    });
}

fn for_each_node_in_pat<F>(pkg: &Package, pat_id: PatId, visit: &mut F)
where
    F: FnMut(CallableNode),
{
    visit(CallableNode::Pat(pat_id));
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(_) | PatKind::Discard => {}
        PatKind::Tuple(pats) => {
            for &p in pats {
                for_each_node_in_pat(pkg, p, visit);
            }
        }
    }
}

/// A single classified occurrence of a local, emitted by
/// [`for_each_use_event`]. This is what both
/// [`ParamUse`] and [`UseClass`] are derived from.
enum UseEvent {
    /// A `Field::Path` or `Field::Prim` projection over the local.
    FieldAccess,
    /// A whole-tuple assignment whose right-hand side is a tuple literal.
    Decomposable,
    /// A bare `Var(Res::Local(local))` read at the given expression.
    WholeValueRead(ExprId),
    /// A use that prevents promotion: a non-tuple whole-value reassignment, a
    /// closure capture, or a non-`Path`/`Prim` field projection.
    HardBlock,
}

/// Aggregate view of how a local is used, folded from [`UseEvent`]s by
/// [`UseClass::observe`]. Forms the lattice `Unused < FieldOnly < GeneralUse`
/// with `GeneralUse` as the absorbing top. The variant declaration order
/// matches the lattice order so the derived [`Ord`] agrees with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UseClass {
    /// The local is never mentioned.
    Unused,
    /// Every mention is a field-only projection or a decomposable assignment.
    FieldOnly,
    /// At least one whole-value read or promotion-blocking use is present.
    GeneralUse,
}

impl UseClass {
    /// Raises the class to account for `event`, never lowering it.
    ///
    /// `FieldAccess`/`Decomposable` contribute at least `FieldOnly`;
    /// `WholeValueRead`/`HardBlock` contribute `GeneralUse` (the absorbing
    /// top).
    fn observe(&mut self, event: &UseEvent) {
        let level = match event {
            UseEvent::FieldAccess | UseEvent::Decomposable => UseClass::FieldOnly,
            UseEvent::WholeValueRead(_) | UseEvent::HardBlock => UseClass::GeneralUse,
        };
        *self = (*self).max(level);
    }
}

/// Classification of a single use of a local variable.
///
/// Records the [`ExprId`] of every whole-value read so a later pass can
/// rewrite those sites in place rather than disqualifying the local outright.
/// This is the per-site carrier produced by [`classify_uses_in_block`], the
/// `Vec<ParamUse>` sink over [`for_each_use_event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamUse {
    /// A `Field::Path` or `Field::Prim` projection over the local
    /// (for example `p.0` or `p::Item`).
    FieldAccess,
    /// A bare `Var(Res::Local(local))` read at the given expression.
    WholeValueRead(ExprId),
    /// A use that prevents promotion: a whole-value reassignment (tuple-literal
    /// or other right-hand side), a closure capture, or a non-`Path`/`Prim`
    /// field projection.
    HardBlock,
}

/// Classifies uses of `local_id` in a block, recording each as a [`ParamUse`].
///
/// This is the `Vec<ParamUse>` sink over [`for_each_use_event`]: it preserves
/// the whole-value read sites (as [`ParamUse::WholeValueRead`]) so a later pass
/// can rewrite them in place. Callers needing only the aggregate
/// classification should use [`classify_block_use`] instead, which avoids the
/// allocation.
pub(crate) fn classify_uses_in_block(
    package: &Package,
    block_id: BlockId,
    local_id: LocalVarId,
    out: &mut Vec<ParamUse>,
) {
    for_each_use_event_in_block(package, block_id, local_id, &mut |event| {
        out.push(match event {
            UseEvent::FieldAccess => ParamUse::FieldAccess,
            // A tuple-literal reassignment (`set local = (..)`) disqualifies the
            // local from promotion, so it folds into `HardBlock`. For the only
            // caller (parameter classification) it is also borrowck-unreachable:
            // `Qdk.Qsc.BorrowCk.Mutability` forbids assigning to a parameter.
            UseEvent::Decomposable | UseEvent::HardBlock => ParamUse::HardBlock,
            UseEvent::WholeValueRead(id) => ParamUse::WholeValueRead(id),
        });
    });
}

/// Folds the uses of `local_id` in a block into a single [`UseClass`].
///
/// Runs the [`for_each_use_event`] traversal without allocating, raising the
/// aggregate via [`UseClass::observe`]. An empty block yields
/// [`UseClass::Unused`].
pub(crate) fn classify_block_use(
    package: &Package,
    block_id: BlockId,
    local_id: LocalVarId,
) -> UseClass {
    let mut class = UseClass::Unused;
    for_each_use_event_in_block(package, block_id, local_id, &mut |event| {
        class.observe(&event);
    });
    class
}

/// Drives [`for_each_use_event`] over every statement expression in a block.
fn for_each_use_event_in_block<F: FnMut(UseEvent)>(
    package: &Package,
    block_id: BlockId,
    local_id: LocalVarId,
    visit: &mut F,
) {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                for_each_use_event(package, *e, local_id, false, visit);
            }
            StmtKind::Local(_, _, expr) => {
                for_each_use_event(package, *expr, local_id, false, visit);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Recursively emits a [`UseEvent`] for each use of `local_id` in an
/// expression.
///
/// This is the single source-of-truth `ExprKind` match for local-use
/// classification; [`classify_uses_in_block`] and [`classify_block_use`] are
/// thin sinks over it.
///
/// `inside_field` is true when `expr_id` is the direct child of a
/// `Field(_, Path(_) | Prim(_))` or non-empty `AssignField(_, Path(_), _)` —
/// meaning the variable reference is being used for field access.
#[allow(clippy::too_many_lines)] // Exhaustive `ExprKind` match: the sole local-use classifier traversal.
fn for_each_use_event<F: FnMut(UseEvent)>(
    package: &Package,
    expr_id: ExprId,
    local_id: LocalVarId,
    inside_field: bool,
    visit: &mut F,
) {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(var_id), _) if *var_id == local_id => {
            if inside_field {
                visit(UseEvent::FieldAccess);
            } else {
                visit(UseEvent::WholeValueRead(expr_id));
            }
        }
        ExprKind::Field(inner, Field::Path(_) | Field::Prim(_)) => {
            for_each_use_event(package, *inner, local_id, true, visit);
        }
        ExprKind::AssignField(record, Field::Path(path), value) if !path.indices.is_empty() => {
            for_each_use_event(package, *record, local_id, true, visit);
            for_each_use_event(package, *value, local_id, false, visit);
        }
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                for_each_use_event(package, e, local_id, false, visit);
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
                    visit(UseEvent::Decomposable);
                    for &e in elements {
                        for_each_use_event(package, e, local_id, false, visit);
                    }
                } else {
                    // Non-tuple whole-value reassignment: block.
                    visit(UseEvent::HardBlock);
                    for_each_use_event(package, *b, local_id, false, visit);
                }
            } else {
                for_each_use_event(package, *a, local_id, false, visit);
                for_each_use_event(package, *b, local_id, false, visit);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            for_each_use_event(package, *a, local_id, false, visit);
            for_each_use_event(package, *b, local_id, false, visit);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            for_each_use_event(package, *a, local_id, false, visit);
            for_each_use_event(package, *b, local_id, false, visit);
            for_each_use_event(package, *c, local_id, false, visit);
        }
        ExprKind::Block(block_id) => {
            for_each_use_event_in_block(package, *block_id, local_id, visit);
        }
        ExprKind::Fail(e) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            for_each_use_event(package, *e, local_id, false, visit);
        }
        ExprKind::Field(inner, _) => {
            // Non-`Path`/`Prim` field projection keeps the whole value live.
            let inner_expr = package.get_expr(*inner);
            if let ExprKind::Var(Res::Local(var_id), _) = &inner_expr.kind
                && *var_id == local_id
            {
                visit(UseEvent::HardBlock);
            } else {
                for_each_use_event(package, *inner, local_id, false, visit);
            }
        }
        ExprKind::If(cond, body, otherwise) => {
            for_each_use_event(package, *cond, local_id, false, visit);
            for_each_use_event(package, *body, local_id, false, visit);
            if let Some(e) = otherwise {
                for_each_use_event(package, *e, local_id, false, visit);
            }
        }
        ExprKind::Range(s, st, e) => {
            for x in [s, st, e].into_iter().flatten() {
                for_each_use_event(package, *x, local_id, false, visit);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    for_each_use_event(package, *e, local_id, false, visit);
                }
            }
        }
        ExprKind::While(cond, block_id) => {
            for_each_use_event(package, *cond, local_id, false, visit);
            for_each_use_event_in_block(package, *block_id, local_id, visit);
        }
        ExprKind::Closure(vars, _) => {
            if vars.contains(&local_id) {
                visit(UseEvent::HardBlock);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                for_each_use_event(package, *c, local_id, false, visit);
            }
            for fa in fields {
                for_each_use_event(package, fa.value, local_id, false, visit);
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

/// Conservatively decides whether evaluating `expr_id` has no observable side
/// effects.
///
/// Returns `true` only for pure value constructors, reads, and projections:
/// literals, holes, variable references, and closures (the body is not invoked
/// here); compound constructors (`Tuple`, `Array`, `ArrayLit`, `ArrayRepeat`,
/// `Range`, `String`, `Struct`) and projections (`Field`, `Index`,
/// `UpdateField`, `UpdateIndex`) whose children are all side-effect-free; `If`
/// with both arms and `Block` (empty or a single trailing `Expr` stmt) whose
/// subexpressions are side-effect-free; and non-trapping operators (the
/// `Functor`, `NotB`, `NotL`, `Pos`, and `Unwrap` unary operators and the
/// logical, bitwise, and comparison binary operators) over side-effect-free
/// operands.
///
/// Returns `false` for everything else, including `Call`, `Assign*`, `Return`,
/// `Fail`, `While`, else-less `If`, the trapping arithmetic/shift operators,
/// and `Neg`. The match is exhaustive with no wildcard arm, so a new
/// `ExprKind` variant breaks the build here and must have its purity decided
/// explicitly rather than defaulting to either answer.
pub(crate) fn expr_is_side_effect_free(package: &Package, expr_id: ExprId) -> bool {
    match &package.get_expr(expr_id).kind {
        ExprKind::Lit(_) | ExprKind::Hole | ExprKind::Var(_, _) | ExprKind::Closure(_, _) => true,
        ExprKind::Tuple(items) | ExprKind::Array(items) | ExprKind::ArrayLit(items) => items
            .iter()
            .all(|&id| expr_is_side_effect_free(package, id)),
        ExprKind::ArrayRepeat(value, count) | ExprKind::Index(value, count) => {
            expr_is_side_effect_free(package, *value) && expr_is_side_effect_free(package, *count)
        }
        ExprKind::Field(record, _) => expr_is_side_effect_free(package, *record),
        ExprKind::UpdateField(record, _, value) => {
            expr_is_side_effect_free(package, *record) && expr_is_side_effect_free(package, *value)
        }
        ExprKind::UpdateIndex(arr, idx, value) => {
            expr_is_side_effect_free(package, *arr)
                && expr_is_side_effect_free(package, *idx)
                && expr_is_side_effect_free(package, *value)
        }
        ExprKind::Range(start, step, end) => [start, step, end].iter().all(|opt| match opt {
            Some(id) => expr_is_side_effect_free(package, *id),
            None => true,
        }),
        ExprKind::String(parts) => parts.iter().all(|p| match p {
            StringComponent::Lit(_) => true,
            StringComponent::Expr(e) => expr_is_side_effect_free(package, *e),
        }),
        ExprKind::Struct(_, copy, fields) => {
            copy.is_none_or(|id| expr_is_side_effect_free(package, id))
                && fields
                    .iter()
                    .all(|f| expr_is_side_effect_free(package, f.value))
        }
        ExprKind::If(cond, then, Some(else_id)) => {
            expr_is_side_effect_free(package, *cond)
                && expr_is_side_effect_free(package, *then)
                && expr_is_side_effect_free(package, *else_id)
        }
        ExprKind::UnOp(op, operand) => {
            matches!(
                op,
                UnOp::Functor(_) | UnOp::NotB | UnOp::NotL | UnOp::Pos | UnOp::Unwrap
            ) && expr_is_side_effect_free(package, *operand)
        }
        ExprKind::BinOp(op, lhs, rhs) => {
            matches!(
                op,
                BinOp::AndL
                    | BinOp::OrL
                    | BinOp::AndB
                    | BinOp::OrB
                    | BinOp::XorB
                    | BinOp::Eq
                    | BinOp::Neq
                    | BinOp::Lt
                    | BinOp::Lte
                    | BinOp::Gt
                    | BinOp::Gte
            ) && expr_is_side_effect_free(package, *lhs)
                && expr_is_side_effect_free(package, *rhs)
        }
        ExprKind::Block(bid) => {
            let blk = package.get_block(*bid);
            match blk.stmts.as_slice() {
                [] => true,
                [only] => match &package.get_stmt(*only).kind {
                    StmtKind::Expr(tail) => expr_is_side_effect_free(package, *tail),
                    _ => false,
                },
                _ => false,
            }
        }
        // Side-effecting or potentially-trapping variants. `If` without an
        // else arm is included here: it has `Unit` type but its `then` branch
        // may run for effect. No wildcard arm — a new `ExprKind` variant
        // breaks the build here so its purity is decided explicitly.
        ExprKind::Call(_, _)
        | ExprKind::Assign(_, _)
        | ExprKind::AssignOp(_, _, _)
        | ExprKind::AssignField(_, _, _)
        | ExprKind::AssignIndex(_, _, _)
        | ExprKind::Fail(_)
        | ExprKind::Return(_)
        | ExprKind::While(_, _)
        | ExprKind::If(_, _, None) => false,
    }
}
