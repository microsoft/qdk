// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Conditional-guard normalization preserves selection-time values for later
//! dispatch. Statement conditions and callable-valued selections share the same
//! guard storage, while retaining their original evaluation positions.
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
//! **Statement-position** `if` conditions (an `if` that is itself an
//! `Expr`/`Semi` statement, e.g. the retained `if cond { op = X }` of a
//! mutable-reassignment) are normalized — these are the conditions
//! defunctionalization both keeps and reuses. Every condition in such a chain
//! is normalized, not just the outer one.
//!
//! **Callable selections** additionally include value-position `if`s, logical
//! short-circuit expressions and compound logical assignments. Guards that may
//! have effects or fail, or whose reads are overwritten by their selected body
//! or store, are snapshotted. Discard-safe evaluation is not enough to prove
//! that a mutable read remains stable.
//!
//! Non-callable value-position `if`s and `while` guards are left untouched.
//! Loop-contained selection guards are refreshed at their original evaluation
//! point on each iteration; lazy operands stay lazy.
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
//! Callable-selection snapshots use the same root declaration/original-point
//! assignment mechanism even inside operands. Defunctionalization requests this
//! normalization again when specialization or callee rewriting creates new code.
//! Pure guards that read nested-block locals also need root-scoped storage:
//! their original bindings do not dominate a later dispatch outside that block.
//!
//! Synthesized nodes use [`crate::EMPTY_EXEC_RANGE`];
//! [`crate::exec_graph_rebuild`] rebuilds exec graphs later.

#[cfg(test)]
mod tests;

use crate::fir_builder::{
    alloc_assign_expr, alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr, alloc_expr_stmt,
    alloc_local_var, alloc_local_var_expr, alloc_semi_stmt, functored_specs,
    reachable_local_callables,
};
use crate::package_assigners::PackageAssigners;
use crate::reachability::{collect_reachable_from_entry, collect_reachable_package_closure};
use crate::walk_utils::expr_is_side_effect_free;
use crate::walk_utils::{
    DirectChild, assignment_written_locals, for_each_direct_child, for_each_expr,
    for_each_expr_in_block,
};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, BlockId, CallableImpl, Expr, ExprId, ExprKind, ItemKind, Mutability, Package, PackageId,
    PackageLookup, PackageStore, Res, SpecImpl, StmtId, StmtKind, StoreItemId,
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

/// Normalizes statement conditions and callable selections across reachable
/// packages, preserving evaluation order and dominating guard storage.
///
/// Runs after return unification and before defunctionalization. Defunc also
/// calls [`normalize_callable_selections`] for newly generated selections.
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
        let assigner = assigners.get_mut(store, pkg);
        normalize_callable_selections(store.get_mut(pkg), assigner);
    }
}

/// A dominating declaration and its original-point evaluation, plus the read
/// used by both the original selection and any later dispatch.
struct GuardSnapshot {
    declaration: StmtId,
    evaluation: StmtId,
    read: ExprId,
}

/// Allocates the storage for one Boolean guard without choosing where to place
/// it. All guard-normalization paths share this declaration/assignment/read
/// contract.
///
/// # Before
/// ```text
/// condition
/// ```
/// # After
/// ```text
/// declaration: mutable guard = false;
/// evaluation:  set guard = condition;
/// read:        guard
/// ```
///
/// # Placement
/// The caller places `declaration` in a scope that dominates every use, keeps
/// `evaluation` at the condition's original evaluation point, and replaces the
/// original guard with `read`. In a loop, the declaration may be outside the
/// loop, but the assignment must execute on each reached iteration.
///
/// # Mutations
/// - Allocates the local, binding pattern, statements, and read/write expressions.
/// - Reuses `condition` as the assignment RHS; does not rewrite it or insert the
///   returned statements into any block.
fn snapshot_guard(
    package: &mut Package,
    assigner: &mut Assigner,
    condition: ExprId,
    name: &str,
) -> GuardSnapshot {
    let condition = package.get_expr(condition).clone();
    let initial = alloc_bool_lit(package, assigner, false, condition.span);
    let (local, declaration) = alloc_local_var(
        package,
        assigner,
        name,
        &condition.ty,
        initial,
        Mutability::Mutable,
    );
    let target = alloc_local_var_expr(
        package,
        assigner,
        local,
        condition.ty.clone(),
        condition.span,
    );
    let assignment = alloc_assign_expr(package, assigner, target, condition.id, condition.span);
    let evaluation = alloc_semi_stmt(package, assigner, assignment, condition.span);
    let read = alloc_local_var_expr(package, assigner, local, condition.ty, condition.span);
    GuardSnapshot {
        declaration,
        evaluation,
        read,
    }
}

/// Normalizes callable-selection guards in a package, including fresh code
/// introduced during defunctionalization. The declaration dominates dispatch;
/// the assignment stays inside the original expression, including lazy operands
/// and loop bodies. Already-normalized reads are left unchanged.
///
/// Like the other package-local defunc prerequisites, this visits every explicit
/// specialization and the package entry, without following closure target edges.
///
/// # Before
/// ```text
/// let op = if Guard() { First } else { Second };
/// let value = Gate() and { set f = Times2; true };
/// set enabled and= { set f = Times2; false };
/// ```
/// # After
/// ```text
/// // Declarations go at the specialization root, or in a new entry wrapper.
/// mutable op_guard = false;
/// mutable gate_guard = false;
/// mutable store_guard = false;
///
/// let op = { set op_guard = Guard(); if op_guard { First } else { Second } };
/// let value = { set gate_guard = Gate(); gate_guard and { set f = Times2; true } };
/// { set store_guard = enabled;
///   set enabled = store_guard and { set f = Times2; false }; }
/// // Later dispatch reads the saved guards, not Guard(), Gate(), or enabled.
/// ```
///
/// `or` and `or=` follow the same scheme with their original short-circuit
/// behavior. Each assignment remains inside the original operand: it is not
/// moved ahead of earlier operands or out of a conditional branch.
///
/// # Eligibility
/// [`callable_guard_to_snapshot`] selects guards that participate in callable
/// selection and either may have effects/failures, read nested-block locals, or
/// read locals overwritten by their branches or compound store. Stable,
/// root-scoped, discard-safe guards and selections without callable-valued
/// results or writes remain unchanged.
///
/// # Mutations
/// - Allocates guard storage through [`snapshot_guard`].
/// - Replaces each selected expression's kind with a block containing the
///   guard assignment and rewritten selection, preserving its type.
/// - Prepends declarations to the owning specialization root, or wraps the
///   package entry to give its declarations a dominating scope.
/// - Leaves callable signatures unchanged. Re-running on normalized selections
///   allocates nothing; defunc can call this again for newly generated code.
pub(crate) fn normalize_callable_selections(package: &mut Package, assigner: &mut Assigner) {
    let mut roots = Vec::new();
    if let Some(entry) = package.entry {
        let mut expressions = Vec::new();
        for_each_expr(package, entry, &mut |id, _| expressions.push(id));
        roots.push((None, expressions));
    }
    for (_, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(specs) = &decl.implementation
        {
            for spec in std::iter::once(&specs.body).chain(functored_specs(specs)) {
                let mut expressions = Vec::new();
                for_each_expr_in_block(package, spec.block, &mut |id, _| expressions.push(id));
                roots.push((Some(spec.block), expressions));
            }
        }
    }
    for (root, expressions) in roots {
        let lexical_root = root.or_else(|| {
            package
                .entry
                .and_then(|entry| match package.get_expr(entry).kind {
                    ExprKind::Block(block) => Some(block),
                    _ => None,
                })
        });
        let nested_locals = nested_block_locals(package, &expressions, lexical_root);
        let mut declarations = Vec::new();
        for id in expressions {
            let expression = package.get_expr(id).clone();
            let Some(condition) = callable_guard_to_snapshot(package, &expression, &nested_locals)
            else {
                continue;
            };
            let snapshot = snapshot_guard(package, assigner, condition, "_.branch_guard");
            declarations.push(snapshot.declaration);
            let kind = match expression.kind {
                ExprKind::If(_, body, otherwise) => ExprKind::If(snapshot.read, body, otherwise),
                ExprKind::BinOp(op, _, rhs) => ExprKind::BinOp(op, snapshot.read, rhs),
                ExprKind::AssignOp(op, lhs, rhs) => {
                    let value = alloc_expr(
                        package,
                        assigner,
                        package.get_expr(condition).ty.clone(),
                        ExprKind::BinOp(op, snapshot.read, rhs),
                        expression.span,
                    );
                    ExprKind::Assign(lhs, value)
                }
                _ => unreachable!("only conditional expressions are selected"),
            };
            let selected = alloc_expr(
                package,
                assigner,
                expression.ty.clone(),
                kind,
                expression.span,
            );
            let tail = alloc_expr_stmt(package, assigner, selected, expression.span);
            let block = alloc_block(
                package,
                assigner,
                vec![snapshot.evaluation, tail],
                expression.ty,
                expression.span,
            );
            package.exprs.get_mut(id).expect("selection exists").kind = ExprKind::Block(block);
        }
        if declarations.is_empty() {
            continue;
        }
        if let Some(root) = root {
            let block = package.blocks.get_mut(root).expect("root block exists");
            declarations.append(&mut block.stmts);
            block.stmts = declarations;
        } else if let Some(entry) = package.entry {
            let expression = package.get_expr(entry).clone();
            declarations.push(alloc_expr_stmt(package, assigner, entry, expression.span));
            let block = alloc_block(
                package,
                assigner,
                declarations,
                expression.ty.clone(),
                expression.span,
            );
            package.entry = Some(alloc_block_expr(
                package,
                assigner,
                block,
                expression.ty,
                expression.span,
            ));
        }
    }
}

/// Locals declared below the root cannot be referenced by a dispatch moved
/// outside that block, even when reading the guard is pure and immutable.
fn nested_block_locals(
    package: &Package,
    expressions: &[ExprId],
    root: Option<BlockId>,
) -> FxHashSet<qsc_fir::fir::LocalVarId> {
    let mut locals = FxHashSet::default();
    for id in expressions {
        for_each_direct_child(&package.get_expr(*id).kind, |child| {
            let DirectChild::Block(block) = child else {
                return;
            };
            if Some(block) == root {
                return;
            }
            for stmt in &package.get_block(block).stmts {
                let StmtKind::Local(_, pat, _) = package.get_stmt(*stmt).kind else {
                    continue;
                };
                let mut patterns = vec![pat];
                while let Some(pat) = patterns.pop() {
                    match &package.get_pat(pat).kind {
                        qsc_fir::fir::PatKind::Bind(binding) => {
                            locals.insert(binding.id);
                        }
                        qsc_fir::fir::PatKind::Tuple(items) => patterns.extend(items),
                        qsc_fir::fir::PatKind::Discard => {}
                    }
                }
            }
        });
    }
    locals
}

/// Selects the original guard operand that needs a selection-time snapshot.
///
/// Recognizes `If`, short-circuit `and`/`or`, and compound `and=`/`or=`. A
/// callable selection has an arrow at its result type's root or beneath tuple
/// fields, or assigns such a value within a branch. Arrays and nominal UDTs
/// remain opaque to this narrow test.
///
/// A guard needs storage if it is not safe to discard, or if one of its local
/// reads is declared in a nested block or overwritten by a branch or the
/// compound assignment itself. For
/// example, `enabled and= { set f = Times2; false }` must save the old `enabled`,
/// even though reading it is pure: the final stored value is not the decision
/// that selected `f`.
///
/// Returns `None` when no snapshot is needed. This is a read-only eligibility
/// check; [`normalize_callable_selections`] performs the before/after rewrite.
fn callable_guard_to_snapshot(
    package: &Package,
    expression: &Expr,
    nested_locals: &FxHashSet<qsc_fir::fir::LocalVarId>,
) -> Option<ExprId> {
    let (condition, branches) = match &expression.kind {
        ExprKind::If(condition, body, otherwise) => (
            *condition,
            std::iter::once(*body).chain(*otherwise).collect::<Vec<_>>(),
        ),
        ExprKind::BinOp(BinOp::AndL | BinOp::OrL, condition, rhs)
        | ExprKind::AssignOp(BinOp::AndL | BinOp::OrL, condition, rhs) => (*condition, vec![*rhs]),
        _ => return None,
    };
    // Compound logical stores can overwrite their own selector even if the
    // selected RHS writes only a different callable local.
    let mut writes: FxHashSet<_> = assignment_written_locals(package, expression)
        .into_iter()
        .collect();
    let mut selects_callable = crate::defunctionalize::ty_contains_arrow(&expression.ty);
    for branch in branches {
        for_each_expr(package, branch, &mut |_, expression| {
            writes.extend(assignment_written_locals(package, expression));
            if let ExprKind::Assign(_, value)
            | ExprKind::AssignField(_, _, value)
            | ExprKind::AssignIndex(_, _, value) = expression.kind
            {
                selects_callable |=
                    crate::defunctionalize::ty_contains_arrow(&package.get_expr(value).ty);
            }
        });
    }
    if !selects_callable {
        return None;
    }
    if !crate::walk_utils::expr_is_safe_to_discard(package, package.id, condition) {
        return Some(condition);
    }
    let mut needs_snapshot = false;
    for_each_expr(package, condition, &mut |_, expression| {
        if let ExprKind::Var(Res::Local(local), _) = expression.kind {
            needs_snapshot |= writes.contains(&local) || nested_locals.contains(&local);
        }
    });
    needs_snapshot.then_some(condition)
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
        collect_targets_in_callable_impl(package, package_id, &decl.implementation, &mut targets);
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
            package_id,
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
    package_id: PackageId,
    callable_impl: &CallableImpl,
    targets: &mut Vec<ConditionTarget>,
) {
    match callable_impl {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            collect_targets_in_spec_impl(package, package_id, spec_impl, targets);
        }
    }
}

/// Collects normalization targets from every present specialization (`body`,
/// `adj`, `ctl`, `ctl-adj`) of a spec implementation.
fn collect_targets_in_spec_impl(
    package: &Package,
    package_id: PackageId,
    spec_impl: &SpecImpl,
    targets: &mut Vec<ConditionTarget>,
) {
    collect_targets_in_block(
        package,
        package_id,
        spec_impl.body.block,
        spec_impl.body.block,
        targets,
    );
    if let Some(adj) = &spec_impl.adj {
        collect_targets_in_block(package, package_id, adj.block, adj.block, targets);
    }
    if let Some(ctl) = &spec_impl.ctl {
        collect_targets_in_block(package, package_id, ctl.block, ctl.block, targets);
    }
    if let Some(ctl_adj) = &spec_impl.ctl_adj {
        collect_targets_in_block(package, package_id, ctl_adj.block, ctl_adj.block, targets);
    }
}

/// Records each statement-position `if` with a side-effecting condition,
/// recursing into nested blocks so `if`s at any depth are caught. `root_block`
/// is threaded unchanged so each target knows the dominating scope.
fn collect_targets_in_block(
    package: &Package,
    package_id: PackageId,
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
                    && if_chain_has_side_effecting_cond(package, package_id, surface)
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
            collect_targets_in_block(package, package_id, root_block, child, targets);
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
/// chain headed by `if_expr_id` carries side effects (per
/// [`expr_is_side_effect_free`]). Only `else if` links — an `If`
/// expression in `otherwise` position — are followed; a final `else { .. }`
/// block has no condition and stops the walk.
fn if_chain_has_side_effecting_cond(
    package: &Package,
    package_id: PackageId,
    if_expr_id: ExprId,
) -> bool {
    let mut current = if_expr_id;
    loop {
        let ExprKind::If(cond, _, otherwise) = &package.get_expr(current).kind else {
            return false;
        };
        if !expr_is_side_effect_free(package, package_id, *cond) {
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
    package_id: PackageId,
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
    if !expr_is_side_effect_free(package, package_id, cond_expr_id) {
        let cond_ty = package.get_expr(cond_expr_id).ty.clone();
        let cond_span = package.get_expr(cond_expr_id).span;

        if nested {
            // `mutable __cond = false;` in the root block, with `__cond = cond;`
            // at the original point so side-effect timing is unchanged.
            let snapshot = snapshot_guard(
                package,
                assigner,
                cond_expr_id,
                &next_cond_temp_name(cond_temp_counter),
            );
            root_prepends.push((root_block, snapshot.declaration));

            // Rewrite the `if` in place to test a pure read of the accumulator.
            package
                .exprs
                .get_mut(if_expr_id)
                .expect("if expr not found")
                .kind = ExprKind::If(snapshot.read, body, otherwise);

            // Splice the `set` immediately before the `if` statement.
            package
                .blocks
                .get_mut(enclosing_block)
                .expect("block not found")
                .stmts
                .insert(insert_index, snapshot.evaluation);
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
        package_id,
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
    package_id: PackageId,
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

        if !expr_is_side_effect_free(package, package_id, elif_cond) {
            let if_ty = package.get_expr(elif_id).ty.clone();
            let if_span = package.get_expr(elif_id).span;

            // `mutable __cond = false;`, where `false` encodes "this guard did
            // not hold". Declared in the dominating block.
            let snapshot = snapshot_guard(
                package,
                assigner,
                elif_cond,
                &next_cond_temp_name(cond_temp_counter),
            );
            if nested {
                root_prepends.push((root_block, snapshot.declaration));
            } else {
                package
                    .blocks
                    .get_mut(enclosing_block)
                    .expect("block not found")
                    .stmts
                    .insert(insert_index, snapshot.declaration);
                insert_index += 1;
                inserted += 1;
            }

            // Rewrite the `else if` to test a pure read of the accumulator.
            if let ExprKind::If(cond, _, _) = &mut package
                .exprs
                .get_mut(elif_id)
                .expect("else-if expr not found")
                .kind
            {
                *cond = snapshot.read;
            }

            // Wrap the rewritten `if` as the trailing expression of a fresh
            // block `{ __cond = c; if __cond { .. } .. }` and repoint the
            // parent's `otherwise` at that block.
            let if_stmt = alloc_expr_stmt(package, assigner, elif_id, if_span);
            let else_block = alloc_block(
                package,
                assigner,
                vec![snapshot.evaluation, if_stmt],
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
