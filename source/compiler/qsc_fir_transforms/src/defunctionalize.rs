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
//!   its own specialized clone of the HOF, with the callable parameter replaced
//!   by a direct call. `Apply(q => Y(q), target)` becomes a call to an
//!   `Apply_specialized_Y` clone. A callable value nested inside a single tuple
//!   parameter is located by a top-level parameter slot plus a nested field
//!   path.
//! - **Establishes [`crate::invariants::InvariantLevel::PostDefunc`]:** no
//!   `ExprKind::Closure`, no arrow-typed parameters, and all dispatch is
//!   direct in reachable code.
//! - **Fixpoint loop.** Each iteration runs five steps in order. The pre-pass
//!   promotes single-use callable locals and collapses identity closures such
//!   as `(a) => f(a)` down to `f`. Analysis finds callable parameters and
//!   concrete call sites. Specialize clones a HOF once per concrete argument
//!   combination, deduplicated by [`types::SpecKey`]. Rewrite redirects call
//!   sites, drops the callable argument, and threads captured values through as
//!   extra arguments. A final closure-cleanup step is convergence-critical: it
//!   replaces consumed closures with `Tuple([])` so they stop counting as
//!   remaining work. The iteration cap scales dynamically between
//!   `MIN_ITERATIONS` and `MAX_ITERATIONS`. Non-convergence appends
//!   [`Error::FixpointNotReached`], but only when no other diagnostic already
//!   fired, so a real earlier error is not buried.
//! - **Diagnostics:** [`Error::ExcessiveSpecializations`] is a non-fatal
//!   warning. Other errors are fatal because the intermediate FIR may violate
//!   downstream invariants.
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   `crate::exec_graph_rebuild` repairs exec graphs later.

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
    ExprId, ExprKind, ItemId, ItemKind, LocalItemId, Package, PackageId, PackageLookup,
    PackageStore, Res, StoreItemId,
};
use qsc_fir::ty::Ty;
use rustc_hash::{FxHashMap, FxHashSet};
use types::{
    AnalysisResult, CallSite, CallableParam, ConcreteCallable, ConcreteCallableKey, SpecKey,
    peel_body_functors,
};

/// Lower bound on the analysis => specialize => rewrite iteration limit.
///
/// The loop always runs at least this many iterations. After the first
/// iteration [`check_convergence`] recomputes the limit as
/// `max(callable_params.len(), remaining_count).clamp(MIN_ITERATIONS, MAX_ITERATIONS)`.
/// The floor of 5 gives one iteration of margin beyond the deepest HOF chain
/// seen in practice, which is the four-level Trotter simulation pipeline in the
/// chemistry library.
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
        // can live in foreign bodies so rewrite runs once per package
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

    // A `UnsupportedMultipleCallableArrays` guard skips its offending group, so
    // the callable arrays that group would have specialized stay unresolved and
    // their forwarding consumers (e.g. an inner HOF call taking the still-
    // abstract array parameters) surface as generic `DynamicCallable`
    // diagnostics. Those are downstream consequences of the guarded shape, so
    // drop them and report only the specific root-cause diagnostic, mirroring
    // how `emit_fixpoint_error` withholds a generic non-convergence report once
    // a more actionable error has already fired.
    if errors
        .iter()
        .any(|e| matches!(e, Error::UnsupportedMultipleCallableArrays(_)))
    {
        errors.retain(|e| !matches!(e, Error::DynamicCallable(_)));
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
    // `UnsupportedMultipleCallableArrays` is intentionally not swept by the
    // per-iteration `DynamicCallable` retain, so the guarded group re-reports it
    // every fixpoint iteration. Drop any whose span already survives in `errors`
    // so a single diagnostic persists across iterations rather than one copy per
    // pass.
    spec_errors.retain(|e| match e {
        Error::UnsupportedMultipleCallableArrays(span) => !errors.iter().any(
            |existing| matches!(existing, Error::UnsupportedMultipleCallableArrays(s) if s == span),
        ),
        _ => true,
    });
    errors.append(&mut spec_errors);
    spec_map
}

/// Rewrites call sites in every package that owns one. Call sites can live in
/// foreign bodies so rewrite is driven once per owning package using that
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
    // Group by package and call expression once, shared by the consistency
    // check below and the combined-keying registration. Grouping matches the
    // specializer's grouping and stays correct when call sites span packages.
    let mut groups: FxHashMap<(PackageId, ExprId), Vec<&CallSite>> = FxHashMap::default();
    for cs in &analysis.call_sites {
        groups
            .entry((cs.call_pkg_id, cs.call_expr_id))
            .or_default()
            .push(cs);
    }

    // Single-arg keying: records producer closures consumed by branch-split /
    // condition-dispatch specializations, whose per-candidate specs are keyed
    // individually.
    for cs in &analysis.call_sites {
        let spec_key = build_spec_key(cs);
        if spec_map.contains_key(&spec_key)
            && let ConcreteCallable::Closure { target, .. } = &cs.callable_arg
        {
            // Internal consistency check. When a producer-closure argument is a
            // single-valued sibling of a parameter that is dispatched over
            // several candidates, recording it as consumed here would let
            // `cleanup_consumed_closures` clear its producer body while the
            // dispatched siblings are still live, un-inlined call sites. The
            // next iteration
            // would then re-read the cleared body as `Dynamic` and the call
            // would compile to incorrect output. The combined per-candidate
            // specialization handles this shape instead, so this
            // single-argument per-row specialization should never exist for it.
            // If it does, that specialization did not run, so stop with a clear
            // error rather than emitting incorrect QIR.
            if let Some(group) = groups.get(&(cs.call_pkg_id, cs.call_expr_id))
                && closure_constant_sibling_of_dispatch(group, cs)
            {
                panic!(
                    "internal error in defunctionalize: producer-closure target {target:?} is a \
                     single-valued sibling of a parameter dispatched over several candidates at \
                     call expression {:?} in package {:?}, but is being recorded as consumed via \
                     its own per-row specialization without combined specialization. Clearing its \
                     producer body now would leave the dispatched siblings referring to a removed \
                     body and produce incorrect output.",
                    cs.call_expr_id, cs.call_pkg_id,
                );
            }
            specialized_closure_targets.insert(StoreItemId::from((cs.call_pkg_id, *target)));
        }
    }
    // Combined keying: a multi-arrow-param call produces one specialization
    // keyed by the combined key, so every participating producer body must be
    // recorded under that combined key. The combined and single-arg key spaces
    // are disjoint by argument count, so this is additive: missing a member
    // here would leave a stray `Closure` that `exec_graph_rebuild` rejects.
    for group in groups.values() {
        let combined_key = build_combined_spec_key_for_group(group[0].hof_item_id, group);
        if spec_map.contains_key(&combined_key) {
            for cs in group {
                if let ConcreteCallable::Closure { target, .. } = &cs.callable_arg {
                    specialized_closure_targets
                        .insert(StoreItemId::from((cs.call_pkg_id, *target)));
                }
            }
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
/// the loop should break, either because it has converged or because it is
/// stuck.
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

    // A freshly specialized item can still be the only live path to a producer
    // in the same iteration. Defer that producer so cleanup does not erase the
    // body before the next specialization pass can inline it.
    let deferred_items = items_called_from_skipped_items(store, skip_items);

    for pkg_id in collect_reachable_package_closure(entry_pkg_id, reachable) {
        let targets_local: FxHashSet<LocalItemId> = specialized_targets
            .iter()
            .filter(|s| s.package == pkg_id)
            .map(|s| s.item)
            .collect();
        if targets_local.is_empty() {
            continue;
        }
        let mut skip_local: FxHashSet<LocalItemId> = skip_items
            .iter()
            .filter(|s| s.package == pkg_id)
            .map(|s| s.item)
            .collect();
        skip_local.extend(
            deferred_items
                .iter()
                .filter(|s| s.package == pkg_id)
                .map(|s| s.item),
        );
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

/// Finds direct callees used by freshly specialized items so their producer
/// bodies survive until the next defunctionalization iteration.
fn items_called_from_skipped_items(
    store: &PackageStore,
    skip_items: &FxHashSet<StoreItemId>,
) -> FxHashSet<StoreItemId> {
    let mut called_items = FxHashSet::default();

    for skipped_item in skip_items {
        let package = store.get(skipped_item.package);
        let item = package.get_item(skipped_item.item);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_expr_id, expr| {
                    if let ExprKind::Call(callee_id, _) = &expr.kind {
                        let (base_id, _) = peel_body_functors(package, *callee_id);
                        if let ExprKind::Var(Res::Item(item_id), _) =
                            &package.get_expr(base_id).kind
                        {
                            called_items.insert(StoreItemId::from((item_id.package, item_id.item)));
                        }
                    }
                },
            );
        }
    }

    called_items
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

/// Maps a single concrete callable argument to its hashable dedup key.
///
/// Closures are keyed only by their package-qualified target and functor;
/// captured values are threaded as ordinary call arguments and are not part of
/// the dispatch identity. A `Dynamic` argument is filtered out before reaching
/// specialization but still yields a deterministic key.
fn concrete_callable_key(
    call_pkg_id: PackageId,
    callable_arg: &ConcreteCallable,
    hof_item_id: ItemId,
) -> ConcreteCallableKey {
    match callable_arg {
        ConcreteCallable::Global { item_id, functor } => ConcreteCallableKey::Global {
            item_id: *item_id,
            functor: *functor,
        },
        ConcreteCallable::Closure {
            target, functor, ..
        } => ConcreteCallableKey::Closure {
            target: StoreItemId::from((call_pkg_id, *target)),
            functor: *functor,
            occurrence: None,
        },
        ConcreteCallable::Dynamic => ConcreteCallableKey::Global {
            item_id: hof_item_id,
            functor: FunctorApp::default(),
        },
    }
}

/// Builds the deduplication key for a single call site's specialization. This
/// is the length-1 shim over [`build_combined_spec_key`]; single-arrow-param
/// HOF keys are therefore byte-identical to the pre-combined behavior.
pub(crate) fn build_spec_key(call_site: &CallSite) -> SpecKey {
    build_combined_spec_key(call_site.hof_item_id, &[call_site])
}

/// Builds the combined deduplication key for a group of `Single`-resolved call
/// sites that share one `call_expr_id`, one per arrow parameter of the HOF.
///
/// The group is sorted by `(top_level_param, field_path)` ascending so that the
/// resulting `concrete_args` ordering is deterministic and position-aligned
/// with the parameter order the specialize/rewrite sides consume. Distinct
/// argument combinations therefore map to distinct keys, while identical
/// combinations deduplicate to one specialization, including same-target
/// producer closures whose differing captures are not part of the key.
pub(crate) fn build_combined_spec_key(hof_id: ItemId, group: &[&CallSite]) -> SpecKey {
    build_combined_spec_key_with_occurrences(hof_id, group, false)
}

pub(crate) fn build_combined_spec_key_for_group(hof_id: ItemId, group: &[&CallSite]) -> SpecKey {
    if is_static_callable_array_combined_group(group) {
        build_static_callable_array_combined_spec_key(hof_id, group)
    } else {
        build_combined_spec_key(hof_id, group)
    }
}

pub(crate) fn build_static_callable_array_combined_spec_key(
    hof_id: ItemId,
    group: &[&CallSite],
) -> SpecKey {
    build_combined_spec_key_with_occurrences(hof_id, group, true)
}

fn build_combined_spec_key_with_occurrences(
    hof_id: ItemId,
    group: &[&CallSite],
    preserve_repeated_occurrences: bool,
) -> SpecKey {
    let mut members: Vec<&CallSite> = group.to_vec();
    members.sort_by(|a, b| {
        a.top_level_param
            .cmp(&b.top_level_param)
            .then_with(|| a.field_path.cmp(&b.field_path))
    });
    let mut position_counts: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    if preserve_repeated_occurrences {
        for cs in &members {
            *position_counts
                .entry((cs.top_level_param, cs.field_path.clone()))
                .or_default() += 1;
        }
    }
    let mut occurrences: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    let concrete_args = members
        .iter()
        .map(|cs| {
            let position = (cs.top_level_param, cs.field_path.clone());
            let occurrence = (preserve_repeated_occurrences
                && position_counts.get(&position).copied().unwrap_or_default() > 1)
                .then(|| {
                    let next = occurrences.entry(position).or_default();
                    let value = *next;
                    *next += 1;
                    value
                });
            let mut key = concrete_callable_key(cs.call_pkg_id, &cs.callable_arg, cs.hof_item_id);
            if let ConcreteCallableKey::Closure {
                occurrence: slot, ..
            } = &mut key
            {
                *slot = occurrence;
            }
            key
        })
        .collect();
    SpecKey {
        hof_id: StoreItemId::from((hof_id.package, hof_id.item)),
        concrete_args,
    }
}

pub(crate) fn is_static_callable_array_combined_group(group: &[&CallSite]) -> bool {
    let mut positions: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    for call_site in group {
        *positions
            .entry((call_site.top_level_param, call_site.field_path.clone()))
            .or_default() += 1;
    }
    positions.values().any(|count| *count >= 2)
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

/// Determines whether a group of call sites that share one call expression
/// forms a genuine multi-argument higher-order call eligible for combined
/// specialization, where every arrow parameter is specialized together against
/// one clone in a single fixpoint iteration.
///
/// Both the specialize and rewrite phases consult this predicate so they agree
/// on exactly which call sites are combined. Any disagreement would strand a
/// combined specialization without a matching call-site rewrite, or a rewrite
/// without its specialization. A group qualifies only when all of the following
/// hold:
///
/// - it has at least two members. A single arrow parameter stays on the per-row
///   path, byte-identical to the pre-combined behavior.
/// - every member resolves a static callable with no branch condition and is
///   not `Dynamic`, so branch-split candidate sets keep their dispatch path.
/// - every member supplies a callable for a distinct parameter position, which
///   is its top-level slot plus the field path into any nested tuple. This makes
///   the group a genuine multi-argument call rather than a branch-split
///   candidate set that resolves the same parameter many ways.
/// - the call carries no outer controlled functor, whose nested argument tuple
///   the top-level combined removal does not model.
/// - every nested member, meaning one that selects an arrow field of a
///   tuple-valued parameter, is single-level, and the group covers every field
///   of that parameter's tuple, so the combined removal can drop the whole
///   top-level slot. Partial field coverage such as a surviving non-arrow
///   element, deeper nesting, or a slot whose type does not resolve to a direct
///   tuple keeps the call on the per-row path.
///
/// `package` must own `group`'s shared call expression.
pub(super) fn is_combined_eligible(package: &Package, group: &[&CallSite]) -> bool {
    if group.len() < 2 {
        return false;
    }
    if group
        .iter()
        .any(|s| !s.condition.is_empty() || matches!(s.callable_arg, ConcreteCallable::Dynamic))
    {
        return false;
    }
    // Distinct parameter positions mean a genuine multi-argument call rather
    // than a branch-split candidate set that resolves the same parameter many
    // ways. Static candidates for one array-of-arrow parameter are the one
    // exception: the array index lives inside the HOF body, so one clone needs
    // all candidates in order to synthesize the in-body dispatch.
    let static_callable_array_group = is_static_callable_array_group(package, group)
        || has_static_top_level_callable_array_position(package, group);
    let mut param_positions: Vec<(usize, &[usize])> = group
        .iter()
        .map(|s| (s.top_level_param, s.field_path.as_slice()))
        .collect();
    param_positions.sort_unstable();
    if !param_positions.windows(2).all(|w| w[0] != w[1]) && !static_callable_array_group {
        return false;
    }
    // An outer controlled functor nests the argument tuple one level per
    // control layer; the combined top-level removal does not model that
    // nesting, so such calls stay on the per-row path.
    let call_expr = package.get_expr(group[0].call_expr_id);
    let ExprKind::Call(callee_id, _) = call_expr.kind else {
        return false;
    };
    let (_, functor) = peel_body_functors(package, callee_id);
    if functor.controlled != 0 {
        return false;
    }
    if static_callable_array_group {
        return true;
    }
    // A member selects either a top-level arrow parameter, identified by an
    // empty field path, or a single immediate arrow field of a tuple-valued
    // parameter. The combined removal drops a whole top-level slot, so a nested
    // member is only eligible when its group covers every field of that slot's
    // tuple; otherwise the surviving fields would be dropped along with the
    // removed ones.
    let Ty::Arrow(ref arrow) = package.get_expr(callee_id).ty else {
        return false;
    };
    let mut nested_fields: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
    let mut uses_tuple_input = false;
    for s in group {
        match s.field_path.as_slice() {
            [] => {}
            [field] => {
                uses_tuple_input = s.hof_input_is_tuple;
                nested_fields
                    .entry(s.top_level_param)
                    .or_default()
                    .push(*field);
            }
            // Deeper nesting is not modeled by the single-level combined
            // removal, so the whole group stays on the per-row path.
            _ => return false,
        }
    }
    let arrow_input = resolve_udt_ty(package, &arrow.input);
    for (slot, mut fields) in nested_fields {
        // For a multi-parameter HOF the arrow input is a tuple of parameters
        // and the tuple-valued parameter sits at `slot`; for a single
        // tuple-valued parameter the arrow input is that tuple.
        let container = if uses_tuple_input {
            match &arrow_input {
                Ty::Tuple(tys) => tys.get(slot),
                _ => None,
            }
        } else {
            Some(&arrow_input)
        };
        let Some(Ty::Tuple(slot_tys)) = container else {
            return false;
        };
        fields.sort_unstable();
        fields.dedup();
        if fields.len() != slot_tys.len() {
            return false;
        }
    }
    true
}

fn is_static_callable_array_group(package: &Package, group: &[&CallSite]) -> bool {
    let Some(first) = group.first() else {
        return false;
    };
    if group.iter().any(|call_site| {
        call_site.top_level_param != first.top_level_param
            || call_site.field_path != first.field_path
            || call_site.hof_input_is_tuple != first.hof_input_is_tuple
            || !call_site.condition.is_empty()
            || matches!(call_site.callable_arg, ConcreteCallable::Dynamic)
    }) {
        return false;
    }

    let call_expr = package.get_expr(first.call_expr_id);
    let ExprKind::Call(callee_id, _) = call_expr.kind else {
        return false;
    };
    let Ty::Arrow(ref arrow) = package.get_expr(callee_id).ty else {
        return false;
    };

    let arrow_input = resolve_udt_ty(package, &arrow.input);
    let container = if first.hof_input_is_tuple {
        match &arrow_input {
            Ty::Tuple(tys) => tys.get(first.top_level_param),
            _ => None,
        }
    } else {
        Some(&arrow_input)
    };

    let Some(container) = container else {
        return false;
    };
    let selected_ty = first
        .field_path
        .iter()
        .try_fold(container, |ty, index| match ty {
            Ty::Tuple(tys) => tys.get(*index),
            _ => None,
        });
    matches!(selected_ty, Some(Ty::Array(item_ty)) if matches!(item_ty.as_ref(), Ty::Arrow(_)))
}

/// Returns every repeated top-level position in `group`, regardless of the
/// forwarded value's type.
///
/// A position is `(top_level_param, field_path)`; it is *repeated* when two or
/// more members populate it. This is the raw grouping the callable-array
/// analysis builds on before any type filter, letting callers reason about
/// *all* repeated positions rather than only the callable-array ones.
///
/// Returns an empty vector when any member carries a branch condition or a
/// `Dynamic` callable, matching the fail-fast guard the single-array
/// eligibility check applied before this shared analysis was factored out.
fn repeated_top_level_positions(group: &[&CallSite]) -> Vec<(usize, Vec<usize>)> {
    let mut candidates_per_position: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    for call_site in group {
        if !call_site.condition.is_empty()
            || matches!(call_site.callable_arg, ConcreteCallable::Dynamic)
        {
            return Vec::new();
        }
        *candidates_per_position
            .entry((call_site.top_level_param, call_site.field_path.clone()))
            .or_default() += 1;
    }

    let mut positions: Vec<(usize, Vec<usize>)> = candidates_per_position
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(position, _)| position)
        .collect();
    positions.sort_unstable();
    positions
}

/// Returns every repeated top-level position in `group` whose forwarded value
/// resolves to an array of callables (`Ty::Array` of `Ty::Arrow`).
///
/// A position is `(top_level_param, field_path)`; it is *repeated* when two or
/// more members supply a callable for it, which is how the analysis records the
/// elements of one forwarded callable array. A single such position is the
/// supported single-array shape; two or more mean two distinct callable arrays
/// are forwarded through the same call, which the combined removal does not
/// model.
///
/// This layers the `Array(Arrow)` type filter on top of
/// [`repeated_top_level_positions`], so it inherits the same branch-condition
/// and `Dynamic` fail-fast guard.
///
/// `package` must own `group`'s shared call expression.
pub(super) fn static_callable_array_positions(
    package: &Package,
    group: &[&CallSite],
) -> Vec<(usize, Vec<usize>)> {
    let positions = repeated_top_level_positions(group);
    if positions.is_empty() {
        return Vec::new();
    }

    let call_expr = package.get_expr(group[0].call_expr_id);
    let ExprKind::Call(callee_id, _) = call_expr.kind else {
        return Vec::new();
    };
    let Ty::Arrow(ref arrow) = package.get_expr(callee_id).ty else {
        return Vec::new();
    };
    let arrow_input = resolve_udt_ty(package, &arrow.input);

    // Filtering the already-sorted `positions` preserves the sort order, so the
    // result stays sorted like the pre-refactor implementation guaranteed.
    positions
        .into_iter()
        .filter(|(top_level_param, field_path)| {
            let container = if group[0].hof_input_is_tuple {
                match &arrow_input {
                    Ty::Tuple(input_tys) => input_tys.get(*top_level_param),
                    _ => None,
                }
            } else {
                Some(&arrow_input)
            };
            let Some(container) = container else {
                return false;
            };
            let selected_ty = field_path.iter().try_fold(container, |ty, index| match ty {
                Ty::Tuple(tys) => tys.get(*index),
                _ => None,
            });
            matches!(
                selected_ty,
                Some(Ty::Array(item_ty)) if matches!(item_ty.as_ref(), Ty::Arrow(_))
            )
        })
        .collect()
}

/// Returns `true` only when `group` has **exactly one repeated top-level
/// position across all types** and that single position is a callable array.
///
/// The combined single-array removal models exactly one forwarded callable
/// array, so the eligibility check is deliberately strict. Requiring the full
/// [`repeated_top_level_positions`] set (any type) to hold a single element —
/// rather than only counting the callable-array positions from
/// [`static_callable_array_positions`] — rejects a group that also repeats a
/// second, non-callable-array position. Such a second repeated position (of
/// *any* type) carries state the combined path cannot represent, so the group
/// must stay on the per-row path. This preserves the pre-refactor behavior,
/// where the `Array(Arrow)` type filter was applied only *after* the
/// exactly-one-repeated-position check.
fn has_static_top_level_callable_array_position(package: &Package, group: &[&CallSite]) -> bool {
    let repeated_positions = repeated_top_level_positions(group);
    let [position] = repeated_positions.as_slice() else {
        return false;
    };
    if !static_callable_array_positions(package, group).contains(position) {
        return false;
    }
    if !position.1.is_empty()
        && group.iter().any(|call_site| {
            call_site.top_level_param != position.0 || call_site.field_path.is_empty()
        })
    {
        return false;
    }
    true
}

/// Returns `true` when `group` forwards two or more distinct callable arrays
/// through a single higher-order-function call, meaning two or more repeated
/// top-level positions each resolve to an array of callables.
///
/// This shape is not supported by the single-array combined removal: leaving it
/// on the per-row path would silently collapse each multi-candidate array to a
/// single member, so the specialization driver rejects it with a hard
/// diagnostic instead.
pub(super) fn has_multiple_forwarded_callable_arrays(
    package: &Package,
    group: &[&CallSite],
) -> bool {
    static_callable_array_positions(package, group).len() >= 2
}

fn resolve_udt_ty(package: &Package, ty: &Ty) -> Ty {
    match ty {
        Ty::Udt(Res::Item(item_id)) => {
            let Some(item) = package.items.get(item_id.item) else {
                return ty.clone();
            };
            let ItemKind::Ty(_, udt) = &item.kind else {
                return ty.clone();
            };
            resolve_udt_ty(package, &udt.get_pure_ty())
        }
        Ty::Tuple(elems) => Ty::Tuple(
            elems
                .iter()
                .map(|elem| resolve_udt_ty(package, elem))
                .collect(),
        ),
        Ty::Array(elem) => Ty::Array(Box::new(resolve_udt_ty(package, elem))),
        Ty::Arrow(arrow) => Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: arrow.kind,
            input: Box::new(resolve_udt_ty(package, &arrow.input)),
            output: Box::new(resolve_udt_ty(package, &arrow.output)),
            functors: arrow.functors,
        })),
        _ => ty.clone(),
    }
}

/// Splits a per-row group that shares one call expression into the parameter
/// that is dispatched over several candidates and its single-valued sibling
/// parameters, when the group has the mixed branch-split shape that the
/// combined per-candidate specialization handles.
///
/// Returns `Some((dispatch_candidates, constants))` only when all of the
/// following hold:
///
/// - exactly one parameter position, meaning a top-level slot plus field path,
///   carries two or more candidates. This is the dispatched parameter, for
///   example `f = [H, X]`.
/// - at least one member sits at a different position. These are the
///   single-valued siblings, for example `g = Make(0.5)` and `h = Z`.
/// - at least one sibling is a producer `Closure`. This is the case the per-row
///   path compiles incorrectly; sibling globals alone keep the
///   restricted-dispatch path, which already threads them as runtime arguments.
/// - no sibling is `Dynamic`. An unresolved sibling cannot be specialized
///   together and must surface its own `DynamicCallable` diagnostic.
///
/// `dispatch_candidates` are every member at the dispatched position, with their
/// conditions preserved; `constants` are every other member. Both the
/// specialize and rewrite phases consult this predicate so they agree on which
/// groups route through the combined per-candidate specializations.
pub(super) fn partition_mixed_branch_split<'a>(
    group: &[&'a CallSite],
) -> Option<(Vec<&'a CallSite>, Vec<&'a CallSite>)> {
    let mut candidates_per_position: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    for cs in group {
        *candidates_per_position
            .entry((cs.top_level_param, cs.field_path.clone()))
            .or_default() += 1;
    }
    let dispatched_positions: Vec<(usize, Vec<usize>)> = candidates_per_position
        .iter()
        .filter(|(_, count)| **count >= 2)
        .map(|(position, _)| position.clone())
        .collect();
    if dispatched_positions.len() != 1 {
        return None;
    }
    let dispatch_position = &dispatched_positions[0];
    let dispatch: Vec<&CallSite> = group
        .iter()
        .copied()
        .filter(|cs| (cs.top_level_param, cs.field_path.clone()) == *dispatch_position)
        .collect();
    let constants: Vec<&CallSite> = group
        .iter()
        .copied()
        .filter(|cs| (cs.top_level_param, cs.field_path.clone()) != *dispatch_position)
        .collect();
    if constants.is_empty() {
        return None;
    }
    if constants
        .iter()
        .any(|cs| matches!(cs.callable_arg, ConcreteCallable::Dynamic))
    {
        return None;
    }
    if !constants
        .iter()
        .any(|cs| matches!(cs.callable_arg, ConcreteCallable::Closure { .. }))
    {
        return None;
    }
    Some((dispatch, constants))
}

/// Consistency-check predicate: returns `true` when `cs` is a single-valued
/// producer-closure sibling, meaning its own parameter position carries exactly
/// one candidate, of a parameter that is dispatched over several candidates at a
/// different position with two or more candidates within `group`.
///
/// This is exactly the shape whose producer body `track_specialized_closures`
/// must not record as consumed before the combined per-candidate specialization
/// removes it from the live call sites. Recording it clears the producer body
/// while the dispatched siblings still reference it, reintroducing the incorrect
/// output. The combined specialization prevents the per-row specialization that
/// triggers the recording, so this predicate is only true when that
/// specialization did not run.
fn closure_constant_sibling_of_dispatch(group: &[&CallSite], cs: &CallSite) -> bool {
    if !matches!(cs.callable_arg, ConcreteCallable::Closure { .. }) {
        return false;
    }
    let own_position = (cs.top_level_param, cs.field_path.clone());
    let mut candidates_per_position: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    for member in group {
        *candidates_per_position
            .entry((member.top_level_param, member.field_path.clone()))
            .or_default() += 1;
    }
    let own_count = candidates_per_position
        .get(&own_position)
        .copied()
        .unwrap_or(0);
    let has_other_dispatched_position = candidates_per_position
        .iter()
        .any(|(position, count)| *position != own_position && *count >= 2);
    own_count == 1 && has_other_dispatched_position
}
