// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Argument promotion pass.
//!
//! Decomposes tuple-typed parameters of callable declarations into individual
//! scalar parameters, eliminating intermediate tuple allocations at call sites
//! and field-access overhead in callable bodies.
//!
//! Establishes [`crate::invariants::InvariantLevel::PostArgPromote`]:
//! synthesized callable input tuple patterns agree with their
//! input types.
//!
//! For each entry-reachable callable, the pass:
//! - Identifies parameters bound via `PatKind::Bind(p)` with `Ty::Tuple(...)`
//!   that have at least one field access and no use that blocks promotion.
//!   Standalone whole-value reads of the parameter do not block promotion;
//!   they are reconstructed from the scalar leaves during the body rewrite.
//! - Verifies the callable is not used as a first-class value, referenced
//!   as a closure target, or otherwise left indirectly dispatched.
//!   First-class and closure-target detection together cover the
//!   partial-application cases.
//! - Decomposes the binding in `CallableDecl.input` and rewrites field
//!   accesses in all specialization bodies.
//! - Rewrites all call sites to pass individual fields instead of the whole
//!   tuple/struct argument.
//!
//! Callables that appear as first-class values (a `Var(Res::Item(_))` with
//! `Ty::Arrow` outside the direct callee position of a `Call`) or as closure
//! targets in reachable code are disqualified, because their parameter layout
//! must remain stable for indirect invocation.
//!
//! The pass iterates to a fixed point, peeling one level of tuple nesting
//! per iteration, like tuple-decompose.
//!
//! # Pipeline position
//!
//! This pass runs after tuple-decompose and before unreachable-node GC. At this point,
//! tuple-heavy parameter shapes are already simplified by earlier passes, and
//! argument promotion can rewrite callable signatures and direct call sites
//! without fighting major later structural rewrites.
//!
//! # Architecture
//!
//! One fixed-point iteration performs:
//!
//! 1. **Reachability scan** ([`collect_reachable_from_entry`]):
//!    Limit work to entry-reachable callables.
//! 2. **Eligibility analysis** ([`check_candidates`]):
//!    Find tuple-typed `PatKind::Bind` inputs that have at least one field
//!    access and no promotion-blocking use across every specialization.
//! 3. **Safety filters** ([`collect_first_class_callables`],
//!    [`collect_closure_targets`]):
//!    Exclude callables used as first-class values or closure targets.
//! 4. **Signature/body rewrite** ([`promote_callable`]):
//!    Replace promoted bind patterns with tuple patterns over fresh scalar
//!    locals and rewrite body uses to read those locals.
//! 5. **Call-site rewrite** ([`rewrite_call_sites`]):
//!    Rewrite direct call arguments to match promoted input shapes.
//!
//! After the fixed point converges, [`normalize_call_arg_types`] performs a
//! package-wide call-shape normalization pass to ensure argument expression
//! types exactly match callable input types (for example,
//! `T` to `(T)` wrapping for single-element tuple inputs).
//!
//! # Input patterns
//!
//! - `operation Foo(p : (Int, Qubit)) { use(p::0); apply(p::1); }` — a
//!   tuple-typed parameter whose every use is a field projection.
//!
//! # Rewrites
//!
//! ```text
//! // Before
//! operation Foo(p : (Int, Qubit)) { use(p::0); apply(p::1); }
//! Foo((42, q));
//!
//! // After
//! operation Foo(p_0 : Int, p_1 : Qubit) { use(p_0); apply(p_1); }
//! Foo(42, q);
//! ```
//!
//! Nested fixed-point example:
//!
//! ```text
//! // Before
//! operation Foo(p : ((Int, Bool), Qubit)) : Unit {
//!     let x = p::0::0;
//!     let b = p::0::1;
//!     let q = p::1;
//!     if b { X(q); }
//! }
//!
//! // After first promotion pass
//! operation Foo(p_0 : (Int, Bool), p_1 : Qubit) : Unit {
//!     let x = p_0::0;
//!     let b = p_0::1;
//!     let q = p_1;
//!     if b { X(q); }
//! }
//!
//! // After fixed-point convergence
//! operation Foo(p_0_0 : Int, p_0_1 : Bool, p_1 : Qubit) : Unit {
//!     let x = p_0_0;
//!     let b = p_0_1;
//!     let q = p_1;
//!     if b { X(q); }
//! }
//! ```
//!
//! Single-element tuple call-shape normalization example:
//!
//! ```text
//! // Callable input after prior rewrites/promotions
//! operation UseOne(p : (Qubit[])) : Unit { ... }
//!
//! // Call expression before normalization (type mismatch)
//! UseOne(qs);              // arg type: Qubit[]
//!
//! // Call expression after normalization
//! UseOne((qs,));           // arg type: (Qubit[])
//! ```
//!
//! # Notes
//!
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds correct exec graphs at the end
//!   of the pipeline.
//! - Functor-applied callees (`Adjoint`, `Controlled`) are handled directly:
//!   [`resolve_direct_item_callee`] unwraps `UnOp(Functor::Adj)` and
//!   `UnOp(Functor::Ctl)` wrappers (counting controlled depth) to find the
//!   underlying item callee, and [`rewrite_controlled_call_site`] rewrites
//!   the payload while preserving the surrounding control-tuple layers and
//!   their evaluation order.
//!
//! # References
//!
//! This pass is named after LLVM's `ArgumentPromotion` pass (also
//! `argpromotion`), which promotes pointer arguments to pass-by-value.
//! This Q# variant operates on tuple aggregates rather than pointers.
//!
//! <https://llvm.org/docs/Passes.html#argpromotion-promote-by-reference-arguments-to-scalars>

#[cfg(test)]
mod tests;

#[cfg(test)]
mod semantic_equivalence_tests;

use crate::EMPTY_EXEC_RANGE;
use crate::fir_builder::{decompose_binding_to_leaves, functored_specs, reachable_local_callables};
use crate::reachability::collect_reachable_from_entry;
use crate::tuple_decompose::collect_all_block_ids_in_callable;
use crate::walk_utils::{
    ParamUse, classify_uses_in_block, collect_expr_ids_in_entry_and_local_callables,
    collect_expr_ids_in_local_callables, for_each_expr, for_each_expr_in_callable_impl,
};
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Block, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath, Functor,
    Ident, ItemKind, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup,
    PackageStore, Pat, PatId, PatKind, Res, SpecDecl, SpecImpl, Stmt, StmtId, StmtKind,
    StoreItemId, UnOp,
};
use qsc_fir::ty::{Prim, Ty};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// Name given to the synthesized local that holds a materialized call
/// argument before it is projected into a promoted callable's scalar inputs
/// (see [`create_projection_temp_binding`]).
const ARG_PROMOTE_TMP_NAME: &str = "__arg_promote_tmp";

/// Runs argument promotion on the entry-reachable portion of a package.
///
/// # Before
/// ```text
/// operation Foo(p : (Int, Qubit)) : Unit { use(p::0); apply(p::1); }
/// Foo((42, q));
/// ```
///
/// # After
/// ```text
/// operation Foo(p_0 : Int, p_1 : Qubit) : Unit { use(p_0); apply(p_1); }
/// Foo(42, q);
/// ```
///
/// # Requires
/// - `package_id` exists in `store`.
/// - `assigner` is the pipeline-global assigner (ID continuity across passes).
/// - Package with `package_id` has an entry expression.
///
/// # Ensures
/// - Rewrites only entry-reachable callables.
/// - Leaves first-class and closure-target callables unchanged.
/// - Normalizes call argument shapes to match callable input types via
///   [`normalize_call_arg_types`].
///
/// # Mutations
/// - Rewrites callable input patterns and specialization bodies.
/// - Rewrites direct call expressions targeting promoted callables.
/// - Allocates fresh FIR nodes via `assigner` with `EMPTY_EXEC_RANGE`.
///
/// # Panics
///
/// Panics if the package has no entry expression. The reachability scans
/// in this pass go through [`collect_reachable_from_entry`], which asserts
/// `package.entry.is_some()`.
pub fn arg_promote(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> bool {
    let changed = promote_to_fixed_point(store, package_id, assigner);
    normalize_reachable_call_arg_types(store, package_id, assigner);
    changed
}

/// Iterates promotion rounds until no more candidates are found.
///
/// Each iteration peels one level of tuple nesting from eligible parameters,
/// rewrites their bodies and call sites, then recomputes reachability for
/// the next round.
///
/// # Returns
///
/// `true` if any promotion or normalize rewrite was applied; `false` otherwise.
pub(crate) fn promote_to_fixed_point(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> bool {
    let mut changed = false;
    loop {
        changed |= normalize_param_destructuring(store, package_id, assigner);
        let candidates = find_promotion_candidates(store, package_id);
        if candidates.is_empty() {
            break;
        }
        changed = true;
        apply_promotions(store, package_id, assigner, &candidates);
    }
    changed
}

/// A pending rewrite of a tuple-destructuring `let` into positional
/// field projections, collected under a shared borrow before mutation.
struct DestructureRewrite {
    /// The block containing the destructuring statement.
    block_id: BlockId,
    /// The destructuring `let` statement to rewrite in place.
    stmt_id: StmtId,
    /// Mutability of the original `let`.
    mutability: Mutability,
    /// The source local read as a whole value on the right-hand side.
    source_local: LocalVarId,
    /// The full tuple type of the source local.
    tuple_ty: Ty,
    /// The element sub-patterns of the destructuring tuple pattern.
    element_pat_ids: Vec<PatId>,
}

/// Normalizes tuple-destructuring `let`s into positional field projections
/// so the destructured source local's only uses become field accesses.
///
/// For a statement `let (a, b, ...) = src;` where `src` is read as a bare
/// whole-value `Var(Local)` — an input-bound parameter or any other local —
/// this rewrites it into `let a = src::0; let b = src::1; ...`, emitting one
/// projection per non-discard element. After this rewrite the source local's
/// only uses are field projections, which:
/// - lets [`find_promotion_candidates`] treat an input parameter as a
///   promotion candidate, and
/// - makes a non-parameter source local field-only, so the subsequent
///   tuple-decompose pass can scalar-replace it.
///
/// Only a bare `Var(Local)` right-hand side is rewritten. A `Call`, `Tuple`
/// literal, or any other RHS is left untouched, since tuple-decompose already
/// handles those once the destructured local is field-only.
///
/// Runs at the top of each [`promote_to_fixed_point`] iteration, scoped to
/// reachable local callable bodies.
///
/// # Returns
///
/// `true` if any destructuring rewrite was applied; `false` otherwise.
///
/// # Element handling
///
/// Each destructuring element is recursively descended to its `Bind` leaves,
/// threading a cumulative positional index path. Every leaf emits a single
/// direct multi-index projection — no intermediate whole-value temporary is
/// created for nested elements:
/// - `PatKind::Discard`: emits no binding, since the projection is a pure
///   read of an already-evaluated local.
/// - `PatKind::Bind`: emits `let <bind> = src::Path[i, ...];`, reusing the
///   existing sub-binding's `PatId` so its `LocalVarId` is preserved.
/// - `PatKind::Tuple` (nested): recurses into each child, so `(y, z)` at
///   index `i` flattens directly to `let y = src::Path[i, 0]; let z =
///   src::Path[i, 1];`.
///
/// # Mutations
/// - Rewrites the original destructuring statement in place to the first
///   emitted projection (or removes it from its block when every element is
///   a discard).
/// - Allocates fresh `Expr`/`Pat`/`Stmt` nodes (with `EMPTY_EXEC_RANGE`)
///   for the remaining projections and splices them into the block.
fn normalize_param_destructuring(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> bool {
    let reachable = collect_reachable_from_entry(store, package_id);
    let package = store.get(package_id);
    let local_item_ids: Vec<LocalItemId> =
        reachable_local_callables(package, package_id, &reachable)
            .map(|(id, _)| id)
            .collect();

    // Note: the entry callable is intentionally *not* excluded here. This pass
    // only rewrites body-local `let (a, b) = local;` destructures into positional
    // projections; it never reshapes `decl.input`. The entry input ABI is
    // protected solely by the exclusion in `find_promotion_candidates`, which is
    // the only place `decl.input` is flattened.
    let mut rewrites: Vec<DestructureRewrite> = Vec::new();
    for item_id in local_item_ids {
        let item = package.get_item(item_id);
        let ItemKind::Callable(_) = &item.kind else {
            continue;
        };

        for block_id in collect_all_block_ids_in_callable(package, item_id) {
            let block = package.get_block(block_id);
            for &stmt_id in &block.stmts {
                let stmt = package.get_stmt(stmt_id);
                let StmtKind::Local(mutability, pat_id, rhs_id) = &stmt.kind else {
                    continue;
                };
                let pat = package.get_pat(*pat_id);
                let PatKind::Tuple(element_pat_ids) = &pat.kind else {
                    continue;
                };
                let rhs = package.get_expr(*rhs_id);
                // Only normalize a bare whole-value `Var(Local)` RHS. Any other
                // RHS (call, tuple literal, ...) is handled by tuple-decompose directly.
                let ExprKind::Var(Res::Local(source_local), _) = &rhs.kind else {
                    continue;
                };
                // Only normalize when the RHS tuple arity matches the
                // destructuring pattern arity; per-leaf element types are
                // read directly from each leaf sub-pattern's `Pat.ty`.
                match &rhs.ty {
                    Ty::Tuple(elems) if elems.len() == element_pat_ids.len() => {}
                    _ => continue,
                }
                rewrites.push(DestructureRewrite {
                    block_id,
                    stmt_id,
                    mutability: *mutability,
                    source_local: *source_local,
                    tuple_ty: rhs.ty.clone(),
                    element_pat_ids: element_pat_ids.clone(),
                });
            }
        }
    }

    if rewrites.is_empty() {
        return false;
    }

    let package = store.get_mut(package_id);
    for rewrite in rewrites {
        apply_destructure_rewrite(package, assigner, &rewrite);
    }
    true
}

/// Rewrites a single parameter-destructuring statement into positional field
/// projections (see [`normalize_param_destructuring`]).
fn apply_destructure_rewrite(
    package: &mut Package,
    assigner: &mut Assigner,
    rewrite: &DestructureRewrite,
) {
    // Recursively descend each element pattern to its `Bind` leaves under a
    // shared borrow, collecting `(leaf_pat_id, index_path, leaf_ty)`. This
    // avoids holding the shared borrow across the mutating projection
    // helpers below.
    let mut leaves: Vec<(PatId, Vec<usize>, Ty)> = Vec::new();
    {
        let mut indices: Vec<usize> = Vec::new();
        for (i, &elem_pat_id) in rewrite.element_pat_ids.iter().enumerate() {
            indices.push(i);
            collect_leaf_projections(package, elem_pat_id, &mut indices, &mut leaves);
            indices.pop();
        }
    }

    // Build one `(mutability, pat, rhs)` projection descriptor per leaf bind.
    let mut descriptors: Vec<(Mutability, PatId, ExprId)> = Vec::with_capacity(leaves.len());
    for (leaf_pat_id, indices, leaf_ty) in leaves {
        let proj = create_local_projection_path(
            package,
            assigner,
            rewrite.source_local,
            &rewrite.tuple_ty,
            &leaf_ty,
            &indices,
        );
        descriptors.push((rewrite.mutability, leaf_pat_id, proj));
    }

    if descriptors.is_empty() {
        // Every element is a discard: drop the now-dead destructuring use of
        // the source local so it no longer blocks promotion or tuple-decompose.
        let block = package
            .blocks
            .get_mut(rewrite.block_id)
            .expect("block should exist");
        if let Some(pos) = block.stmts.iter().position(|&s| s == rewrite.stmt_id) {
            block.stmts.remove(pos);
        }
        return;
    }

    // Reuse the original statement for the first projection.
    {
        let (mutability, pat_id, rhs_id) = descriptors[0];
        let stmt = package
            .stmts
            .get_mut(rewrite.stmt_id)
            .expect("stmt should exist");
        stmt.kind = StmtKind::Local(mutability, pat_id, rhs_id);
    }

    // Allocate fresh statements for the remaining projections.
    let mut new_stmt_ids: Vec<StmtId> = Vec::with_capacity(descriptors.len() - 1);
    for &(mutability, pat_id, rhs_id) in &descriptors[1..] {
        let stmt_id = assigner.next_stmt();
        package.stmts.insert(
            stmt_id,
            Stmt {
                id: stmt_id,
                span: Span::default(),
                kind: StmtKind::Local(mutability, pat_id, rhs_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        new_stmt_ids.push(stmt_id);
    }

    // Splice the new statements into the block after the original.
    let block = package
        .blocks
        .get_mut(rewrite.block_id)
        .expect("block should exist");
    if let Some(pos) = block.stmts.iter().position(|&s| s == rewrite.stmt_id) {
        for (offset, new_id) in new_stmt_ids.into_iter().enumerate() {
            block.stmts.insert(pos + 1 + offset, new_id);
        }
    }
}

/// Recursively descends a destructuring element pattern to its `Bind`
/// leaves, collecting `(leaf_pat_id, index_path, leaf_ty)` for each.
///
/// `indices` carries the cumulative positional path from the source tuple to
/// the current pattern; it is pushed/popped around each child so callers see
/// it unchanged on return. `Discard` leaves contribute nothing. Each leaf's
/// type is read directly from its `Pat.ty` (set by frontend lowering and
/// preserved through earlier passes).
fn collect_leaf_projections(
    package: &Package,
    pat_id: PatId,
    indices: &mut Vec<usize>,
    leaves: &mut Vec<(PatId, Vec<usize>, Ty)>,
) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Discard => {}
        PatKind::Bind(_) => {
            leaves.push((pat_id, indices.clone(), pat.ty.clone()));
        }
        PatKind::Tuple(sub_pats) => {
            for (i, &sub_pat_id) in sub_pats.iter().enumerate() {
                indices.push(i);
                collect_leaf_projections(package, sub_pat_id, indices, leaves);
                indices.pop();
            }
        }
    }
}

/// Allocates a `src::Path[indices...]` field projection expression over a
/// fresh `Var(Res::Local(src))` base carrying the full tuple type.
///
/// The multi-index `Field::Path` projects directly to a (possibly nested)
/// leaf in a single expression; downstream tuple-decompose / arg-promote field rewrites
/// decompose arbitrary-depth paths via their `remaining`-slice recursion.
///
/// # Mutations
/// - Inserts a fresh base `Var` `Expr` and a `Field` `Expr` (with
///   `EMPTY_EXEC_RANGE`) through `assigner`.
fn create_local_projection_path(
    package: &mut Package,
    assigner: &mut Assigner,
    source_local: LocalVarId,
    tuple_ty: &Ty,
    leaf_ty: &Ty,
    indices: &[usize],
) -> ExprId {
    let base_id = create_local_var_expr(package, assigner, source_local, tuple_ty);
    let field_expr_id = assigner.next_expr();
    package.exprs.insert(
        field_expr_id,
        Expr {
            id: field_expr_id,
            span: Span::default(),
            ty: leaf_ty.clone(),
            kind: ExprKind::Field(
                base_id,
                Field::Path(FieldPath {
                    indices: indices.to_vec(),
                }),
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    field_expr_id
}

/// Finds all eligible promotion candidates in the current reachable set,
/// excluding callables used as first-class values or closure targets.
fn find_promotion_candidates(
    store: &PackageStore,
    package_id: PackageId,
) -> Vec<ArgPromoCandidate> {
    let reachable = collect_reachable_from_entry(store, package_id);
    let package = store.get(package_id);

    let entry_item = resolve_entry_callable_item(package, package_id);
    let first_class = collect_first_class_callables(package, package_id, &reachable);
    let closure_targets = collect_closure_targets(package, package_id, &reachable);

    let mut candidates: Vec<ArgPromoCandidate> = Vec::new();
    for (item_id, decl) in reachable_local_callables(package, package_id, &reachable) {
        // The entry-point callable's input is the program's externally-visible
        // ABI and must never be flattened, regardless of its input shape.
        // This is a forward looking check as all inputs are currently `Unit`
        if Some(item_id) == entry_item {
            continue;
        }
        if first_class.contains(&item_id) || closure_targets.contains(&item_id) {
            continue;
        }
        // Skip intrinsics entirely: their signatures must stay tuple-shaped and
        // simulatable bodies are never analyzed or rewritten. Any invalid types will
        // fail later in code generation
        if matches!(
            decl.implementation,
            CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_)
        ) {
            continue;
        }
        candidates.extend(check_candidates(package, package_id, item_id, decl));
    }
    candidates
}

/// Applies a batch of promotion candidates: decomposes parameters, rewrites
/// bodies, and rewrites call sites scoped to reachable expressions.
fn apply_promotions(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
    candidates: &[ArgPromoCandidate],
) {
    let reachable = collect_reachable_from_entry(store, package_id);
    let package = store.get(package_id);
    let local_item_ids: Vec<_> = reachable_local_callables(package, package_id, &reachable)
        .map(|(id, _)| id)
        .collect();
    let reachable_expr_ids =
        collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids);

    let package = store.get_mut(package_id);

    // Group candidates by their declaring callable so each callable's entire
    // input is flattened exactly once, dissolving all inter-parameter
    // grouping, and preserve first-seen order for deterministic ID allocation.
    let mut order: Vec<LocalItemId> = Vec::new();
    let mut groups: FxHashMap<LocalItemId, Vec<&ArgPromoCandidate>> = FxHashMap::default();
    for candidate in candidates {
        if !groups.contains_key(&candidate.item_id) {
            order.push(candidate.item_id);
        }
        groups.entry(candidate.item_id).or_default().push(candidate);
    }

    let mut promotions: Vec<PromotionResult> = Vec::new();
    for item_id in order {
        let cands = &groups[&item_id];
        if let Some(result) = promote_callable(package, assigner, item_id, cands) {
            promotions.push(result);
        }
    }

    if !promotions.is_empty() {
        rewrite_call_sites(
            package,
            package_id,
            assigner,
            promotions,
            &reachable_expr_ids,
        );
    }
}

/// Normalizes call-argument types across all reachable call sites after
/// promotion has converged.
pub(crate) fn normalize_reachable_call_arg_types(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) {
    let reachable = collect_reachable_from_entry(store, package_id);
    let package = store.get(package_id);
    let local_item_ids: Vec<_> = reachable_local_callables(package, package_id, &reachable)
        .map(|(id, _)| id)
        .collect();
    let reachable_expr_ids =
        collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids);
    let package = store.get_mut(package_id);
    normalize_call_arg_types(package, package_id, assigner, &reachable_expr_ids);
}

/// A candidate for argument promotion.
struct ArgPromoCandidate {
    /// The `LocalItemId` of the callable.
    item_id: LocalItemId,
    /// The `LocalVarId` bound by the parameter.
    local_id: LocalVarId,
    /// Expression ids of the parameter's standalone whole-value reads. These
    /// sites are reconstructed from the parameter's scalar leaves during the
    /// body rewrite so they keep observing the original tuple value.
    whole_value_reads: Vec<ExprId>,
}

/// Result of promoting a callable — tracks the callable and the flat scalar
/// leaves of its fully-decomposed input so that call sites can be
/// rewritten to pass the flattened arguments.
#[derive(Clone)]
struct PromotionResult {
    /// The callable's `LocalItemId`.
    item_id: LocalItemId,
    /// One entry per scalar leaf of the callable's flattened input: the
    /// positional path of the leaf in the original (nested) input type and
    /// the leaf's type. The path projects the leaf from the original
    /// argument value at each call site. Promotable parameters contribute one
    /// entry per scalar leaf; kept (non-promotable) parameters contribute a
    /// single entry projecting their whole value.
    leaves: Vec<(Vec<usize>, Ty)>,
}

/// Collects the promotable tuple-typed parameter bindings of a callable.
/// Recurses into `PatKind::Tuple` sub-patterns to find inner bindings that
/// became eligible after a previous pass peeled an outer tuple level.
fn check_candidates(
    package: &Package,
    _package_id: PackageId,
    item_id: LocalItemId,
    decl: &CallableDecl,
) -> Vec<ArgPromoCandidate> {
    let mut candidates = Vec::new();
    find_param_binds_in_pat(package, item_id, decl, decl.input, &mut candidates);
    candidates
}

/// Recursively walks a callable's input pattern to find promotable
/// tuple-typed `PatKind::Bind` nodes (see [`param_is_promotable`]).
fn find_param_binds_in_pat(
    package: &Package,
    item_id: LocalItemId,
    decl: &CallableDecl,
    pat_id: PatId,
    candidates: &mut Vec<ArgPromoCandidate>,
) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            let is_tuple = matches!(&pat.ty, Ty::Tuple(elems) if !elems.is_empty());
            if is_tuple {
                let local_id = ident.id;
                let uses = classify_param_uses(package, decl, local_id);
                if let Some(whole_value_reads) = param_is_promotable(&uses) {
                    candidates.push(ArgPromoCandidate {
                        item_id,
                        local_id,
                        whole_value_reads,
                    });
                }
            }
        }
        PatKind::Tuple(sub_pats) => {
            for &sub_pat_id in sub_pats {
                find_param_binds_in_pat(package, item_id, decl, sub_pat_id, candidates);
            }
        }
        PatKind::Discard => {}
    }
}

/// Classifies every use of `local_id` across all specialization bodies of the
/// callable, returning the flat list of [`ParamUse`] classifications.
///
/// Only `CallableImpl::Spec` callables ever reach this function: the intrinsic
/// gate in `find_promotion_candidates` skips both `Intrinsic` and
/// `SimulatableIntrinsic` callables before any candidate is constructed, so the
/// non-`Spec` arms are unreachable.
fn classify_param_uses(
    package: &Package,
    decl: &CallableDecl,
    local_id: LocalVarId,
) -> Vec<ParamUse> {
    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => classify_uses_in_spec_impl(package, spec_impl, local_id),
        // Dead arm: gated by the intrinsic skip in `find_promotion_candidates`
        CallableImpl::Intrinsic => unreachable!(
            "intrinsic callables are skipped by the intrinsic gate in \
             find_promotion_candidates before any candidate reaches \
             classify_param_uses"
        ),
        // Dead arm: same intrinsic gate as the `Intrinsic` arm above.
        CallableImpl::SimulatableIntrinsic(_) => unreachable!(
            "simulatable-intrinsic callables are skipped by the intrinsic gate in \
             find_promotion_candidates before any candidate reaches \
             classify_param_uses"
        ),
    }
}

/// Classifies every use of `local_id` across the body and all functored
/// specializations (adjoint, controlled, controlled-adjoint).
fn classify_uses_in_spec_impl(
    package: &Package,
    spec_impl: &SpecImpl,
    local_id: LocalVarId,
) -> Vec<ParamUse> {
    let mut uses = Vec::new();
    classify_uses_in_spec(package, &spec_impl.body, local_id, &mut uses);
    for spec in functored_specs(spec_impl) {
        classify_uses_in_spec(package, spec, local_id, &mut uses);
    }
    uses
}

/// Appends the classified uses of `local_id` in a single `SpecDecl` body to
/// `out` (per the classifier in [`classify_uses_in_block`]).
fn classify_uses_in_spec(
    package: &Package,
    spec: &SpecDecl,
    local_id: LocalVarId,
    out: &mut Vec<ParamUse>,
) {
    classify_uses_in_block(package, spec.block, local_id, out);
}

/// Decides whether a parameter is promotable from its classified uses and, when
/// it is, returns the expression ids of its standalone whole-value reads.
///
/// Promotion is blocked when any use hard-blocks it. Otherwise the parameter is
/// promotable when it has at least one field-access or decomposable use, which
/// skips pure pass-through parameters (zero field uses) that gain nothing from
/// flattening. The returned whole-value read sites are reconstructed during the
/// body rewrite so they keep observing the original tuple value.
fn param_is_promotable(uses: &[ParamUse]) -> Option<Vec<ExprId>> {
    let mut field = 0_usize;
    let mut whole_value_reads = Vec::new();
    for use_kind in uses {
        match use_kind {
            ParamUse::HardBlock => return None,
            ParamUse::FieldAccess | ParamUse::Decomposable => field += 1,
            ParamUse::WholeValueRead(expr_id) => whole_value_reads.push(*expr_id),
        }
    }
    (field >= 1).then_some(whole_value_reads)
}

/// Collects all `LocalItemId`s of callables in this package that appear as
/// `Var(Res::Item(id))` with an `Arrow` type (i.e., used as a first-class
/// value rather than as the callee of `Call`).
fn collect_first_class_callables(
    package: &Package,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
) -> FxHashSet<LocalItemId> {
    let mut first_class = FxHashSet::default();

    // Scan the entry expression.
    if let Some(entry_id) = package.entry {
        scan_first_class_in_expr(package, package_id, entry_id, &mut first_class);
    }

    // Scan every reachable callable body.
    for item_id in reachable {
        if item_id.package != package_id {
            continue;
        }
        let item = package.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            scan_first_class_in_callable(package, package_id, decl, &mut first_class);
        }
    }

    first_class
}

fn scan_first_class_in_callable(
    package: &Package,
    package_id: PackageId,
    decl: &CallableDecl,
    first_class: &mut FxHashSet<LocalItemId>,
) {
    match &decl.implementation {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            scan_first_class_in_block(package, package_id, spec_impl.body.block, first_class);
            for spec in functored_specs(spec_impl) {
                scan_first_class_in_block(package, package_id, spec.block, first_class);
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            scan_first_class_in_block(package, package_id, spec.block, first_class);
        }
    }
}

fn scan_first_class_in_block(
    package: &Package,
    package_id: PackageId,
    block_id: BlockId,
    first_class: &mut FxHashSet<LocalItemId>,
) {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                scan_first_class_in_expr(package, package_id, *e, first_class);
            }
            StmtKind::Local(_, _, expr) => {
                scan_first_class_in_expr(package, package_id, *expr, first_class);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Scans an expression tree. A `Var(Res::Item(id))` with `Ty::Arrow` is
/// considered first-class UNLESS it appears as the direct callee of a `Call`.
fn scan_first_class_in_expr(
    package: &Package,
    package_id: PackageId,
    expr_id: ExprId,
    first_class: &mut FxHashSet<LocalItemId>,
) {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Call(callee, args) => {
            // The callee position is a direct call — don't mark it, but still
            // recurse into the callee's sub-expressions, since a callee like
            // Field(...) is not a direct Var.
            let callee_expr = package.get_expr(*callee);
            match &callee_expr.kind {
                ExprKind::Var(Res::Item(_), _) => {
                    // Direct call — skip marking, but recurse into args.
                }
                ExprKind::UnOp(_, inner) => {
                    // Functor-applied call: check if inner is a direct item ref.
                    let inner_expr = package.get_expr(*inner);
                    if !matches!(inner_expr.kind, ExprKind::Var(Res::Item(_), _)) {
                        // Not a direct functor application — recurse into callee.
                        scan_first_class_in_expr(package, package_id, *callee, first_class);
                    }
                    // If inner IS a direct Var(Item), this is a direct functor-applied
                    // call (e.g., Adjoint Foo(args)) — don't mark as first-class.
                }
                _ => {
                    scan_first_class_in_expr(package, package_id, *callee, first_class);
                }
            }
            scan_first_class_in_expr(package, package_id, *args, first_class);
        }
        ExprKind::Var(Res::Item(item_id), _) if matches!(&expr.ty, Ty::Arrow(_)) => {
            if item_id.package == package_id {
                first_class.insert(item_id.item);
            }
        }
        // Recurse into all sub-expressions.
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                scan_first_class_in_expr(package, package_id, e, first_class);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            scan_first_class_in_expr(package, package_id, *a, first_class);
            scan_first_class_in_expr(package, package_id, *b, first_class);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            scan_first_class_in_expr(package, package_id, *a, first_class);
            scan_first_class_in_expr(package, package_id, *b, first_class);
            scan_first_class_in_expr(package, package_id, *c, first_class);
        }
        ExprKind::Block(block_id) => {
            scan_first_class_in_block(package, package_id, *block_id, first_class);
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            scan_first_class_in_expr(package, package_id, *e, first_class);
        }
        ExprKind::If(cond, body, otherwise) => {
            scan_first_class_in_expr(package, package_id, *cond, first_class);
            scan_first_class_in_expr(package, package_id, *body, first_class);
            if let Some(e) = otherwise {
                scan_first_class_in_expr(package, package_id, *e, first_class);
            }
        }
        ExprKind::Range(s, st, e) => {
            for x in [s, st, e].into_iter().flatten() {
                scan_first_class_in_expr(package, package_id, *x, first_class);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    scan_first_class_in_expr(package, package_id, *e, first_class);
                }
            }
        }
        ExprKind::While(cond, block_id) => {
            scan_first_class_in_expr(package, package_id, *cond, first_class);
            scan_first_class_in_block(package, package_id, *block_id, first_class);
        }
        ExprKind::Hole
        | ExprKind::Lit(_)
        | ExprKind::Var(_, _)
        | ExprKind::Closure(_, _)
        // `Struct` is dead PostUdtErase: `udt_erase` lowers `Struct` to `Tuple`,
        | ExprKind::Struct(_, _, _) => {}
    }
}

/// Collects all `LocalItemId`s that are targets of `Closure(_, local_item_id)`
/// in the entry-reachable portion of the current package.
fn collect_closure_targets(
    package: &Package,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
) -> FxHashSet<LocalItemId> {
    let mut targets = FxHashSet::default();

    if let Some(entry_id) = package.entry {
        for_each_expr(package, entry_id, &mut |_expr_id, expr| {
            if let ExprKind::Closure(_, local_item_id) = &expr.kind {
                targets.insert(*local_item_id);
            }
        });
    }

    for item_id in reachable {
        if item_id.package != package_id {
            continue;
        }

        let item = package.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_expr_id, expr| {
                if let ExprKind::Closure(_, local_item_id) = &expr.kind {
                    targets.insert(*local_item_id);
                }
            });
        }
    }

    targets
}

/// Flattens an entire callable input into one flat tuple of scalar leaves,
/// dissolving all inter-parameter grouping, then remaps every promotable
/// parameter's body field reads to its scalar leaves.
///
/// Every promotable parameter (those in `candidates`) is decomposed to its
/// scalar leaves; every other parameter (non-tuple, or a tuple read as a
/// whole value) is kept as a single leaf. The leaves of all parameters are
/// concatenated into one flat input tuple, so a multi-parameter callable such
/// as `Add(a : (Int, Int), b : (Int, Int))` flattens to
/// `Add(a_0 : Int, a_1 : Int, b_0 : Int, b_1 : Int)`, and a mixed callable
/// `UsePair(p : (Int, Int), q : Qubit)` flattens to
/// `UsePair(p_0 : Int, p_1 : Int, q : Qubit)` (keeping `q` as a singleton).
///
/// # Before
/// ```text
/// decl.input = Tuple([Bind(a : (Int, Int)), Bind(b : (Int, Int))])
/// body:  Field(Var(Local(a)), Path([0])); Field(Var(Local(b)), Path([1]))
/// ```
/// # After
/// ```text
/// decl.input = Tuple([Bind(a_0 : Int), Bind(a_1 : Int),
///                     Bind(b_0 : Int), Bind(b_1 : Int)])
/// body:  Var(Local(a_0)); Var(Local(b_1))
/// ```
///
/// # Mutations
/// - Rewrites `decl.input`'s `Pat` node (kind + `ty`) in place to the flat
///   tuple, and refreshes every specialization input `ty` to match.
/// - Allocates new `LocalVarId`/`PatId` leaf nodes through `assigner`.
/// - Remaps body expressions of every promoted parameter to read the
///   decomposed leaf locals.
///
/// # Returns
///
/// A `PromotionResult` whose `leaves` lists every flat input leaf with its
/// absolute positional path in the original (nested) input type, used to
/// rewrite call sites. Returns `None` only if the item is not a callable.
fn promote_callable(
    package: &mut Package,
    assigner: &mut Assigner,
    item_id: LocalItemId,
    candidates: &[&ArgPromoCandidate],
) -> Option<PromotionResult> {
    let input_pat_id = {
        let item = package.get_item(item_id);
        let ItemKind::Callable(decl) = &item.kind else {
            return None;
        };
        decl.input
    };

    // The set of parameter locals to expand to scalar leaves. Every other
    // parameter is kept as a single leaf.
    let promotable: FxHashSet<LocalVarId> = candidates.iter().map(|c| c.local_id).collect();

    // Recursively rebuild the input pattern into a flat list of leaf binds,
    // recording each leaf's absolute path/type and, per promoted parameter,
    // the leaf-relative map used to remap its body field reads.
    let mut leaf_pat_ids: Vec<PatId> = Vec::new();
    let mut leaf_entries: Vec<(Vec<usize>, Ty)> = Vec::new();
    let mut remaps: Vec<(LocalVarId, Ty, FxHashMap<Vec<usize>, (LocalVarId, Ty)>)> = Vec::new();
    let mut index_path: Vec<usize> = Vec::new();
    rebuild_input_leaves(
        package,
        assigner,
        input_pat_id,
        &mut index_path,
        &promotable,
        &mut leaf_pat_ids,
        &mut leaf_entries,
        &mut remaps,
    );

    // Set the callable input pattern to the flat tuple of leaf binds, in
    // lockstep with its flat tuple type. Controlled/adjoint specs share this
    // payload pattern node, so the in-place mutation is visible to them.
    let leaf_tys: Vec<Ty> = leaf_entries.iter().map(|(_, ty)| ty.clone()).collect();
    let pat = package
        .pats
        .get_mut(input_pat_id)
        .expect("input pat should exist");
    pat.kind = PatKind::Tuple(leaf_pat_ids);
    pat.ty = Ty::Tuple(leaf_tys);

    // Refresh every specialization input pattern's tuple type so the wrapper
    // control layers (e.g. `(ctls, payload)`) pick up the flattened payload.
    refresh_spec_input_types(package, item_id);

    // Remap each promoted parameter's body field reads to its scalar leaves;
    // interior whole-tuple reads are reconstructed as nested leaf tuples.
    // Each parameter's recorded whole-value read sites are carried alongside
    // so the body rewrite can reconstruct those standalone reads.
    let reads_by_local: FxHashMap<LocalVarId, &[ExprId]> = candidates
        .iter()
        .map(|c| (c.local_id, c.whole_value_reads.as_slice()))
        .collect();
    for (old_local, param_ty, leaf_map) in &remaps {
        let whole_value_reads = reads_by_local.get(old_local).copied().unwrap_or(&[]);
        rewrite_leaf_field_accesses(
            package,
            assigner,
            item_id,
            *old_local,
            param_ty,
            leaf_map,
            whole_value_reads,
        );
    }

    Some(PromotionResult {
        item_id,
        leaves: leaf_entries,
    })
}

/// Recursively rebuilds a callable input subtree into a flat list of leaf
/// binds, dissolving tuple grouping.
///
/// `index_path` carries the cumulative positional path from `decl.input` to
/// the current pattern; it is pushed/popped around each tuple element so
/// callers observe it unchanged on return.
///
/// - A `Bind` of a promotable parameter is decomposed (via
///   [`decompose_binding_to_leaves`]) into scalar-leaf binds, which are
///   hoisted directly into the flat leaf list (not left nested). The
///   parameter's leaf-relative `(path -> (local, ty))` map and full type are
///   recorded in `remaps` for body remapping.
/// - Any other `Bind` (non-tuple, or a tuple read as a whole value) and any
///   `Discard` is kept as a single leaf, reusing the existing pattern node.
/// - A `Tuple` recurses into each element and concatenates the children's
///   leaves, which is what flattens nested grouping.
fn rebuild_input_leaves(
    package: &mut Package,
    assigner: &mut Assigner,
    pat_id: PatId,
    index_path: &mut Vec<usize>,
    promotable: &FxHashSet<LocalVarId>,
    leaf_pat_ids: &mut Vec<PatId>,
    leaf_entries: &mut Vec<(Vec<usize>, Ty)>,
    remaps: &mut Vec<(LocalVarId, Ty, FxHashMap<Vec<usize>, (LocalVarId, Ty)>)>,
) {
    let pat = package.get_pat(pat_id);
    let pat_ty = pat.ty.clone();
    let kind = pat.kind.clone();
    match kind {
        PatKind::Bind(ident) if promotable.contains(&ident.id) => {
            // Decompose this promotable parameter to a flat tuple of scalar
            // leaves in place, then hoist those leaf pat ids up into the
            // enclosing flat list (dissolving the per-parameter grouping).
            let rel_leaves =
                decompose_binding_to_leaves(package, assigner, pat_id, &ident.name, &pat_ty);
            let child_pat_ids = match &package.get_pat(pat_id).kind {
                PatKind::Tuple(children) => children.clone(),
                _ => unreachable!("decompose_binding_to_leaves sets a Tuple pattern"),
            };
            leaf_pat_ids.extend(child_pat_ids);

            let mut leaf_map: FxHashMap<Vec<usize>, (LocalVarId, Ty)> = FxHashMap::default();
            for (rel_path, leaf_local, leaf_ty) in &rel_leaves {
                let mut full_path = index_path.clone();
                full_path.extend_from_slice(rel_path);
                leaf_entries.push((full_path, leaf_ty.clone()));
                leaf_map.insert(rel_path.clone(), (*leaf_local, leaf_ty.clone()));
            }
            remaps.push((ident.id, pat_ty, leaf_map));
        }
        PatKind::Bind(_) | PatKind::Discard => {
            // Kept parameter: a single leaf projecting the whole value.
            leaf_pat_ids.push(pat_id);
            leaf_entries.push((index_path.clone(), pat_ty));
        }
        PatKind::Tuple(sub_pats) => {
            for (i, sub_pat_id) in sub_pats.into_iter().enumerate() {
                index_path.push(i);
                rebuild_input_leaves(
                    package,
                    assigner,
                    sub_pat_id,
                    index_path,
                    promotable,
                    leaf_pat_ids,
                    leaf_entries,
                    remaps,
                );
                index_path.pop();
            }
        }
    }
}

/// Recomputes the tuple types of every specialization input pattern of a
/// callable bottom-up from their child pattern types.
///
/// After a top-level parameter is flattened, the controlled/adjoint
/// specialization input patterns (which wrap the shared payload pattern in
/// control layers, e.g. `(ctls, payload)`) must have their tuple types
/// refreshed so the pattern shape continues to match the type shape, as
/// required by the `PostArgPromote` tuple-pattern invariant.
fn refresh_spec_input_types(package: &mut Package, item_id: LocalItemId) {
    let spec_input_pats: Vec<PatId> = {
        let item = package.get_item(item_id);
        let ItemKind::Callable(decl) = &item.kind else {
            return;
        };
        match &decl.implementation {
            CallableImpl::Spec(spec_impl) => functored_specs(spec_impl)
                .filter_map(|spec| spec.input)
                .chain(spec_impl.body.input)
                .collect(),
            CallableImpl::SimulatableIntrinsic(spec) => spec.input.into_iter().collect(),
            CallableImpl::Intrinsic => Vec::new(),
        }
    };
    for pat_id in spec_input_pats {
        refresh_pat_tuple_ty(package, pat_id);
    }
}

/// Recomputes a pattern's tuple type from its children, recursively. Leaf
/// (`Bind`/`Discard`) pattern types are authoritative and left unchanged.
fn refresh_pat_tuple_ty(package: &mut Package, pat_id: PatId) {
    let sub_pat_ids = match &package.get_pat(pat_id).kind {
        PatKind::Tuple(sub_pats) => sub_pats.clone(),
        PatKind::Bind(_) | PatKind::Discard => return,
    };
    let mut elem_tys = Vec::with_capacity(sub_pat_ids.len());
    for &sub_pat_id in &sub_pat_ids {
        refresh_pat_tuple_ty(package, sub_pat_id);
        elem_tys.push(package.get_pat(sub_pat_id).ty.clone());
    }
    package.pats.get_mut(pat_id).expect("pat should exist").ty = Ty::Tuple(elem_tys);
}

/// Remaps every body field read of a fully-flattened parameter to the
/// matching scalar leaf local, scoped to the promoted callable's bodies.
///
/// `whole_value_reads` carries the parameter's standalone whole-value read
/// sites so they can be reconstructed from the scalar leaves; it is consumed by
/// the standalone-read rewrite.
fn rewrite_leaf_field_accesses(
    package: &mut Package,
    assigner: &mut Assigner,
    item_id: LocalItemId,
    old_local: LocalVarId,
    param_ty: &Ty,
    leaf_map: &FxHashMap<Vec<usize>, (LocalVarId, Ty)>,
    whole_value_reads: &[ExprId],
) {
    let expr_ids = collect_expr_ids_in_local_callables(&*package, &[item_id]);
    for expr_id in expr_ids {
        rewrite_single_leaf_field_expr(package, assigner, expr_id, old_local, param_ty, leaf_map);
    }

    // Reconstruct each standalone whole-value read of the now-flattened
    // parameter. These are the exact `Var(old_local)` sites that are not the
    // base of a field projection (field bases are consumed when their parent
    // `Field` node is rewritten above), so reconstructing them in place is
    // safe and never clobbers a `Field(Var(old_local), Path)` base.
    for &expr_id in whole_value_reads {
        reconstruct_whole_value_read(package, assigner, expr_id, old_local, param_ty, leaf_map);
    }
}

/// Reconstructs a single standalone whole-value `Var(old_local)` read of a
/// fully-flattened parameter into a (possibly nested) tuple of its scalar leaf
/// `Var`s, overwriting the node's kind and type in place so the reconstructed
/// value has the same shape and type as the original parameter.
fn reconstruct_whole_value_read(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    old_local: LocalVarId,
    param_ty: &Ty,
    leaf_map: &FxHashMap<Vec<usize>, (LocalVarId, Ty)>,
) {
    let expr = package.exprs.get(expr_id).expect("expr should exist");
    let ExprKind::Var(Res::Local(var_id), _) = &expr.kind else {
        return;
    };
    if *var_id != old_local {
        return;
    }

    let new_id = build_leaf_tuple(package, assigner, param_ty, &[], leaf_map);
    let kind = package
        .exprs
        .get(new_id)
        .expect("rebuilt expr exists")
        .kind
        .clone();
    let ty = package
        .exprs
        .get(new_id)
        .expect("rebuilt expr exists")
        .ty
        .clone();
    let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
    expr_mut.kind = kind;
    expr_mut.ty = ty;
}

/// Rewrites a single body expression that projects a field of the fully
/// flattened parameter.
///
/// An exact-path read (`Field(Var(old), Path(p))` where `p` is a leaf path)
/// becomes a direct `Var(leaf)`. An interior whole-tuple read (`p` is a strict
/// prefix of one or more leaf paths) is reconstructed as a nested
/// `Tuple([Var(leaf), ...])` of all leaves under `p`, so callers that read a
/// sub-tuple of the parameter whole still observe the same value.
fn rewrite_single_leaf_field_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    old_local: LocalVarId,
    param_ty: &Ty,
    leaf_map: &FxHashMap<Vec<usize>, (LocalVarId, Ty)>,
) {
    let expr = package.exprs.get(expr_id).expect("expr should exist");
    let ExprKind::Field(inner_id, Field::Path(path)) = expr.kind.clone() else {
        return;
    };
    let inner = package
        .exprs
        .get(inner_id)
        .expect("inner expr should exist");
    let ExprKind::Var(Res::Local(var_id), _) = &inner.kind else {
        return;
    };
    if *var_id != old_local || path.indices.is_empty() {
        return;
    }

    if let Some((leaf_local, leaf_ty)) = leaf_map.get(&path.indices) {
        let leaf_local = *leaf_local;
        let leaf_ty = leaf_ty.clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
        expr_mut.kind = ExprKind::Var(Res::Local(leaf_local), vec![]);
        expr_mut.ty = leaf_ty;
    } else {
        // Interior whole-tuple read: reconstruct a nested tuple of the leaf
        // locals under this prefix path.
        let new_id = build_leaf_tuple(package, assigner, param_ty, &path.indices, leaf_map);
        let kind = package
            .exprs
            .get(new_id)
            .expect("rebuilt expr exists")
            .kind
            .clone();
        let ty = package
            .exprs
            .get(new_id)
            .expect("rebuilt expr exists")
            .ty
            .clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
        expr_mut.kind = kind;
        expr_mut.ty = ty;
    }
}

/// Reconstructs a (possibly nested) tuple of leaf-local `Var`s for the
/// sub-tree of `param_ty` rooted at `prefix`, used for interior whole-tuple
/// reads of a flattened parameter.
fn build_leaf_tuple(
    package: &mut Package,
    assigner: &mut Assigner,
    param_ty: &Ty,
    prefix: &[usize],
    leaf_map: &FxHashMap<Vec<usize>, (LocalVarId, Ty)>,
) -> ExprId {
    if let Some((leaf_local, leaf_ty)) = leaf_map.get(prefix) {
        return create_local_var_expr(package, assigner, *leaf_local, leaf_ty);
    }

    let sub_ty = navigate_tuple_ty(param_ty, prefix);
    let Ty::Tuple(elems) = sub_ty else {
        // Defensive totality: every non-tuple leaf path is present in `leaf_map`
        // (handled by the early return above), so this fallback is unreachable for
        // well-formed flattened inputs. Fall back to a unit tuple to keep the
        // rewrite total.
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            Expr {
                id: expr_id,
                span: Span::default(),
                ty: sub_ty.clone(),
                kind: ExprKind::Tuple(vec![]),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        return expr_id;
    };

    let mut child_ids = Vec::with_capacity(elems.len());
    let mut child_path = prefix.to_vec();
    for i in 0..elems.len() {
        child_path.push(i);
        child_ids.push(build_leaf_tuple(
            package,
            assigner,
            param_ty,
            &child_path,
            leaf_map,
        ));
        child_path.pop();
    }

    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: sub_ty.clone(),
            kind: ExprKind::Tuple(child_ids),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Navigates a (possibly nested) tuple type by a positional `path`, returning
/// the type at that path.
fn navigate_tuple_ty<'a>(ty: &'a Ty, path: &[usize]) -> &'a Ty {
    let mut current = ty;
    for &index in path {
        match current {
            Ty::Tuple(elems) => {
                current = elems.get(index).expect("path index within tuple arity");
            }
            // Dead arm: `build_leaf_tuple` recurses only on `Ty::Tuple` and
            // intercepts leaves via `leaf_map` before recursing, so a non-tuple
            // type never reaches here for well-formed flattened inputs.
            _ => panic!("path navigates into non-tuple type"),
        }
    }
    current
}

/// Rewrites all call sites for promoted callables. At each direct item call,
/// including `Call(UnOp(Functor, Var(Item(id))), arg)`, where `id` is a
/// promoted callable, replaces the payload tuple argument with explicit field
/// extractions wrapped in a `Tuple`.
///
/// # Before
/// ```text
/// Foo(struct_arg)   // single composite argument
/// ```
/// # After
/// ```text
/// Foo((struct_arg.0, struct_arg.1))   // explicit field projections
/// ```
///
/// # Mutations
/// - Rewrites call-site `Expr.kind` in place or wraps in a block when
///   a temporary is needed to avoid evaluating the argument multiple times.
/// - Allocates field-projection and tuple `Expr` nodes through `assigner`.
fn rewrite_call_sites(
    package: &mut Package,
    package_id: PackageId,
    assigner: &mut Assigner,
    promotions: Vec<PromotionResult>,
    reachable_expr_ids: &[ExprId],
) {
    // Build a set of promoted item IDs for quick lookup.
    let promoted_map: FxHashMap<LocalItemId, PromotionResult> =
        promotions.into_iter().map(|p| (p.item_id, p)).collect();

    // Collect all call-site ExprIds that target a promoted callable.
    let call_sites: Vec<(ExprId, LocalItemId, usize)> = reachable_expr_ids
        .iter()
        .filter_map(|&expr_id| {
            let expr = package.exprs.get(expr_id)?;
            if let ExprKind::Call(callee_id, _) = &expr.kind {
                let callee = resolve_promoted_direct_item_callee(
                    package,
                    package_id,
                    *callee_id,
                    &promoted_map,
                )?;
                return Some((expr_id, callee.item_id, callee.controlled_depth));
            }
            None
        })
        .collect();

    for (call_expr_id, item_id, controlled_depth) in call_sites {
        let promotion = promoted_map
            .get(&item_id)
            .expect("promotion should exist for promoted item");
        if controlled_depth == 0 {
            rewrite_single_call_site(package, assigner, call_expr_id, promotion);
        } else {
            rewrite_controlled_call_site(
                package,
                assigner,
                call_expr_id,
                promotion,
                controlled_depth,
            );
        }
    }
}

#[derive(Clone, Copy)]
struct DirectItemCallee {
    item_id: LocalItemId,
    controlled_depth: usize,
}

/// Resolves `callee_id` as a promoted direct item callee, including functor
/// wrappers around the direct item reference.
fn resolve_promoted_direct_item_callee(
    package: &Package,
    package_id: PackageId,
    callee_id: ExprId,
    promoted: &FxHashMap<LocalItemId, PromotionResult>,
) -> Option<DirectItemCallee> {
    let callee = resolve_direct_item_callee(package, package_id, callee_id)?;
    promoted.contains_key(&callee.item_id).then_some(callee)
}

/// Resolves a callee expression to a target-package item, unwrapping adjoint
/// and controlled functor applications while counting controlled layers.
fn resolve_direct_item_callee(
    package: &Package,
    package_id: PackageId,
    callee_id: ExprId,
) -> Option<DirectItemCallee> {
    let mut current = callee_id;
    let mut controlled_depth = 0usize;

    loop {
        let expr = package.exprs.get(current)?;
        match &expr.kind {
            ExprKind::Var(Res::Item(item_id), _) if item_id.package == package_id => {
                return Some(DirectItemCallee {
                    item_id: item_id.item,
                    controlled_depth,
                });
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Adj), inner_id) => {
                current = *inner_id;
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Ctl), inner_id) => {
                controlled_depth += 1;
                current = *inner_id;
            }
            _ => return None,
        }
    }
}

/// Resolves the entry-point callable's [`LocalItemId`] from `package.entry`.
///
/// The entry callable's input is the program's externally-visible ABI and must
/// never be flattened by argument promotion. The entry expression is a direct
/// `Call(callee, _)`; its callee is resolved via [`resolve_direct_item_callee`]
/// so adjoint/controlled functor wrappers are unwrapped. Returns `None` when
/// there is no entry expression or it is not a direct call, leaving behavior
/// unchanged in those cases.
fn resolve_entry_callable_item(package: &Package, package_id: PackageId) -> Option<LocalItemId> {
    let entry_id = package.entry?;
    if let ExprKind::Call(callee_id, _) = &package.get_expr(entry_id).kind {
        resolve_direct_item_callee(package, package_id, *callee_id).map(|c| c.item_id)
    } else {
        None
    }
}

/// Returns `true` when an argument expression can be projected repeatedly
/// without side effects (e.g. literals, plain `Var` references), letting
/// the caller inline each projected field without introducing a
/// temporary.
fn expr_is_safe_to_project_repeatedly(package: &Package, expr_id: ExprId) -> bool {
    match &package.get_expr(expr_id).kind {
        ExprKind::Var(Res::Local(_), _) => true,
        ExprKind::Field(inner_id, Field::Path(_)) => {
            expr_is_safe_to_project_repeatedly(package, *inner_id)
        }
        _ => false,
    }
}

/// Creates a temporary `let temp = arg_expr;` binding for argument
/// expressions that cannot be projected repeatedly without
/// side-effect duplication. The caller replaces subsequent field
/// projections with references to `temp`.
///
/// # Before
/// ```text
/// (no binding)
/// ```
/// # After
/// ```text
/// let __arg_promote_tmp : T = arg_expr;
/// ```
///
/// # Mutations
/// - Allocates a new `Pat`, `LocalVarId`, and `Stmt` through `assigner`.
fn create_projection_temp_binding(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    arg_ty: &Ty,
) -> (LocalVarId, StmtId) {
    let local_id = assigner.next_local();
    let pat_id = assigner.next_pat();
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span: Span::default(),
            ty: arg_ty.clone(),
            kind: PatKind::Bind(Ident {
                id: local_id,
                span: Span::default(),
                name: Rc::from(ARG_PROMOTE_TMP_NAME),
            }),
        },
    );

    let stmt_id = assigner.next_stmt();
    package.stmts.insert(
        stmt_id,
        Stmt {
            id: stmt_id,
            span: Span::default(),
            kind: StmtKind::Local(Mutability::Immutable, pat_id, arg_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    (local_id, stmt_id)
}

/// Returns `true` when the promotion leaf at `path` can be projected out of the
/// tuple-literal argument `arg_id` by reusing existing sub-expressions, without
/// introducing a temporary.
///
/// Navigation descends through nested tuple literals. Once a non-literal
/// sub-expression is reached with path remaining, the remainder is a field
/// projection that is only duplication-safe when that sub-expression is itself
/// safe to project repeatedly. A leaf whose path is fully consumed by tuple
/// literals lands on a sub-expression that is referenced exactly once, so it is
/// always safe to reuse in place.
fn leaf_projects_through_tuple_literal(package: &Package, arg_id: ExprId, path: &[usize]) -> bool {
    let mut current = arg_id;
    let mut rest = path;
    while !rest.is_empty() {
        let ExprKind::Tuple(elems) = &package.get_expr(current).kind else {
            return expr_is_safe_to_project_repeatedly(package, current);
        };
        let Some(&next) = elems.get(rest[0]) else {
            return false;
        };
        current = next;
        rest = &rest[1..];
    }
    true
}

/// Projects the promotion leaf at `path` out of the tuple-literal argument
/// `arg_id`, reusing existing sub-expressions in place. Descends through nested
/// tuple literals; if a non-literal sub-expression is reached with path
/// remaining, a `Field` projection of that sub-expression is allocated.
///
/// Callers must first confirm the leaf is projectable via
/// [`leaf_projects_through_tuple_literal`].
fn project_leaf_through_tuple_literal(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    path: &[usize],
    leaf_ty: &Ty,
) -> ExprId {
    let mut current = arg_id;
    let mut rest = path;
    while !rest.is_empty() {
        let next = {
            let ExprKind::Tuple(elems) = &package.get_expr(current).kind else {
                break;
            };
            elems[rest[0]]
        };
        current = next;
        rest = &rest[1..];
    }

    if rest.is_empty() {
        return current;
    }

    let field_expr_id = assigner.next_expr();
    package.exprs.insert(
        field_expr_id,
        Expr {
            id: field_expr_id,
            span: Span::default(),
            ty: leaf_ty.clone(),
            kind: ExprKind::Field(
                current,
                Field::Path(FieldPath {
                    indices: rest.to_vec(),
                }),
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    field_expr_id
}

/// Attempts to build the flat projected tuple argument directly from a
/// tuple-literal argument by reusing each leaf sub-expression in place, instead
/// of binding the whole argument to a temporary and projecting from it.
///
/// Returns `None` when the argument is not a tuple literal, or when some leaf
/// would require duplicating a sub-expression that is not safe to project
/// repeatedly, in which case the caller falls back to a temporary binding.
///
/// # Before
/// ```text
/// Foo(((a, b), c - 1))   // nested tuple literal argument
/// ```
/// # After
/// ```text
/// Foo((a, b, c - 1))     // flat leaf projection, no temporary
/// ```
///
/// This keeps a promoted multi-leaf call site in clean flat form with no
/// surviving projection temporary, the common shape for promoted self-calls and
/// tuple-literal arguments.
///
/// # Mutations
/// - Allocates per-leaf `Field` `Expr` nodes (only for residual sub-paths) and
///   the outer `Tuple` `Expr` through `assigner`.
fn try_inline_tuple_literal_projection(
    package: &mut Package,
    assigner: &mut Assigner,
    promotion: &PromotionResult,
    arg_id: ExprId,
) -> Option<ExprId> {
    if !matches!(package.get_expr(arg_id).kind, ExprKind::Tuple(_)) {
        return None;
    }
    if !promotion
        .leaves
        .iter()
        .all(|(path, _)| leaf_projects_through_tuple_literal(package, arg_id, path))
    {
        return None;
    }

    let field_expr_ids: Vec<ExprId> = promotion
        .leaves
        .iter()
        .map(|(path, leaf_ty)| {
            project_leaf_through_tuple_literal(package, assigner, arg_id, path, leaf_ty)
        })
        .collect();

    let tuple_ty = Ty::Tuple(
        promotion
            .leaves
            .iter()
            .map(|(_, leaf_ty)| leaf_ty.clone())
            .collect(),
    );
    let new_arg_id = assigner.next_expr();
    package.exprs.insert(
        new_arg_id,
        Expr {
            id: new_arg_id,
            span: Span::default(),
            ty: tuple_ty,
            kind: ExprKind::Tuple(field_expr_ids),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    Some(new_arg_id)
}

/// Allocates a fresh `ExprKind::Var(Res::Local(var))` expression with the
/// given type, used to materialize references to synthesized temporaries
/// and promoted parameters.
///
/// # Mutations
/// - Inserts one `Expr` node through `assigner`.
fn create_local_var_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    local_id: LocalVarId,
    ty: &Ty,
) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: ty.clone(),
            kind: ExprKind::Var(Res::Local(local_id), vec![]),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Builds the projected tuple that replaces the original tuple argument at
/// a call site, projecting each flat scalar leaf of the promoted callable's
/// (fully decomposed) parameter from the original argument value.
///
/// # Before
/// ```text
/// (no expression)
/// ```
/// # After
/// ```text
/// Tuple([Field(arg, Path(p_0)), ..., Field(arg, Path(p_{n-1}))])
/// ```
/// where each `p_i` is the positional path of a leaf in the original
/// (possibly nested) parameter type.
///
/// # Mutations
/// - Allocates per-leaf `Field` `Expr` nodes and the outer `Tuple`
///   `Expr` through `assigner`.
fn create_projected_tuple_arg(
    package: &mut Package,
    assigner: &mut Assigner,
    promotion: &PromotionResult,
    arg_id: ExprId,
    arg_ty: &Ty,
    temp_local: Option<LocalVarId>,
) -> ExprId {
    let mut field_expr_ids: Vec<ExprId> = Vec::with_capacity(promotion.leaves.len());

    for (path, leaf_ty) in &promotion.leaves {
        let field_base_id = if let Some(temp_local) = temp_local {
            create_local_var_expr(package, assigner, temp_local, arg_ty)
        } else {
            arg_id
        };
        let field_expr_id = assigner.next_expr();
        let field_expr = qsc_fir::fir::Expr {
            id: field_expr_id,
            span: Span::default(),
            ty: leaf_ty.clone(),
            kind: ExprKind::Field(
                field_base_id,
                Field::Path(FieldPath {
                    indices: path.clone(),
                }),
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        };
        package.exprs.insert(field_expr_id, field_expr);
        field_expr_ids.push(field_expr_id);
    }

    let new_arg_id = assigner.next_expr();
    let tuple_ty = Ty::Tuple(
        promotion
            .leaves
            .iter()
            .map(|(_, leaf_ty)| leaf_ty.clone())
            .collect(),
    );
    let new_arg = qsc_fir::fir::Expr {
        id: new_arg_id,
        span: Span::default(),
        ty: tuple_ty,
        kind: ExprKind::Tuple(field_expr_ids),
        exec_graph_range: EMPTY_EXEC_RANGE,
    };
    package.exprs.insert(new_arg_id, new_arg);
    new_arg_id
}

/// Wraps a single promoted payload expression in a one-element tuple argument.
fn create_single_tuple_arg(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    elem_types: &[Ty],
) -> ExprId {
    let new_arg_id = assigner.next_expr();
    let new_arg = qsc_fir::fir::Expr {
        id: new_arg_id,
        span: Span::default(),
        ty: Ty::Tuple(elem_types.to_vec()),
        kind: ExprKind::Tuple(vec![arg_id]),
        exec_graph_range: EMPTY_EXEC_RANGE,
    };
    package.exprs.insert(new_arg_id, new_arg);
    new_arg_id
}

/// Builds a block expression that evaluates a leading statement before
/// returning `result_expr_id`.
fn create_payload_block(
    package: &mut Package,
    assigner: &mut Assigner,
    leading_stmt_id: StmtId,
    result_expr_id: ExprId,
) -> ExprId {
    let result_ty = package.get_expr(result_expr_id).ty.clone();

    let result_stmt_id = assigner.next_stmt();
    package.stmts.insert(
        result_stmt_id,
        Stmt {
            id: result_stmt_id,
            span: Span::default(),
            kind: StmtKind::Expr(result_expr_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let block_id = assigner.next_block();
    package.blocks.insert(
        block_id,
        Block {
            id: block_id,
            span: Span::default(),
            ty: result_ty.clone(),
            stmts: vec![leading_stmt_id, result_stmt_id],
        },
    );

    let block_expr_id = assigner.next_expr();
    package.exprs.insert(
        block_expr_id,
        Expr {
            id: block_expr_id,
            span: Span::default(),
            ty: result_ty,
            kind: ExprKind::Block(block_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    block_expr_id
}

/// Returns `true` when `elems` is already the fully-flattened argument list:
/// one element per promotion leaf, each carrying the leaf's scalar type. A
/// top-level arity match alone is insufficient, because an element may itself
/// be a nested tuple (for example a single-field struct erased to a 1-tuple)
/// that still needs projection into the flat leaf list.
fn arg_tuple_matches_flat_leaves(
    package: &Package,
    elems: &[ExprId],
    promotion: &PromotionResult,
) -> bool {
    elems.len() == promotion.leaves.len()
        && elems
            .iter()
            .zip(&promotion.leaves)
            .all(|(elem_id, (_, leaf_ty))| {
                package
                    .exprs
                    .get(*elem_id)
                    .expect("arg element expr exists")
                    .ty
                    == *leaf_ty
            })
}

/// Creates a promoted payload argument, returning `None` when the existing
/// payload already has the expected tuple shape.
fn create_rewritten_payload_arg(
    package: &mut Package,
    assigner: &mut Assigner,
    promotion: &PromotionResult,
    arg_id: ExprId,
) -> Option<ExprId> {
    let arg_expr = package.exprs.get(arg_id).expect("arg expr exists");
    let arg_ty = arg_expr.ty.clone();
    let arg_tuple_elems = match &arg_expr.kind {
        ExprKind::Tuple(elems) => Some(elems.clone()),
        _ => None,
    };

    if let Some(elems) = &arg_tuple_elems
        && arg_tuple_matches_flat_leaves(package, elems, promotion)
    {
        return None;
    }

    if promotion.leaves.len() == 1 {
        let leaf_tys: Vec<Ty> = promotion.leaves.iter().map(|(_, ty)| ty.clone()).collect();
        return Some(create_single_tuple_arg(
            package, assigner, arg_id, &leaf_tys,
        ));
    }

    if let Some(new_arg_id) =
        try_inline_tuple_literal_projection(package, assigner, promotion, arg_id)
    {
        return Some(new_arg_id);
    }

    let temp_binding = if expr_is_safe_to_project_repeatedly(package, arg_id) {
        None
    } else {
        Some(create_projection_temp_binding(
            package, assigner, arg_id, &arg_ty,
        ))
    };
    let new_arg_id = create_projected_tuple_arg(
        package,
        assigner,
        promotion,
        arg_id,
        &arg_ty,
        temp_binding.map(|(temp_local, _)| temp_local),
    );

    Some(if let Some((_, temp_stmt_id)) = temp_binding {
        create_payload_block(package, assigner, temp_stmt_id, new_arg_id)
    } else {
        new_arg_id
    })
}

/// Wraps an existing `Call` expression in a synthesized block that places
/// a pre-built leading statement (typically a temporary binding) before
/// the call, preserving evaluation order.
///
/// # Before
/// ```text
/// call_expr_id = Call(callee_id, _)
/// ```
/// # After
/// ```text
/// call_expr_id = Block {
///     leading_stmt;                       // supplied by caller
///     Expr(Call(callee_id, new_arg_id))   // inner call with rewritten args
/// }
/// ```
///
/// # Mutations
/// - Replaces `call_expr_id`'s `ExprKind` with `Block(..)` in place.
/// - Allocates inner `Call`, `Stmt`, and `Block` nodes through `assigner`.
fn wrap_call_in_block(
    package: &mut Package,
    assigner: &mut Assigner,
    call_expr_id: ExprId,
    callee_id: ExprId,
    new_arg_id: ExprId,
    call_ty: &Ty,
    leading_stmt_id: StmtId,
) {
    let inner_call_id = assigner.next_expr();
    package.exprs.insert(
        inner_call_id,
        Expr {
            id: inner_call_id,
            span: Span::default(),
            ty: call_ty.clone(),
            kind: ExprKind::Call(callee_id, new_arg_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let call_stmt_id = assigner.next_stmt();
    package.stmts.insert(
        call_stmt_id,
        Stmt {
            id: call_stmt_id,
            span: Span::default(),
            kind: StmtKind::Expr(inner_call_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let block_id = assigner.next_block();
    package.blocks.insert(
        block_id,
        Block {
            id: block_id,
            span: Span::default(),
            ty: call_ty.clone(),
            stmts: vec![leading_stmt_id, call_stmt_id],
        },
    );

    let call_mut = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr exists");
    call_mut.kind = ExprKind::Block(block_id);
}

/// Rewrites a single call site: `Foo(arg)` → `Foo((arg.0, arg.1, ...))`.
///
/// # Before
/// ```text
/// Call(Var(Foo), composite_arg)
/// ```
/// # After
/// ```text
/// Call(Var(Foo), Tuple([arg.0, arg.1, ...]))   // or Block wrapping
/// ```
///
/// If the argument is already a `Tuple(...)` with the correct arity, the
/// existing tuple elements are used directly. Otherwise, field-extraction
/// expressions are created.
///
/// # Mutations
/// - Rewrites `call_expr_id`'s `ExprKind` in place.
/// - May allocate projection, tuple, and temporary `Expr`/`Stmt` nodes
///   through `assigner`.
fn rewrite_single_call_site(
    package: &mut Package,
    assigner: &mut Assigner,
    call_expr_id: ExprId,
    promotion: &PromotionResult,
) {
    let call_expr = package.exprs.get(call_expr_id).expect("call expr exists");
    let ExprKind::Call(callee_id, arg_id) = call_expr.kind else {
        return;
    };
    let call_ty = call_expr.ty.clone();

    let arg_expr = package.exprs.get(arg_id).expect("arg expr exists");
    let arg_ty = arg_expr.ty.clone();
    let arg_tuple_elems = match &arg_expr.kind {
        ExprKind::Tuple(elems) => Some(elems.clone()),
        _ => None,
    };

    // If the argument is already a flat tuple literal whose elements match the
    // promotion leaf types, the call site is already structured correctly.
    if let Some(elems) = &arg_tuple_elems
        && arg_tuple_matches_flat_leaves(package, elems, promotion)
    {
        return;
    }

    if promotion.leaves.len() == 1 {
        let leaf_tys: Vec<Ty> = promotion.leaves.iter().map(|(_, ty)| ty.clone()).collect();
        let new_arg_id = create_single_tuple_arg(package, assigner, arg_id, &leaf_tys);

        let call_mut = package
            .exprs
            .get_mut(call_expr_id)
            .expect("call expr exists");
        call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
        return;
    }

    if let Some(new_arg_id) =
        try_inline_tuple_literal_projection(package, assigner, promotion, arg_id)
    {
        let call_mut = package
            .exprs
            .get_mut(call_expr_id)
            .expect("call expr exists");
        call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
        return;
    }

    let temp_binding = if expr_is_safe_to_project_repeatedly(package, arg_id) {
        None
    } else {
        Some(create_projection_temp_binding(
            package, assigner, arg_id, &arg_ty,
        ))
    };
    let new_arg_id = create_projected_tuple_arg(
        package,
        assigner,
        promotion,
        arg_id,
        &arg_ty,
        temp_binding.map(|(temp_local, _)| temp_local),
    );

    if let Some((_, temp_stmt_id)) = temp_binding {
        wrap_call_in_block(
            package,
            assigner,
            call_expr_id,
            callee_id,
            new_arg_id,
            &call_ty,
            temp_stmt_id,
        );
    } else {
        let call_mut = package
            .exprs
            .get_mut(call_expr_id)
            .expect("call expr exists");
        call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
    }
}

/// Rewrites the payload portion of a controlled call while preserving the
/// existing control layers and their evaluation order.
fn rewrite_controlled_call_site(
    package: &mut Package,
    assigner: &mut Assigner,
    call_expr_id: ExprId,
    promotion: &PromotionResult,
    controlled_depth: usize,
) {
    let call_expr = package.exprs.get(call_expr_id).expect("call expr exists");
    let ExprKind::Call(callee_id, arg_id) = call_expr.kind else {
        return;
    };

    let Some((control_ids, payload_id)) =
        peel_controlled_arg_layers(package, arg_id, controlled_depth)
    else {
        return;
    };

    let Some(new_payload_id) =
        create_rewritten_payload_arg(package, assigner, promotion, payload_id)
    else {
        return;
    };

    let new_arg_id = rebuild_controlled_arg_layers(package, assigner, &control_ids, new_payload_id);
    let call_mut = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr exists");
    call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
}

/// Peels nested controlled-call argument tuples into their control expressions
/// and the final payload expression.
fn peel_controlled_arg_layers(
    package: &Package,
    arg_id: ExprId,
    controlled_depth: usize,
) -> Option<(Vec<ExprId>, ExprId)> {
    let mut control_ids = Vec::with_capacity(controlled_depth);
    let mut current = arg_id;

    for _ in 0..controlled_depth {
        let expr = package.exprs.get(current)?;
        let ExprKind::Tuple(items) = &expr.kind else {
            return None;
        };
        let [controls, payload] = items.as_slice() else {
            return None;
        };
        control_ids.push(*controls);
        current = *payload;
    }

    Some((control_ids, current))
}

/// Rebuilds controlled-call argument tuple layers around a rewritten payload.
fn rebuild_controlled_arg_layers(
    package: &mut Package,
    assigner: &mut Assigner,
    control_ids: &[ExprId],
    payload_id: ExprId,
) -> ExprId {
    let mut current = payload_id;

    for &controls in control_ids.iter().rev() {
        let tuple_ty = Ty::Tuple(vec![
            package.get_expr(controls).ty.clone(),
            package.get_expr(current).ty.clone(),
        ]);
        let tuple_id = assigner.next_expr();
        package.exprs.insert(
            tuple_id,
            Expr {
                id: tuple_id,
                span: Span::default(),
                ty: tuple_ty,
                kind: ExprKind::Tuple(vec![controls, current]),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current = tuple_id;
    }

    current
}

/// Normalizes call argument expression shapes to exactly match callee input
/// types.
///
/// This pass is intentionally run after fixed-point promotion converges,
/// because prior rewrites can leave call arguments with shape-equivalent but
/// type-distinct forms (most notably `T` vs `(T)` for single-element tuples).
///
/// # Before
/// ```text
/// operation UseOne(p : (Qubit[])) : Unit { ... }
/// UseOne(qs);        // arg type Qubit[]
/// ```
///
/// # After
/// ```text
/// operation UseOne(p : (Qubit[])) : Unit { ... }
/// UseOne((qs,));     // arg type (Qubit[])
/// ```
///
/// # Ensures
/// - For every direct call expression, argument type structure matches the
///   expected callable input type where normalization can be done locally.
/// - Does not rewrite callee declarations; only argument expression shape.
fn normalize_call_arg_types(
    package: &mut Package,
    package_id: PackageId,
    assigner: &mut Assigner,
    reachable_expr_ids: &[ExprId],
) {
    let call_sites: Vec<(ExprId, Ty)> = reachable_expr_ids
        .iter()
        .filter_map(|&expr_id| {
            let expr = package.exprs.get(expr_id)?;
            let ExprKind::Call(callee_id, arg_id) = expr.kind else {
                return None;
            };
            resolve_expected_input(package, package_id, callee_id)
                .map(|expected_input| (arg_id, expected_input))
        })
        .collect();

    for (arg_id, expected_input) in call_sites {
        normalize_arg_to_expected_input(package, assigner, arg_id, &expected_input);
    }
}

fn resolve_expected_input(
    package: &Package,
    package_id: PackageId,
    callee_id: ExprId,
) -> Option<Ty> {
    if let Some(callee) = resolve_direct_item_callee(package, package_id, callee_id) {
        let item = package.items.get(callee.item_id)?;
        if let ItemKind::Callable(decl) = &item.kind {
            let input_ty = package.get_pat(decl.input).ty.clone();
            return Some(apply_controlled_input_layers(
                input_ty,
                callee.controlled_depth,
            ));
        }
    }

    let callee = package.get_expr(callee_id);
    if let Ty::Arrow(arrow) = &callee.ty {
        return Some((*arrow.input).clone());
    }

    None
}

/// Applies one controlled-functor input layer per controlled wrapper.
fn apply_controlled_input_layers(mut input_ty: Ty, controlled_depth: usize) -> Ty {
    for _ in 0..controlled_depth {
        input_ty = Ty::Tuple(vec![Ty::Array(Box::new(Ty::Prim(Prim::Qubit))), input_ty]);
    }
    input_ty
}

/// Reconciles a rewritten call-site argument subtree with the callee's current
/// input type.
///
/// Before, `arg_id` may still reflect the pre-promotion shape, such as a scalar
/// where the promoted callee now expects `(scalar,)`, or nested tuple children
/// whose wrapper structure no longer matches the updated input pattern. After,
/// the subtree rooted at `arg_id` mirrors `expected_input`: single-element tuple
/// wrappers are inserted only where required and tuple types are refreshed after
/// recursive normalization.
fn normalize_arg_to_expected_input(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    expected_input: &Ty,
) {
    let arg = package.get_expr(arg_id).clone();
    if arg.ty == *expected_input {
        return;
    }

    let Ty::Tuple(expected_items) = expected_input else {
        return;
    };

    if expected_items.len() == 1 && arg.ty == expected_items[0] {
        wrap_arg_in_single_tuple(package, assigner, arg_id);
        return;
    }

    let ExprKind::Tuple(arg_items) = arg.kind else {
        return;
    };
    if arg_items.len() != expected_items.len() {
        return;
    }

    for (arg_item, expected_item) in arg_items.iter().zip(expected_items) {
        normalize_arg_to_expected_input(package, assigner, *arg_item, expected_item);
    }

    let updated_tys = arg_items
        .iter()
        .map(|arg_item| package.get_expr(*arg_item).ty.clone())
        .collect();
    let arg_mut = package.exprs.get_mut(arg_id).expect("arg expr exists");
    arg_mut.ty = Ty::Tuple(updated_tys);
}

/// Replaces `arg_id` with a one-element tuple node while preserving the
/// original argument under a freshly allocated child expression.
///
/// Before, `arg_id` points directly at the scalar or tuple element supplied at
/// the call site. After, the original payload lives at `preserved_arg_id` and
/// `arg_id` becomes `(payload)`, matching callees whose promoted signature still
/// expects a single tuple layer.
fn wrap_arg_in_single_tuple(package: &mut Package, assigner: &mut Assigner, arg_id: ExprId) {
    let original_arg = package.get_expr(arg_id).clone();
    let preserved_arg_id = assigner.next_expr();
    package.exprs.insert(
        preserved_arg_id,
        Expr {
            id: preserved_arg_id,
            span: original_arg.span,
            ty: original_arg.ty.clone(),
            kind: original_arg.kind,
            exec_graph_range: original_arg.exec_graph_range,
        },
    );

    let arg = package.exprs.get_mut(arg_id).expect("arg expr exists");
    arg.kind = ExprKind::Tuple(vec![preserved_arg_id]);
    arg.ty = Ty::Tuple(vec![original_arg.ty]);
}
