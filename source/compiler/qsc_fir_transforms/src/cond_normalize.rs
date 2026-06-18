// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Conditional-guard normalization — hoists side-effecting `if` conditions
//! into a single-evaluation `let` binding so downstream passes can reuse the
//! guard value without re-running its effects.
//!
//! # Motivation
//!
//! [`crate::defunctionalize`] reconstructs branch dispatch from `if` guards: it
//! reuses each guard's `ExprId` in a synthesized dispatch chain while leaving
//! the original `if` in place. For a side-effecting guard — e.g.
//! `if MResetZ(q) == One { ... }` — that reuse would evaluate the effect twice.
//! This pass runs first and removes the hazard: each side-effecting condition
//! is evaluated once into a temporary, and the `if` tests that temporary, so
//! the reused guard is a pure `Var` read.
//!
//! # Rewrite shape
//!
//! For a statement-position `if cond { .. }` whose `cond` may have side
//! effects, the condition is bound immediately before the statement and the
//! `if` is rewritten to read it:
//!
//! ```text
//! if cond { body } else { otherwise }
//! // becomes (within the enclosing block)
//! let __cond = cond;
//! if __cond { body } else { otherwise }
//! ```
//!
//! Binding `__cond` as a *sibling* statement (not inside a wrapper block) is
//! what makes reuse sound: defunctionalization reuses the guard at a later
//! dispatch site, and a binding in the enclosing block dominates that site
//! while a binding in a nested block would not. Lifting the condition to the
//! line above preserves evaluation order.
//!
//! # Scope
//!
//! Only **statement-position** `if` conditions (an `if` that is itself an
//! `Expr`/`Semi` statement, e.g. the retained `if cond { op = X }` of a
//! mutable-reassignment) are normalized — these are the conditions
//! defunctionalization both keeps and reuses. Every condition in such a chain
//! is normalized, not just the outer one.
//!
//! Left untouched: value-position `if`s (defunctionalization removes the
//! binding and rebuilds a tree referencing each guard once); `while` guards
//! (re-evaluated per iteration); and provably-pure conditions, per the
//! conservative [`crate::walk_utils::expr_is_side_effect_free`] predicate.
//!
//! # Binding placement
//!
//! Placement depends on whether the `if` is *nested* (its enclosing block is
//! not the specialization's root block):
//!
//! - **Top-level `if`**: the outer condition becomes `let __cond = cond;` in the
//!   enclosing block; each `else if c { .. }` becomes
//!   `else { __cond = c; if __cond { .. } }` using a `mutable __cond = false;`
//!   accumulator in that same block.
//! - **Nested `if`**: defunctionalization can lift the reused guard to a
//!   dispatch site in an *outer* block, which the enclosing block does not
//!   dominate. So the accumulator *declaration* is lifted to the root block
//!   (which dominates every dispatch site) while the side-effecting
//!   *evaluation* stays at the original point as `__cond = cond;`. The
//!   `false` default is read only on paths where the branch was not taken,
//!   matching the original fall-through.
//!
//! Temporaries are named `__cond_<n>` (counter scoped per root block) so
//! co-resident guards render with distinct suffixes, mirroring return
//! unification's `__operand_tmp_<n>` scheme.
//!
//! Synthesized nodes use [`crate::EMPTY_EXEC_RANGE`];
//! [`crate::exec_graph_rebuild`] rebuilds exec graphs later.

#[cfg(test)]
mod tests;

use crate::fir_builder::{
    alloc_assign_expr, alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr_stmt,
    alloc_local_var, alloc_local_var_expr, alloc_semi_stmt, reachable_local_callables,
};
use crate::package_assigners::PackageAssigners;
use crate::reachability::{collect_reachable_from_entry, collect_reachable_package_closure};
use crate::walk_utils::expr_is_side_effect_free;
use crate::walk_utils::{DirectChild, for_each_direct_child};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BlockId, CallableImpl, ExprId, ExprKind, Mutability, Package, PackageId, PackageLookup,
    PackageStore, SpecImpl, StmtId, StmtKind, StoreItemId,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// Base name for the condition temporaries minted by this pass; a per-root-
/// block counter is appended. The in-memory `Ident.name` carries a `.`
/// sentinel (`_.cond_0`, `_.cond_1`, …), which is never a valid Q# identifier
/// character; the Parseable render (`render_ident`) restores the original
/// `__cond_0` / `__cond_1` spelling.
const COND_TEMP: &str = "_.cond";

/// Mints the next `_.cond_<n>` temporary name and advances `counter`.
fn next_cond_temp_name(counter: &mut u32) -> String {
    let name = format!("{COND_TEMP}_{counter}");
    *counter += 1;
    name
}

/// A statement-position `if` whose condition chain needs normalization:
/// `(root_block, enclosing_block, stmt_index, if_expr)`. `root_block` is the
/// specialization's top-level block (where nested-`if` accumulators are
/// declared so they dominate the reuse site); `enclosing_block` directly holds
/// the `if` statement at `stmt_index`.
type ConditionTarget = (BlockId, BlockId, usize, ExprId);

/// Normalizes side-effecting, statement-position `if` conditions across every
/// reachable callable in every reachable package by hoisting each into a
/// single-evaluation `let` binding spliced immediately before its statement.
///
/// Runs once, after return unification and before defunctionalization, so that
/// defunctionalization's guard reuse references only pure `Var` reads bound in
/// a scope that dominates the dispatch site.
///
/// Reachability is rooted once at the entry package; the resulting closure
/// spans the user, std, and core packages. Each reachable package is processed
/// against its own arena and assigner so foreign-package id spaces stay
/// collision-free. The pass is body-only and signature-preserving (it
/// introduces no `Return`).
pub(crate) fn normalize_conditions(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) {
    let reachable = collect_reachable_from_entry(store, package_id);
    let pkg_ids: Vec<PackageId> = collect_reachable_package_closure(package_id, &reachable)
        .into_iter()
        .collect();
    for pkg in pkg_ids {
        normalize_conditions_in_package(store, pkg, assigners, &reachable);
    }
}

/// Normalizes the statement-position `if` conditions of every reachable
/// callable that lives in `package_id`, minting condition temporaries into
/// that package's assigner.
fn normalize_conditions_in_package(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    reachable: &FxHashSet<StoreItemId>,
) {
    let assigner = assigners.get_mut(store, package_id);
    let package = store.get(package_id);

    // Collect the statement-position `if`s whose conditions need hoisting under
    // a shared borrow, then apply the splice-and-rewrite below. Only reachable
    // callables in this package are considered.
    let mut targets: Vec<ConditionTarget> = Vec::new();
    for (_item_id, decl) in reachable_local_callables(package, package_id, reachable) {
        collect_targets_in_callable_impl(package, &decl.implementation, &mut targets);
    }

    if targets.is_empty() {
        return;
    }

    // Apply hoists block-by-block in ascending statement-index order so each
    // splice shifts only the statements after it. Sorting by
    // `(enclosing_block, index)` also keeps synthesized-node ID assignment
    // deterministic across runs.
    targets.sort_unstable_by_key(|&(_root, enclosing, stmt_index, _)| (enclosing, stmt_index));

    let package = store.get_mut(package_id);

    // Nested-`if` accumulators are declared in the root block (the enclosing
    // block does not dominate the dispatch site). Collected here and prepended
    // after all index-based splicing; prepending shifts spliced statements
    // uniformly, leaving the indices used above intact.
    let mut root_prepends: Vec<(BlockId, StmtId)> = Vec::new();

    // `__cond_<n>` counters keyed by root block, so each body numbers
    // independently.
    let mut cond_temp_counters: FxHashMap<BlockId, u32> = FxHashMap::default();

    let mut current_block: Option<BlockId> = None;
    let mut inserted_in_block = 0usize;
    for (root_block, enclosing_block, stmt_index, if_expr_id) in targets {
        if current_block != Some(enclosing_block) {
            current_block = Some(enclosing_block);
            inserted_in_block = 0;
        }
        let cond_temp_counter = cond_temp_counters.entry(root_block).or_default();
        let inserted = hoist_condition(
            package,
            assigner,
            root_block,
            enclosing_block,
            stmt_index + inserted_in_block,
            if_expr_id,
            &mut root_prepends,
            cond_temp_counter,
        );
        inserted_in_block += inserted;
    }

    // Prepend the collected accumulator declarations to their root blocks.
    // Iterating in reverse and inserting at index 0 preserves collection order
    // (declarations appear at the top of the block in the order discovered).
    for &(root_block, stmt_id) in root_prepends.iter().rev() {
        package
            .blocks
            .get_mut(root_block)
            .expect("root block not found")
            .stmts
            .insert(0, stmt_id);
    }
}

/// Collects every statement-position `if` whose condition needs normalization,
/// across all functored specializations of a callable implementation.
fn collect_targets_in_callable_impl(
    package: &Package,
    callable_impl: &CallableImpl,
    targets: &mut Vec<ConditionTarget>,
) {
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            collect_targets_in_spec_impl(package, spec_impl, targets);
        }
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            collect_targets_in_block(package, spec_decl.block, spec_decl.block, targets);
        }
    }
}

fn collect_targets_in_spec_impl(
    package: &Package,
    spec_impl: &SpecImpl,
    targets: &mut Vec<ConditionTarget>,
) {
    collect_targets_in_block(package, spec_impl.body.block, spec_impl.body.block, targets);
    if let Some(adj) = &spec_impl.adj {
        collect_targets_in_block(package, adj.block, adj.block, targets);
    }
    if let Some(ctl) = &spec_impl.ctl {
        collect_targets_in_block(package, ctl.block, ctl.block, targets);
    }
    if let Some(ctl_adj) = &spec_impl.ctl_adj {
        collect_targets_in_block(package, ctl_adj.block, ctl_adj.block, targets);
    }
}

/// Records each statement-position `if` with a side-effecting condition,
/// recursing into nested blocks so `if`s at any depth are caught. `root_block`
/// is threaded unchanged so each target knows the dominating scope.
fn collect_targets_in_block(
    package: &Package,
    root_block: BlockId,
    block_id: BlockId,
    targets: &mut Vec<ConditionTarget>,
) {
    let block = package.get_block(block_id);
    for (stmt_index, &stmt_id) in block.stmts.iter().enumerate() {
        let value_expr = match &package.get_stmt(stmt_id).kind {
            // Statement-position `if`: a candidate for hoisting if any
            // condition in its `else if` chain carries side effects.
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                let surface = *e;
                if matches!(&package.get_expr(surface).kind, ExprKind::If(..))
                    && if_chain_has_side_effecting_cond(package, surface)
                {
                    targets.push((root_block, block_id, stmt_index, surface));
                }
                surface
            }
            // Value-position `if`s (let initializers) are left to
            // defunctionalization; only recurse to find nested
            // statement-position `if`s.
            StmtKind::Local(_, _, e) => *e,
            StmtKind::Item(_) => continue,
        };
        let mut child_blocks = Vec::new();
        collect_child_blocks(package, value_expr, &mut child_blocks);
        for child in child_blocks {
            collect_targets_in_block(package, root_block, child, targets);
        }
    }
}

/// Collects the block IDs that are *direct* children of `expr_id` — the blocks
/// reachable without crossing another block boundary. The caller re-enters each
/// collected block separately, so every block is visited once with its own
/// statement indices.
fn collect_child_blocks(package: &Package, expr_id: ExprId, out: &mut Vec<BlockId>) {
    for_each_direct_child(&package.get_expr(expr_id).kind, |child| match child {
        DirectChild::Expr(e) => collect_child_blocks(package, e, out),
        DirectChild::Block(block) => out.push(block),
    });
}

/// Returns `true` when the outer condition or any `else if` condition in the
/// chain headed by `if_expr_id` carries side effects (per the conservative
/// [`expr_is_side_effect_free`] predicate). Only `else if` links — an `If`
/// expression in `otherwise` position — are followed; a final `else { .. }`
/// block has no condition and stops the walk.
fn if_chain_has_side_effecting_cond(package: &Package, if_expr_id: ExprId) -> bool {
    let mut current = if_expr_id;
    loop {
        let ExprKind::If(cond, _, otherwise) = &package.get_expr(current).kind else {
            return false;
        };
        if !expr_is_side_effect_free(package, *cond) {
            return true;
        }
        match otherwise {
            Some(else_id) if matches!(&package.get_expr(*else_id).kind, ExprKind::If(..)) => {
                current = *else_id;
            }
            _ => return false,
        }
    }
}

/// Normalizes the `if` chain headed by `if_expr_id` so every side-effecting
/// condition is evaluated once and stays in a scope that dominates the dispatch
/// site. Placement depends on whether the `if` is nested (see module docs):
///
/// - The **outer** condition runs whenever its statement is reached. Top-level:
///   `let __cond = cond;`. Nested: `mutable __cond = false;` in the root block
///   plus `__cond = cond;` at the original point.
/// - Each **`else if`** runs only when preceding guards are false, so it always
///   uses a `mutable` accumulator assigned in its else scope:
///   `else if c { .. }` becomes `else { __cond = c; if __cond { .. } }`.
///
/// Returns the number of statements spliced into `enclosing_block` so the
/// caller can keep later statement indices aligned. Root-block declarations
/// (via `root_prepends`) are applied separately and not counted.
#[allow(clippy::too_many_arguments)]
fn hoist_condition(
    package: &mut Package,
    assigner: &mut Assigner,
    root_block: BlockId,
    enclosing_block: BlockId,
    insert_index: usize,
    if_expr_id: ExprId,
    root_prepends: &mut Vec<(BlockId, StmtId)>,
    cond_temp_counter: &mut u32,
) -> usize {
    let (cond_expr_id, body, otherwise) = match &package.get_expr(if_expr_id).kind {
        ExprKind::If(cond, body, otherwise) => (*cond, *body, *otherwise),
        _ => return 0,
    };

    let nested = enclosing_block != root_block;
    let mut inserted = 0;

    // Outer condition: unconditionally evaluated when the statement is reached.
    if !expr_is_side_effect_free(package, cond_expr_id) {
        let cond_ty = package.get_expr(cond_expr_id).ty.clone();
        let cond_span = package.get_expr(cond_expr_id).span;

        if nested {
            // `mutable __cond = false;` in the root block, with `__cond = cond;`
            // at the original point so side-effect timing is unchanged.
            let false_lit = alloc_bool_lit(package, assigner, false, cond_span);
            let (cond_local, mut_decl) = alloc_local_var(
                package,
                assigner,
                &next_cond_temp_name(cond_temp_counter),
                &cond_ty,
                false_lit,
                Mutability::Mutable,
            );
            root_prepends.push((root_block, mut_decl));

            let assign_lhs =
                alloc_local_var_expr(package, assigner, cond_local, cond_ty.clone(), cond_span);
            let assign = alloc_assign_expr(package, assigner, assign_lhs, cond_expr_id, cond_span);
            let set_stmt = alloc_semi_stmt(package, assigner, assign, cond_span);

            // Rewrite the `if` in place to test a pure read of the accumulator.
            let cond_var = alloc_local_var_expr(package, assigner, cond_local, cond_ty, cond_span);
            package
                .exprs
                .get_mut(if_expr_id)
                .expect("if expr not found")
                .kind = ExprKind::If(cond_var, body, otherwise);

            // Splice the `set` immediately before the `if` statement.
            package
                .blocks
                .get_mut(enclosing_block)
                .expect("block not found")
                .stmts
                .insert(insert_index, set_stmt);
            inserted = 1;
        } else {
            // `let __cond = cond;` — moves the original condition `ExprId` into
            // the initializer so it is evaluated exactly once.
            let (cond_local, let_stmt) = alloc_local_var(
                package,
                assigner,
                &next_cond_temp_name(cond_temp_counter),
                &cond_ty,
                cond_expr_id,
                Mutability::Immutable,
            );

            // Rewrite the `if` in place to test a pure read of the temporary.
            let cond_var = alloc_local_var_expr(package, assigner, cond_local, cond_ty, cond_span);
            package
                .exprs
                .get_mut(if_expr_id)
                .expect("if expr not found")
                .kind = ExprKind::If(cond_var, body, otherwise);

            // Splice the binding immediately before the `if` statement.
            package
                .blocks
                .get_mut(enclosing_block)
                .expect("block not found")
                .stmts
                .insert(insert_index, let_stmt);
            inserted = 1;
        }
    }

    // `else if` conditions: lift each side-effecting guard into a mutable
    // accumulator and conditionally assign it in its own else scope.
    inserted += hoist_else_if_chain(
        package,
        assigner,
        root_block,
        enclosing_block,
        insert_index + inserted,
        if_expr_id,
        root_prepends,
        cond_temp_counter,
    );

    inserted
}

/// Rewrites each side-effecting `else if c { body } ..` in the chain headed by
/// `head_if_id` into `else { __cond = c; if __cond { body } .. }`, where
/// `__cond` is a fresh `mutable __cond = false;` accumulator. The accumulator is
/// declared in the dominating block: `root_block` when nested (via
/// `root_prepends`), otherwise `enclosing_block`. The conditional `set`
/// preserves short-circuit order — `c` runs only when preceding guards were
/// false, and only once.
///
/// Returns the number of declarations spliced into `enclosing_block` (nested
/// declarations routed through `root_prepends` are not counted).
#[allow(clippy::too_many_arguments)]
fn hoist_else_if_chain(
    package: &mut Package,
    assigner: &mut Assigner,
    root_block: BlockId,
    enclosing_block: BlockId,
    mut insert_index: usize,
    head_if_id: ExprId,
    root_prepends: &mut Vec<(BlockId, StmtId)>,
    cond_temp_counter: &mut u32,
) -> usize {
    let nested = enclosing_block != root_block;
    let mut inserted = 0;
    let mut parent_if = head_if_id;
    loop {
        // The `else if` link is an `If` expression sitting in the parent's
        // `otherwise` position. A final `else { .. }` block (or no else) ends
        // the chain.
        let elif_id = match &package.get_expr(parent_if).kind {
            ExprKind::If(_, _, Some(other)) => *other,
            _ => return inserted,
        };
        let elif_cond = match &package.get_expr(elif_id).kind {
            ExprKind::If(cond, _, _) => *cond,
            _ => return inserted,
        };

        if !expr_is_side_effect_free(package, elif_cond) {
            let cond_ty = package.get_expr(elif_cond).ty.clone();
            let cond_span = package.get_expr(elif_cond).span;
            let if_ty = package.get_expr(elif_id).ty.clone();
            let if_span = package.get_expr(elif_id).span;

            // `mutable __cond = false;`, where `false` encodes "this guard did
            // not hold". Declared in the dominating block.
            let false_lit = alloc_bool_lit(package, assigner, false, cond_span);
            let (cond_local, mut_decl) = alloc_local_var(
                package,
                assigner,
                &next_cond_temp_name(cond_temp_counter),
                &cond_ty,
                false_lit,
                Mutability::Mutable,
            );
            if nested {
                root_prepends.push((root_block, mut_decl));
            } else {
                package
                    .blocks
                    .get_mut(enclosing_block)
                    .expect("block not found")
                    .stmts
                    .insert(insert_index, mut_decl);
                insert_index += 1;
                inserted += 1;
            }

            // `__cond = c;` — evaluates the original condition once, only
            // when this else scope is reached.
            let assign_lhs =
                alloc_local_var_expr(package, assigner, cond_local, cond_ty.clone(), cond_span);
            let assign = alloc_assign_expr(package, assigner, assign_lhs, elif_cond, cond_span);
            let set_stmt = alloc_semi_stmt(package, assigner, assign, cond_span);

            // Rewrite the `else if` to test a pure read of the accumulator.
            let cond_var = alloc_local_var_expr(package, assigner, cond_local, cond_ty, cond_span);
            if let ExprKind::If(cond, _, _) = &mut package
                .exprs
                .get_mut(elif_id)
                .expect("else-if expr not found")
                .kind
            {
                *cond = cond_var;
            }

            // Wrap the rewritten `if` as the trailing expression of a fresh
            // block `{ __cond = c; if __cond { .. } .. }` and repoint the
            // parent's `otherwise` at that block.
            let if_stmt = alloc_expr_stmt(package, assigner, elif_id, if_span);
            let else_block = alloc_block(
                package,
                assigner,
                vec![set_stmt, if_stmt],
                if_ty.clone(),
                if_span,
            );
            let else_block_expr = alloc_block_expr(package, assigner, else_block, if_ty, if_span);
            if let ExprKind::If(_, _, otherwise) = &mut package
                .exprs
                .get_mut(parent_if)
                .expect("if expr not found")
                .kind
            {
                *otherwise = Some(else_block_expr);
            }
        }

        // Continue down the chain: the `else if` node's own `otherwise` still
        // points to the next link (wrapping it in a block did not change its
        // condition/body/otherwise wiring).
        parent_if = elif_id;
    }
}
