// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defunctionalization pass — runs after return unification, before UDT
//! erasure.
//!
//! Eliminates all callable-valued expressions — arrow-typed locals, closures,
//! and functor-applied callable values — in entry-reachable code. Required for
//! QIR, which mandates direct calls to known callees.
//!
//! # What to know before diving in
//!
//! - **Specialization, not classical defunctionalization.** Instead of a
//!   tagged union plus an `apply` dispatcher, each higher-order-function (HOF)
//!   call site whose concrete callable argument is known at compile time gets
//!   its own specialized clone of the HOF with the callable parameter replaced
//!   by a direct call. `Apply(q => Y(q), target)` becomes a call to a
//!   `Apply_specialized_Y` clone. Single-bound tuple parameters containing
//!   callable values are handled via a split locator (top-level slot + nested
//!   field path).
//! - **Establishes [`crate::invariants::InvariantLevel::PostDefunc`]:** no
//!   `ExprKind::Closure`, no arrow-typed parameters, and all dispatch is
//!   direct in reachable code.
//! - **Fixpoint loop.** Each iteration runs: pre-pass (promote single-use
//!   callable locals, collapse identity closures `(a) => f(a)` to `f`) →
//!   analysis (find callable params + concrete call sites) → specialize (clone
//!   per concrete arg combo, deduped by [`types::SpecKey`]) → rewrite (redirect
//!   call sites, drop the callable arg, thread captures as extra args) →
//!   closure tracking/cleanup. **Closure cleanup is convergence-critical:** it
//!   replaces consumed closures with `Tuple([])` so they stop counting as
//!   work. The iteration cap is scaled dynamically between [`MIN_ITERATIONS`]
//!   and [`MAX_ITERATIONS`].
//!   Non-convergence appends [`Error::FixpointNotReached`] only if no other
//!   diagnostic fired (so a real earlier error is not buried).
//! - **Diagnostics:** [`Error::ExcessiveSpecializations`] is a non-fatal
//!   warning; other errors are fatal because the intermediate FIR may violate
//!   downstream invariants.
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] repairs exec graphs later.

mod analysis;
mod prepass;
mod rewrite;
mod specialize;
pub mod types;

pub use types::Error;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::fir_builder::reachable_local_callables;
use crate::package_assigners::PackageAssigners;
use crate::reachability::{collect_reachable_from_entry, collect_reachable_package_closure};
use crate::walk_utils::collect_expr_ids_in_entry_and_local_callables;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::fir::{
    ExprId, ExprKind, ItemKind, LocalItemId, Package, PackageId, PackageLookup, PackageStore, Res,
    StoreItemId,
};
use qsc_fir::ty::Ty;
use rustc_hash::{FxHashMap, FxHashSet};
use types::{
    AnalysisResult, CallSite, CallableParam, ConcreteCallable, ConcreteCallableKey, SpecKey,
    peel_body_functors,
};

/// Lower bound on the analysis → specialize → rewrite iteration limit.
///
/// The loop always runs at least this many iterations. After the first
/// iteration the limit is recomputed (see [`check_convergence`]) as
/// `max(callable_params.len(), remaining_count).clamp(MIN_ITERATIONS, MAX_ITERATIONS)`,
/// so the floor of 5 gives one iteration of margin beyond the deepest observed
/// HOF chain (4 levels in the chemistry library's Trotter simulation pipeline).
const MIN_ITERATIONS: usize = 5;

/// Upper bound on the dynamically-computed iteration limit, capping the work
/// for pathological programs.
const MAX_ITERATIONS: usize = 20;

/// Defunctionalizes all callable-valued expressions in the entry-reachable
/// portion of a package.
///
/// After this pass:
/// - No `ExprKind::Closure` nodes remain in reachable code.
/// - No arrow-typed parameters remain in reachable callable declarations.
/// - All indirect callable dispatch is replaced with direct dispatch calls.
///
/// Returns diagnostics encountered during defunctionalization.
///
/// # Requires
/// - Package with `package_id` has an entry expression
///
/// [`Error::ExcessiveSpecializations`] is a non-fatal warning. Other
/// diagnostics are fatal to the production pipeline because the intermediate
/// FIR may not satisfy downstream invariants.
///
/// # Panics
///
/// Panics if the package has no entry expression. The reachability scans
/// in this pass go through [`collect_reachable_from_entry`], which asserts
/// `package.entry.is_some()`.
pub(crate) fn defunctionalize(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) -> Vec<Error> {
    let mut errors: Vec<Error> = Vec::new();
    let mut warnings: Vec<Error> = Vec::new();
    // Start at the floor; `check_convergence` raises this to the dynamically
    // computed limit after the first iteration, once the analysis has reported
    // how many callable values actually need resolving.
    let mut max_iterations = MIN_ITERATIONS;
    let mut iteration_count = 0;
    let mut specialized_closure_targets: FxHashSet<StoreItemId> = FxHashSet::default();
    let mut specialized_items: FxHashSet<StoreItemId> = FxHashSet::default();

    // Direct call sites whose `Var(Res::Local)` callee resolved to `Dynamic` on
    // the most recent iteration. Refreshed every pass; surfaced as diagnostics
    // only if the loop terminates with work remaining (see
    // `emit_fixpoint_error`), so transient forwarding calls resolved by a later
    // specialization never reach that terminal state.
    let mut unresolved_direct_call_sites: Vec<ExprId> = Vec::new();

    // Capture the initial callable-value count for before/after progress
    // tracking, mirroring LLVM's DevirtSCCRepeatedPass: detect when an
    // iteration fails to reduce the remaining work set.
    let (_, mut prev_remaining_count, _) = remaining_callable_value_info(store, package_id);

    while iteration_count < max_iterations {
        iteration_count += 1;

        // Clear DynamicCallable errors from prior iterations. They are
        // re-discovered each pass by the HOF path; transient ones (e.g.
        // parameter forwarding like `Inner(op, q)` in a not-yet-specialized
        // HOF) disappear once the outer HOF is specialized, so only the final
        // iteration's emissions survive.
        errors.retain(|e| !matches!(e, Error::DynamicCallable(_)));

        let reachable = collect_reachable_from_entry(store, package_id);

        let (_, reachable_expr_ids) = collect_reachable_scope(store, package_id, &reachable);

        // Simplify defunctionalization analysis by eliminating callable
        // indirection patterns and exposing direct call sites.
        let collapsed_spans = prepass::run(store, package_id, &reachable_expr_ids);

        let analysis = analysis::analyze(store, package_id, &reachable, &collapsed_spans);

        // Record (do not yet emit) direct calls whose callee resolved to
        // `Dynamic`; emission is deferred to `emit_fixpoint_error` so calls
        // that are only transiently `Dynamic` never produce spurious errors.
        unresolved_direct_call_sites.clone_from(&analysis.unresolved_direct_call_sites);

        let spec_map = run_specialization(store, &analysis, assigners, &mut errors, &mut warnings);

        // Rewrite call sites and run dead callable-local cleanup even on
        // iterations where no new specializations were discovered. Call sites
        // can live in foreign bodies (e.g. generic HOFs relocated into their
        // owning package by monomorphization), so rewrite runs once per package
        // that owns call sites, each with that package's own assigner.
        rewrite_call_sites(store, package_id, &analysis, &spec_map, assigners);

        track_specialized_closures(
            &analysis,
            &spec_map,
            &mut specialized_closure_targets,
            &mut specialized_items,
        );
        // Closures consumed by specialization can live in foreign bodies (a
        // closure passed to a HOF inside a relocated generic body), so cleanup
        // runs once per package that owns a consumed closure.
        cleanup_consumed_closures_per_package(
            store,
            package_id,
            &reachable,
            &specialized_closure_targets,
            &specialized_items,
        );

        let converged = check_convergence(
            store,
            package_id,
            &analysis,
            iteration_count,
            &mut max_iterations,
            &mut prev_remaining_count,
        );
        if converged {
            break;
        }
    }

    emit_fixpoint_error(
        store,
        package_id,
        iteration_count,
        &unresolved_direct_call_sites,
        &mut errors,
    );
    errors.extend(warnings);

    errors
}

/// Computes the reachable local callable IDs and expression IDs for scoping
/// the prepass and cleanup to entry-reachable code.
fn collect_reachable_scope(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
) -> (Vec<LocalItemId>, Vec<ExprId>) {
    let package = store.get(package_id);
    let local_item_ids: Vec<_> = reachable_local_callables(package, package_id, reachable)
        .map(|(id, _)| id)
        .collect();
    let reachable_expr_ids =
        collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids);
    (local_item_ids, reachable_expr_ids)
}

/// Runs specialization if there are call sites, separating warnings from
/// errors. Returns the specialization map.
fn run_specialization(
    store: &mut PackageStore,
    analysis: &AnalysisResult,
    assigners: &mut PackageAssigners,
    errors: &mut Vec<Error>,
    warnings: &mut Vec<Error>,
) -> FxHashMap<SpecKey, StoreItemId> {
    let (spec_map, mut spec_errors) = if analysis.call_sites.is_empty() {
        (Default::default(), Vec::new())
    } else {
        specialize::specialize(store, analysis, assigners)
    };
    // Separate warnings from errors so the `retain` at the top of each
    // iteration does not discard them.
    warnings.append(
        &mut (spec_errors
            .extract_if(.., |e| matches!(e, Error::ExcessiveSpecializations(..)))
            .collect()),
    );
    spec_errors.retain(|e| !matches!(e, Error::ExcessiveSpecializations(..)));
    errors.append(&mut spec_errors);
    spec_map
}

/// Rewrites call sites in every package that owns one. Call sites can live in
/// foreign bodies (e.g. generic HOFs relocated into their owning package by
/// monomorphization), so rewrite is driven once per owning package using that
/// package's own assigner. The entry package is always rewritten so that
/// iterations with only direct-call cleanup still run.
fn rewrite_call_sites(
    store: &mut PackageStore,
    package_id: PackageId,
    analysis: &AnalysisResult,
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    assigners: &mut PackageAssigners,
) {
    let mut packages: Vec<PackageId> = vec![package_id];
    for cs in &analysis.call_sites {
        if !packages.contains(&cs.call_pkg_id) {
            packages.push(cs.call_pkg_id);
        }
    }
    for dcs in &analysis.direct_call_sites {
        if !packages.contains(&dcs.call_pkg_id) {
            packages.push(dcs.call_pkg_id);
        }
    }

    for pkg_id in packages {
        let assigner = assigners.get_mut(store, pkg_id);
        let package = store.get_mut(pkg_id);
        rewrite::rewrite(package, pkg_id, analysis, spec_map, assigner);
    }
}

/// Records which closure targets were consumed by specialization or direct-call
/// rewrite in this iteration.
fn track_specialized_closures(
    analysis: &AnalysisResult,
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    specialized_closure_targets: &mut FxHashSet<StoreItemId>,
    specialized_items: &mut FxHashSet<StoreItemId>,
) {
    for cs in &analysis.call_sites {
        let spec_key = build_spec_key(cs);
        if spec_map.contains_key(&spec_key)
            && let ConcreteCallable::Closure { target, .. } = &cs.callable_arg
        {
            specialized_closure_targets.insert(StoreItemId::from((cs.call_pkg_id, *target)));
        }
    }
    for direct_call_site in &analysis.direct_call_sites {
        if let ConcreteCallable::Closure { target, .. } = &direct_call_site.callable {
            specialized_closure_targets
                .insert(StoreItemId::from((direct_call_site.call_pkg_id, *target)));
        }
    }
    specialized_items.extend(spec_map.values().copied());
}

/// Checks whether the fixed-point loop should terminate. Returns `true` when
/// the loop should break (converged or stuck).
fn check_convergence(
    store: &PackageStore,
    package_id: PackageId,
    analysis: &AnalysisResult,
    iteration_count: usize,
    max_iterations: &mut usize,
    prev_remaining_count: &mut usize,
) -> bool {
    let (has_remaining, remaining_count, _) = remaining_callable_value_info(store, package_id);

    let made_progress = remaining_count < *prev_remaining_count || !analysis.call_sites.is_empty();
    *prev_remaining_count = remaining_count;

    // On the first iteration, compute a dynamic iteration limit based on
    // the number of remaining callable values discovered.
    if iteration_count == 1 {
        *max_iterations = analysis
            .callable_params
            .len()
            .max(remaining_count)
            .clamp(MIN_ITERATIONS, MAX_ITERATIONS);
    }

    if !has_remaining {
        return true;
    }

    // No progress was made — the loop is stuck. Break out and let
    // `emit_fixpoint_error` report the remaining callable values.
    if !made_progress {
        return true;
    }

    false
}

/// Emits a convergence diagnostic if callable values remain after the loop
/// exits. If any unresolved direct call had a statically-unresolvable callee,
/// emits an actionable `DynamicCallable` per such call site; otherwise falls
/// back to `FixpointNotReached`. `unresolved_direct_call_sites` reflects the
/// terminal iteration, so transiently-`Dynamic` calls are never surfaced.
fn emit_fixpoint_error(
    store: &PackageStore,
    package_id: PackageId,
    iteration_count: usize,
    unresolved_direct_call_sites: &[ExprId],
    errors: &mut Vec<Error>,
) {
    let (has_remaining, remaining_count, span) = remaining_callable_value_info(store, package_id);
    if has_remaining && errors.is_empty() {
        if unresolved_direct_call_sites.is_empty() {
            errors.push(Error::FixpointNotReached(
                iteration_count,
                remaining_count,
                span,
            ));
        } else {
            let package = store.get(package_id);
            for &call_expr_id in unresolved_direct_call_sites {
                errors.push(Error::DynamicCallable(package.get_expr(call_expr_id).span));
            }
        }
    }
}

/// Runs [`cleanup_consumed_closures`] over every package in the entry-reachable
/// closure that owns a consumed closure. Consumed closures can live in foreign
/// bodies (a closure passed to a HOF inside a relocated generic body), so the
/// cross-package `specialized_targets` / `skip_items` sets are projected to each
/// package's local item ids before running the single-package cleanup there.
fn cleanup_consumed_closures_per_package(
    store: &mut PackageStore,
    entry_pkg_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    specialized_targets: &FxHashSet<StoreItemId>,
    skip_items: &FxHashSet<StoreItemId>,
) {
    if specialized_targets.is_empty() {
        return;
    }

    for pkg_id in collect_reachable_package_closure(entry_pkg_id, reachable) {
        let targets_local: FxHashSet<LocalItemId> = specialized_targets
            .iter()
            .filter(|s| s.package == pkg_id)
            .map(|s| s.item)
            .collect();
        if targets_local.is_empty() {
            continue;
        }
        let skip_local: FxHashSet<LocalItemId> = skip_items
            .iter()
            .filter(|s| s.package == pkg_id)
            .map(|s| s.item)
            .collect();
        let local_item_ids: Vec<LocalItemId> = {
            let package = store.get(pkg_id);
            reachable_local_callables(package, pkg_id, reachable)
                .map(|(id, _)| id)
                .collect()
        };
        let package = store.get_mut(pkg_id);
        cleanup_consumed_closures(
            package,
            pkg_id,
            &targets_local,
            &skip_local,
            &local_item_ids,
        );
    }
}

/// Replaces all remaining closure expressions whose target callable was
/// consumed by specialization with Unit values, clearing references so
/// subsequent iterations do not count them as work remaining.
///
/// A closure is "consumed" when its target callable has been specialized, so
/// the HOF call site that passed it has been rewritten to a direct call. The
/// closure node in the producer body is now dead, but
/// `remaining_callable_value_info` would still count it as work remaining,
/// causing false convergence failure.
///
/// Only closures that are not direct children of a `Call` argument subtree
/// are eligible for cleanup. Closures that are still live as arguments to a
/// call expression (e.g., in a multi-param HOF where only one param has been
/// specialized so far) must survive to the next iteration.
///
/// UDT-constructor `Call`s are an exception: their argument subtree is a
/// structural wrapper, not a live HOF argument, so closures inside it remain
/// eligible for cleanup.
///
/// Rewrites `Expr.kind` to `Tuple([])` and `Expr.ty` to `Unit` for consumed
/// closure expressions outside call-argument subtrees.
///
/// Closures inside `skip_items` (callables specialized this iteration) are
/// left untouched, since their bodies are freshly cloned and handled on a
/// subsequent pass.
///
/// # Returns
///
/// The number of closure expressions replaced.
fn cleanup_consumed_closures(
    package: &mut Package,
    package_id: PackageId,
    specialized_targets: &FxHashSet<LocalItemId>,
    skip_items: &FxHashSet<LocalItemId>,
    reachable_item_ids: &[LocalItemId],
) -> usize {
    if specialized_targets.is_empty() {
        return 0;
    }

    // First pass: collect the ExprIds of all call-argument subtrees. Closures
    // inside them are still live HOF arguments; UDT-constructor Calls are
    // skipped because their argument is a structural wrapper.
    let mut call_arg_exprs: FxHashSet<ExprId> = FxHashSet::default();
    for &item_id in reachable_item_ids {
        if skip_items.contains(&item_id) {
            continue;
        }
        let item = package.get_item(item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_expr_id, expr| {
                    if let ExprKind::Call(callee_id, args_id) = &expr.kind
                        && !is_udt_ctor_call(package, package_id, *callee_id)
                    {
                        collect_all_expr_ids(package, *args_id, &mut call_arg_exprs);
                    }
                },
            );
        }
    }
    if let Some(entry_id) = package.entry {
        crate::walk_utils::for_each_expr(package, entry_id, &mut |_expr_id, expr| {
            if let ExprKind::Call(callee_id, args_id) = &expr.kind
                && !is_udt_ctor_call(package, package_id, *callee_id)
            {
                collect_all_expr_ids(package, *args_id, &mut call_arg_exprs);
            }
        });
    }

    // Second pass: collect consumed closures that are not in call argument
    // positions.
    let mut to_replace: Vec<ExprId> = Vec::new();
    for &item_id in reachable_item_ids {
        if skip_items.contains(&item_id) {
            continue;
        }
        let item = package.get_item(item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, expr| {
                    if let ExprKind::Closure(_, target) = &expr.kind
                        && specialized_targets.contains(target)
                        && !call_arg_exprs.contains(&expr_id)
                    {
                        to_replace.push(expr_id);
                    }
                },
            );
        }
    }

    if let Some(entry_id) = package.entry {
        crate::walk_utils::for_each_expr(package, entry_id, &mut |expr_id, expr| {
            if let ExprKind::Closure(_, target) = &expr.kind
                && specialized_targets.contains(target)
                && !call_arg_exprs.contains(&expr_id)
            {
                to_replace.push(expr_id);
            }
        });
    }

    let count = to_replace.len();
    for expr_id in to_replace {
        let expr = package.exprs.get_mut(expr_id).expect("expr must exist");
        expr.kind = ExprKind::Tuple(Vec::new());
        expr.ty = Ty::UNIT;
    }

    count
}

/// Returns true when the given callee expression resolves to a same-package
/// UDT constructor (i.e. an `ItemKind::Ty`). Conservative: returns false for
/// cross-package callees and any non-`Var(Res::Item(_))` callee shape.
fn is_udt_ctor_call(package: &Package, package_id: PackageId, callee_id: ExprId) -> bool {
    let callee = package.get_expr(callee_id);
    if let ExprKind::Var(Res::Item(item_id), _) = &callee.kind
        && item_id.package == package_id
    {
        matches!(package.get_item(item_id.item).kind, ItemKind::Ty(_, _))
    } else {
        false
    }
}

/// Recursively collects all `ExprId`s reachable from an expression node.
fn collect_all_expr_ids(package: &Package, expr_id: ExprId, ids: &mut FxHashSet<ExprId>) {
    crate::walk_utils::for_each_expr(package, expr_id, &mut |child_id, _| {
        ids.insert(child_id);
    });
}

/// Checks whether any reachable callable value still requires
/// defunctionalization work.
///
/// Returns `(has_remaining, count, first_span)` in a single reachability scan.
fn remaining_callable_value_info(
    store: &PackageStore,
    package_id: PackageId,
) -> (bool, usize, Span) {
    let reachable = collect_reachable_from_entry(store, package_id);
    let mut count = 0;
    let mut first_span = Span::default();

    let mut record_remaining = |span: Span| {
        if count == 0 {
            first_span = span;
        }
        count += 1;
    };

    // Walk every reachable callable in its owning package. Defunctionalization
    // specializes higher-order functions in place, including generic standard
    // library HOFs relocated into their owning package by monomorphization, so
    // a foreign callable that still carries an arrow-typed parameter, a
    // closure, or an indirect call through an arrow-typed local is genuine
    // pending work: the loop must keep running until the concrete-argument call
    // site rewrites the caller to a specialized clone and the un-specialized
    // HOF drops out of the reachable closure. Restricting this scan to the
    // entry package falsely reports convergence while foreign HOFs are still
    // pending, leaving their concrete call sites unresolved.
    for store_id in &reachable {
        let package = store.get(store_id.package);
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let input_pat = package.get_pat(decl.input);
            if ty_contains_arrow_through_udts(store, &input_pat.ty) {
                record_remaining(input_pat.span);
            }

            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_expr_id, expr| {
                    if matches!(expr.kind, ExprKind::Closure(_, _)) {
                        record_remaining(expr.span);
                    }
                    // Count indirect calls through arrow-typed local variables.
                    // After defunc iteration 1 specializes HOFs and removes callable
                    // parameters, conditional callable bindings like
                    //   let u = if power >= 0 { op } else { Adjoint op };
                    //   u(target);
                    // leave arrow-typed locals with indirect Call expressions.
                    // The existing branch-split infrastructure resolves these in
                    // a subsequent iteration, but only if the convergence check
                    // reports them as remaining.
                    if let ExprKind::Call(callee_id, _) = &expr.kind {
                        let (base_id, _) = peel_body_functors(package, *callee_id);
                        let base_expr = package.get_expr(base_id);
                        if matches!(base_expr.kind, ExprKind::Var(Res::Local(_), _))
                            && ty_contains_arrow(&base_expr.ty)
                        {
                            record_remaining(base_expr.span);
                        }
                    }
                },
            );
        }
    }

    let package = store.get(package_id);
    if let Some(entry_id) = package.entry {
        crate::walk_utils::for_each_expr(package, entry_id, &mut |_expr_id, expr| {
            if matches!(expr.kind, ExprKind::Closure(_, _)) {
                record_remaining(expr.span);
            }
            // Same indirect-call check as callable body walker.
            if let ExprKind::Call(callee_id, _) = &expr.kind {
                let (base_id, _) = peel_body_functors(package, *callee_id);
                let base_expr = package.get_expr(base_id);
                if matches!(base_expr.kind, ExprKind::Var(Res::Local(_), _))
                    && ty_contains_arrow(&base_expr.ty)
                {
                    record_remaining(base_expr.span);
                }
            }
        });
    }

    (count > 0, count, first_span)
}

/// Checks whether a type contains an arrow type anywhere within its structure.
///
/// This intentionally does not recurse into `Ty::Udt` or `Ty::Array`:
///
/// - **`Ty::Udt`**: Defunc runs before UDT erasure, so UDT wrappers are still
///   opaque here. Callable values inside UDTs are handled at the *expression*
///   level by the analysis phase (`extract_arrow_params_from_ty` also ignores
///   `Ty::Udt`, but `build_callable_flow_state` tracks field-extraction
///   expressions like `config.Op` to resolve concrete callable values). After
///   defunc, callable values are either specialized or rejected as
///   `DynamicCallable`. Post-UDT-erasure passes (tuple-decompose, `arg_promote`) may expose
///   bare `Ty::Arrow` parameters, but partial eval handles them correctly
///   because it dispatches on *values* (`Value::Global` / `Value::Closure`),
///   not on the `Ty::Arrow` type annotation.
///
/// - **`Ty::Array`**: Array-of-callable parameters (`(Qubit => Unit)[]`) are
///   dynamically indexed, so defunc cannot specialize them. Ignoring
///   `Ty::Array` is consistent with defunc's capabilities.
///
/// A separate copy of this function in `codegen.rs` does handle `Ty::Array`
/// for codegen routing; unifying the two is unnecessary because their
/// contexts differ.
pub(crate) fn ty_contains_arrow(ty: &Ty) -> bool {
    match ty {
        Ty::Arrow(_) => true,
        Ty::Tuple(tys) => tys.iter().any(ty_contains_arrow),
        _ => false,
    }
}

/// Checks whether a type contains an arrow, expanding UDT pure types recursively.
///
/// The defunctionalization fixpoint uses this for reachable callable inputs so a
/// callable whose parameter is a UDT containing a callable field keeps the loop
/// running until that nested callable field is specialized. The rewrite helpers
/// still use `ty_contains_arrow`, where UDTs intentionally remain opaque.
fn ty_contains_arrow_through_udts(store: &PackageStore, ty: &Ty) -> bool {
    match ty {
        Ty::Arrow(_) => true,
        Ty::Tuple(tys) => tys
            .iter()
            .any(|ty| ty_contains_arrow_through_udts(store, ty)),
        Ty::Udt(Res::Item(item_id)) => {
            let package = store.get(item_id.package);
            let item = package.get_item(item_id.item);
            let ItemKind::Ty(_, udt) = &item.kind else {
                return false;
            };
            ty_contains_arrow_through_udts(store, &udt.get_pure_ty())
        }
        _ => false,
    }
}

/// Builds the deduplication key for a call site's specialization.
pub(crate) fn build_spec_key(call_site: &CallSite) -> SpecKey {
    let concrete_key = match &call_site.callable_arg {
        ConcreteCallable::Global { item_id, functor } => ConcreteCallableKey::Global {
            item_id: *item_id,
            functor: *functor,
        },
        ConcreteCallable::Closure {
            target, functor, ..
        } => ConcreteCallableKey::Closure {
            target: StoreItemId::from((call_site.call_pkg_id, *target)),
            functor: *functor,
        },
        ConcreteCallable::Dynamic => {
            // Dynamic callables are filtered out before reaching here, but
            // provide a deterministic key regardless.
            ConcreteCallableKey::Global {
                item_id: call_site.hof_item_id,
                functor: FunctorApp::default(),
            }
        }
    };
    SpecKey {
        hof_id: StoreItemId::from((call_site.hof_item_id.package, call_site.hof_item_id.item)),
        concrete_args: vec![concrete_key],
    }
}

/// Builds the index path from a call's argument tuple to the position of
/// a callable parameter, accounting for functor control wrappers and
/// tuple-patterned inputs.
pub(crate) fn build_param_input_path(
    uses_tuple_input: bool,
    param: &CallableParam,
    functor: FunctorApp,
) -> Vec<usize> {
    let mut path = vec![1; usize::from(functor.controlled)];
    if uses_tuple_input {
        path.push(param.top_level_param);
    }
    path.extend(param.field_path.iter().copied());
    path
}
