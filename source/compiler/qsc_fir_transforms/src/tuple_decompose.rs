// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tuple decomposition pass — runs after tuple-compare lowering; iterates with
//! `arg_promote` to a fixed point (see [`crate`]).
//!
//! Replaces local variables of tuple type with individual scalar variables,
//! eliminating intermediate tuple allocations and field-access overhead. This
//! is the local-variable counterpart to [`crate::arg_promote`] (which does the
//! same at parameter boundaries) and is modeled on LLVM's "scalar replacement
//! of aggregates", but operates only on Q# tuples, not arrays or memory.
//!
//! # What to know before diving in
//!
//! - **Establishes [`crate::invariants::InvariantLevel::PostTupleDecompose`]:**
//!   synthesized local tuple patterns agree with the tuple types they
//!   decompose.
//! - **Conservative eligibility.** A `Bind` of non-empty `Ty::Tuple` is
//!   decomposed only when *every* use is `Field`, `AssignField`, or
//!   `Assign(Var, Tuple)` (whole-tuple reassignment with a tuple literal). If
//!   the value is ever passed whole (argument, return, closure capture), it is
//!   left intact. `Bind(t)` becomes `Tuple([Bind(t_0), Bind(t_1), ...])`, field
//!   accesses become direct var refs, and whole-tuple assigns split per element.
//! - **Iterative fixed point.** Each iteration peels one nesting level; a
//!   newly exposed tuple-typed leaf (`t_0: (Int, Int)`) is decomposed on the
//!   next round. Terminates when no candidates remain. `tuple_compare_lower`
//!   must run first to remove whole-value comparison uses.
//! - **Aliased `ExprId`s from `tuple_compare_lower` (cross-pass contract).**
//!   The preceding pass can leave a single element `ExprId` under multiple
//!   parent edges. `replace_expr_references` (used by `rewrite_field_accesses`)
//!   redirects parent edges by walking every reachable edge in the owning
//!   callable rather than shared-mutating child nodes, so redirecting one edge
//!   does not corrupt the others. See the mirror note in
//!   [`crate::tuple_compare_lower`].
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds exec graphs later.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod semantic_equivalence_tests;

use crate::fir_builder::{
    alloc_local_var_expr, decompose_binding, functored_specs, reachable_local_callables,
};
use crate::package_assigners::PackageAssigners;
use crate::reachability::{
    collect_reachable_from_entry, collect_reachable_package_closure, collect_reachable_with_seeds,
};
use crate::tuple_destructuring::{normalize_tuple_copy_assignment, normalize_tuple_destructuring};
use crate::walk_utils::{UseClass, classify_block_use, collect_expr_ids_in_local_callables};
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Block, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath, ItemKind,
    LocalItemId, LocalVarId, Package, PackageId, PackageLookup, PackageStore, Pat, PatId, PatKind,
    Res, SpecDecl, SpecImpl, Stmt, StmtId, StmtKind, StoreItemId,
};
use qsc_fir::ty::Ty;
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::EMPTY_EXEC_RANGE;

/// Runs the tuple-decompose pass across the entry-reachable package closure.
///
/// For each local binding whose type resolves to a multi-field tuple,
/// the pass decomposes the binding into one scalar local per
/// element when **every** use of the binding falls into one of the
/// shapes the in-pass classifier accepts (see
/// `crate::walk_utils::for_each_use_event`):
///
/// - `Field(Var(t), Path(..))` — a field projection out of the binding,
///   rewritten by [`rewrite_field_accesses`] into a direct
///   `Var(t_i)` reference (or `Field(Var(t_i), Path(..))` for nested
///   paths).
/// - `Assign(Var(t), Tuple([e0, e1, ...]))` — a whole-tuple
///   reassignment whose RHS is a tuple literal, split by
///   [`rewrite_assign_tuples`] into per-element `Assign(Var(t_i), ei)`
///   statements.
///
/// Bindings with any other use shape — passing the binding as a whole
/// argument, returning it, capturing it in a closure, assigning it from
/// a non-literal RHS — are rejected by `all_uses_are_field_access` and
/// left intact.
///
/// # Requires
/// - Package with `package_id` has an entry expression.
/// - [`crate::tuple_compare_lower`] has already run, so no `BinOp(Eq |
///   Neq)` on tuple-typed operands remains in reachable code.
///
/// # Returns
///
/// `true` if at least one decomposition round was applied; `false` when
/// no candidates existed.
///
/// # Panics
///
/// Panics if the package has no entry expression. The reachability scans
/// in this pass go through [`collect_reachable_from_entry`], which asserts
/// `package.entry.is_some()`.
pub fn tuple_decompose(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) -> bool {
    let reachable = collect_reachable_from_entry(store, package_id);
    let pkg_ids: Vec<PackageId> = collect_reachable_package_closure(package_id, &reachable)
        .into_iter()
        .collect();
    let mut changed = false;
    for pkg in pkg_ids {
        let assigner = assigners.get_mut(store, pkg);
        changed |= tuple_decompose_in_package_to_fixpoint(store, pkg, assigner, &reachable);
    }
    changed
}

/// Runs the tuple-decompose fixpoint over the reachable callables that live in
/// `package_id`, minting fresh locals through that package's `assigner`.
///
/// `reachable` is the entry-rooted closure (spanning every package); the
/// per-package helpers filter it to `package_id`. tuple-decompose is body-only,
/// so the closure is stable across rounds and is computed once by the caller.
fn tuple_decompose_in_package_to_fixpoint(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
    reachable: &FxHashSet<StoreItemId>,
) -> bool {
    let mut changed = false;
    loop {
        changed |= normalize_tuple_destructuring(store, package_id, assigner, reachable);
        changed |= normalize_tuple_copy_assignment(store, package_id, assigner, reachable);

        let package = store.get(package_id);

        // Collect candidates across all reachable callables in this package.
        let mut all_candidates: Vec<TupleDecomposeCandidate> = Vec::new();

        for (item_id, decl) in reachable_local_callables(package, package_id, reachable) {
            collect_candidates_in_callable(store, package_id, item_id, decl, &mut all_candidates);
        }

        if all_candidates.is_empty() {
            break;
        }
        changed = true;

        // Apply decomposition.
        let package = store.get_mut(package_id);
        for candidate in &all_candidates {
            decompose_candidate(package, assigner, candidate);
        }
    }
    changed
}

/// Seed-rooted variant of [`tuple_decompose`] for the signature-preserving
/// sub-pipeline.
///
/// Decomposes tuple-typed locals in entry-reachable code and in the `seeds`
/// roots (pinned target bodies and their transitive callees), each in its
/// owning package via [`PackageAssigners`]. Entry-reachable code already
/// processed by this pass exposes no further candidates, so the re-walk is a
/// no-op there.
pub fn tuple_decompose_with_seeds(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    seeds: &[StoreItemId],
) -> bool {
    let reachable = collect_reachable_with_seeds(store, package_id, seeds);
    let pkg_ids: Vec<PackageId> = collect_reachable_package_closure(package_id, &reachable)
        .into_iter()
        .collect();
    let mut changed = false;
    for pkg in pkg_ids {
        let assigner = assigners.get_mut(store, pkg);
        changed |= tuple_decompose_in_package_to_fixpoint(store, pkg, assigner, &reachable);
    }
    changed
}

/// A candidate for tuple-decompose decomposition.
struct TupleDecomposeCandidate {
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

/// Scans a callable's body for tuple-decompose candidates.
fn collect_candidates_in_callable(
    store: &PackageStore,
    package_id: PackageId,
    owner_item: LocalItemId,
    decl: &CallableDecl,
    candidates: &mut Vec<TupleDecomposeCandidate>,
) {
    match &decl.implementation {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            collect_candidates_in_spec_impl(store, package_id, owner_item, spec_impl, candidates);
        }
    }
}

/// Recurses into every specialization of a `SpecImpl` to collect tuple-decompose
/// candidates.
fn collect_candidates_in_spec_impl(
    store: &PackageStore,
    package_id: PackageId,
    owner_item: LocalItemId,
    spec_impl: &SpecImpl,
    candidates: &mut Vec<TupleDecomposeCandidate>,
) {
    collect_candidates_in_spec(store, package_id, owner_item, &spec_impl.body, candidates);
    for spec in functored_specs(spec_impl) {
        collect_candidates_in_spec(store, package_id, owner_item, spec, candidates);
    }
}

/// Collects tuple-decompose candidates within a single `SpecDecl` body by walking
/// tuple-typed bindings and checking every use for field-only or
/// decomposable-tuple-assignment eligibility.
fn collect_candidates_in_spec(
    store: &PackageStore,
    package_id: PackageId,
    owner_item: LocalItemId,
    spec: &SpecDecl,
    candidates: &mut Vec<TupleDecomposeCandidate>,
) {
    let package = store.get(package_id);
    // Collect all local bindings with a multi-field tuple type.
    let bindings = find_tuple_bindings_in_block(package, spec.block);

    for binding in bindings {
        // Verify all uses are field-only.
        if all_uses_are_field_access(package, spec.block, binding.local_id) {
            candidates.push(TupleDecomposeCandidate {
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

/// Collects tuple-typed local bindings (`StmtKind::Local` patterns whose
/// `PatKind::Bind` carries a non-empty `Ty::Tuple`) reachable from a block.
struct TupleBindingCollector<'a> {
    package: &'a Package,
    bindings: Vec<TupleBinding>,
}

impl<'a> Visitor<'a> for TupleBindingCollector<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.package.get_block(id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.package.get_expr(id)
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.package.get_pat(id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.package.get_stmt(id)
    }

    fn visit_pat(&mut self, id: PatId) {
        let pat = self.package.get_pat(id);
        if let PatKind::Bind(ident) = &pat.kind
            && let Ty::Tuple(elems) = &pat.ty
            && !elems.is_empty()
        {
            self.bindings.push(TupleBinding {
                local_id: ident.id,
                pat_id: id,
                elem_types: elems.clone(),
                name: ident.name.clone(),
            });
        }
        visit::walk_pat(self, id);
    }
}

/// Finds all tuple-typed local bindings reachable from a block.
fn find_tuple_bindings_in_block(package: &Package, block_id: BlockId) -> Vec<TupleBinding> {
    let mut collector = TupleBindingCollector {
        package,
        bindings: Vec::new(),
    };
    collector.visit_block(block_id);
    collector.bindings
}

/// Returns `true` if every use of `local_id` in the block is a field access
/// (`ExprKind::Field(Var(Local(id)), Path(_))`) or a field assignment
/// (`ExprKind::AssignField(Var(Local(id)), _, _)`).
///
/// Returns `false` if `local_id` is used in any other context: passed as an
/// argument, returned, captured by closure, assigned whole, etc.
fn all_uses_are_field_access(package: &Package, block_id: BlockId, local_id: LocalVarId) -> bool {
    classify_block_use(package, block_id, local_id) != UseClass::GeneralUse
}

/// Decomposes a single tuple-decompose candidate in-place.
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
fn decompose_candidate(
    package: &mut Package,
    assigner: &mut Assigner,
    candidate: &TupleDecomposeCandidate,
) {
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
    // assignments. This must run after field access rewriting so that any
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

/// Rewrites all `ExprKind::Field(Var(Local(old)), Path([i, ...]))` uses across
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

/// Rewrites a single expression to replace references to an tuple-decompose-decomposed
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
    if let ExprKind::Field(inner_id, Field::Path(ref path)) = expr.kind {
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
}

/// Rewrites every reference to `old_expr_id` in the owner callable to point at
/// `new_expr_id`.
///
/// Before, entry, statements, and parent expressions still point at the
/// aggregate expression that tuple-decompose wants to replace. After, every such edge
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

/// Redirects a statement's direct expression edge from `old_expr_id` to
/// `new_expr_id`. Companion to [`replace_expr_in_expr`] for
/// [`replace_expr_references`].
fn replace_expr_in_stmt(stmt: &mut Stmt, old_expr_id: ExprId, new_expr_id: ExprId) {
    match &mut stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            replace_expr_id(expr_id, old_expr_id, new_expr_id);
        }
        StmtKind::Item(_) => {}
    }
}

/// Redirects an expression's direct child edges from `old_expr_id` to
/// `new_expr_id`. Rewrites only the immediate children (one structural level);
/// [`replace_expr_references`] visits every expression in the callable, so deep
/// recursion here would be redundant.
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
        | ExprKind::Index(a, b) => {
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
        // No direct child `ExprId` edges to patch. This function only rewrites
        // the `ExprId`s a node stores in its own `ExprKind`; it does not recurse
        // (the caller `replace_expr_references` visits every expr and every
        // block's statements of the callable independently). `Block` stores a
        // `BlockId`, and a `Block`'s child expressions live in its `Stmt` nodes,
        // which are patched by the sibling `replace_expr_in_stmt`. `Closure`
        // stores a `LocalItemId`, and `Hole`/`Lit`/`Var` are leaves.
        ExprKind::Block(_)
        | ExprKind::Closure(_, _)
        | ExprKind::Hole
        | ExprKind::Lit(_)
        | ExprKind::Var(_, _) => {}
        // Dead child-bearing variants. These are kept as an explicit arm — rather
        // than folded into the leaf arm above — so that if a future change makes
        // any of them reachable here, this becomes a visible prompt to patch
        // their child edges instead of silently dropping them.
        //
        // `Struct` is dead PostTupleDecompose: `check_expr_udt_erase_invariants`
        // panics on `Struct`, enforced PostTupleDecompose.
        //
        // `AssignField`/`UpdateField` are dead PostUdtErase: `udt_erase` lowers
        // every `Field::Path` form to `Assign(record, Tuple)`, and these nodes
        // never carry any other field kind in reachable code (`Prim` is
        // read-only, `Err` is error-recovery). `check_expr_udt_erase_invariants`
        // panics on `Field::Path` in either, enforced PostUdtErase.
        #[allow(clippy::match_same_arms)]
        // Kept separate from the leaf arm on purpose; see comment.
        ExprKind::Struct(_, _, _)
        | ExprKind::AssignField(_, _, _)
        | ExprKind::UpdateField(_, _, _) => {}
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

/// Collects block IDs reachable from a callable's implementation.
///
/// For a `Spec` implementation this includes each specialization's root
/// block plus every block nested within expressions. `Intrinsic` and
/// `SimulatableIntrinsic` implementations contribute no spec-level root
/// block; any blocks nested within a `SimulatableIntrinsic` body are still
/// picked up by the expression walk.
pub(crate) fn collect_all_block_ids_in_callable(
    package: &Package,
    item_id: LocalItemId,
) -> Vec<BlockId> {
    let Some(item) = package.items.get(item_id) else {
        return Vec::new();
    };
    let ItemKind::Callable(decl) = &item.kind else {
        return Vec::new();
    };
    let mut block_ids = Vec::new();
    // Include spec-level blocks.
    match &decl.implementation {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            block_ids.push(spec_impl.body.block);
            for spec in functored_specs(spec_impl) {
                block_ids.push(spec.block);
            }
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
            assign.ty = Ty::UNIT;
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
                ty: Ty::UNIT,
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
        let block_id = stmt_block_map
            .get(&stmt_id)
            .expect("stmt_id is always valid");
        let block = package
            .blocks
            .get_mut(*block_id)
            .expect("block should exist");
        let pos = block
            .stmts
            .iter()
            .position(|&s| s == stmt_id)
            .expect("stmt_id should be in block");
        for (offset, new_id) in new_stmt_ids.into_iter().enumerate() {
            block.stmts.insert(pos + 1 + offset, new_id);
        }
    }
}
