// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defunctionalization pass — runs after return unification, before UDT
//! erasure.
//!
//! Specializes statically resolvable callable values in entry-reachable code.
//! Unresolved callable residue is deferred to capability analysis and partial
//! evaluation, which must resolve dispatch or reject it before QIR generation.
//!
//! # What to know before diving in
//!
//! - **Specialization, not classical defunctionalization.** Instead of a
//!   tagged union plus an `apply` dispatcher, each higher-order-function (HOF)
//!   call site whose concrete callable argument is known at compile time uses
//!   a specialized clone of the HOF, with the callable parameter replaced
//!   by a direct call. `Apply(q => Y(q), target)` becomes a call to an
//!   `Apply_specialized_Y` clone. A callable value nested inside a single tuple
//!   parameter is located by a top-level parameter slot plus a nested field
//!   path.
//! - **Establishes [`crate::invariants::InvariantLevel::PostDefunc`]:** resolved
//!   callables use direct dispatch. Reported residue relaxes callable-elimination
//!   checks, not structural type, scope, or call-shape guarantees.
//! - **Fixpoint loop.** Each iteration normalizes operand evaluation and branch
//!   guards, exposes eligible capture bindings, promotes callable aliases, and
//!   collapses identity closures such as `(a) => f(a)` down to `f`. Analysis
//!   finds callable parameters and concrete call sites. Specialize clones a HOF
//!   once per concrete argument combination, deduplicated by [`types::SpecKey`].
//!   Rewrite redirects call sites, drops the callable argument, and threads
//!   captured values through as extra arguments. Cleanup replaces eligible
//!   consumed closures with typed callable references, subject to local
//!   dependency and producer-owner protection. Cleanup and convergence counting
//!   share the same consumption predicate; a consumed target alone does not
//!   prove that every occurrence is dead.
//!   The iteration cap scales dynamically between
//!   `MIN_ITERATIONS` and `MAX_ITERATIONS`. If no error was recorded, remaining
//!   work produces [`Error::DynamicCallable`] for unresolved direct calls, or
//!   [`Error::FixpointNotReached`] when no such call site was identified.
//! - **Capture ownership and dispatch identity are separate.** [`types::ScopedLocal`]
//!   qualifies a runtime operand by its callable, clone, or entry scope within
//!   the call site's package. Such operands stay attached to individual call
//!   occurrences across fixpoint iterations, even when they share a lifted
//!   target. Runtime capture values do not distinguish [`types::SpecKey`]s;
//!   capture-free callable identities embedded into generated code do.
//! - **Diagnostics:** [`Error::ExcessiveSpecializations`] is a warning.
//!   [`Error::DynamicCallable`] and [`Error::FixpointNotReached`] are deferred
//!   to downstream analysis. Unsupported-shape and resource backstops are fatal.
//! - **Relies on an acyclic UDT graph.** Several type walks in this pass and
//!   its submodules expand `Ty::Udt` through the referenced type's definition
//!   and keep descending, with no visited set — `ty_contains_arrow_through_udts`,
//!   `analysis::extract_arrow_params_from_ty`, `analysis::output_path_resolves_to_arrow`,
//!   and `types::UdtMetadata`'s structural queries. They terminate only because a
//!   user-defined type cannot reference itself. The Q# type checker enforces
//!   this in `qsc_frontend::typeck::check`, rejecting any cyclic declaration
//!   with `Qdk.Qsc.TypeCk.RecursiveUdt` before HIR passes run; the guarantee
//!   covers the package under compilation and extends to the whole store only
//!   where dependency errors are also gated. Q# has no indirection primitive,
//!   so `A[]` and `A -> Int` are descended through exactly as a bare `A` is and
//!   are equally fatal — this pass is where the arrow-mediated case was
//!   originally observed to overflow the stack.
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   `crate::exec_graph_rebuild` repairs exec graphs later.

mod analysis;
mod captures;
mod prepass;
mod rewrite;
mod specialize;
pub mod types;

pub use types::Error;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod semantic_equivalence_tests;

#[cfg(test)]
mod codegen_tests;

#[cfg(test)]
mod test_cases;

use crate::fir_builder::reachable_local_callables;
use crate::package_assigners::PackageAssigners;
use crate::reachability::{collect_reachable_from_entry, collect_reachable_package_closure};
use crate::walk_utils::collect_expr_ids_in_entry_and_local_callables;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::{PackageSpan, Span};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Expr, ExprId, ExprKind, ItemId, ItemKind, LocalItemId, Mutability, Package, PackageId,
    PackageLookup, PackageStore, PatKind, Res, StmtKind, StoreExprId, StoreItemId,
};
use qsc_fir::ty::{Arrow, FunctorSet, Ty};
use rustc_hash::{FxHashMap, FxHashSet};
use types::{
    AnalysisResult, CallSite, CallableParam, ConcreteCallable, ConcreteCallableKey, SpecKey,
    UdtMetadata, peel_body_functors,
};

/// Replaces the innermost input slot beneath `controlled_layers` nested
/// controlled-operation tuples with `target_input`, returning the rewritten
/// outer type.
fn apply_target_input_at_control_path(
    current_input: &Ty,
    target_input: &Ty,
    controlled_layers: usize,
) -> Ty {
    if controlled_layers == 0 {
        return target_input.clone();
    }

    match current_input {
        Ty::Tuple(items) if items.len() > 1 => {
            let mut new_items = items.clone();
            new_items[1] = apply_target_input_at_control_path(
                &new_items[1],
                target_input,
                controlled_layers - 1,
            );
            Ty::Tuple(new_items)
        }
        _ => target_input.clone(),
    }
}

/// Lower bound on the analysis => specialize => rewrite iteration limit.
///
/// This is a floor on the iteration budget, not the number of iterations run:
/// convergence or lack of progress can stop the loop earlier. After the first
/// iteration [`check_convergence`] recomputes the budget as
/// `max(callable_params.len(), remaining_count).clamp(MIN_ITERATIONS, MAX_ITERATIONS)`.
/// The floor leaves room for small HOF chains whose later call sites become
/// resolvable only after earlier specializations have been rewritten.
const MIN_ITERATIONS: usize = 5;

/// Upper bound on the dynamically-computed iteration limit, capping the work
/// for pathological programs.
const MAX_ITERATIONS: usize = 20;

/// Result of the [`defunctionalize`] entry point.
///
/// Includes diagnostics and reachable items with callable-valued residue. The
/// pipeline defers convergence failures to downstream analysis, using those
/// items to relax post-defunctionalization invariants.
pub(crate) struct DefuncOutcome {
    /// Fixpoint diagnostics, classified by the pipeline driver.
    pub diagnostics: Vec<Error>,
    /// Reachable callable items with residue.
    pub residue_items: FxHashSet<StoreItemId>,
    /// Whether the package entry expression itself contains residue.
    pub entry_has_residue: bool,
}

/// Specializes supported callable-valued forms in the entry-reachable code,
/// including reachable callees in other packages.
///
/// Resolved callable arguments are replaced by direct dispatch and captures
/// are threaded as ordinary arguments. Unresolved forms remain for downstream
/// analysis, subject to the pipeline's structural invariants.
///
/// Returns diagnostics and item-keyed callable-valued residue.
///
/// # Requires
/// - Package with `package_id` has an entry expression
///
/// [`Error::ExcessiveSpecializations`] is a warning. The driver defers
/// [`Error::FixpointNotReached`] and [`Error::DynamicCallable`] to downstream
/// analysis; other diagnostics remain fatal.
///
/// # Panics
///
/// Panics if the package has no entry expression. The reachability scans
/// in this pass go through [`collect_reachable_from_entry`], which asserts
/// `package.entry.is_some()`.
#[allow(clippy::too_many_lines)]
pub(crate) fn defunctionalize(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) -> DefuncOutcome {
    let mut errors: Vec<Error> = Vec::new();
    let mut warnings: Vec<Error> = Vec::new();
    // Start at the floor; `check_convergence` raises this to the dynamically
    // computed limit after the first iteration, once the analysis has reported
    // how many callable values actually need resolving.
    let mut max_iterations = MIN_ITERATIONS;
    let mut iteration_count = 0;
    let mut specialized_closure_targets: FxHashSet<StoreItemId> = FxHashSet::default();
    let mut specialized_items: FxHashSet<StoreItemId> = FxHashSet::default();

    // Distinct specializations accumulated per HOF across every iteration.
    // Keyed by HOF item and holding the set of `SpecKey`s generated for it,
    // this is the running budget checked against
    // `specialize::CUMULATIVE_SPECIALIZATION_CAP`. A `HashSet` is used rather
    // than a raw counter so that re-deriving an already-seen `SpecKey` on a
    // later pass — which happens whenever a still-generic HOF is re-analyzed —
    // does not inflate the total and cause a false positive. The cumulative
    // count for a HOF is therefore the number of *distinct* specializations it
    // ever required.
    let mut cumulative_specs_per_hof: FxHashMap<StoreItemId, FxHashSet<SpecKey>> =
        FxHashMap::default();

    // Direct call sites with unresolved callees or inadmissible captures on
    // the most recent iteration. Refreshed every pass; surfaced as diagnostics
    // only if the loop terminates with work remaining (see
    // `emit_fixpoint_error`), so transient forwarding calls resolved by a later
    // specialization never reach that terminal state.
    let mut unresolved_direct_call_sites: Vec<StoreExprId> = Vec::new();
    // Callables outside a rewritten package that are side-effect free and total.
    // Dead-binding cleanup needs them to prove that discarding a producer call
    // is unobservable, and the package set does not change during the loop.
    // Include foreign functions proven discardable, not just known intrinsics,
    // so call-site admission and cleanup use the same effect-safety proof.
    let total_foreign = {
        let mut total = crate::walk_utils::collect_total_foreign_callables(store);
        crate::walk_utils::extend_with_discardable_foreign_callables(store, &mut total);
        total
    };

    // Rewriting a closure callee mutates its occurrence into `Var(Item(.lambda))`.
    // Preserve prior occurrence-local operands so the next analysis can retain
    // them without attaching runtime values to the global lambda item.
    let mut preserved_direct_lambda_calls = Vec::new();

    // Capture the initial callable-value count for before/after progress
    // tracking, mirroring LLVM's DevirtSCCRepeatedPass: detect when an
    // iteration fails to reduce the remaining work set.
    // Use the same consumption-aware count as later iterations, initially with
    // no consumed targets.
    let mut consumed_closures = ConsumedClosures::default();
    let (_, mut prev_remaining_count, _, _) =
        remaining_callable_value_info(store, package_id, &consumed_closures);

    // Fail-bodied stand-ins for consumed closures with incompatible lifted
    // signatures. Cache across iterations, with one item per signature and package.
    let mut stand_ins = ClosureStandInCache::default();

    while iteration_count < max_iterations {
        iteration_count += 1;

        // Clear DynamicCallable errors from prior iterations. They are
        // re-discovered each pass by the HOF path; transient ones (e.g.
        // parameter forwarding like `Inner(op, q)` in a not-yet-specialized
        // HOF) disappear once the outer HOF is specialized, so only the final
        // iteration's emissions survive.
        errors.retain(|e| !matches!(e, Error::DynamicCallable(_)));

        let reachable = collect_reachable_from_entry(store, package_id);

        // Every package whose calls can be rewritten needs the same capture
        // timing and normalization prerequisites, not just the entry package.
        let mut collapsed_spans = FxHashMap::default();
        let packages: FxHashSet<_> = std::iter::once(package_id)
            .chain(reachable.iter().map(|item| item.package))
            .collect();
        for owner in packages {
            let (_, expressions) = collect_reachable_scope(store, owner, &reachable);
            let assigner = assigners.get_mut(store, owner);
            collapsed_spans.extend(
                prepass::run(store, owner, &expressions, assigner)
                    .into_iter()
                    .map(|(expr, span)| ((owner, expr), span)),
            );
            let (_, expressions) = collect_reachable_scope(store, owner, &reachable);
            let normalized = rewrite::normalize_direct_callee_control_flow(
                store.get_mut(owner),
                expressions,
                assigner,
            );
            if normalized {
                crate::cond_normalize::normalize_callable_selections(
                    store.get_mut(owner),
                    assigner,
                );
                let (_, expressions) = collect_reachable_scope(store, owner, &reachable);
                prepass::normalize_capture_operands(store.get_mut(owner), &expressions, assigner);
            }
        }

        let analysis = analysis::analyze(
            store,
            package_id,
            &reachable,
            &specialized_items,
            &collapsed_spans,
            &preserved_direct_lambda_calls,
            &total_foreign,
        );
        preserved_direct_lambda_calls.clone_from(&analysis.direct_call_sites);

        // Record (do not yet emit) direct calls whose callee resolved to
        // `Dynamic`; emission is deferred to `emit_fixpoint_error` so calls
        // that are only transiently `Dynamic` never produce spurious errors.
        unresolved_direct_call_sites.clone_from(&analysis.unresolved_direct_call_sites);
        let spec_map = run_specialization(store, &analysis, assigners, &mut errors, &mut warnings);
        // Fold this pass's specializations into the cumulative per-HOF budget
        // and fail closed if any HOF has now required more distinct
        // specializations than the hard cap allows. This backstops the
        // per-iteration `ExcessiveSpecializations` warning (which fires on a
        // single wide pass) by catching growth that persists across iterations,
        // the signature of a degenerate recursive shape. Breaking here stops
        // the loop before it can expand further; the fatal diagnostic is
        // preserved by the after-loop `emit_fixpoint_error` gate, which only
        // adds a generic non-convergence report when no other error fired.
        if let Some(error) = accumulate_and_check_specialization_budget(
            store,
            &spec_map,
            &mut cumulative_specs_per_hof,
        ) {
            errors.push(error);
            break;
        }

        // Rewrite call sites and run dead callable-local cleanup even on
        // iterations where no new specializations were discovered. Call sites
        // can live in foreign bodies so rewrite runs once per package
        // that owns call sites, each with that package's own assigner.
        rewrite_call_sites(
            store,
            package_id,
            &analysis,
            &spec_map,
            &specialized_items,
            assigners,
            &total_foreign,
        );

        track_specialized_closures(
            &analysis,
            &spec_map,
            &mut specialized_closure_targets,
            &mut specialized_items,
        );

        #[cfg(debug_assertions)]
        crate::invariants::debug_check_local_scopes(store, package_id);
        // Recompute reachability after rewriting: orphaned producers need no
        // closure cleanup and can be removed later by item DCE. Cleanup and
        // convergence share the resulting package-qualified consumption state.
        let post_rewrite_reachable = collect_reachable_from_entry(store, package_id);
        consumed_closures =
            ConsumedClosures::new(store, &specialized_closure_targets, &specialized_items);
        let live_producers = live_callable_producer_items(
            store,
            package_id,
            &post_rewrite_reachable,
            &consumed_closures,
        );
        consumed_closures.skipped.extend(live_producers);
        cleanup_consumed_closures_per_package(
            store,
            package_id,
            &post_rewrite_reachable,
            &consumed_closures,
            assigners,
            &mut stand_ins,
        );

        let converged = check_convergence(
            store,
            package_id,
            &analysis,
            iteration_count,
            &mut max_iterations,
            &mut prev_remaining_count,
            &consumed_closures,
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
        &consumed_closures,
        &mut errors,
    );
    errors.extend(warnings);

    // The driver relaxes callable-elimination checks for discovered residue;
    // structural invariants remain enforced at their pipeline checkpoints.
    let (residue_items, entry_has_residue) = collect_residue_items(store, package_id);

    DefuncOutcome {
        diagnostics: errors,
        residue_items,
        entry_has_residue,
    }
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

/// Folds one pass's specializations into the cumulative per-HOF budget and
/// returns a fatal [`Error::RecursiveSpecialization`] for the first HOF whose
/// distinct specialization set now exceeds
/// [`specialize::CUMULATIVE_SPECIALIZATION_CAP`].
///
/// Each `SpecKey` from `spec_map` is inserted into the HOF's set, so a key that
/// was already seen on a prior iteration does not inflate the count. Returns
/// `None` while every HOF stays within budget.
fn accumulate_and_check_specialization_budget(
    store: &PackageStore,
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    cumulative_specs_per_hof: &mut FxHashMap<StoreItemId, FxHashSet<SpecKey>>,
) -> Option<Error> {
    for key in spec_map.keys() {
        cumulative_specs_per_hof
            .entry(key.hof_id)
            .or_default()
            .insert(key.clone());
    }
    for (hof_id, keys) in cumulative_specs_per_hof.iter() {
        let count = keys.len();
        if count > specialize::CUMULATIVE_SPECIALIZATION_CAP {
            let package = store.get(hof_id.package);
            let item = package.get_item(hof_id.item);
            let (name, span) = if let ItemKind::Callable(decl) = &item.kind {
                (decl.name.name.to_string(), decl.name.span)
            } else {
                (format!("Item({hof_id})"), package.synthetic_span())
            };
            return Some(Error::RecursiveSpecialization(
                name,
                count,
                PackageSpan::new(hof_id.package, span.span),
            ));
        }
    }
    None
}

/// Rewrites call sites in every package that owns one. Call sites can live in
/// foreign bodies so rewrite is driven once per owning package using that
/// package's own assigner. The entry package is always rewritten so that
/// iterations with only direct-call cleanup still run. Snapshot generated inputs
/// by qualified identity before borrowing callers: a shared specialization can
/// live in a different package from the call being rewritten.
fn rewrite_call_sites(
    store: &mut PackageStore,
    package_id: PackageId,
    analysis: &AnalysisResult,
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    specialized_items: &FxHashSet<StoreItemId>,
    assigners: &mut PackageAssigners,
    total_foreign: &FxHashSet<ItemId>,
) {
    let specialized_inputs: FxHashMap<StoreItemId, Ty> = spec_map
        .values()
        .map(|&target| {
            let package = store.get(target.package);
            let ItemKind::Callable(decl) = &package.get_item(target.item).kind else {
                unreachable!("specializations must refer to callable declarations");
            };
            (target, package.get_pat(decl.input).ty.clone())
        })
        .collect();
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
        rewrite::rewrite(
            package,
            pkg_id,
            analysis,
            spec_map,
            &specialized_inputs,
            specialized_items,
            assigner,
            total_foreign,
        );
    }
}

/// Records closure targets selected by specialization or direct-call analysis
/// in this iteration. This target-level set supplies cleanup candidates; it is
/// not occurrence-level liveness and does not replace cleanup's dependency checks.
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
            // A shared per-row key may originate at another call site. For
            // this mixed occurrence, require complete combined dispatch
            // coverage before its sibling producer can be consumed.
            if let Some(group) = groups.get(&(cs.call_pkg_id, cs.call_expr_id))
                && closure_constant_sibling_of_dispatch(group, cs)
            {
                // A `Dynamic` sibling means `partition_mixed_branch_split`
                // intentionally declined this group so the unresolved argument
                // can surface as `DynamicCallable`. In that case the per-row
                // closure spec may exist only as a transient side effect of
                // collecting diagnostics; do not mark its producer body as
                // consumed, and do not treat the absence of the mixed combined
                // spec as an internal rewrite/specialization disagreement.
                if group
                    .iter()
                    .any(|member| matches!(member.callable_arg, ConcreteCallable::Dynamic))
                {
                    continue;
                }
                // The same per-row key can belong to a different call site.
                // This occurrence is covered only if every mixed dispatch leaf
                // has its own complete, position-aligned specialization.
                if mixed_dispatch_is_specialized(group, spec_map) {
                    continue;
                }
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
            // Per-candidate specializations do not prove the whole dispatch was
            // rewritten. Without a combined specialization, keep its closures
            // live rather than replacing values the original call may still invoke.
            if let Some(group) = groups.get(&(cs.call_pkg_id, cs.call_expr_id))
                && group.len() > 1
                && !spec_map.contains_key(&build_combined_spec_key_for_group(
                    group[0].hof_item_id,
                    group,
                ))
            {
                continue;
            }
            specialized_closure_targets.insert(StoreItemId::from((cs.call_pkg_id, *target)));
        }
    }
    // Combined keying: record every participating closure target under the
    // combined key. Multi-argument and single-argument keys differ in argument
    // count; omitting a member here would leave consumed callable residue.
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
    consumed: &ConsumedClosures,
) -> bool {
    let (has_remaining, remaining_count, _, _) =
        remaining_callable_value_info(store, package_id, consumed);

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
    unresolved_direct_call_sites: &[StoreExprId],
    consumed: &ConsumedClosures,
    errors: &mut Vec<Error>,
) {
    let (has_remaining, remaining_count, owner, span) =
        remaining_callable_value_info(store, package_id, consumed);
    if has_remaining && errors.is_empty() {
        if unresolved_direct_call_sites.is_empty() {
            errors.push(Error::FixpointNotReached(
                iteration_count,
                remaining_count,
                (owner, span).into(),
            ));
        } else {
            for &call_site in unresolved_direct_call_sites {
                let package = store.get(call_site.package);
                errors.push(Error::DynamicCallable(
                    package.get_expr(call_site.expr).span,
                ));
            }
        }
    }
}

/// Package-qualified bookkeeping shared by cleanup and remaining-work counting.
///
/// A closure occurrence is eligible only when its target is consumed, its owner
/// is not skipped, and no live value dependency protects it. This type stores
/// the first two conditions; [`collect_live_call_arg_exprs`] supplies the third.
/// Consuming one use of a target does not make all its closure occurrences dead.
#[derive(Default)]
struct ConsumedClosures {
    /// Closure target callables consumed by specialization or direct-call
    /// rewrite. Accumulated across iterations by [`track_specialized_closures`].
    targets: FxHashSet<StoreItemId>,
    /// Items whose closures are left alone: every specialized clone produced so
    /// far, plus producers needed by surviving callable-value dependencies.
    skipped: FxHashSet<StoreItemId>,
}

/// [`ConsumedClosures`] projected to the local item ids of one package.
struct ConsumedClosuresInPackage {
    targets: FxHashSet<LocalItemId>,
    skipped: FxHashSet<LocalItemId>,
}

impl ConsumedClosures {
    /// Starts an iteration's state with cumulative consumed targets and specialized
    /// items. Protects those items and their direct callees; the driver then adds
    /// transitive live producers before cleanup and convergence counting.
    fn new(
        store: &PackageStore,
        specialized_targets: &FxHashSet<StoreItemId>,
        specialized_items: &FxHashSet<StoreItemId>,
    ) -> Self {
        let mut skipped = specialized_items.clone();
        skipped.extend(items_called_from_skipped_items(store, specialized_items));
        Self {
            targets: specialized_targets.clone(),
            skipped,
        }
    }

    /// Whether there are no consumed targets.
    fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// Narrows both sets to the local item ids of `pkg_id`.
    fn project(&self, pkg_id: PackageId) -> ConsumedClosuresInPackage {
        ConsumedClosuresInPackage {
            targets: project_to_package(&self.targets, pkg_id),
            skipped: project_to_package(&self.skipped, pkg_id),
        }
    }
}

impl ConsumedClosuresInPackage {
    /// Whether this package owns any consumed closure targets.
    fn has_targets(&self) -> bool {
        !self.targets.is_empty()
    }

    /// True when closures inside `item_id` are left alone this iteration.
    fn item_is_skipped(&self, item_id: LocalItemId) -> bool {
        self.skipped.contains(&item_id)
    }

    /// Whether the target is consumed and the owner is not skipped.
    ///
    /// `None` denotes the package entry, which is never a skipped owner.
    /// Callers must also exclude occurrences protected by live value dependencies.
    fn set_conditions_hold(&self, owner_item: Option<LocalItemId>, target: LocalItemId) -> bool {
        self.targets.contains(&target)
            && !owner_item.is_some_and(|item_id| self.item_is_skipped(item_id))
    }
}

/// Narrows a cross-package item set to the local item ids belonging to
/// `pkg_id`.
fn project_to_package(items: &FxHashSet<StoreItemId>, pkg_id: PackageId) -> FxHashSet<LocalItemId> {
    items
        .iter()
        .filter(|id| id.package == pkg_id)
        .map(|id| id.item)
        .collect()
}

/// Collects live call-argument subtrees and their local value dependencies.
///
/// Local reads, callees, arguments, and captures protect all possible reaching
/// values, including initializers and assignments. Same-package UDT constructors
/// are structural wrappers, so their arguments alone do not establish liveness.
///
/// `skipped` excludes owners protected from cleanup. Producer discovery passes
/// an empty set: even an owner whose closures cannot be cleaned can still call
/// a producer whose returned closure must remain live.
fn collect_live_call_arg_exprs(
    package: &Package,
    package_id: PackageId,
    reachable_item_ids: &[LocalItemId],
    skipped: &FxHashSet<LocalItemId>,
) -> FxHashSet<ExprId> {
    let mut call_arg_exprs: FxHashSet<ExprId> = FxHashSet::default();
    for &item_id in reachable_item_ids {
        if skipped.contains(&item_id) {
            continue;
        }
        let item = package.get_item(item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            let mut nodes = Vec::new();
            crate::walk_utils::for_each_node_in_callable(package, decl, &mut |node| {
                nodes.push(node);
            });
            collect_live_call_argument_exprs(package, package_id, &nodes, &mut call_arg_exprs);
        }
    }
    if let Some(entry_id) = package.entry {
        let mut nodes = Vec::new();
        crate::walk_utils::for_each_node_from_expr_root(package, entry_id, &mut |node| {
            nodes.push(node);
        });
        collect_live_call_argument_exprs(package, package_id, &nodes, &mut call_arg_exprs);
    }

    call_arg_exprs
}

/// Finds callable-producing calls and factory values that remain live, then
/// follows their producer chains across packages. All reachable owners are
/// scanned, including specialized clones and their protected callees: skipping
/// cleanup of an owner does not protect its transitive producers.
fn live_callable_producer_items(
    store: &PackageStore,
    entry_package: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    consumed: &ConsumedClosures,
) -> FxHashSet<StoreItemId> {
    let mut producers = FxHashSet::default();
    if consumed.is_empty() {
        return producers;
    }
    for package_id in collect_reachable_package_closure(entry_package, reachable) {
        let package = store.get(package_id);
        let items: Vec<_> = reachable_local_callables(package, package_id, reachable)
            .map(|(item, _)| item)
            .collect();
        let protected =
            collect_live_call_arg_exprs(package, package_id, &items, &FxHashSet::default());
        let data_only = data_only_producer_results(store, package, &items);
        let value_uses = callable_value_uses(package, &items);
        for expr_id in protected {
            if data_only.contains(&expr_id) {
                continue;
            }
            if let Some(producer) = referenced_callable_producer(
                store,
                package,
                package.get_expr(expr_id),
                value_uses.contains(&expr_id),
            ) {
                producers.insert(producer);
            }
        }
    }
    // A live aggregate producer may forward through other factories whose
    // return expressions are not themselves direct call operands.
    let mut pending: Vec<_> = producers.iter().copied().collect();
    while let Some(producer) = pending.pop() {
        let package = store.get(producer.package);
        let ItemKind::Callable(decl) = &package.get_item(producer.item).kind else {
            continue;
        };
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |_, expr| {
                if let Some(target) = referenced_callable_producer(store, package, expr, true)
                    && producers.insert(target)
                {
                    pending.push(target);
                }
            },
        );
    }
    producers
}

/// A factory retained as a value may be invoked indirectly even if analysis
/// cannot resolve that call. Its closure-producing body must stay intact just
/// like the body of a directly called producer.
/// A reference used only as a direct callee is covered by its call expression;
/// it must not independently undo the data-only-result exclusion above.
fn referenced_callable_producer(
    store: &PackageStore,
    package: &Package,
    expr: &Expr,
    used_as_value: bool,
) -> Option<StoreItemId> {
    let factory = if let ExprKind::Call(callee, _) = expr.kind {
        if !callable_output_contains_arrow(store, &expr.ty) {
            return None;
        }
        let (base, _) = peel_body_functors(package, callee);
        package.get_expr(base)
    } else if used_as_value
        && let Ty::Arrow(arrow) = &expr.ty
        && callable_output_contains_arrow(store, &arrow.output)
    {
        expr
    } else {
        return None;
    };
    match factory.kind {
        ExprKind::Var(Res::Item(item), _) => Some((item.package, item.item).into()),
        ExprKind::Closure(_, target) => Some((package.id, target).into()),
        _ => None,
    }
}

/// Records value-position expression edges, excluding direct callee edges.
/// An expression shared between both positions is still a value use.
fn callable_value_uses(package: &Package, items: &[LocalItemId]) -> FxHashSet<ExprId> {
    use crate::walk_utils::{CallableNode, DirectChild, for_each_direct_child};
    let mut uses = FxHashSet::default();
    let mut record = |node| match node {
        CallableNode::Stmt(id) => match package.get_stmt(id).kind {
            StmtKind::Local(_, _, value) | StmtKind::Expr(value) | StmtKind::Semi(value) => {
                uses.insert(value);
            }
            StmtKind::Item(_) => {}
        },
        CallableNode::Expr(id) => match &package.get_expr(id).kind {
            ExprKind::Call(_, args) => {
                uses.insert(*args);
            }
            kind => for_each_direct_child(kind, |child| {
                if let DirectChild::Expr(id) = child {
                    uses.insert(id);
                }
            }),
        },
        CallableNode::Block(_) | CallableNode::Pat(_) => {}
    };
    for &item in items {
        if let ItemKind::Callable(decl) = &package.get_item(item).kind {
            crate::walk_utils::for_each_node_in_callable(package, decl, &mut record);
        }
    }
    if let Some(entry) = package.entry {
        crate::walk_utils::for_each_node_from_expr_root(package, entry, &mut record);
        uses.insert(entry);
    }
    uses
}

/// A retained aggregate need not retain its consumed callable fields when every
/// remaining use reads only non-callable data. Whole-value uses and captures
/// remain live, as do shared initializer expressions.
fn data_only_producer_results(
    store: &PackageStore,
    package: &Package,
    items: &[LocalItemId],
) -> FxHashSet<ExprId> {
    use crate::walk_utils::{
        CallableNode, for_each_node_from_expr_root, for_each_node_in_callable,
    };
    let mut scopes = Vec::new();
    for &item in items {
        if let ItemKind::Callable(decl) = &package.get_item(item).kind {
            let mut nodes = Vec::new();
            for_each_node_in_callable(package, decl, &mut |node| nodes.push(node));
            scopes.push(nodes);
        }
    }
    if let Some(entry) = package.entry {
        let mut nodes = Vec::new();
        for_each_node_from_expr_root(package, entry, &mut |node| nodes.push(node));
        scopes.push(nodes);
    }
    let mut occurrences: FxHashMap<ExprId, usize> = FxHashMap::default();
    for nodes in &scopes {
        for node in nodes {
            if let CallableNode::Expr(id) = node {
                *occurrences.entry(*id).or_default() += 1;
            }
        }
    }
    let mut data_only = FxHashSet::default();
    for nodes in &scopes {
        for node in nodes {
            let CallableNode::Stmt(statement) = node else {
                continue;
            };
            let StmtKind::Local(Mutability::Immutable, pattern, initializer) =
                package.get_stmt(*statement).kind
            else {
                continue;
            };
            let PatKind::Bind(binding) = &package.get_pat(pattern).kind else {
                continue;
            };
            if occurrences.get(&initializer) != Some(&1)
                || !callable_output_contains_arrow(store, &package.get_expr(initializer).ty)
            {
                continue;
            }
            let data_reads: FxHashSet<_> = nodes.iter().filter_map(|node| {
                let CallableNode::Expr(id) = node else { return None };
                let expr = package.get_expr(*id);
                let ExprKind::Field(record, _) = expr.kind else { return None };
                (matches!(package.get_expr(record).kind, ExprKind::Var(Res::Local(var), _) if var == binding.id)
                    && !callable_output_contains_arrow(store, &expr.ty)).then_some(record)
            }).collect();
            let only_data = nodes.iter().all(|node| {
                let CallableNode::Expr(id) = node else {
                    return true;
                };
                match &package.get_expr(*id).kind {
                    ExprKind::Var(Res::Local(var), _) if *var == binding.id => {
                        data_reads.contains(id)
                    }
                    ExprKind::Closure(captures, _) => !captures.contains(&binding.id),
                    _ => true,
                }
            });
            if only_data {
                data_only.insert(initializer);
            }
        }
    }
    data_only
}

fn callable_output_contains_arrow(store: &PackageStore, ty: &Ty) -> bool {
    match ty {
        Ty::Array(element) => callable_output_contains_arrow(store, element),
        Ty::Tuple(elements) => elements
            .iter()
            .any(|ty| callable_output_contains_arrow(store, ty)),
        Ty::Udt(Res::Item(item)) => {
            let ItemKind::Ty(_, udt) = &store.get(item.package).get_item(item.item).kind else {
                return false;
            };
            callable_output_contains_arrow(store, &udt.get_pure_ty())
        }
        Ty::Arrow(_) => true,
        _ => false,
    }
}

fn mixed_dispatch_is_specialized(
    group: &[&CallSite],
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
) -> bool {
    partition_mixed_branch_split(group).is_some_and(|(dispatch, constants)| {
        dispatch.iter().all(|candidate| {
            let mut members = vec![*candidate];
            members.extend(constants.iter().copied());
            spec_map.contains_key(&build_combined_spec_key(candidate.hof_item_id, &members))
        })
    })
}

/// Runs [`cleanup_consumed_closures`] over every package in the entry-reachable
/// closure that owns a consumed closure. Consumed closures can live in foreign
/// bodies (a closure passed to a HOF inside a relocated generic body), so the
/// cross-package [`ConsumedClosures`] sets are projected to each package's
/// local item ids before running the single-package cleanup there.
///
/// Each package is mutated with its own assigner, so a stand-in synthesized for
/// a foreign package is minted into that package's id arena.
fn cleanup_consumed_closures_per_package(
    store: &mut PackageStore,
    entry_pkg_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    consumed: &ConsumedClosures,
    assigners: &mut PackageAssigners,
    stand_ins: &mut ClosureStandInCache,
) {
    if consumed.is_empty() {
        return;
    }

    for pkg_id in collect_reachable_package_closure(entry_pkg_id, reachable) {
        let consumed_local = consumed.project(pkg_id);
        if !consumed_local.has_targets() {
            continue;
        }
        let local_item_ids: Vec<LocalItemId> = {
            let package = store.get(pkg_id);
            reachable_local_callables(package, pkg_id, reachable)
                .map(|(id, _)| id)
                .collect()
        };
        let assigner = assigners.get_mut(store, pkg_id);
        let package = store.get_mut(pkg_id);
        cleanup_consumed_closures(
            package,
            assigner,
            pkg_id,
            &consumed_local,
            &local_item_ids,
            stand_ins,
        );
    }
}

/// Finds direct callees of the cumulative specialized items so cleanup preserves
/// any producer bodies those specializations can still invoke.
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

/// Replaces eligible closure expressions whose target was consumed by
/// specialization or direct-call rewriting with a typed callable reference.
///
/// Applies the three conditions documented on [`ConsumedClosures`]. Live value
/// dependencies and skipped owners retain their closures. Only post-rewrite
/// reachable items are visited; orphaned producers remain intact until item DCE.
///
/// Replacements must retain the closure's arrow type even when only non-callable
/// fields of the containing aggregate are still read. A capture-free closure can
/// reference its own target only when the full signatures match. Captures and
/// tuple-wrapped lifted inputs can make those signatures differ.
///
/// Otherwise, [`ClosureStandInCache`] supplies a same-signature, fail-bodied
/// placeholder. Eligibility must establish that the replaced value will not be
/// invoked; the placeholder preserves the containing expression's type, not the
/// closure's behavior. [`remaining_callable_value_info`] uses the same eligibility
/// predicate when counting pending work.
fn cleanup_consumed_closures(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    consumed: &ConsumedClosuresInPackage,
    reachable_item_ids: &[LocalItemId],
    stand_ins: &mut ClosureStandInCache,
) {
    if !consumed.has_targets() {
        return;
    }

    // Protect live dependencies before selecting replacements.
    let call_arg_exprs =
        collect_live_call_arg_exprs(package, package_id, reachable_item_ids, &consumed.skipped);

    // Collect consumed closures not protected by an owner or live dependency.
    let mut to_replace: Vec<ExprId> = Vec::new();
    for &item_id in reachable_item_ids {
        if consumed.item_is_skipped(item_id) {
            continue;
        }
        let item = package.get_item(item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, expr| {
                    if let ExprKind::Closure(_, target) = &expr.kind
                        && consumed.set_conditions_hold(Some(item_id), *target)
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
                && consumed.set_conditions_hold(None, *target)
                && !call_arg_exprs.contains(&expr_id)
            {
                to_replace.push(expr_id);
            }
        });
    }

    for expr_id in to_replace {
        let expr = package.get_expr(expr_id);
        let ExprKind::Closure(captures, target) = &expr.kind else {
            unreachable!("only closure expressions are collected for replacement")
        };

        let Ty::Arrow(arrow) = &expr.ty else {
            unreachable!("a closure expression always carries an arrow type")
        };
        let ItemKind::Callable(decl) = &package.get_item(*target).kind else {
            unreachable!("a closure target always refers to a callable")
        };
        // No captures does not guarantee matching signatures: a lifted `Int -> Int`
        // lambda can still take `(Int,)`. Only reuse a target with the exact arrow
        // signature; changing the expression's annotation cannot adapt its arguments.
        let target_matches = captures.is_empty()
            && decl.kind == arrow.kind
            && package.get_pat(decl.input).ty == *arrow.input
            && decl.output == *arrow.output
            && FunctorSet::Value(decl.functors) == arrow.functors;
        let replacement = if target_matches {
            *target
        } else {
            // Cleanup proved the value is no longer invoked, but a surviving aggregate
            // may still store it. Supply a correctly typed stand-in until later cleanup.
            let arrow = arrow.clone();
            stand_ins.get_or_insert(package, assigner, package_id, &arrow)
        };

        // The expression's own type is left alone: it is the arrow type the
        // parent slot declares, and both replacements satisfy it.
        let expr = package.exprs.get_mut(expr_id).expect("expr must exist");
        expr.kind = ExprKind::Var(
            Res::Item(ItemId {
                package: package_id,
                item: replacement,
            }),
            Vec::new(),
        );
    }
}

/// Caches fail-bodied stand-ins for neutralized closures whose lifted targets
/// cannot be referenced with the closure's arrow type.
///
/// The package-qualified key prevents a cached [`LocalItemId`] from being reused
/// in another package. Within each package, identical arrow types share an item.
#[derive(Default)]
struct ClosureStandInCache {
    items: FxHashMap<(PackageId, String), LocalItemId>,
}

impl ClosureStandInCache {
    fn get_or_insert(
        &mut self,
        package: &mut Package,
        assigner: &mut Assigner,
        package_id: PackageId,
        arrow: &Arrow,
    ) -> LocalItemId {
        // `Arrow`'s `Display` renders kind, input, output, and functors, so the
        // rendered form distinguishes every signature the stand-in must match.
        let key = (package_id, arrow.to_string());
        if let Some(&id) = self.items.get(&key) {
            return id;
        }
        let FunctorSet::Value(functors) = arrow.functors else {
            unreachable!("monomorphization resolves every functor parameter before this pass")
        };
        let id = crate::fir_builder::alloc_fail_callable(
            package,
            assigner,
            "__defunc_consumed_closure",
            "consumed closure stand-in invoked",
            arrow.kind,
            &arrow.input,
            &arrow.output,
            functors,
        );
        self.items.insert(key, id);
        id
    }
}

/// Protects expressions that surviving call arguments or local reads depend on from
/// [`cleanup_consumed_closures`].
///
/// Specializing one use of a lifted target does not make every closure for that
/// target dead. Another call may still receive it through an alias, an assignment,
/// or a capturing closure. Replacing that initializer with a fail-bodied
/// stand-in would destroy a still-invoked callable value despite keeping its type.
/// Reads in alias bindings and assignments also matter after dispatch is gone:
/// a retained alias cycle must still evaluate callable-valued initializers.
///
/// This is deliberately conservative, not flow-sensitive liveness: all recorded
/// definitions of a referenced local are protected, including references on
/// assignment left-hand sides. Retaining an obsolete definition may defer
/// cleanup; dropping a needed one corrupts the program.
/// `nodes` must belong to one callable or the entry expression, since local IDs
/// can collide across callables. The accumulated `live` set is package-local.
///
/// UDT constructors do not independently make their arguments live: they wrap
/// values rather than invoke them. Treating them as consumers would prevent
/// cleanup of already-specialized callable fields and stall convergence.
fn collect_live_call_argument_exprs(
    package: &Package,
    package_id: PackageId,
    nodes: &[crate::walk_utils::CallableNode],
    live: &mut FxHashSet<ExprId>,
) {
    use crate::walk_utils::CallableNode;
    use qsc_fir::fir::{LocalVarId, PatKind, StmtKind};

    let mut definitions: FxHashMap<LocalVarId, Vec<ExprId>> = FxHashMap::default();
    let mut pending = Vec::new();
    for node in nodes {
        match node {
            CallableNode::Stmt(stmt_id) => {
                if let StmtKind::Local(_, pat_id, init) = package.get_stmt(*stmt_id).kind {
                    let mut patterns = vec![pat_id];
                    while let Some(pat_id) = patterns.pop() {
                        match &package.get_pat(pat_id).kind {
                            PatKind::Bind(ident) => {
                                definitions.entry(ident.id).or_default().push(init);
                            }
                            PatKind::Tuple(items) => patterns.extend(items),
                            PatKind::Discard => {}
                        }
                    }
                }
            }
            CallableNode::Expr(expr_id) => {
                let expr = package.get_expr(*expr_id);
                for local in crate::walk_utils::assignment_written_locals(package, expr) {
                    definitions.entry(local).or_default().push(*expr_id);
                }
                match expr.kind {
                    ExprKind::Var(Res::Local(_), _) => pending.push(*expr_id),
                    ExprKind::Call(callee, args)
                        if !is_udt_ctor_call(package, package_id, callee) =>
                    {
                        pending.push(args);
                    }
                    _ => {}
                }
            }
            CallableNode::Block(_) | CallableNode::Pat(_) => {}
        }
    }

    let mut visited = FxHashSet::default();
    while let Some(root) = pending.pop() {
        if !visited.insert(root) {
            continue;
        }
        crate::walk_utils::for_each_expr(package, root, &mut |id, expr| {
            live.insert(id);
            match &expr.kind {
                ExprKind::Var(Res::Local(var), _) => {
                    pending.extend(definitions.get(var).into_iter().flatten().copied());
                }
                ExprKind::Closure(captures, _) => {
                    for var in captures {
                        pending.extend(definitions.get(var).into_iter().flatten().copied());
                    }
                }
                _ => {}
            }
        });
    }
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

/// Checks whether any reachable callable value still requires
/// defunctionalization work.
///
/// Counts arrow-bearing input patterns, closures, and indirect calls through
/// arrow-typed locals or computed values. Only closures consult `consumed`:
/// occurrences eligible for cleanup do not count, whether or not replacement
/// has run. The live-dependency walk is recomputed using current reachability.
///
/// Returns `(has_remaining, count, first_package, first_span)` in a single
/// reachability scan.
fn remaining_callable_value_info(
    store: &PackageStore,
    package_id: PackageId,
    consumed: &ConsumedClosures,
) -> (bool, usize, PackageId, Span) {
    let reachable = collect_reachable_from_entry(store, package_id);
    let consumed_scopes = collect_consumed_closure_scopes(store, package_id, &reachable, consumed);
    let mut count = 0;
    let mut first_package = package_id;
    let mut first_span = Span::default();

    let mut record_remaining = |owner: PackageId, span: Span| {
        if count == 0 {
            first_package = owner;
            first_span = span;
        }
        count += 1;
    };

    // Walk every reachable callable in its owning package. HOF call sites and
    // their specialized clones can live outside the entry package, including
    // generic standard-library HOFs instantiated there by monomorphization, so
    // a foreign callable that still carries an arrow-typed parameter, a
    // closure, or an indirect call through a local or computed callee is genuine
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
                record_remaining(store_id.package, input_pat.span.span);
            }

            let scope = consumed_scopes.get(&store_id.package);
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, expr| {
                    if let ExprKind::Closure(_, target) = &expr.kind
                        && !closure_is_consumed(scope, Some(store_id.item), *target, expr_id)
                    {
                        record_remaining(store_id.package, expr.span.span);
                    }
                    // Count indirect calls through locals and computed values.
                    // After defunc iteration 1 specializes HOFs and removes callable
                    // parameters, conditional callable bindings like
                    //   let u = if power >= 0 { op } else { Adjoint op };
                    //   u(target);
                    // leave arrow-typed locals with indirect Call expressions.
                    // The existing branch-split infrastructure resolves these in
                    // a subsequent iteration, but only if the convergence check
                    // reports them as remaining.
                    if let Some(callee) = indirect_callee_id(package, expr) {
                        record_remaining(store_id.package, package.get_expr(callee).span.span);
                    }
                },
            );
        }
    }

    let package = store.get(package_id);
    if let Some(entry_id) = package.entry {
        let scope = consumed_scopes.get(&package_id);
        crate::walk_utils::for_each_expr(package, entry_id, &mut |expr_id, expr| {
            if let ExprKind::Closure(_, target) = &expr.kind
                && !closure_is_consumed(scope, None, *target, expr_id)
            {
                record_remaining(package_id, expr.span.span);
            }
            // Same indirect-call check as callable body walker.
            if let Some(callee) = indirect_callee_id(package, expr) {
                record_remaining(package_id, package.get_expr(callee).span.span);
            }
        });
    }

    (count > 0, count, first_package, first_span)
}

/// Finds reachable callable items with residue that requires deferred invariant
/// enforcement. Entry-expression residue uses a compilation-scoped tolerance.
fn collect_residue_items(
    store: &PackageStore,
    package_id: PackageId,
) -> (FxHashSet<StoreItemId>, bool) {
    let reachable = collect_reachable_from_entry(store, package_id);
    let mut residue_items: FxHashSet<StoreItemId> = FxHashSet::default();

    for store_id in &reachable {
        let package = store.get(store_id.package);
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_node_in_callable(package, decl, &mut |node| match node {
                crate::walk_utils::CallableNode::Pat(pat_id) => {
                    if ty_contains_arrow_through_udts(store, &package.get_pat(pat_id).ty) {
                        residue_items.insert(*store_id);
                    }
                }
                crate::walk_utils::CallableNode::Expr(expr_id) => {
                    if expr_is_defunc_residue(package, package.get_expr(expr_id)) {
                        residue_items.insert(*store_id);
                    }
                }
                crate::walk_utils::CallableNode::Block(_)
                | crate::walk_utils::CallableNode::Stmt(_) => {}
            });
        }
    }

    let package = store.get(package_id);
    let mut entry_has_residue = false;
    if let Some(entry) = package.entry {
        crate::walk_utils::for_each_node_from_expr_root(package, entry, &mut |node| match node {
            crate::walk_utils::CallableNode::Pat(pat_id) => {
                if ty_contains_arrow_through_udts(store, &package.get_pat(pat_id).ty) {
                    entry_has_residue = true;
                }
            }
            crate::walk_utils::CallableNode::Expr(expr_id) => {
                if expr_is_defunc_residue(package, package.get_expr(expr_id)) {
                    entry_has_residue = true;
                }
            }
            crate::walk_utils::CallableNode::Block(_)
            | crate::walk_utils::CallableNode::Stmt(_) => {}
        });
    }

    (residue_items, entry_has_residue)
}

fn expr_is_defunc_residue(package: &Package, expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::Closure(_, _)) || indirect_callee_id(package, expr).is_some()
}

/// Finds callees whose dispatch still needs resolution, including computed
/// factory results. Literal closures are counted separately as closure residue.
fn indirect_callee_id(package: &Package, expr: &Expr) -> Option<ExprId> {
    let ExprKind::Call(callee, _) = expr.kind else {
        return None;
    };
    let (base, _) = peel_body_functors(package, callee);
    let callee = package.get_expr(base);
    (!matches!(
        callee.kind,
        ExprKind::Var(Res::Item(_), _) | ExprKind::Closure(..)
    ) && ty_contains_arrow(&callee.ty))
    .then_some(base)
}

/// The per-package inputs the remaining-work count needs to decide whether a
/// closure is already consumed.
struct ConsumedClosureScope {
    /// The two set conditions, projected to this package.
    consumed: ConsumedClosuresInPackage,
    /// Expression ids inside a live call-argument subtree in this package.
    call_args: FxHashSet<ExprId>,
}

/// Builds counting scopes only for reachable packages with consumed targets,
/// using the same package projection and live-dependency walk as cleanup.
fn collect_consumed_closure_scopes(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    consumed: &ConsumedClosures,
) -> FxHashMap<PackageId, ConsumedClosureScope> {
    let mut scopes = FxHashMap::default();
    if consumed.is_empty() {
        return scopes;
    }

    for pkg_id in collect_reachable_package_closure(package_id, reachable) {
        let consumed_local = consumed.project(pkg_id);
        if !consumed_local.has_targets() {
            continue;
        }
        let package = store.get(pkg_id);
        let local_item_ids: Vec<LocalItemId> =
            reachable_local_callables(package, pkg_id, reachable)
                .map(|(id, _)| id)
                .collect();
        let call_args =
            collect_live_call_arg_exprs(package, pkg_id, &local_item_ids, &consumed_local.skipped);
        scopes.insert(
            pkg_id,
            ConsumedClosureScope {
                consumed: consumed_local,
                call_args,
            },
        );
    }

    scopes
}

/// Applies cleanup's full eligibility predicate to exclude replaced or
/// replaceable closure occurrences from the remaining-work count.
fn closure_is_consumed(
    scope: Option<&ConsumedClosureScope>,
    owner_item: Option<LocalItemId>,
    target: LocalItemId,
    expr_id: ExprId,
) -> bool {
    scope.is_some_and(|scope| {
        scope.consumed.set_conditions_hold(owner_item, target)
            && !scope.call_args.contains(&expr_id)
    })
}

/// Checks for an arrow at the root or beneath tuple fields.
///
/// UDTs and arrays are opaque to this narrow predicate. That is not a statement
/// of the pass's supported inputs: analysis expands UDTs and recognizes
/// callable-array parameters separately, including statically resolvable
/// indexed dispatch. Use a predicate that expands the required wrappers when
/// checking those forms.
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
/// Arrays remain opaque here as well.
///
/// Unguarded UDT recursion; terminates only because the frontend rejects cyclic UDTs.
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
/// Runtime capture values are threaded as ordinary arguments; a capture-free
/// callable embedded into the body must also participate in identity.
/// A `Dynamic` argument is filtered out before reaching
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
            target,
            functor,
            captures,
        } => ConcreteCallableKey::Closure {
            target: StoreItemId::from((call_pkg_id, *target)),
            functor: *functor,
            occurrence: None,
            embedded: (captures.len() == 1)
                .then(|| captures[0].static_callable)
                .flatten(),
        },
        ConcreteCallable::Dynamic => ConcreteCallableKey::Global {
            item_id: hof_item_id,
            functor: FunctorApp::default(),
        },
    }
}

/// Resolves the concrete callable supplied at `path` within a call's argument
/// expression to the dispatch key it would produce, when that can be decided
/// from the expression tree alone.
///
/// A synthesized recursive specialization must check each of its self-calls
/// against the specialization's own key before its callable slot is stripped
/// and the call retargeted; otherwise a self-call that forwards a *different*
/// callable would be silently rerouted to the wrong specialization. This
/// resolver recognizes the two capture-free argument forms — a direct global
/// item reference and a capture-free closure — each optionally wrapped in `Adj`/`Ctl`
/// body functors, and mints their key through [`concrete_callable_key`], the
/// same reduction used when a specialization's [`SpecKey`] is built, so a
/// resolved self-call argument keys identically to the specialization it
/// targets. This syntax-only resolver does not recover embedded capture facts;
/// such a self-call cannot match an embedding specialization without analysis.
/// Closures with runtime captures also need the normal call-site rewrite to
/// append their environment operands; merely removing their callable slot
/// would leave the recursive call missing arguments.
///
/// Arguments that would require flow-sensitive reaching definitions (such as a
/// forwarded local parameter) or cross-package return tracing are reported as
/// `None`, so a caller can fail safe rather than assume a match. This is a
/// deliberately narrow subset of the analysis-phase [`analysis`] resolver,
/// which additionally traces locals, blocks, `if` branches, indexed arrays, and
/// same-package returns using state that is not reconstructed here.
pub(crate) fn resolve_self_call_arg_key(
    package: &Package,
    package_id: PackageId,
    args_expr_id: ExprId,
    path: &[usize],
    hof_item_id: ItemId,
) -> Option<ConcreteCallableKey> {
    let slot_expr_id = arg_expr_at_path(package, args_expr_id, path)?;
    let (base_id, outer_functor) = peel_body_functors(package, slot_expr_id);
    let callable = match &package.get_expr(base_id).kind {
        ExprKind::Var(Res::Item(item_id), _) => ConcreteCallable::Global {
            item_id: *item_id,
            functor: outer_functor,
        },
        ExprKind::Closure(captures, target) if captures.is_empty() => ConcreteCallable::Closure {
            target: *target,
            captures: Vec::new(),
            functor: outer_functor,
        },
        _ => return None,
    };
    Some(concrete_callable_key(package_id, &callable, hof_item_id))
}

/// Walks `path` (a sequence of tuple element indices) into an argument
/// expression and returns the sub-expression found at that position.
///
/// An empty path yields the argument expression itself. Only literal tuple
/// layers are traversed; a path that runs past a non-tuple expression or an
/// out-of-range index yields `None`, matching the fail-safe contract of
/// [`resolve_self_call_arg_key`].
fn arg_expr_at_path(package: &Package, expr_id: ExprId, path: &[usize]) -> Option<ExprId> {
    let Some((&index, rest)) = path.split_first() else {
        return Some(expr_id);
    };
    let ExprKind::Tuple(elements) = &package.get_expr(expr_id).kind else {
        return None;
    };
    let &element_id = elements.get(index)?;
    arg_expr_at_path(package, element_id, rest)
}

/// Builds the deduplication key for a single call site's specialization. This
/// is the length-1 shim over [`build_combined_spec_key`], including the exact
/// callable parameter position removed by the specialization.
pub(crate) fn build_spec_key(call_site: &CallSite) -> SpecKey {
    build_combined_spec_key(call_site.hof_item_id, &[call_site])
}

/// Builds the combined deduplication key for a group of `Single`-resolved call
/// sites that share one `call_expr_id`, one per arrow parameter of the HOF.
///
/// The group is sorted by `(top_level_param, field_path)` ascending so that the
/// resulting `concrete_args` ordering is deterministic and position-aligned
/// with the parameter order the specialize/rewrite sides consume. Distinct
/// argument combinations and removed positions therefore map to distinct keys,
/// while identical combinations deduplicate to one specialization, including same-target
/// producer closures whose differing runtime scalar captures are not part of
/// the key.
pub(crate) fn build_combined_spec_key(hof_id: ItemId, group: &[&CallSite]) -> SpecKey {
    build_combined_spec_key_with_occurrences(hof_id, group, false)
}

/// Builds the combined dedup key for a group, picking the right occurrence
/// policy for the group's shape.
///
/// A normal multi-argument group has one member per distinct parameter
/// position, so occurrences never repeat. A *static callable-array* group is
/// the exception: several members fill the same array parameter position, and
/// those repeats must stay distinct in the key (see
/// [`build_static_callable_array_combined_spec_key`]). This dispatches to the
/// occurrence-preserving builder for array groups and the plain builder
/// otherwise.
pub(crate) fn build_combined_spec_key_for_group(hof_id: ItemId, group: &[&CallSite]) -> SpecKey {
    if is_static_callable_array_combined_group(group) {
        build_static_callable_array_combined_spec_key(hof_id, group)
    } else {
        build_combined_spec_key(hof_id, group)
    }
}

/// Builds the combined dedup key for a static callable-array group, keeping
/// each repeated array slot distinct.
///
/// Same-target closures can compare equal despite different runtime captures.
/// An occurrence index gives each repeated slot a distinct identity without
/// including runtime values in the key. Embedded callable identities still
/// participate in the underlying closure key.
pub(crate) fn build_static_callable_array_combined_spec_key(
    hof_id: ItemId,
    group: &[&CallSite],
) -> SpecKey {
    build_combined_spec_key_with_occurrences(hof_id, group, true)
}

/// Shared implementation behind the combined-spec-key builders: turns a group
/// of call sites into one deterministic [`SpecKey`].
///
/// The members are sorted by parameter position so the key's argument order is
/// stable and aligns with what the specialize/rewrite phases consume. Each
/// position is also stored in the key: sorting alone cannot distinguish
/// single-argument specializations that remove different slots. Each member is
/// then reduced to its concrete-callable key.
///
/// `preserve_repeated_occurrences` controls how repeats at the same position
/// are keyed. Same-target closures with the same functor and embedded callable
/// identity normally have equal keys, independent of runtime capture values.
/// For array groups, repeated positions receive occurrence indices so their
/// closure entries remain distinguishable. Neither mode removes vector entries.
///
/// # Transformation
///
/// ```text
/// // array position filled by three closures over the same target `f`:
/// //   preserve_repeated_occurrences = false =>  [f, f, f]   (equal entry keys)
/// //   preserve_repeated_occurrences = true  =>  [f#0, f#1, f#2]  (distinct)
/// ```
fn build_combined_spec_key_with_occurrences(
    hof_id: ItemId,
    group: &[&CallSite],
    preserve_repeated_occurrences: bool,
) -> SpecKey {
    // Order distinct parameter slots consistently, preserving discovery order
    // within a repeated slot because that order represents array elements.
    let mut members: Vec<&CallSite> = group.to_vec();
    members.sort_by(|a, b| {
        a.top_level_param
            .cmp(&b.top_level_param)
            .then_with(|| a.field_path.cmp(&b.field_path))
    });
    // For array groups, first tally how many members land on each position so we
    // only bother stamping occurrence indices where a position actually repeats.
    let mut position_counts: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    if preserve_repeated_occurrences {
        for cs in &members {
            *position_counts
                .entry((cs.top_level_param, cs.field_path.clone()))
                .or_default() += 1;
        }
    }
    // Running per-position counter used to hand out 0, 1, 2, ... to repeats.
    let mut occurrences: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    let concrete_args = members
        .iter()
        .map(|cs| {
            let position = (cs.top_level_param, cs.field_path.clone());
            // Only assign an occurrence index when this is an array group and
            // this position is used more than once; otherwise leave it `None`
            // so ordinary calls keep their original (dedup-friendly) keys.
            let occurrence = (preserve_repeated_occurrences
                && position_counts.get(&position).copied().unwrap_or_default() > 1)
                .then(|| {
                    let next = occurrences.entry(position).or_default();
                    let value = *next;
                    *next += 1;
                    value
                });
            // Reduce the argument to its dedup key, then stamp the occurrence
            // index into it when the value is a closure.
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
        param_positions: members
            .iter()
            .map(|cs| (cs.top_level_param, cs.field_path.clone()))
            .collect(),
        concrete_args,
    }
}

/// Reports whether a group is a *static callable-array* group: two or more
/// members filling the exact same parameter position.
///
/// Under normal combined specialization each member occupies a distinct
/// position. This is only a positional pre-check; eligibility checks elsewhere
/// distinguish array elements from conditional candidates for the same slot.
pub(crate) fn is_static_callable_array_combined_group(group: &[&CallSite]) -> bool {
    // Count repeated positions without inspecting types or branch conditions.
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

/// Returns whether removing the given callable paths consumes an entire
/// structural input. UDT wrappers must be resolved before calling this helper.
///
/// A surviving unit-valued field is still data: an empty result type alone is
/// not evidence that every original field was removed.
pub(super) fn callable_removals_consume_ty(ty: &Ty, paths: &[&[usize]]) -> bool {
    if paths.iter().any(|path| path.is_empty()) {
        return true;
    }
    let Ty::Tuple(fields) = ty else {
        return false;
    };
    !fields.is_empty()
        && fields.iter().enumerate().all(|(index, field)| {
            let children: Vec<_> = paths
                .iter()
                .filter_map(|path| {
                    let (head, tail) = path.split_first()?;
                    (*head == index).then_some(tail)
                })
                .collect();
            callable_removals_consume_ty(field, &children)
        })
}

/// Detects a dispatched tuple field separated from a later static global field.
///
/// Per-row specialization and removal do not agree on this mixed layout.
/// Both phases conservatively decline it, preserving residual dispatch rather
/// than producing a call whose arguments no longer match its specialization.
pub(super) fn dispatched_precedes_detached_static(group: &[&CallSite]) -> bool {
    group.iter().any(|dispatched| {
        !dispatched.condition.is_empty()
            && dispatched.field_path.len() == 1
            && group.iter().any(|constant| {
                constant.condition.is_empty()
                    && matches!(constant.callable_arg, ConcreteCallable::Global { .. })
                    && constant.top_level_param == dispatched.top_level_param
                    && constant.field_path.len() == 1
                    && constant.field_path[0] > dispatched.field_path[0] + 1
            })
    })
}

/// Whether static arguments can share one specialization and call-site rewrite.
///
/// Both phases use this decision to keep their argument layouts synchronized.
/// Distinct parameter positions are combined only when nested tuple slots can
/// be removed whole. Static callable arrays are the exception: all candidate
/// occurrences must reach one clone for its in-body index dispatch.
/// Outer controlled calls stay on the per-row path.
///
/// `package` must own `group`'s shared call expression.
pub(super) fn is_combined_eligible(
    package: &Package,
    group: &[&CallSite],
    udts: &UdtMetadata,
) -> bool {
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
    let static_callable_array_group = is_static_callable_array_group(package, group, udts)
        || has_static_top_level_callable_array_position(package, group, udts);
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
    // Ordinary groups are eligible only for top-level callable parameters or
    // complete sets of immediate callable fields in a tuple parameter.
    // This is a conservative routing policy, not a restriction of the shared
    // batched remover; callable-array groups above can use deeper partial paths.
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
            // Keep deeper ordinary groups on the per-row path.
            _ => return false,
        }
    }
    let arrow_input = udts.resolve(&arrow.input);
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

/// Reports whether `group` is a set of static candidates for a single
/// array-of-callable parameter (`(Qubit => Unit)[]` and the like).
///
/// Unlike [`is_static_callable_array_combined_group`], which only counts
/// repeated positions, this also confirms two things: that every member truly
/// sits at the *same* parameter position (and is a clean static candidate — no
/// branch condition, not `Dynamic`), and that the type at that position
/// resolves to an `Array` whose element is an `Arrow`. Those extra checks are
/// why this is used for combined-eligibility, where the actual element type
/// matters, rather than the cheap positional pre-check.
///
/// The members describe the elements of one forwarded callable array, so the
/// group specializes to a single clone that dispatches on the array index
/// inside the HOF body.
fn is_static_callable_array_group(
    package: &Package,
    group: &[&CallSite],
    udts: &UdtMetadata,
) -> bool {
    // Take the first member as the reference position; an empty group is not an
    // array group.
    let Some(first) = group.first() else {
        return false;
    };
    // Every member must fill the exact same parameter slot with a clean static
    // candidate. Any position mismatch, branch condition, or `Dynamic` value
    // means this is not a single-array candidate set.
    if group.iter().any(|call_site| {
        call_site.top_level_param != first.top_level_param
            || call_site.field_path != first.field_path
            || call_site.hof_input_is_tuple != first.hof_input_is_tuple
            || !call_site.condition.is_empty()
            || matches!(call_site.callable_arg, ConcreteCallable::Dynamic)
    }) {
        return false;
    }

    // Recover the HOF's arrow type so we can inspect the type sitting at the
    // shared parameter position.
    let call_expr = package.get_expr(first.call_expr_id);
    let ExprKind::Call(callee_id, _) = call_expr.kind else {
        return false;
    };
    let Ty::Arrow(ref arrow) = package.get_expr(callee_id).ty else {
        return false;
    };

    // Descend to the container type at the top-level slot: for a tuple-input HOF
    // that is the element at `top_level_param`; otherwise the whole input is the
    // single parameter.
    let arrow_input = udts.resolve(&arrow.input);
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
    // Follow the field path into any nested tuple to reach the exact selected
    // type; a non-tuple hop along the way disqualifies the group.
    let selected_ty = first
        .field_path
        .iter()
        .try_fold(container, |ty, index| match ty {
            Ty::Tuple(tys) => tys.get(*index),
            _ => None,
        });
    // The group is a static callable array only if that type is an array of
    // arrows.
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
    udts: &UdtMetadata,
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
    let arrow_input = udts.resolve(&arrow.input);

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
fn has_static_top_level_callable_array_position(
    package: &Package,
    group: &[&CallSite],
    udts: &UdtMetadata,
) -> bool {
    let repeated_positions = repeated_top_level_positions(group);
    let [position] = repeated_positions.as_slice() else {
        return false;
    };
    if !static_callable_array_positions(package, group, udts).contains(position) {
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
    udts: &UdtMetadata,
) -> bool {
    static_callable_array_positions(package, group, udts).len() >= 2
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
/// removes it from the live call sites. The caller treats this shape together
/// with a per-row specialization entry as a consistency failure. This predicate
/// itself examines only the call-site group, not which specializations ran.
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
