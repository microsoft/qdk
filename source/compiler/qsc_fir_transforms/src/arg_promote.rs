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
//!   or `Ty::Udt(Res::Item(_))` where every use in every specialization body
//!   is a field access.
//! - Verifies the callable is not used as a first-class value, referenced
//!   as a closure target, or otherwise left indirectly dispatched.
//!   First-class detection and closure-target detection together cover
//!   the partial-application cases that used to be enumerated separately.
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
//! per iteration (identical to SROA's iterative strategy).
//!
//! # Pipeline position
//!
//! This pass runs after SROA and before unreachable-node GC. At this point,
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
//!    Find `PatKind::Bind` inputs of tuple/UDT shape whose uses are field-only
//!    across every specialization.
//! 3. **Safety filters** ([`collect_first_class_callables`],
//!    [`collect_closure_targets`]):
//!    Exclude callables used as first-class values or closure targets.
//! 4. **Signature/body rewrite** ([`promote_candidate`]):
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

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::EMPTY_EXEC_RANGE;
use crate::fir_builder::{
    decompose_binding, functored_specs, reachable_local_callables, resolve_udt_element_types,
};
use crate::reachability::collect_reachable_from_entry;
use crate::walk_utils::{collect_uses_in_block, for_each_expr, for_each_expr_in_callable_impl};
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Block, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath, Ident,
    ItemKind, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup, PackageStore,
    Pat, PatId, PatKind, Res, SpecDecl, SpecImpl, Stmt, StmtId, StmtKind, StoreItemId,
};
use qsc_fir::ty::Ty;
use rustc_hash::FxHashSet;
use std::rc::Rc;

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
pub fn arg_promote(store: &mut PackageStore, package_id: PackageId, assigner: &mut Assigner) {
    let package = store.get(package_id);
    if package.entry.is_none() {
        return;
    }

    loop {
        let reachable = collect_reachable_from_entry(store, package_id);
        let package = store.get(package_id);

        // Collect first-class and closure uses of callables to disqualify them.
        let first_class = collect_first_class_callables(package, package_id, &reachable);
        let closure_targets = collect_closure_targets(package, package_id, &reachable);

        // Identify candidates.
        let mut candidates: Vec<ArgPromoCandidate> = Vec::new();

        for (item_id, decl) in reachable_local_callables(package, package_id, &reachable) {
            // Skip callables used as first-class values or partially applied.
            if first_class.contains(&item_id) || closure_targets.contains(&item_id) {
                continue;
            }

            candidates.extend(check_candidates(store, package, package_id, item_id, decl));
        }

        if candidates.is_empty() {
            break;
        }

        // Apply promotion: decompose parameters and rewrite bodies.
        let package = store.get_mut(package_id);
        let mut promotions: Vec<PromotionResult> = Vec::new();
        for candidate in &candidates {
            if let Some(result) = promote_candidate(package, assigner, candidate) {
                promotions.push(result);
            }
        }

        // Rewrite call sites (only for top-level promotions).
        if !promotions.is_empty() {
            rewrite_call_sites(package, package_id, assigner, &promotions);
        }
    }

    // Normalize call-arg types across all call sites so that argument
    // expressions match the expected callable input shape (e.g. single-
    // element tuple wrapping after promotion changes a signature).
    let package = store.get_mut(package_id);
    normalize_call_arg_types(package, package_id, assigner);
}

/// A candidate for argument promotion.
struct ArgPromoCandidate {
    /// The `LocalItemId` of the callable.
    item_id: LocalItemId,
    /// The `LocalVarId` bound by the parameter.
    local_id: LocalVarId,
    /// The `PatId` of the input binding pattern.
    pat_id: PatId,
    /// Element types from the tuple.
    elem_types: Vec<Ty>,
    /// The name of the original parameter.
    name: Rc<str>,
    /// Whether this is a top-level promotion (`pat_id == decl.input`).
    /// Top-level promotions require call-site rewriting; sub-parameter
    /// promotions (inside a `PatKind::Tuple`) do not.
    is_top_level: bool,
}

/// Result of promoting a candidate — tracks the callable and its element types
/// so that call sites can be rewritten.
struct PromotionResult {
    /// The callable's `LocalItemId`.
    item_id: LocalItemId,
    /// Element types.
    elem_types: Vec<Ty>,
}

/// Checks whether a callable's input parameter is a single tuple-typed or
/// UDT-typed binding whose only uses in all specialization bodies are field
/// accesses. Also recurses into `PatKind::Tuple` sub-patterns to find
/// inner bindings eligible for promotion after a previous pass.
fn check_candidates(
    store: &PackageStore,
    package: &Package,
    _package_id: PackageId,
    item_id: LocalItemId,
    decl: &CallableDecl,
) -> Vec<ArgPromoCandidate> {
    let mut candidates = Vec::new();
    find_param_binds_in_pat(
        store,
        package,
        item_id,
        decl,
        decl.input,
        true,
        &mut candidates,
    );
    candidates
}

/// Recursively walks a callable's input pattern to find `PatKind::Bind` nodes
/// with tuple or UDT types whose uses are all field accesses.
fn find_param_binds_in_pat(
    store: &PackageStore,
    package: &Package,
    item_id: LocalItemId,
    decl: &CallableDecl,
    pat_id: PatId,
    is_top_level: bool,
    candidates: &mut Vec<ArgPromoCandidate>,
) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            let elem_types = match &pat.ty {
                Ty::Tuple(elems) if !elems.is_empty() => Some(elems.clone()),
                Ty::Udt(Res::Item(udt_item_id)) => resolve_udt_element_types(store, udt_item_id),
                _ => None,
            };
            if let Some(elem_types) = elem_types {
                let local_id = ident.id;
                if all_param_uses_are_field_access(package, decl, local_id) {
                    candidates.push(ArgPromoCandidate {
                        item_id,
                        local_id,
                        pat_id,
                        elem_types,
                        name: ident.name.clone(),
                        is_top_level,
                    });
                }
            }
        }
        PatKind::Tuple(sub_pats) => {
            for &sub_pat_id in sub_pats {
                find_param_binds_in_pat(
                    store, package, item_id, decl, sub_pat_id, false, candidates,
                );
            }
        }
        PatKind::Discard => {}
    }
}

/// Returns `true` if every use of `local_id` across all specialization bodies
/// of the callable is a field access.
///
/// Intrinsic callables short-circuit to `true`: they have no user body to
/// analyze for field-projection eligibility, so the callable parameter
/// layout for an intrinsic is considered trivially field-only.
fn all_param_uses_are_field_access(
    package: &Package,
    decl: &CallableDecl,
    local_id: LocalVarId,
) -> bool {
    match &decl.implementation {
        CallableImpl::Intrinsic => true,
        CallableImpl::Spec(spec_impl) => all_uses_in_spec_impl(package, spec_impl, local_id),
        CallableImpl::SimulatableIntrinsic(spec) => all_uses_in_spec(package, spec, local_id),
    }
}

/// Returns `true` when every specialization (body, adjoint, controlled,
/// controlled-adjoint) uses `local_id` exclusively via field access.
fn all_uses_in_spec_impl(package: &Package, spec_impl: &SpecImpl, local_id: LocalVarId) -> bool {
    if !all_uses_in_spec(package, &spec_impl.body, local_id) {
        return false;
    }
    for spec in functored_specs(spec_impl) {
        if !all_uses_in_spec(package, spec, local_id) {
            return false;
        }
    }
    true
}

/// Returns `true` when every use of `local_id` in a single `SpecDecl` body
/// is a field access (per the classifier in [`collect_uses_in_block`]).
fn all_uses_in_spec(package: &Package, spec: &SpecDecl, local_id: LocalVarId) -> bool {
    let mut uses = Vec::new();
    collect_uses_in_block(package, spec.block, local_id, &mut uses);
    uses.iter().all(|u| *u)
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
            // The callee position is a direct call — don't mark it.
            // But still recurse into the callee's sub-expressions
            // (e.g., if callee is Field(...), that's not a direct Var).
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
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                scan_first_class_in_expr(package, package_id, *c, first_class);
            }
            for fa in fields {
                scan_first_class_in_expr(package, package_id, fa.value, first_class);
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
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) | ExprKind::Closure(_, _) => {}
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

/// Promotes a single candidate in-place: decomposes the input pattern and
/// rewrites field accesses in all specialization bodies. Returns a
/// `PromotionResult` only for top-level promotions (which require call-site
/// rewriting).
///
/// # Before
/// ```text
/// input pat = Bind(p : (A, B))
/// body:  Field(Var(Local(p)), Path([0])); Field(Var(Local(p)), Path([1]))
/// ```
/// # After
/// ```text
/// input pat = Tuple([Bind(p_0 : A), Bind(p_1 : B)])
/// body:  Var(Local(p_0)); Var(Local(p_1))
/// ```
///
/// # Mutations
/// - Rewrites the input `Pat` from `Bind` to `Tuple` of per-element `Bind`s.
/// - Allocates new `LocalVarId`, `PatId` nodes through `assigner`.
/// - Delegates to [`rewrite_field_accesses`] to rewrite body expressions.
fn promote_candidate(
    package: &mut Package,
    assigner: &mut Assigner,
    candidate: &ArgPromoCandidate,
) -> Option<PromotionResult> {
    let new_locals = decompose_binding(
        package,
        assigner,
        candidate.pat_id,
        &candidate.name,
        &candidate.elem_types,
    );

    // Rewrite all field accesses across the entire package.
    rewrite_field_accesses(
        package,
        assigner,
        candidate.local_id,
        &new_locals,
        &candidate.elem_types,
    );

    if candidate.is_top_level {
        Some(PromotionResult {
            item_id: candidate.item_id,
            elem_types: candidate.elem_types.clone(),
        })
    } else {
        None
    }
}

/// Rewrites field accesses on the old local to use the new decomposed locals.
///
/// # Before
/// ```text
/// Field(Var(Local(old)), Path([i]))   // param.i
/// ```
/// # After
/// ```text
/// Var(Local(old_i))   // direct scalar reference
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` in place for matching `Field` and `AssignField`
///   expressions via [`rewrite_single_field_expr`].
fn rewrite_field_accesses(
    package: &mut Package,
    assigner: &mut Assigner,
    old_local: LocalVarId,
    new_locals: &[LocalVarId],
    elem_types: &[Ty],
) {
    let expr_ids: Vec<ExprId> = package.exprs.iter().map(|(id, _)| id).collect();
    for expr_id in expr_ids {
        rewrite_single_field_expr(
            package, assigner, expr_id, old_local, new_locals, elem_types,
        );
    }
}

/// Rewrites a single expression that projects a field of the now-promoted
/// parameter so it references the corresponding new scalar parameter
/// binding directly.
///
/// Handles two expression shapes:
///
/// # Before (`Field` read)
/// ```text
/// Field(Var(Local(old_param)), Path([i]))      // single-index
/// Field(Var(Local(old_param)), Path([i, j]))   // nested
/// ```
/// # After (`Field` read)
/// ```text
/// Var(Local(param_i))                          // single-index: direct
/// Field(Var(Local(param_i)), Path([j]))         // nested: re-rooted
/// ```
///
/// # Before (`AssignField`)
/// ```text
/// AssignField(Var(Local(old_param)), Path([i]), value)
/// ```
/// # After (`AssignField`)
/// ```text
/// Assign(Var(Local(param_i)), value)
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` and `Expr.ty` in place for the matched expression.
/// - Allocates new `Var` `Expr` nodes through `assigner` for nested and
///   assign-field paths.
fn rewrite_single_field_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    old_local: LocalVarId,
    new_locals: &[LocalVarId],
    elem_types: &[Ty],
) {
    let expr = package.exprs.get(expr_id).expect("expr should exist");
    match expr.kind.clone() {
        ExprKind::Field(inner_id, Field::Path(path)) => {
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
                    if path.indices.len() == 1 {
                        let new_local = new_locals[idx];
                        let new_ty = elem_types[idx].clone();
                        let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
                        expr_mut.kind = ExprKind::Var(Res::Local(new_local), vec![]);
                        expr_mut.ty = new_ty;
                    } else {
                        let new_local = new_locals[idx];
                        let remaining: Vec<usize> = path.indices[1..].to_vec();

                        let new_inner_id = assigner.next_expr();
                        package.exprs.insert(
                            new_inner_id,
                            Expr {
                                id: new_inner_id,
                                span: Span::default(),
                                ty: elem_types[idx].clone(),
                                kind: ExprKind::Var(Res::Local(new_local), vec![]),
                                exec_graph_range: EMPTY_EXEC_RANGE,
                            },
                        );

                        let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
                        expr_mut.kind = ExprKind::Field(
                            new_inner_id,
                            Field::Path(FieldPath { indices: remaining }),
                        );
                    }
                }
            }
        }
        ExprKind::AssignField(record_id, Field::Path(path), value_id) => {
            let record = package
                .exprs
                .get(record_id)
                .expect("record expr should exist");
            if let ExprKind::Var(Res::Local(var_id), _) = &record.kind
                && *var_id == old_local
                && !path.indices.is_empty()
            {
                let idx = path.indices[0];
                if idx < new_locals.len() && path.indices.len() == 1 {
                    let new_local = new_locals[idx];

                    let new_record_id = assigner.next_expr();
                    package.exprs.insert(
                        new_record_id,
                        Expr {
                            id: new_record_id,
                            span: Span::default(),
                            ty: elem_types[idx].clone(),
                            kind: ExprKind::Var(Res::Local(new_local), vec![]),
                            exec_graph_range: EMPTY_EXEC_RANGE,
                        },
                    );

                    let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
                    expr_mut.kind = ExprKind::Assign(new_record_id, value_id);
                }
            }
        }
        _ => {}
    }
}

/// Rewrites all call sites for promoted callables. At each `Call(Var(Item(id)),
/// arg)` where `id` is a promoted callable, replaces the single tuple argument
/// with explicit field extractions wrapped in a `Tuple`.
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
    promotions: &[PromotionResult],
) {
    // Build a set of promoted item IDs for quick lookup.
    let promoted: FxHashSet<LocalItemId> = promotions.iter().map(|p| p.item_id).collect();

    // Collect all call-site ExprIds that target a promoted callable.
    let call_sites: Vec<(ExprId, LocalItemId)> = package
        .exprs
        .iter()
        .filter_map(|(expr_id, expr)| {
            if let ExprKind::Call(callee_id, _) = &expr.kind {
                let callee = package.exprs.get(*callee_id)?;
                if let ExprKind::Var(Res::Item(item_id), _) = &callee.kind
                    && item_id.package == package_id
                    && promoted.contains(&item_id.item)
                {
                    return Some((expr_id, item_id.item));
                }
            }
            None
        })
        .collect();

    for (call_expr_id, item_id) in call_sites {
        let promotion = promotions
            .iter()
            .find(|p| p.item_id == item_id)
            .expect("promotion should exist for promoted item");
        rewrite_single_call_site(package, assigner, call_expr_id, promotion);
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
                name: Rc::from("__arg_promote_tmp"),
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
/// a call site, pairing each projected sub-expression with the type slot
/// expected by the promoted callable signature.
///
/// # Before
/// ```text
/// (no expression)
/// ```
/// # After
/// ```text
/// Tuple([Field(arg, Path([0])), ..., Field(arg, Path([n-1]))])
/// ```
///
/// # Mutations
/// - Allocates per-element `Field` `Expr` nodes and the outer `Tuple`
///   `Expr` through `assigner`.
fn create_projected_tuple_arg(
    package: &mut Package,
    assigner: &mut Assigner,
    promotion: &PromotionResult,
    arg_id: ExprId,
    arg_ty: &Ty,
    temp_local: Option<LocalVarId>,
) -> ExprId {
    let n = promotion.elem_types.len();
    let mut field_expr_ids: Vec<ExprId> = Vec::with_capacity(n);

    for i in 0..n {
        let field_base_id = if let Some(temp_local) = temp_local {
            create_local_var_expr(package, assigner, temp_local, arg_ty)
        } else {
            arg_id
        };
        let field_expr_id = assigner.next_expr();
        let field_expr = qsc_fir::fir::Expr {
            id: field_expr_id,
            span: Span::default(),
            ty: promotion.elem_types[i].clone(),
            kind: ExprKind::Field(field_base_id, Field::Path(FieldPath { indices: vec![i] })),
            exec_graph_range: EMPTY_EXEC_RANGE,
        };
        package.exprs.insert(field_expr_id, field_expr);
        field_expr_ids.push(field_expr_id);
    }

    let new_arg_id = assigner.next_expr();
    let tuple_ty = Ty::Tuple(promotion.elem_types.clone());
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

    // If the argument is already a tuple literal with matching arity,
    // the call site is already structured correctly.
    if let ExprKind::Tuple(elems) = &arg_expr.kind
        && elems.len() == promotion.elem_types.len()
    {
        return;
    }

    if promotion.elem_types.len() == 1 {
        let new_arg_id = assigner.next_expr();
        let new_arg = qsc_fir::fir::Expr {
            id: new_arg_id,
            span: Span::default(),
            ty: Ty::Tuple(promotion.elem_types.clone()),
            kind: ExprKind::Tuple(vec![arg_id]),
            exec_graph_range: EMPTY_EXEC_RANGE,
        };
        package.exprs.insert(new_arg_id, new_arg);

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
fn normalize_call_arg_types(package: &mut Package, package_id: PackageId, assigner: &mut Assigner) {
    let call_sites: Vec<(ExprId, Ty)> = package
        .exprs
        .iter()
        .filter_map(|(_, expr)| {
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
    let callee = package.get_expr(callee_id);
    if let ExprKind::Var(Res::Item(item_id), _) = &callee.kind
        && item_id.package == package_id
    {
        let item = package.items.get(item_id.item)?;
        if let ItemKind::Callable(decl) = &item.kind {
            return Some(package.get_pat(decl.input).ty.clone());
        }
    }

    if let Ty::Arrow(arrow) = &callee.ty {
        return Some((*arrow.input).clone());
    }

    None
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
        if matches!(&arg.kind, ExprKind::Tuple(items) if items.len() == 1) {
            return;
        }
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
