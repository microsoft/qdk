// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Scalar Replacement of Aggregates (SROA) pass.
//!
//! Replaces local variables of tuple type with individual scalar variables,
//! eliminating intermediate tuple allocations and field-access overhead.
//!
//! Establishes [`crate::invariants::InvariantLevel::PostSroa`]:
//! synthesized local tuple patterns agree with the tuple types they
//! decompose.
//!
//! ## Prerequisites
//!
//! The `tuple_compare_lower` pass must run before SROA. It rewrites
//! equality and inequality on non-empty tuples into element-wise scalar
//! comparisons, which eliminates whole-value uses that would otherwise
//! prevent decomposition.
//!
//! ## Decomposition
//!
//! For each entry-reachable callable, the pass:
//! - Identifies local bindings whose type is `Ty::Tuple(...)` or
//!   `Ty::Udt(Res::Item(_))` (resolving to a multi-field UDT) and whose
//!   every use is `ExprKind::Field`, `ExprKind::AssignField`, or
//!   `ExprKind::Assign(Var, Tuple)` (whole-tuple reassignment with a
//!   tuple-literal RHS).
//! - Decomposes those bindings in-place: `PatKind::Bind(t)` becomes
//!   `PatKind::Tuple([Bind(t_0), Bind(t_1), ...])`, field accesses become
//!   direct variable references, and whole-tuple assignments are split into
//!   per-element assignments.
//!
//! The eligibility criterion is conservative: a binding is decomposed only
//! when *all* of its uses are field-only accesses or decomposable tuple
//! assignments. If the variable is ever passed as a whole value (e.g., as
//! an argument, a return value, or a closure capture), it is left intact.
//!
//! ## Iterative fixed-point
//!
//! The pass runs iteratively to a fixed point. Each iteration peels one
//! level of nesting: `Bind(t: (A, B))` → `Tuple([Bind(t_0: A), Bind(t_1: B)])`.
//! When `A` is itself a tuple (e.g., `(Int, Int)`), the next iteration
//! discovers `Bind(t_0: (Int, Int))` as a new candidate and decomposes it
//! further. The loop terminates when no new candidates remain.
//!
//! # Input patterns
//!
//! - `let t = (a, b, c);` where every later reference is `t::0`, `t::1`,
//!   `t::2`, `t = (a', b', c')`, or `t::0 = a'`.
//!
//! # Rewrites
//!
//! ```text
//! // Before
//! let t = (a, b, c);
//! let x = t::1;
//! t = (a', b', c');
//!
//! // After
//! let (t_0, t_1, t_2) = (a, b, c);
//! let x = t_1;
//! (t_0, t_1, t_2) = (a', b', c');
//! ```
//!
//! # Notes
//!
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds correct exec graphs at the end
//!   of the pipeline.

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::fir_builder::{
    alloc_local_var_expr, decompose_binding, functored_specs, reachable_local_callables,
    resolve_udt_element_types,
};
use crate::reachability::collect_reachable_from_entry;
use crate::walk_utils::{collect_expr_ids_in_local_callables, collect_uses_in_block};
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath, ItemKind,
    LocalItemId, LocalVarId, Package, PackageId, PackageLookup, PackageStore, PatId, PatKind, Res,
    SpecDecl, SpecImpl, Stmt, StmtId, StmtKind,
};
use qsc_fir::ty::Ty;
use rustc_hash::FxHashMap;
use std::rc::Rc;

use crate::EMPTY_EXEC_RANGE;

/// Runs the SROA pass on the entry-reachable portion of a package.
///
/// For each local binding of tuple type where every use is a field access
/// or field assignment, decomposes the binding into individual scalar
/// variables and rewrites field accesses into direct variable references.
pub fn sroa(store: &mut PackageStore, package_id: PackageId, assigner: &mut Assigner) {
    let package = store.get(package_id);
    if package.entry.is_none() {
        return;
    }

    loop {
        let reachable = collect_reachable_from_entry(store, package_id);
        let package = store.get(package_id);

        // Collect candidates across all reachable callables.
        let mut all_candidates: Vec<SroaCandidate> = Vec::new();

        for (item_id, decl) in reachable_local_callables(package, package_id, &reachable) {
            collect_candidates_in_callable(store, package_id, item_id, decl, &mut all_candidates);
        }

        if all_candidates.is_empty() {
            break;
        }

        // Apply decomposition.
        let package = store.get_mut(package_id);
        for candidate in &all_candidates {
            decompose_candidate(package, assigner, candidate);
        }
    }
}

/// A candidate for SROA decomposition.
struct SroaCandidate {
    /// The `LocalVarId` bound by the original `PatKind::Bind`.
    local_id: LocalVarId,
    /// The `PatId` of the binding pattern.
    pat_id: PatId,
    /// Element types from the tuple.
    elem_types: Vec<Ty>,
    /// The name of the original binding.
    name: Rc<str>,
    /// The callable item that owns this local binding.
    owner_item: LocalItemId,
}

/// Scans a callable's body for SROA candidates.
fn collect_candidates_in_callable(
    store: &PackageStore,
    package_id: PackageId,
    owner_item: LocalItemId,
    decl: &CallableDecl,
    candidates: &mut Vec<SroaCandidate>,
) {
    match &decl.implementation {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            collect_candidates_in_spec_impl(store, package_id, owner_item, spec_impl, candidates);
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            collect_candidates_in_spec(store, package_id, owner_item, spec, candidates);
        }
    }
}

/// Recurses into every specialization of a `SpecImpl` to collect SROA
/// candidates.
fn collect_candidates_in_spec_impl(
    store: &PackageStore,
    package_id: PackageId,
    owner_item: LocalItemId,
    spec_impl: &SpecImpl,
    candidates: &mut Vec<SroaCandidate>,
) {
    collect_candidates_in_spec(store, package_id, owner_item, &spec_impl.body, candidates);
    for spec in functored_specs(spec_impl) {
        collect_candidates_in_spec(store, package_id, owner_item, spec, candidates);
    }
}

/// Collects SROA candidates within a single `SpecDecl` body by walking
/// tuple-typed bindings and checking every use for field-only or
/// decomposable-tuple-assignment eligibility.
fn collect_candidates_in_spec(
    store: &PackageStore,
    package_id: PackageId,
    owner_item: LocalItemId,
    spec: &SpecDecl,
    candidates: &mut Vec<SroaCandidate>,
) {
    let package = store.get(package_id);
    // Collect all local bindings with composite (tuple or UDT) type.
    let bindings = find_tuple_bindings_in_block(store, package_id, spec.block);

    for binding in bindings {
        // Verify ALL uses are field-only.
        if all_uses_are_field_access(package, spec.block, binding.local_id) {
            candidates.push(SroaCandidate {
                local_id: binding.local_id,
                pat_id: binding.pat_id,
                elem_types: binding.elem_types,
                name: binding.name,
                owner_item,
            });
        }
    }
}

/// Information about a tuple-typed local binding.
struct TupleBinding {
    local_id: LocalVarId,
    pat_id: PatId,
    elem_types: Vec<Ty>,
    name: Rc<str>,
}

/// Recursively walks a pattern to find `PatKind::Bind` nodes with tuple or
/// UDT types. This handles patterns produced by a previous SROA pass that
/// transformed `Bind(t)` into `Tuple([Bind(t_0), Bind(t_1), ...])` — the
/// inner `Bind(t_0)` would otherwise be invisible to the scanner.
fn find_binds_in_pat(
    store: &PackageStore,
    package_id: PackageId,
    pat_id: PatId,
    bindings: &mut Vec<TupleBinding>,
) {
    let package = store.get(package_id);
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            let elem_types = match &pat.ty {
                Ty::Tuple(elems) if !elems.is_empty() => Some(elems.clone()),
                Ty::Udt(Res::Item(item_id)) => resolve_udt_element_types(store, item_id),
                _ => None,
            };
            if let Some(elem_types) = elem_types {
                bindings.push(TupleBinding {
                    local_id: ident.id,
                    pat_id,
                    elem_types,
                    name: ident.name.clone(),
                });
            }
        }
        PatKind::Tuple(sub_pats) => {
            for &sub_pat_id in sub_pats {
                find_binds_in_pat(store, package_id, sub_pat_id, bindings);
            }
        }
        PatKind::Discard => {}
    }
}

/// Finds all `StmtKind::Local(_, pat, _)` in a block where `pat` is
/// `PatKind::Bind(ident)` with `Ty::Tuple(elems)` or `Ty::Udt(Res::Item(_))`
/// resolving to a multi-field UDT, and the composite type is non-empty.
fn find_tuple_bindings_in_block(
    store: &PackageStore,
    package_id: PackageId,
    block_id: BlockId,
) -> Vec<TupleBinding> {
    let mut bindings = Vec::new();
    find_tuple_bindings_recursive(store, package_id, block_id, &mut bindings);
    bindings
}

/// Walks a block (recursively through nested statements and expressions)
/// collecting every candidate tuple-typed binding into `bindings`.
fn find_tuple_bindings_recursive(
    store: &PackageStore,
    package_id: PackageId,
    block_id: BlockId,
    bindings: &mut Vec<TupleBinding>,
) {
    let package = store.get(package_id);
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Local(_, pat_id, expr_id) => {
                find_binds_in_pat(store, package_id, *pat_id, bindings);
                // Recurse into nested blocks in the RHS expression.
                find_tuple_bindings_in_expr_id(store, package_id, *expr_id, bindings);
            }
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                find_tuple_bindings_in_expr_id(store, package_id, *e, bindings);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Descends into an expression subtree collecting candidate bindings from
/// nested blocks, conditionals, while-loops, and match-like constructs.
fn find_tuple_bindings_in_expr_id(
    store: &PackageStore,
    package_id: PackageId,
    expr_id: ExprId,
    bindings: &mut Vec<TupleBinding>,
) {
    let package = store.get(package_id);
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Block(block_id) | ExprKind::While(_, block_id) => {
            find_tuple_bindings_recursive(store, package_id, *block_id, bindings);
        }
        ExprKind::If(_, body, otherwise) => {
            find_tuple_bindings_in_expr_id(store, package_id, *body, bindings);
            if let Some(e) = otherwise {
                find_tuple_bindings_in_expr_id(store, package_id, *e, bindings);
            }
        }
        _ => {}
    }
}

/// Returns `true` if every use of `local_id` in the block is a field access
/// (`ExprKind::Field(Var(Local(id)), Path(_))`) or a field assignment
/// (`ExprKind::AssignField(Var(Local(id)), _, _)`).
///
/// Returns `false` if `local_id` is used in any other context: passed as an
/// argument, returned, captured by closure, assigned whole, etc.
fn all_uses_are_field_access(package: &Package, block_id: BlockId, local_id: LocalVarId) -> bool {
    let mut uses = Vec::new();
    collect_uses_in_block(package, block_id, local_id, &mut uses);
    uses.iter().all(|u| *u)
}

/// Decomposes a single SROA candidate in-place.
///
/// # Before
/// ```text
/// let t : (A, B) = (a, b);   // single tuple binding
/// use(t.0); use(t.1);        // only field accesses
/// ```
/// # After
/// ```text
/// let (t_0, t_1) : (A, B) = (a, b);   // binding split to scalars
/// use(t_0); use(t_1);                  // field accesses → direct vars
/// ```
///
/// # Mutations
/// - Rewrites the binding `Pat` from `Bind` to `Tuple` of per-element `Bind`s.
/// - Allocates new `LocalVarId`, `PatId` nodes through `assigner`.
/// - Delegates to [`rewrite_field_accesses`] and [`rewrite_assign_tuples`].
fn decompose_candidate(package: &mut Package, assigner: &mut Assigner, candidate: &SroaCandidate) {
    let new_locals = decompose_binding(
        package,
        assigner,
        candidate.pat_id,
        &candidate.name,
        &candidate.elem_types,
    );

    // Rewrite all field accesses and assign-field expressions.
    rewrite_field_accesses(
        package,
        assigner,
        candidate.owner_item,
        candidate.local_id,
        &new_locals,
        &candidate.elem_types,
    );

    // Split `Assign(Var(Local(old)), Tuple([e0, e1, ...]))` into per-element
    // assignments. This must run AFTER field access rewriting so that any
    // `Field(Var(Local(old)), Path([i]))` references in the RHS elements
    // have already been rewritten to `Var(Local(new_i))`.
    rewrite_assign_tuples(
        package,
        assigner,
        candidate.owner_item,
        candidate.local_id,
        &new_locals,
        &candidate.elem_types,
    );
}

/// Rewrites all `ExprKind::Field(Var(Local(old)), Path([i, ...]))` and
/// `ExprKind::AssignField(Var(Local(old)), Path([i, ...]), value)` uses across
/// the entire package so they target the decomposed scalar or nested aggregate
/// for the first path segment.
///
/// # Before
/// ```text
/// Field(Var(Local(old)), Path([1]))   // tuple.1
/// ```
/// # After
/// ```text
/// Var(Local(old_1))   // direct scalar reference
/// ```
///
/// # Mutations
/// - Allocates replacement `Var` and `Field` `Expr` nodes through `assigner`.
/// - Redirects all parent references from old to new via
///   [`replace_expr_references`].
fn rewrite_field_accesses(
    package: &mut Package,
    assigner: &mut Assigner,
    owner_item: LocalItemId,
    old_local: LocalVarId,
    new_locals: &[LocalVarId],
    elem_types: &[Ty],
) {
    // Collect ExprIds only from the owning callable (locals cannot escape).
    let expr_ids = collect_expr_ids_in_local_callables(&*package, &[owner_item]);

    for expr_id in expr_ids {
        rewrite_single_expr(
            package, assigner, owner_item, expr_id, old_local, new_locals, elem_types,
        );
    }
}

/// Rewrites a single expression to replace references to an SROA-decomposed
/// tuple local with references to its scalar replacements.
///
/// Handles two `ExprKind::Field` cases:
///
/// - **Single-index path** (`t.i`): synthesize a fresh `Var(t_i)` expression
///   and redirect references to the old projection expression to it.
/// - **Nested path** (`t.i.j...`): synthesize a fresh `Var(t_i)` expression
///   and a fresh `Field(.., Path([j, ...]))` wrapper. Redirecting references
///   instead of mutating the original projection keeps shared expression nodes
///   stable for sibling projections created by earlier passes.
#[allow(clippy::too_many_lines)]
fn rewrite_single_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    owner_item: LocalItemId,
    expr_id: ExprId,
    old_local: LocalVarId,
    new_locals: &[LocalVarId],
    elem_types: &[Ty],
) {
    let expr = package.exprs.get(expr_id).expect("expr should exist");
    match expr.kind.clone() {
        ExprKind::Field(inner_id, Field::Path(path)) => {
            let span = expr.span;
            let expr_ty = expr.ty.clone();
            let inner = package
                .exprs
                .get(inner_id)
                .expect("inner expr should exist");
            if let ExprKind::Var(Res::Local(var_id), _) = &inner.kind
                && *var_id == old_local
                && !path.indices.is_empty()
            {
                let idx = path.indices[0];
                if idx < new_locals.len() {
                    let new_local = new_locals[idx];
                    if path.indices.len() == 1 {
                        let replacement_id = {
                            let ty = elem_types[idx].clone();
                            alloc_local_var_expr(package, assigner, new_local, ty, span)
                        };
                        replace_expr_references(package, owner_item, expr_id, replacement_id);
                    } else {
                        // Nested: t.i.j... -> Field(Var(t_i), Path([j, ...]))
                        let remaining: Vec<usize> = path.indices[1..].to_vec();

                        let new_inner_id = {
                            let ty = elem_types[idx].clone();
                            alloc_local_var_expr(package, assigner, new_local, ty, span)
                        };
                        let replacement_id = assigner.next_expr();
                        package.exprs.insert(
                            replacement_id,
                            Expr {
                                id: replacement_id,
                                span,
                                ty: expr_ty,
                                kind: ExprKind::Field(
                                    new_inner_id,
                                    Field::Path(FieldPath { indices: remaining }),
                                ),
                                exec_graph_range: EMPTY_EXEC_RANGE,
                            },
                        );
                        replace_expr_references(package, owner_item, expr_id, replacement_id);
                    }
                }
            }
        }
        ExprKind::AssignField(record_id, Field::Path(path), value_id) => {
            let span = expr.span;
            let expr_ty = expr.ty.clone();
            let record = package
                .exprs
                .get(record_id)
                .expect("record expr should exist");
            if let ExprKind::Var(Res::Local(var_id), _) = &record.kind
                && *var_id == old_local
                && !path.indices.is_empty()
            {
                let idx = path.indices[0];
                if idx < new_locals.len() {
                    let new_local = new_locals[idx];
                    let new_record_id = {
                        let ty = elem_types[idx].clone();
                        alloc_local_var_expr(package, assigner, new_local, ty, span)
                    };

                    let replacement_id = assigner.next_expr();
                    let replacement_kind = if path.indices.len() == 1 {
                        ExprKind::Assign(new_record_id, value_id)
                    } else {
                        ExprKind::AssignField(
                            new_record_id,
                            Field::Path(FieldPath {
                                indices: path.indices[1..].to_vec(),
                            }),
                            value_id,
                        )
                    };
                    package.exprs.insert(
                        replacement_id,
                        Expr {
                            id: replacement_id,
                            span,
                            ty: expr_ty,
                            kind: replacement_kind,
                            exec_graph_range: EMPTY_EXEC_RANGE,
                        },
                    );
                    replace_expr_references(package, owner_item, expr_id, replacement_id);
                }
            }
        }
        _ => {}
    }
}

/// Rewrites every reference to `old_expr_id` in the owner callable to point at
/// `new_expr_id`.
///
/// Before, entry, statements, and parent expressions still point at the
/// aggregate expression that SROA wants to replace. After, every such edge
/// points at the scalarized replacement, allowing the old node to become dead.
fn replace_expr_references(
    package: &mut Package,
    owner_item: LocalItemId,
    old_expr_id: ExprId,
    new_expr_id: ExprId,
) {
    if package.entry == Some(old_expr_id) {
        package.entry = Some(new_expr_id);
    }

    // Collect owner's block IDs and expr IDs with immutable borrow, then mutate.
    let (block_ids, expr_ids) = {
        let blocks = collect_all_block_ids_in_callable(&*package, owner_item);
        let exprs = collect_expr_ids_in_local_callables(&*package, &[owner_item]);
        (blocks, exprs)
    };

    for block_id in &block_ids {
        let stmts: Vec<StmtId> = package.get_block(*block_id).stmts.clone();
        for stmt_id in stmts {
            let stmt = package.stmts.get_mut(stmt_id).expect("stmt should exist");
            replace_expr_in_stmt(stmt, old_expr_id, new_expr_id);
        }
    }

    for expr_id in expr_ids {
        let expr = package.exprs.get_mut(expr_id).expect("expr should exist");
        replace_expr_in_expr(expr, old_expr_id, new_expr_id);
    }
}

fn replace_expr_in_stmt(stmt: &mut Stmt, old_expr_id: ExprId, new_expr_id: ExprId) {
    match &mut stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            replace_expr_id(expr_id, old_expr_id, new_expr_id);
        }
        StmtKind::Item(_) => {}
    }
}

fn replace_expr_in_expr(expr: &mut Expr, old_expr_id: ExprId, new_expr_id: ExprId) {
    match &mut expr.kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for expr_id in exprs {
                replace_expr_id(expr_id, old_expr_id, new_expr_id);
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
            replace_expr_id(a, old_expr_id, new_expr_id);
            replace_expr_id(b, old_expr_id, new_expr_id);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            replace_expr_id(a, old_expr_id, new_expr_id);
            replace_expr_id(b, old_expr_id, new_expr_id);
            replace_expr_id(c, old_expr_id, new_expr_id);
        }
        ExprKind::Fail(expr_id)
        | ExprKind::Field(expr_id, _)
        | ExprKind::Return(expr_id)
        | ExprKind::UnOp(_, expr_id) => {
            replace_expr_id(expr_id, old_expr_id, new_expr_id);
        }
        ExprKind::If(cond, body, otherwise) => {
            replace_expr_id(cond, old_expr_id, new_expr_id);
            replace_expr_id(body, old_expr_id, new_expr_id);
            if let Some(expr_id) = otherwise {
                replace_expr_id(expr_id, old_expr_id, new_expr_id);
            }
        }
        ExprKind::Range(start, step, end) => {
            for expr_id in [start, step, end].into_iter().flatten() {
                replace_expr_id(expr_id, old_expr_id, new_expr_id);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(expr_id) = copy {
                replace_expr_id(expr_id, old_expr_id, new_expr_id);
            }
            for field in fields {
                replace_expr_id(&mut field.value, old_expr_id, new_expr_id);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(expr_id) = component {
                    replace_expr_id(expr_id, old_expr_id, new_expr_id);
                }
            }
        }
        ExprKind::While(cond, _) => {
            replace_expr_id(cond, old_expr_id, new_expr_id);
        }
        ExprKind::Block(_)
        | ExprKind::Closure(_, _)
        | ExprKind::Hole
        | ExprKind::Lit(_)
        | ExprKind::Var(_, _) => {}
    }
}

fn replace_expr_id(expr_id: &mut ExprId, old_expr_id: ExprId, new_expr_id: ExprId) {
    if *expr_id == old_expr_id {
        *expr_id = new_expr_id;
    }
}

/// Builds a mapping from `StmtId` → `BlockId` for the owner callable's blocks.
fn build_stmt_block_map_for_callable(
    package: &Package,
    item_id: LocalItemId,
) -> FxHashMap<StmtId, BlockId> {
    let mut map = FxHashMap::default();
    let block_ids = collect_all_block_ids_in_callable(package, item_id);
    for block_id in block_ids {
        let block = package.get_block(block_id);
        for &stmt_id in &block.stmts {
            map.insert(stmt_id, block_id);
        }
    }
    map
}

/// Collects all block IDs reachable from a callable's implementation.
fn collect_all_block_ids_in_callable(package: &Package, item_id: LocalItemId) -> Vec<BlockId> {
    let Some(item) = package.items.get(item_id) else {
        return Vec::new();
    };
    let ItemKind::Callable(decl) = &item.kind else {
        return Vec::new();
    };
    let mut block_ids = Vec::new();
    // Include spec-level blocks.
    match &decl.implementation {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            block_ids.push(spec_impl.body.block);
            for spec in functored_specs(spec_impl) {
                block_ids.push(spec.block);
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            block_ids.push(spec.block);
        }
    }
    // Include nested blocks found via expression walking.
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_, expr| match &expr.kind {
            ExprKind::Block(bid) | ExprKind::While(_, bid) => {
                block_ids.push(*bid);
            }
            _ => {}
        },
    );
    block_ids
}

/// Splits `Assign(Var(Local(old)), Tuple([e0, e1, ...]))` into per-element
/// assignments across the containing block.
///
/// # Before
/// ```text
/// set old = (a, b);   // single Semi(Assign(..)) statement
/// ```
/// # After
/// ```text
/// set old_0 = a;   // original stmt rewritten in-place
/// set old_1 = b;   // new stmt inserted after
/// ```
///
/// # Mutations
/// - Rewrites the original `Assign` `ExprKind` in-place for element 0.
/// - Allocates new `Expr` and `Stmt` nodes for elements 1..n-1.
/// - Inserts new statements into the containing block after the original.
fn rewrite_assign_tuples(
    package: &mut Package,
    assigner: &mut Assigner,
    owner_item: LocalItemId,
    old_local: LocalVarId,
    new_locals: &[LocalVarId],
    elem_types: &[Ty],
) {
    let stmt_block_map = build_stmt_block_map_for_callable(package, owner_item);

    // Collect (stmt_id, expr_id, elements) for all matching Assign-Tuple patterns.
    let mut rewrites: Vec<(StmtId, ExprId, Vec<ExprId>)> = Vec::new();

    for &stmt_id in stmt_block_map.keys() {
        let stmt = package.stmts.get(stmt_id).expect("stmt should exist");
        let semi_expr_id = match &stmt.kind {
            StmtKind::Semi(e) => *e,
            _ => continue,
        };
        let expr = package.exprs.get(semi_expr_id).expect("expr should exist");
        if let ExprKind::Assign(lhs_id, rhs_id) = &expr.kind {
            let lhs = package.exprs.get(*lhs_id).expect("lhs should exist");
            if let ExprKind::Var(Res::Local(var_id), _) = &lhs.kind
                && *var_id == old_local
            {
                let rhs = package.exprs.get(*rhs_id).expect("rhs should exist");
                if let ExprKind::Tuple(elements) = &rhs.kind {
                    rewrites.push((stmt_id, semi_expr_id, elements.clone()));
                }
            }
        }
    }

    for (stmt_id, assign_expr_id, elements) in rewrites {
        let n = elements.len().min(new_locals.len());
        if n == 0 {
            continue;
        }

        // Rewrite the original Assign in-place to target the first element.
        {
            // Create a new Var expr for the first element's LHS.
            let new_lhs_id = assigner.next_expr();
            let new_lhs = Expr {
                id: new_lhs_id,
                span: Span::default(),
                ty: elem_types[0].clone(),
                kind: ExprKind::Var(Res::Local(new_locals[0]), vec![]),
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
            package.exprs.insert(new_lhs_id, new_lhs);

            let assign = package
                .exprs
                .get_mut(assign_expr_id)
                .expect("assign expr exists");
            assign.kind = ExprKind::Assign(new_lhs_id, elements[0]);
            assign.ty = elem_types[0].clone();
        }

        // For elements 1..n, create new Assign exprs and Semi stmts.
        let mut new_stmt_ids: Vec<StmtId> = Vec::with_capacity(n - 1);
        for i in 1..n {
            let lhs_id = assigner.next_expr();
            let lhs_expr = Expr {
                id: lhs_id,
                span: Span::default(),
                ty: elem_types[i].clone(),
                kind: ExprKind::Var(Res::Local(new_locals[i]), vec![]),
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
            package.exprs.insert(lhs_id, lhs_expr);

            let assign_id = assigner.next_expr();
            let assign_expr = Expr {
                id: assign_id,
                span: Span::default(),
                ty: elem_types[i].clone(),
                kind: ExprKind::Assign(lhs_id, elements[i]),
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
            package.exprs.insert(assign_id, assign_expr);

            let new_stmt_id = assigner.next_stmt();
            let new_stmt = Stmt {
                id: new_stmt_id,
                span: Span::default(),
                kind: StmtKind::Semi(assign_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
            package.stmts.insert(new_stmt_id, new_stmt);
            new_stmt_ids.push(new_stmt_id);
        }

        // Insert the new stmts into the containing block after the original stmt.
        if let Some(&block_id) = stmt_block_map.get(&stmt_id) {
            let block = package
                .blocks
                .get_mut(block_id)
                .expect("block should exist");
            if let Some(pos) = block.stmts.iter().position(|&s| s == stmt_id) {
                for (offset, new_id) in new_stmt_ids.into_iter().enumerate() {
                    block.stmts.insert(pos + 1 + offset, new_id);
                }
            }
        }
    }
}
