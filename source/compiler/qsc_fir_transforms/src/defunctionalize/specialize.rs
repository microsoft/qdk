// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Specialization phase of the defunctionalization pass.
//!
//! For each call site where a higher-order function is invoked with a concrete
//! callable argument, this module clones the HOF body and transforms it so
//! that the callable parameter is replaced by a direct call to the concrete
//! callee. A deduplication map ensures that identical `SpecKey`s produce only
//! one specialization.
//!
//! # Post-transform retyping
//!
//! Cloning a HOF body replaces one or more indirect callable references,
//! typed as arrow, with direct item references typed as the callable's
//! concrete signature. The surrounding expressions, statements, and blocks
//! that flowed those callable values can still carry stale types, so a cascade
//! of `refresh_*_types` helpers
//! ([`refresh_rewritten_value_types`], [`refresh_block_types`],
//! [`refresh_stmt_types`], [`refresh_expr_types`]) re-runs type propagation
//! across the cloned body. This updates supported enclosing value shapes;
//! it does not remove every arrow type. Direct callee expressions remain
//! arrow-typed, and [`crate::invariants::InvariantLevel::PostDefunc`] permits
//! some residual callable values.

use super::captures::captures_belong_to_destination;
use super::rewrite::CallConvention;
use super::types::{
    AnalysisResult, CallSite, CallableParam, CaptureScope, CapturedVar, ConcreteCallable,
    ConcreteCallableKey, Error, InputRemoval, ScopedLocal, SpecKey, UdtMetadata, compose_functors,
    peel_body_functors,
};
use super::{
    apply_target_input_at_control_path, build_combined_spec_key, build_combined_spec_key_for_group,
    build_spec_key, concrete_callable_key, dispatched_precedes_detached_static,
    has_multiple_forwarded_callable_arrays, is_combined_eligible, partition_mixed_branch_split,
    resolve_self_call_arg_key,
};
use crate::cloner::FirCloner;
use crate::fir_builder::{
    alloc_block, alloc_block_expr, alloc_call_expr, alloc_expr, alloc_expr_stmt,
    alloc_functor_wrapped_expr, alloc_item_var_expr, alloc_local_var, alloc_local_var_expr,
    alloc_semi_stmt, alloc_unit_expr, functored_specs,
};
use crate::package_assigners::PackageAssigners;
use crate::walk_utils::{
    expr_is_side_effect_free, expressions_in_postorder_from_roots, for_each_expr_in_callable_impl,
};
use qsc_data_structures::functors::FunctorApp;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::PackageSpan;
use qsc_fir::fir::{
    Block, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath, Ident,
    Item, ItemId, ItemKind, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup,
    PackageStore, Pat, PatId, PatKind, Res, Stmt, StmtId, StoreItemId, Visibility,
};
use qsc_fir::ty::Prim;
use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, Ty};
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write as _;
use std::rc::Rc;

#[cfg(test)]
mod tests;

/// Maximum number of specializations a single HOF may generate before a
/// warning diagnostic is emitted. Mirrors the LLVM `FuncSpec` `MaxClones`
/// budget, adapted as a diagnostic-only threshold.
const EXCESSIVE_SPECIALIZATION_THRESHOLD: usize = 10;

/// Hard upper bound on the number of *distinct* specializations a single HOF may
/// accumulate across all fixpoint iterations before defunctionalization fails
/// closed with a fatal [`crate::defunctionalize::Error::RecursiveSpecialization`].
///
/// This is the fatal backstop for [`EXCESSIVE_SPECIALIZATION_THRESHOLD`]: the
/// warning threshold flags a single wide pass, whereas this cap detects a HOF
/// whose distinct specialization set keeps growing across iterations, the
/// signature of a degenerate recursive shape that could otherwise expand
/// without bound. It is set an order of magnitude above the warning threshold
/// so that legitimately wide dispatch (dozens of distinct callables forwarded
/// to one HOF) never trips it, while runaway recursive growth is still stopped.
pub(super) const CUMULATIVE_SPECIALIZATION_CAP: usize = 100;

/// Base name for synthesized closure-capture locals; a per-call counter is
/// appended (`_.capture_0`, `_.capture_1`, …). The in-memory `Ident.name`
/// carries a `.` sentinel, which is never a valid Q# identifier character; the
/// Parseable render restores the original `__capture_0` spelling.
pub(super) const CAPTURE_NAME_PREFIX: &str = "_.capture";

/// Set of `LocalVarId`s that alias a nested callable parameter after
/// destructuring (e.g. `let (op, _) = pair;` makes `op` an alias).
type AliasSet = FxHashSet<LocalVarId>;

/// Closure-capture threading record for one specialized parameter: the closure
/// body's `LocalItemId` paired with the captured variables and their types.
/// `None` marks an argument slot that holds a global callable rather than a
/// closure.
type ClosureInfo = Option<ClosureSpecializationInfo>;

/// Per-parameter record produced when a closure argument is specialized: the
/// closure's lifted-lambda target, the runtime captures threaded onto the
/// specialized HOF input as trailing parameters, and an optional sole capture
/// with `static_callable` metadata to be embedded in a private target clone.
#[derive(Clone)]
struct ClosureSpecializationInfo {
    target: LocalItemId,
    capture_bindings: Vec<CaptureBinding>,
    callable_capture: Option<CapturedCallableSpecialization>,
}

#[derive(Clone)]
struct CaptureBinding {
    local: ScopedLocal,
    ty: Ty,
}

/// A captured value that is itself a statically-known callable, recorded so the
/// closure target can be specialized for it — the callable is inlined into the
/// target body and its capture slot removed — rather than threaded as a runtime
/// operand.
#[derive(Clone)]
struct CapturedCallableSpecialization {
    capture_idx: usize,
    capture_ty: Ty,
    concrete: ConcreteCallable,
}
/// A lifted closure target and the input binding being specialized within it.
/// Caller-local identities can differ across occurrences of the same target,
/// and numeric capture positions can shift when earlier slots are replaced.
type SpecializedCaptureKey = (LocalItemId, LocalVarId);

/// Tracks captured-parameter specialization across closure occurrences sharing
/// lifted targets, including occurrences in different functor specializations.
/// Target bindings share a mutation only when their embedded callable selections
/// agree; per-occurrence layouts keep later capture indices aligned as each
/// capture list is rewritten.
struct CaptureSpecializations<'a> {
    udts: &'a UdtMetadata,
    failed: bool,
    /// Ordered embedded-callable keys selected for each target input binding.
    specialized: FxHashMap<SpecializedCaptureKey, Vec<ConcreteCallableKey>>,
    /// Each target's input bindings when first encountered by this tracker.
    /// Patterns without a direct binding are represented by `None`.
    original_inputs: FxHashMap<LocalItemId, Vec<Option<LocalVarId>>>,
    /// Each occurrence's current slots mapped back to the saved target bindings.
    /// Replacement slots use `None` and resolve against the current target input.
    occurrence_inputs: FxHashMap<ExprId, Vec<Option<LocalVarId>>>,
}

impl<'a> CaptureSpecializations<'a> {
    fn new(udts: &'a UdtMetadata) -> Self {
        Self {
            udts,
            failed: false,
            specialized: FxHashMap::default(),
            original_inputs: FxHashMap::default(),
            occurrence_inputs: FxHashMap::default(),
        }
    }
    /// Records the binding and callable selection represented by a capture slot,
    /// returning `true` only on its first visit. Reuse requires equal ordered
    /// callable keys; a conflicting selection or missing binding sets `failed`.
    ///
    /// New occurrences start from the saved target layout even if another
    /// occurrence has already changed the target's input. Existing occurrences
    /// retain their own slot shifts through `replace_slot`. A slot without a
    /// saved binding falls back to the current target input, allowing newly
    /// threaded captures to be identified without reusing the removed binding.
    fn first_use(
        &mut self,
        package: &Package,
        target: LocalItemId,
        occurrence: ExprId,
        capture_index: usize,
        concrete: &ConcreteCallable,
        concrete_group: &[ConcreteCallable],
    ) -> bool {
        let ItemKind::Callable(decl) = &package.get_item(target).kind else {
            return false;
        };
        let input = package.get_pat(decl.input);
        let patterns = match &input.kind {
            PatKind::Tuple(patterns) => patterns.clone(),
            _ => vec![decl.input],
        };
        let current_inputs: Vec<_> = patterns
            .iter()
            .map(|pattern| match &package.get_pat(*pattern).kind {
                PatKind::Bind(ident) => Some(ident.id),
                _ => None,
            })
            .collect();
        let original = self
            .original_inputs
            .entry(target)
            .or_insert_with(|| current_inputs.clone());
        let occurrence = self
            .occurrence_inputs
            .entry(occurrence)
            .or_insert_with(|| original.clone());
        let binding = occurrence
            .get(capture_index)
            .copied()
            .flatten()
            .or_else(|| current_inputs.get(capture_index).copied().flatten());
        let Some(binding) = binding else {
            self.failed = true;
            return false;
        };
        let callables = if concrete_group.is_empty() {
            std::slice::from_ref(concrete)
        } else {
            concrete_group
        };
        let selection: Vec<_> = callables
            .iter()
            .map(|callable| {
                super::concrete_callable_key(
                    package.id,
                    callable,
                    ItemId {
                        package: package.id,
                        item: target,
                    },
                )
            })
            .collect();
        if let Some(previous) = self.specialized.get(&(target, binding)) {
            self.failed |= previous != &selection;
            false
        } else {
            self.specialized.insert((target, binding), selection);
            true
        }
    }

    /// Mirrors replacement of one occurrence's capture slot with `count` operands.
    ///
    /// Zero removes the slot; otherwise `None` placeholders stand for the newly
    /// threaded captures. Surviving slots keep their original binding identities
    /// at their shifted positions. This updates only the occurrence map, not FIR
    /// or the shared specialization history; untracked or out-of-range slots are
    /// left unchanged.
    fn replace_slot(&mut self, occurrence: ExprId, capture_index: usize, count: usize) {
        if let Some(bindings) = self.occurrence_inputs.get_mut(&occurrence)
            && capture_index < bindings.len()
        {
            bindings.splice(
                capture_index..=capture_index,
                std::iter::repeat_n(None, count),
            );
        }
    }
}

/// Resolves a `ConcreteCallable` to a compact label for inclusion in
/// specialized callable names.  For globals, produces the callable name
/// with a functor prefix when non-body (e.g. `H`, `Adj S`, `Ctl X`).
/// For closures, produces `closure`.
fn resolve_callable_arg_label(store: &PackageStore, arg: &ConcreteCallable) -> String {
    match arg {
        ConcreteCallable::Global { item_id, functor } => {
            let pkg = store.get(item_id.package);
            let item = pkg.get_item(item_id.item);
            let base = if let ItemKind::Callable(decl) = &item.kind {
                decl.name.name.to_string()
            } else {
                format!("Item({})", item_id.item)
            };
            match (functor.adjoint, functor.controlled > 0) {
                (false, false) => base,
                (true, false) => format!("Adj {base}"),
                (false, true) => format!("Ctl {base}"),
                (true, true) => format!("CtlAdj {base}"),
            }
        }
        ConcreteCallable::Closure { .. } => "closure".to_string(),
        ConcreteCallable::Dynamic => "dynamic".to_string(),
    }
}

/// Specializes higher-order functions for each concrete callable argument
/// discovered during analysis.
///
/// Returns a map from `SpecKey` to the `StoreItemId` of each newly created
/// specialized callable.
pub(super) fn specialize(
    store: &mut PackageStore,
    analysis: &AnalysisResult,
    assigners: &mut PackageAssigners,
) -> (FxHashMap<SpecKey, StoreItemId>, Vec<Error>) {
    let udts = &analysis.udt_metadata;
    let mut dedup: FxHashMap<SpecKey, StoreItemId> = FxHashMap::default();
    let mut errors: Vec<Error> = Vec::new();

    // Build a lookup from each HOF's StoreItemId => CallableParam. This
    // lowest-index entry serves the per-row path, used by single-arrow-param
    // HOFs and branch-split candidate sets where every row resolves the same
    // parameter; it cannot distinguish between separate arrow parameters.
    let mut param_lookup: FxHashMap<StoreItemId, &CallableParam> = FxHashMap::default();
    for p in &analysis.callable_params {
        param_lookup.entry(p.callable_id).or_insert(p);
    }

    // Build a precise lookup keyed by parameter position so a multi-argument
    // call recovers the exact parameter for each distinct slot instead of
    // collapsing every arrow parameter onto the lowest index.
    let mut param_by_position: FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam> =
        FxHashMap::default();
    for p in &analysis.callable_params {
        param_by_position.insert((p.callable_id, p.top_level_param, p.field_path.clone()), p);
    }

    let groups = group_call_sites_by_expression(analysis);

    // Each group is one call expression's rows. The four branches below are
    // tried in priority order: the earliest one that claims a group wins, and
    // the order matters because the later paths would silently mishandle shapes
    // the earlier paths are meant to catch.
    for group in &groups {
        // 1. Hard-decline the unsupported shape first (two or more callable
        //    arrays forwarded through one call). Doing this before anything else
        //    keeps such a group off the per-row path, which would otherwise
        //    quietly collapse each array down to a single member.
        if try_decline_multiple_callable_arrays(udts, store, group, &mut errors) {
            continue;
        }

        // 2. Combined multi-argument specialization. Specialize and rewrite
        //    consult the same eligibility predicate so they agree on which call
        //    sites are combined. The borrow is scoped here so the combined
        //    branch can re-borrow the store mutably below.
        let combined = {
            let package = store.get(group[0].call_pkg_id);
            is_combined_eligible(udts, package, group)
        };
        if combined {
            specialize_combined_group(
                udts,
                store,
                group,
                &param_by_position,
                &mut dedup,
                assigners,
            );
            continue;
        }

        // 3. Mixed branch-split: one parameter dispatched over several
        //    candidates alongside single-valued sibling parameters (at least one
        //    a producer closure). This must be tried before the per-row path so
        //    each dispatch leaf inlines the sibling producer closure in the same
        //    pass; the per-row path would instead emit a lone producer spec that
        //    the closure-consistency check later rejects. Returns `true` only
        //    when the group had this shape and could be handled here.
        if specialize_mixed_branch_split_group(
            udts,
            store,
            group,
            &param_by_position,
            &mut dedup,
            assigners,
        ) {
            continue;
        }

        // 4. Fallback: specialize each row independently under its own
        //    single-argument key. This covers single-arrow-param HOFs and
        //    branch-split candidate sets, and is the path every group reaches
        //    when none of the more specific shapes above applied.
        if dispatched_precedes_detached_static(group) {
            continue;
        }
        specialize_per_row_group(
            udts,
            store,
            group,
            &param_lookup,
            &param_by_position,
            &mut dedup,
            &mut errors,
            assigners,
        );
    }

    refresh_materialized_call_outputs(store, dedup.values().copied().collect());
    report_excessive_specializations(store, &dedup, &mut errors);

    (dedup, errors)
}

/// Retargeting an indirect call can expose stronger output functors. Resolve
/// declarations with package-qualified IDs, then run the existing retyping
/// cascade so enclosing bindings and blocks retain the concrete result type.
/// Follow referenced bodies as well, including lifted capture targets rewritten
/// in place during specialization.
fn refresh_materialized_call_outputs(store: &mut PackageStore, mut pending: Vec<StoreItemId>) {
    let mut visited = FxHashSet::default();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let package = store.get(id.package);
        let Some(item) = package.items.get(id.item) else {
            continue;
        };
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        let implementation = decl.implementation.clone();
        let mut updates = Vec::new();
        for_each_expr_in_callable_impl(package, &implementation, &mut |expr_id, expr| {
            match expr.kind {
                ExprKind::Var(Res::Item(item), _) => {
                    pending.push(StoreItemId::from((item.package, item.item)));
                }
                ExprKind::Closure(_, target) => {
                    pending.push(StoreItemId::from((id.package, target)));
                }
                _ => {}
            }
            let ExprKind::Call(callee_id, _) = expr.kind else {
                return;
            };
            let (base_id, _) = peel_body_functors(package, callee_id);
            let target = match package.get_expr(base_id).kind {
                ExprKind::Var(Res::Item(item), _) => StoreItemId::from((item.package, item.item)),
                ExprKind::Closure(_, item) => StoreItemId::from((id.package, item)),
                _ => return,
            };
            let Some(item) = store.get(target.package).items.get(target.item) else {
                return;
            };
            let ItemKind::Callable(target) = &item.kind else {
                return;
            };
            let Ty::Arrow(arrow) = &package.get_expr(callee_id).ty else {
                return;
            };
            if target.output != expr.ty
                && dispatch_layout_types_compatible(&target.output, &expr.ty)
                && dispatch_layout_types_compatible(&target.output, &arrow.output)
            {
                updates.push((expr_id, callee_id, target.output.clone()));
            }
        });
        if updates.is_empty() {
            continue;
        }
        let package = store.get_mut(id.package);
        for (expr_id, callee_id, output) in updates {
            package.exprs.get_mut(expr_id).expect("call exists").ty = output.clone();
            let callee = package.exprs.get_mut(callee_id).expect("callee exists");
            if let Ty::Arrow(arrow) = &mut callee.ty {
                *arrow.output = output;
            }
        }
        refresh_rewritten_value_types(package, &implementation);
    }
}

/// Groups analysis call sites by their originating `(package, call expression)`.
///
/// A multi-argument HOF call contributes one row per arrow parameter; grouping
/// keeps those rows together so a combined specialization can consume them as a
/// unit. Rows that share an expression but resolve the same parameter (the
/// branch-split candidate sets) also land in the same group and are separated
/// later by the per-row path.
fn group_call_sites_by_expression(analysis: &AnalysisResult) -> Vec<Vec<&CallSite>> {
    let mut groups: Vec<Vec<&CallSite>> = Vec::new();
    let mut group_index: FxHashMap<(PackageId, ExprId), usize> = FxHashMap::default();
    for call_site in &analysis.call_sites {
        let group_key = (call_site.call_pkg_id, call_site.call_expr_id);
        if let Some(&idx) = group_index.get(&group_key) {
            groups[idx].push(call_site);
        } else {
            group_index.insert(group_key, groups.len());
            groups.push(vec![call_site]);
        }
    }
    groups
}

/// Declines a group that forwards two or more callable arrays through one call,
/// which the pass does not support.
///
/// Recording the decline here, before the eligibility branch, keeps the group
/// off the per-row path, which would otherwise collapse each array to a single
/// member. The driver revisits skipped groups every fixpoint iteration, so the
/// decline is deduplicated by call-expression span. Returns `true` when the
/// group was declined and the caller should skip it.
fn try_decline_multiple_callable_arrays(
    udts: &UdtMetadata,
    store: &PackageStore,
    group: &[&CallSite],
    errors: &mut Vec<Error>,
) -> bool {
    let package = store.get(group[0].call_pkg_id);
    if has_multiple_forwarded_callable_arrays(udts, package, group) {
        let span = package.get_expr(group[0].call_expr_id).span;
        let package_span = span;
        if !errors
            .iter()
            .any(|e| matches!(e, Error::UnsupportedMultipleCallableArrays(s) if *s == package_span))
        {
            errors.push(Error::UnsupportedMultipleCallableArrays(package_span));
        }
        return true;
    }
    false
}

/// Resolves each call site in `members_cs` to its exact [`CallableParam`] by
/// position, ordering the result ascending by parameter position so capture
/// threading and the call-site rewrite agree on operand order.
///
/// Returns `None` when any member's parameter cannot be resolved.
fn resolve_group_members<'a>(
    members_cs: &[&'a CallSite],
    hof_store_id: StoreItemId,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &'a CallableParam>,
) -> Option<Vec<(&'a CallSite, &'a CallableParam)>> {
    let mut members: Vec<(&CallSite, &CallableParam)> = Vec::with_capacity(members_cs.len());
    for cs in members_cs {
        let position_key = (hof_store_id, cs.top_level_param, cs.field_path.clone());
        let param = param_by_position.get(&position_key).copied()?;
        members.push((*cs, param));
    }
    members.sort_by(|a, b| {
        a.1.top_level_param
            .cmp(&b.1.top_level_param)
            .then_with(|| a.1.field_path.cmp(&b.1.field_path))
    });
    Some(members)
}

/// Specializes a combined multi-argument HOF call keyed by every resolved
/// argument together, inserting the new spec into `dedup`.
///
/// Recovers the exact parameter for each row via [`resolve_group_members`], then
/// clones the HOF via [`specialize_many`]. Already-specialized keys are skipped.
fn specialize_combined_group(
    udts: &UdtMetadata,
    store: &mut PackageStore,
    group: &[&CallSite],
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    dedup: &mut FxHashMap<SpecKey, StoreItemId>,
    assigners: &mut PackageAssigners,
) {
    // Combined multi-argument specialization keyed by every resolved argument
    // together.
    let hof_item_id = group[0].hof_item_id;
    let spec_key = build_combined_spec_key_for_group(hof_item_id, group);

    if dedup.contains_key(&spec_key) {
        return;
    }

    let hof_store_id = StoreItemId::from((hof_item_id.package, hof_item_id.item));
    let Some(members) = resolve_group_members(group, hof_store_id, param_by_position) else {
        return;
    };

    let target_pkg_id = group[0].call_pkg_id;
    let new_item_id = assigners.with_package(store, target_pkg_id, |store, assigner| {
        let mut assigner = assigner;
        let result = specialize_many(udts, store, target_pkg_id, &members, &mut assigner);
        (assigner, result)
    });

    if let Some(id) = new_item_id {
        dedup.insert(spec_key.clone(), id);
    }
}

/// Specializes each dispatch candidate of a mixed branch-split group into a
/// combined per-candidate specialization (`[candidate] + sibling parameters`).
///
/// A mixed branch-split group has one parameter dispatched over two or more
/// candidates plus one or more single-valued sibling arrow parameters, at least
/// one of which is a producer closure. Specializing each candidate combined with
/// its siblings lets every dispatch leaf inline the single-valued producer
/// closure in the same pass, so the closure is consumed before
/// `track_specialized_closures` or `cleanup_consumed_closures` can clear the
/// producer body on a later iteration. Skipping the per-row path for this group
/// avoids a single-argument producer specialization that the consistency check
/// in `track_specialized_closures` would otherwise reject.
///
/// Returns `true` when the group is a mixed branch-split that can be claimed by
/// this path (whether or not any candidate produced a spec); `false` leaves the
/// group for the per-row path.
fn specialize_mixed_branch_split_group(
    udts: &UdtMetadata,
    store: &mut PackageStore,
    group: &[&CallSite],
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    dedup: &mut FxHashMap<SpecKey, StoreItemId>,
    assigners: &mut PackageAssigners,
) -> bool {
    let Some((dispatch, constants)) = partition_mixed_branch_split(group) else {
        return false;
    };
    let hof_item_id = group[0].hof_item_id;
    let hof_store_id = StoreItemId::from((hof_item_id.package, hof_item_id.item));
    for candidate in &dispatch {
        let mut members_cs: Vec<&CallSite> = Vec::with_capacity(constants.len() + 1);
        members_cs.push(*candidate);
        members_cs.extend(constants.iter().copied());
        let spec_key = build_combined_spec_key(hof_item_id, &members_cs);
        if dedup.contains_key(&spec_key) {
            continue;
        }

        let Some(members) = resolve_group_members(&members_cs, hof_store_id, param_by_position)
        else {
            continue;
        };

        let target_pkg_id = candidate.call_pkg_id;
        let new_item_id = assigners.with_package(store, target_pkg_id, |store, assigner| {
            let mut assigner = assigner;
            let result = specialize_many(udts, store, target_pkg_id, &members, &mut assigner);
            (assigner, result)
        });

        if let Some(id) = new_item_id {
            dedup.insert(spec_key.clone(), id);
        }
    }
    true
}

/// Specializes every call site in a group independently under its
/// single-argument spec key (the per-row / branch-split candidate path).
///
/// Dynamic callables and unresolved parameters record diagnostics or are
/// skipped; each resolved site is cloned via [`specialize_one`], preferring the
/// exact per-position parameter so a per-row spec removes its own slot rather
/// than the lowest-index slot, keeping specialize in agreement with the rewrite
/// side's slot removal at the same parameter position.
#[allow(clippy::too_many_arguments)]
fn specialize_per_row_group(
    udts: &UdtMetadata,
    store: &mut PackageStore,
    group: &[&CallSite],
    param_lookup: &FxHashMap<StoreItemId, &CallableParam>,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    dedup: &mut FxHashMap<SpecKey, StoreItemId>,
    errors: &mut Vec<Error>,
    assigners: &mut PackageAssigners,
) {
    for call_site in group {
        let call_site: &CallSite = call_site;
        let spec_key = build_spec_key(call_site);

        // Already specialized — skip.
        if dedup.contains_key(&spec_key) {
            continue;
        }

        // Dynamic callables cannot be specialized — emit an error with the
        // call-site span so the user gets an actionable diagnostic instead of
        // the generic `FixpointNotReached` convergence error.
        if matches!(call_site.callable_arg, ConcreteCallable::Dynamic) {
            let package = store.get(call_site.call_pkg_id);
            let span = package.get_expr(call_site.call_expr_id).span;
            errors.push(Error::DynamicCallable(span));
            continue;
        }

        // The HOF may live in a foreign package, for example a generic std
        // lib callable monomorphized in place into its owning package.
        let hof_store_id =
            StoreItemId::from((call_site.hof_item_id.package, call_site.hof_item_id.item));

        // Look up the callable parameter for this HOF, preferring the exact
        // per-position match so a per-row specialization removes its own slot
        // rather than the lowest-index slot. This keeps the specialize side
        // in agreement with the rewrite side's slot removal at the same
        // parameter position.
        let position_key = (
            hof_store_id,
            call_site.top_level_param,
            call_site.field_path.clone(),
        );
        let Some(param) = param_by_position
            .get(&position_key)
            .or_else(|| param_lookup.get(&hof_store_id))
            .copied()
        else {
            continue;
        };

        // Clone the HOF and produce a specialized callable. The spec is
        // allocated into the call site's owning package via that package's
        // own assigner. Body nodes and nested closure targets are cloned into
        // that package; references to other global items retain their
        // package-qualified identities.
        let target_pkg_id = call_site.call_pkg_id;
        let new_item_id = assigners.with_package(store, target_pkg_id, |store, assigner| {
            let mut assigner = assigner;
            let result =
                specialize_one(udts, store, target_pkg_id, call_site, param, &mut assigner);
            (assigner, result)
        });

        if let Some(id) = new_item_id {
            dedup.insert(spec_key.clone(), id);
        }
    }
}

/// Emits a warning for each HOF whose specialization count exceeds
/// [`EXCESSIVE_SPECIALIZATION_THRESHOLD`].
///
/// Groups `dedup` entries by the HOF callable id embedded in each [`SpecKey`]
/// and pushes an [`Error::ExcessiveSpecializations`] for every HOF over the
/// threshold.
fn report_excessive_specializations(
    store: &PackageStore,
    dedup: &FxHashMap<SpecKey, StoreItemId>,
    errors: &mut Vec<Error>,
) {
    // Count specializations per HOF and emit a warning when the threshold
    // is exceeded. Group dedup entries by the HOF callable_id embedded in
    // each SpecKey.
    let mut specs_per_hof: FxHashMap<StoreItemId, usize> = FxHashMap::default();
    for key in dedup.keys() {
        *specs_per_hof.entry(key.hof_id).or_default() += 1;
    }
    for (hof_id, count) in &specs_per_hof {
        if *count > EXCESSIVE_SPECIALIZATION_THRESHOLD {
            let package = store.get(hof_id.package);
            let item = package.get_item(hof_id.item);
            let ItemKind::Callable(decl) = &item.kind else {
                panic!(
                    "defunctionalization specialization key references non-callable HOF {hof_id}"
                );
            };
            errors.push(Error::ExcessiveSpecializations(
                decl.name.name.to_string(),
                *count,
                decl.name.span,
            ));
        }
    }
}

/// Drives the post-transform retyping cascade across every specialization body
/// of a callable after callable references become direct.
///
/// Updates supported `Expr.ty`, `Block.ty`, and local `Pat.ty` shapes in place;
/// this is type propagation, not a `PostDefunc` invariant check.
fn refresh_rewritten_value_types(package: &mut Package, callable_impl: &CallableImpl) {
    match callable_impl {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            refresh_block_types(package, spec_impl.body.block);
            for spec in functored_specs(spec_impl) {
                refresh_block_types(package, spec.block);
            }
        }
    }
}

/// Re-computes the type of every statement in a block, returning the
/// refreshed trailing type so enclosing expressions can cascade the update.
/// The block's type becomes its trailing expression's type, or `Unit` when
/// there is no trailing `Expr`.
fn refresh_block_types(package: &mut Package, block_id: qsc_fir::fir::BlockId) -> Ty {
    let stmt_ids = package.get_block(block_id).stmts.clone();
    for stmt_id in stmt_ids {
        refresh_stmt_types(package, stmt_id);
    }

    let new_ty = package
        .get_block(block_id)
        .stmts
        .last()
        .and_then(|stmt_id| match package.get_stmt(*stmt_id).kind {
            qsc_fir::fir::StmtKind::Expr(expr_id) => Some(package.get_expr(expr_id).ty.clone()),
            qsc_fir::fir::StmtKind::Semi(_)
            | qsc_fir::fir::StmtKind::Local(_, _, _)
            | qsc_fir::fir::StmtKind::Item(_) => None,
        })
        .unwrap_or(Ty::UNIT);

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.ty = new_ty.clone();
    new_ty
}

/// Refreshes a statement's expression and retypes a local bind/discard pattern
/// to match its initializer. Tuple patterns are refreshed only when their
/// layout is compatible with the initializer's type.
fn refresh_stmt_types(package: &mut Package, stmt_id: qsc_fir::fir::StmtId) {
    let stmt = package.get_stmt(stmt_id).clone();
    match stmt.kind {
        qsc_fir::fir::StmtKind::Expr(expr_id) | qsc_fir::fir::StmtKind::Semi(expr_id) => {
            let _ = refresh_expr_types(package, expr_id);
        }
        qsc_fir::fir::StmtKind::Local(_, pat_id, expr_id) => {
            let expr_ty = refresh_expr_types(package, expr_id);
            let pat_kind = package.get_pat(pat_id).kind.clone();
            if matches!(pat_kind, PatKind::Bind(_) | PatKind::Discard) {
                let pat = package.pats.get_mut(pat_id).expect("pat not found");
                pat.ty = expr_ty;
            } else if dispatch_layout_types_compatible(&expr_ty, &package.get_pat(pat_id).ty) {
                refresh_tuple_pattern_types(package, pat_id, &expr_ty);
            }
        }
        qsc_fir::fir::StmtKind::Item(_) => {}
    }
}

fn refresh_tuple_pattern_types(package: &mut Package, pat_id: PatId, ty: &Ty) {
    let pat = package.get_pat(pat_id);
    let children = match (&pat.kind, ty) {
        (PatKind::Tuple(children), Ty::Tuple(items)) if children.len() == items.len() => children
            .iter()
            .copied()
            .zip(items.iter().cloned())
            .collect(),
        (PatKind::Bind(_) | PatKind::Discard, _) => Vec::new(),
        _ => return,
    };
    package.pats.get_mut(pat_id).expect("pattern exists").ty = ty.clone();
    for (child, ty) in children {
        refresh_tuple_pattern_types(package, child, &ty);
    }
}

/// Propagates refreshed types through blocks, conditionals, array/tuple
/// construction, and parallel bodies. Other forms recurse into supported
/// children but retain their stored result type, including calls.
#[allow(clippy::too_many_lines)]
fn refresh_expr_types(package: &mut Package, expr_id: ExprId) -> Ty {
    let expr = package.get_expr(expr_id).clone();
    let new_ty = match expr.kind {
        ExprKind::Block(block_id) => refresh_block_types(package, block_id),
        ExprKind::If(cond_id, body_id, otherwise_id) => {
            let _ = refresh_expr_types(package, cond_id);
            let body_ty = refresh_expr_types(package, body_id);
            if let Some(otherwise_id) = otherwise_id {
                let _ = refresh_expr_types(package, otherwise_id);
                body_ty
            } else {
                Ty::UNIT
            }
        }
        ExprKind::Tuple(items) => Ty::Tuple(
            items
                .into_iter()
                .map(|item_id| refresh_expr_types(package, item_id))
                .collect(),
        ),
        ExprKind::Array(items) | ExprKind::ArrayLit(items) => {
            let item_tys: Vec<Ty> = items
                .into_iter()
                .map(|item_id| refresh_expr_types(package, item_id))
                .collect();
            if let Some(item_ty) = item_tys.first() {
                Ty::Array(Box::new(item_ty.clone()))
            } else {
                expr.ty
            }
        }
        ExprKind::ArrayRepeat(value_id, count_id) => {
            let value_ty = refresh_expr_types(package, value_id);
            let _ = refresh_expr_types(package, count_id);
            Ty::Array(Box::new(value_ty))
        }
        ExprKind::Assign(lhs_id, rhs_id)
        | ExprKind::AssignOp(_, lhs_id, rhs_id)
        | ExprKind::BinOp(_, lhs_id, rhs_id)
        | ExprKind::Index(lhs_id, rhs_id)
        | ExprKind::UpdateField(lhs_id, _, rhs_id)
        | ExprKind::UpdateIndex(lhs_id, rhs_id, _)
        | ExprKind::AssignField(lhs_id, _, rhs_id)
        | ExprKind::AssignIndex(lhs_id, rhs_id, _) => {
            let _ = refresh_expr_types(package, lhs_id);
            let _ = refresh_expr_types(package, rhs_id);
            expr.ty
        }
        ExprKind::While(cond_id, block_id) => {
            let _ = refresh_expr_types(package, cond_id);
            let _ = refresh_block_types(package, block_id);
            expr.ty
        }
        ExprKind::Call(callee_id, args_id) => {
            let _ = refresh_expr_types(package, callee_id);
            let _ = refresh_expr_types(package, args_id);
            expr.ty
        }
        ExprKind::UnOp(_, inner_id)
        | ExprKind::Return(inner_id)
        | ExprKind::Fail(inner_id)
        | ExprKind::Field(inner_id, _) => {
            let _ = refresh_expr_types(package, inner_id);
            expr.ty
        }
        ExprKind::Range(start_id, step_id, end_id) => {
            if let Some(start_id) = start_id {
                let _ = refresh_expr_types(package, start_id);
            }
            if let Some(step_id) = step_id {
                let _ = refresh_expr_types(package, step_id);
            }
            if let Some(end_id) = end_id {
                let _ = refresh_expr_types(package, end_id);
            }
            expr.ty
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(component_id) = component {
                    let _ = refresh_expr_types(package, component_id);
                }
            }
            expr.ty
        }
        ExprKind::Struct(_, copy_id, fields) => {
            if let Some(copy_id) = copy_id {
                let _ = refresh_expr_types(package, copy_id);
            }
            for field in fields {
                let _ = refresh_expr_types(package, field.value);
            }
            expr.ty
        }
        ExprKind::Parallel(limit, body) => {
            if let Some(limit) = limit {
                // Refresh the limit independently; the body determines the result type.
                let _ = refresh_expr_types(package, limit);
            }
            // The type of a parallel expression is the same as the type of its body, so refresh that and return it.
            refresh_expr_types(package, body)
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {
            expr.ty
        }
    };

    let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
    expr_mut.ty = new_ty.clone();
    new_ty
}

/// Specializes a higher-order function for a group of concrete callable
/// arguments that share one call expression — one row per arrow parameter.
///
/// `group` must be ordered ascending by parameter position. The HOF is cloned
/// once; every parameter is rewritten against the shared clone so the cleanup
/// that clears a producer closure's body cannot leave a sibling parameter
/// referring to a removed body. Each closure argument's captures are threaded
/// onto the specialized input in ascending parameter order, and each closure's
/// capture-prepend post-pass is scoped to exactly the calls that parameter
/// retargeted so same-target producer closures receive their own captures.
///
/// Single-row groups delegate to [`specialize_one`] so single-arrow-param
/// specializations stay byte-identical.
#[allow(clippy::too_many_lines)]
fn specialize_many(
    udts: &UdtMetadata,
    store: &mut PackageStore,
    package_id: PackageId,
    group: &[(&CallSite, &CallableParam)],
    assigner: &mut Assigner,
) -> Option<StoreItemId> {
    if group.len() == 1 {
        let (call_site, param) = group[0];
        return specialize_one(udts, store, package_id, call_site, param, assigner);
    }

    // Every row shares one call expression and therefore one HOF.
    let hof_item_id = group[0].0.hof_item_id;
    let hof_pkg_id = hof_item_id.package;

    let (body_pkg, decl_snapshot) = {
        let package = store.get(hof_pkg_id);
        let hof_item = package.get_item(hof_item_id.item);
        let ItemKind::Callable(ref hof_decl) = hof_item.kind else {
            return None;
        };
        let body_pkg = extract_callable_body(package, hof_decl);
        let decl_snapshot = hof_decl.as_ref().clone();
        (body_pkg, decl_snapshot)
    }; // immutable borrow released

    // Name the specialization after every argument label in parameter order:
    // `HOF{labelA}{labelB}`.
    let mut spec_name = decl_snapshot.name.name.to_string();
    for (call_site, _) in group {
        let label = resolve_callable_arg_label(store, &call_site.callable_arg);
        write!(spec_name, "{{{label}}}").expect("writing to a String is infallible");
    }
    let hof_name: Rc<str> = Rc::from(spec_name);

    let target = store.get_mut(package_id);
    let new_item_id = assigner.next_item();
    let owned_assigner = std::mem::take(assigner);
    let mut cloner = FirCloner::from_assigner(owned_assigner);
    cloner.reset_maps();
    let new_specialized_item_id = ItemId {
        package: package_id,
        item: new_item_id,
    };
    cloner.set_self_item_remap(hof_item_id, new_specialized_item_id);

    // Clone the input before the impl so `local_map` holds the input parameter
    // mappings when the callable body is walked.
    let cloned_input = cloner.clone_pat(&body_pkg, decl_snapshot.input, target);
    let cloned_impl = cloner.clone_callable_impl(&body_pkg, &decl_snapshot.implementation, target);

    // Remap each parameter through the cloner's maps.
    let remapped_params = remap_group_params(&cloner, group);

    let mut new_decl = CallableDecl {
        span: decl_snapshot.span,
        kind: decl_snapshot.kind,
        name: Ident {
            id: LocalVarId::from(0_u32),
            span: decl_snapshot.name.span,
            name: hof_name,
        },
        generics: decl_snapshot.generics.clone(),
        input: cloned_input,
        output: decl_snapshot.output.clone(),
        functors: decl_snapshot.functors,
        implementation: cloned_impl,
        attrs: decl_snapshot.attrs.clone(),
    };

    // Thread each closure argument's captures onto the specialized input in
    // ascending parameter order, continuing one capture counter across
    // parameters. Capture threading must happen before recovering the assigner
    // because it allocates through the cloner.
    let (closure_infos, total_captures) = thread_group_closure_captures(
        &mut cloner,
        target,
        &mut new_decl,
        group,
        &remapped_params,
        new_item_id,
    );

    // Recover the assigner from the cloner so all subsequent allocations flow
    // through the shared pipeline assigner.
    let nested_items: Vec<_> = cloner.item_map().values().copied().collect();
    *assigner = cloner.into_assigner();

    let callable_array_position = find_callable_array_group_position(&remapped_params, group);

    if !transform_combined_callable_body(
        udts,
        target,
        package_id,
        CaptureScope::CloneScope(new_item_id),
        group,
        &remapped_params,
        &closure_infos,
        callable_array_position.as_ref(),
        &new_decl.implementation,
        assigner,
    ) {
        return None;
    }

    remove_combined_callable_params(
        udts,
        target,
        &mut new_decl,
        &remapped_params,
        callable_array_position.as_ref(),
        total_captures,
        assigner,
    );

    // Recursive references are retargeted by the cloner before the callable
    // parameter slots are removed from the specialization input. Bring those
    // recursive calls back into the same reduced argument shape as external
    // call sites rewritten by the defunctionalization rewrite phase.
    //
    // The per-parameter keys are assembled in group order so they align with
    // `remapped_params`; a self-call is retargeted to this specialization only
    // when every forwarded slot matches, otherwise it is routed back to the
    // original HOF for a sibling specialization.
    let spec_arg_keys: Vec<ConcreteCallableKey> = group
        .iter()
        .map(|(call_site, _)| {
            concrete_callable_key(
                call_site.call_pkg_id,
                &call_site.callable_arg,
                call_site.hof_item_id,
            )
        })
        .collect();
    rewrite_recursive_self_call_args(
        udts,
        target,
        package_id,
        new_item_id,
        hof_item_id,
        &spec_arg_keys,
        &new_decl,
        &remapped_params,
        &nested_items,
        assigner,
    );

    let new_item = Item {
        id: new_item_id,
        span: target.synthetic_span(),
        parent: None,
        doc: Rc::from(""),
        attrs: Vec::new(),
        visibility: Visibility::Internal,
        kind: ItemKind::Callable(Box::new(new_decl)),
    };
    target.items.insert(new_item_id, new_item);

    Some(StoreItemId {
        package: package_id,
        item: new_item_id,
    })
}

/// Remaps each group member's callable parameter through the cloner's id maps
/// so it refers to the freshly cloned input pattern and locals.
fn remap_group_params(
    cloner: &FirCloner,
    group: &[(&CallSite, &CallableParam)],
) -> Vec<CallableParam> {
    group
        .iter()
        .map(|(_, param)| {
            let remapped_param_var = *cloner
                .local_map()
                .get(&param.param_var)
                .expect("param_var should be in local_map after cloning input first");
            CallableParam::new(
                param.callable_id,
                cloner
                    .pat_map()
                    .get(&param.param_pat_id)
                    .copied()
                    .unwrap_or(param.param_pat_id),
                param.top_level_param,
                param.field_path.clone(),
                remapped_param_var,
                param.param_ty.clone(),
                param.hof_input_is_tuple,
            )
        })
        .collect()
}

/// Threads each closure argument's captures onto the combined spec input in
/// ascending parameter order, continuing one capture counter across parameters.
///
/// Capture threading allocates through the cloner, so it must run before the
/// caller recovers the pipeline assigner. Returns the per-parameter
/// [`ClosureInfo`] (present only for closure arguments) and the total number of
/// capture slots threaded.
fn thread_group_closure_captures(
    cloner: &mut FirCloner,
    target: &mut Package,
    new_decl: &mut CallableDecl,
    group: &[(&CallSite, &CallableParam)],
    remapped_params: &[CallableParam],
    clone_item: LocalItemId,
) -> (Vec<ClosureInfo>, usize) {
    let mut closure_infos: Vec<ClosureInfo> = Vec::with_capacity(group.len());
    let mut capture_offset = 0usize;
    for ((call_site, _), remapped_param) in group.iter().zip(remapped_params.iter()) {
        let closure_info = prepare_closure_captures(
            cloner,
            target,
            new_decl,
            remapped_param,
            &call_site.callable_arg,
            capture_offset,
            clone_item,
        );
        if let Some(info) = &closure_info {
            capture_offset += info.capture_bindings.len();
        }
        closure_infos.push(closure_info);
    }
    (closure_infos, capture_offset)
}

fn prepare_closure_captures(
    cloner: &mut FirCloner,
    target: &mut Package,
    new_decl: &mut CallableDecl,
    remapped_param: &CallableParam,
    callable_arg: &ConcreteCallable,
    capture_offset: usize,
    clone_item: LocalItemId,
) -> ClosureInfo {
    let ConcreteCallable::Closure {
        captures,
        target: closure_target,
        ..
    } = callable_arg
    else {
        return None;
    };
    let callable_capture = captured_callable_specialization(target, captures);
    let captures_to_thread: Vec<CapturedVar> = captures
        .iter()
        .enumerate()
        .filter(|(idx, _)| {
            callable_capture
                .as_ref()
                .is_none_or(|callable_capture| callable_capture.capture_idx != *idx)
        })
        .map(|(_, capture)| capture.clone())
        .collect();
    let capture_bindings = thread_closure_captures(
        cloner,
        target,
        new_decl,
        remapped_param,
        &captures_to_thread,
        capture_offset,
        clone_item,
    );
    Some(ClosureSpecializationInfo {
        target: *closure_target,
        capture_bindings,
        callable_capture,
    })
}

/// Transforms the combined spec body once per specialized parameter, replacing
/// each arrow parameter's calls with its concrete callable.
///
/// A single capture tracker is shared across parameters so each captured input
/// binding of a shared lifted lambda is specialized once. For a closure parameter
/// the capture-prepend is scoped to exactly the calls that parameter retargeted (the
/// set difference of calls to the closure target before and after the
/// transform), keeping same-target producer closures from double-prepending each
/// other's captures. Callable-array members skip the scoped prepend because
/// their captures are threaded through the array element rewrite instead.
#[allow(clippy::too_many_arguments)]
fn transform_combined_callable_body(
    udts: &UdtMetadata,
    target: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    group: &[(&CallSite, &CallableParam)],
    remapped_params: &[CallableParam],
    closure_infos: &[ClosureInfo],
    callable_array_position: Option<&(usize, Vec<usize>)>,
    callable_impl: &CallableImpl,
    assigner: &mut Assigner,
) -> bool {
    let impl_clone = callable_impl.clone();
    let mut closure_infos = closure_infos.to_vec();
    for info in closure_infos.iter_mut().flatten() {
        if info.callable_capture.is_some() {
            info.target = clone_closure_target(target, info.target, assigner);
        }
    }
    let mut specialized_capture_targets = CaptureSpecializations::new(udts);
    let concrete_group = callable_array_position
        .map(|position| {
            group
                .iter()
                .zip(remapped_params.iter())
                .zip(closure_infos.iter())
                .filter(|&(((_, _), remapped_param), _)| {
                    (
                        remapped_param.top_level_param,
                        remapped_param.field_path.clone(),
                    ) == *position
                })
                .map(|(((call_site, _), _), closure_info)| {
                    if let Some(info) = closure_info {
                        concrete_with_threaded_captures(&call_site.callable_arg, info)
                    } else {
                        call_site.callable_arg.clone()
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (((call_site, _), remapped_param), closure_info) in group
        .iter()
        .zip(remapped_params.iter())
        .zip(closure_infos.iter())
    {
        if let Some(info) = closure_info {
            if !specialize_closure_target_callable_capture(
                udts,
                target,
                package_id,
                info.target,
                info.callable_capture.as_ref(),
                assigner,
            ) {
                return false;
            }
            let is_callable_array_member = callable_array_position.is_some_and(|position| {
                (
                    remapped_param.top_level_param,
                    remapped_param.field_path.clone(),
                ) == *position
            });
            let before = if is_callable_array_member {
                FxHashSet::default()
            } else {
                collect_calls_to_closure_target(target, &impl_clone, package_id, info.target)
            };
            let concrete = concrete_with_threaded_captures(&call_site.callable_arg, info);
            transform_callable_body(
                target,
                package_id,
                destination,
                &impl_clone,
                remapped_param,
                &concrete,
                &concrete_group,
                &mut specialized_capture_targets,
                assigner,
            );
            if !is_callable_array_member {
                let after =
                    collect_calls_to_closure_target(target, &impl_clone, package_id, info.target);
                let retargeted: Vec<ExprId> = after.difference(&before).copied().collect();
                if !prepend_captures_to_calls(
                    target,
                    &retargeted,
                    package_id,
                    destination,
                    info.target,
                    &info.capture_bindings,
                    assigner,
                ) {
                    return false;
                }
            }
        } else {
            transform_callable_body(
                target,
                package_id,
                destination,
                &impl_clone,
                remapped_param,
                &call_site.callable_arg,
                &concrete_group,
                &mut specialized_capture_targets,
                assigner,
            );
        }
    }
    !specialized_capture_targets.failed
}

/// Removes every specialized arrow parameter slot from the combined spec in a
/// single pass and refreshes value types.
///
/// Group nested paths before any mutation; remove whole slots only after
/// retained payloads have been reduced in their original coordinates.
fn remove_combined_callable_params(
    udts: &UdtMetadata,
    target: &mut Package,
    new_decl: &mut CallableDecl,
    remapped_params: &[CallableParam],
    callable_array_position: Option<&(usize, Vec<usize>)>,
    total_captures: usize,
    assigner: &mut Assigner,
) {
    let mut params_for_removal = unique_params_for_removal(remapped_params);
    params_for_removal.sort_by_key(|(param, _)| param.top_level_param);
    let retain_empty_payloads = callable_array_position.is_some()
        && params_for_removal
            .iter()
            .all(|(param, _)| !param.field_path.is_empty());
    let mut whole = Vec::new();
    let mut consumed_vars = FxHashSet::default();
    for group in params_for_removal
        .chunk_by(|(left, _), (right, _)| left.top_level_param == right.top_level_param)
    {
        let param = group[0].0;
        let paths: Vec<_> = group
            .iter()
            .map(|(param, _)| param.field_path.as_slice())
            .collect();
        let consumes = nested_param_removal_covers_entire_slot(
            udts,
            target,
            new_decl,
            param.top_level_param,
            &paths,
        );
        if consumes && !param.field_path.is_empty() {
            consumed_vars.insert(param.param_var);
        }
        if param.field_path.is_empty() || (consumes && !retain_empty_payloads) {
            whole.push(param);
        } else {
            remove_nested_callable_params(udts, target, new_decl, param, &paths, false, assigner);
        }
    }
    remove_param_destructuring_stmts(target, &new_decl.implementation, &consumed_vars);
    if !whole.is_empty() {
        remove_callable_params(udts, target, new_decl, &whole, total_captures);
    }
    refresh_rewritten_value_types(target, &new_decl.implementation);
}

/// Checks whether the nested fields being removed account for every element in
/// the top-level tuple slot.
///
/// Whole-slot coverage is the only case where `remove_callable_params` can
/// safely drop the top-level parameter and discard its destructuring. If any
/// field survives, callers must preserve the slot and let
/// `remove_nested_callable_params` rewrite the body against the reduced shape.
fn nested_param_removal_covers_entire_slot(
    udts: &UdtMetadata,
    package: &Package,
    decl: &CallableDecl,
    top_level_param: usize,
    paths: &[&[usize]],
) -> bool {
    let input_pat = package.get_pat(decl.input);
    let slot_ty = match &input_pat.kind {
        PatKind::Tuple(patterns) => patterns
            .get(top_level_param)
            .map(|pat_id| package.get_pat(*pat_id).ty.clone()),
        PatKind::Bind(_) => Some(input_pat.ty.clone()),
        PatKind::Discard => None,
    };
    let Some(slot_ty) = slot_ty else {
        return false;
    };
    InputRemoval::new(udts, &udts.resolve(&slot_ty), paths)
        .is_some_and(|removal| removal.consumes_original())
}

/// Clones a HOF callable, transforms its body to replace the callable
/// parameter with the concrete callee, and inserts the specialized callable
/// into the target (`package_id`) package. The HOF body is read from
/// `call_site.hof_item_id.package`, which may differ from the target package.
/// Returns the `StoreItemId` of the new item.
#[allow(clippy::too_many_lines)]
fn specialize_one(
    udts: &UdtMetadata,
    store: &mut PackageStore,
    package_id: PackageId,
    call_site: &CallSite,
    param: &CallableParam,
    assigner: &mut Assigner,
) -> Option<StoreItemId> {
    // Extract needed data from the source package.
    // The HOF may live in a different package (e.g. the standard library),
    // so use hof_item_id.package rather than the target package_id.
    let hof_pkg_id = call_site.hof_item_id.package;
    let arg_label = resolve_callable_arg_label(store, &call_site.callable_arg);
    let (body_pkg, decl_snapshot) = {
        let package = store.get(hof_pkg_id);
        let hof_item = package.get_item(call_site.hof_item_id.item);

        let ItemKind::Callable(ref hof_decl) = hof_item.kind else {
            return None;
        };

        let body_pkg = extract_callable_body(package, hof_decl);
        let decl_snapshot = hof_decl.as_ref().clone();
        (body_pkg, decl_snapshot)
    }; // immutable borrow released

    // Clone body into target, transform, insert.
    let target = store.get_mut(package_id);
    let new_item_id = assigner.next_item();
    let owned_assigner = std::mem::take(assigner);
    let mut cloner = FirCloner::from_assigner(owned_assigner);
    cloner.reset_maps();
    let new_specialized_item_id = ItemId {
        package: package_id,
        item: new_item_id,
    };
    cloner.set_self_item_remap(call_site.hof_item_id, new_specialized_item_id);

    // Clone the input before the impl so `local_map` holds the input parameter
    // mappings when the callable body is walked.
    let cloned_input = cloner.clone_pat(&body_pkg, decl_snapshot.input, target);
    let cloned_impl = cloner.clone_callable_impl(&body_pkg, &decl_snapshot.implementation, target);

    let (remapped_param, mut new_decl) = build_single_spec_decl(
        &cloner,
        param,
        &decl_snapshot,
        cloned_input,
        cloned_impl,
        &arg_label,
    );

    // Thread closure captures before recovering the assigner, since
    // thread_closure_captures uses the cloner for pat/local allocation.
    let closure_info = prepare_closure_captures(
        &mut cloner,
        target,
        &mut new_decl,
        &remapped_param,
        &call_site.callable_arg,
        0,
        new_item_id,
    );

    // Recover the assigner from the cloner so all subsequent allocations
    // flow through the shared pipeline assigner.
    let nested_items: Vec<_> = cloner.item_map().values().copied().collect();
    *assigner = cloner.into_assigner();

    if !apply_single_param_specialization(
        udts,
        target,
        package_id,
        CaptureScope::CloneScope(new_item_id),
        &mut new_decl,
        &remapped_param,
        call_site,
        closure_info,
        assigner,
    ) {
        return None;
    }

    // The cloner has already retargeted recursive references to `new_item_id`.
    // Those calls bypass the normal external call-site rewrite, so reduce their
    // arguments here after the specialized input pattern has its callable slot
    // removed. The single specialization key guards the retarget: a self-call
    // forwarding a different callable is routed back to the original HOF instead.
    let spec_arg_keys = [concrete_callable_key(
        call_site.call_pkg_id,
        &call_site.callable_arg,
        call_site.hof_item_id,
    )];
    rewrite_recursive_self_call_args(
        udts,
        target,
        package_id,
        new_item_id,
        call_site.hof_item_id,
        &spec_arg_keys,
        &new_decl,
        std::slice::from_ref(&remapped_param),
        &nested_items,
        assigner,
    );

    // Insert the new item.
    let new_item = Item {
        id: new_item_id,
        span: target.synthetic_span(),
        parent: None,
        doc: Rc::from(""),
        attrs: Vec::new(),
        visibility: Visibility::Internal,
        kind: ItemKind::Callable(Box::new(new_decl)),
    };
    target.items.insert(new_item_id, new_item);

    Some(StoreItemId {
        package: package_id,
        item: new_item_id,
    })
}

/// Builds the specialized callable declaration for a single-parameter spec,
/// remapping the callable parameter through the cloner's id maps.
///
/// The spec is named `HOF{label}` after the concrete argument. Returns the
/// remapped [`CallableParam`] alongside the new [`CallableDecl`], which still
/// carries the original arrow parameter slot; the caller removes it after the
/// body transform.
fn build_single_spec_decl(
    cloner: &FirCloner,
    param: &CallableParam,
    decl_snapshot: &CallableDecl,
    cloned_input: PatId,
    cloned_impl: CallableImpl,
    arg_label: &str,
) -> (CallableParam, CallableDecl) {
    let remapped_param_var = *cloner
        .local_map()
        .get(&param.param_var)
        .expect("param_var should be in local_map after cloning input first");

    let remapped_param = CallableParam::new(
        param.callable_id,
        cloner
            .pat_map()
            .get(&param.param_pat_id)
            .copied()
            .unwrap_or(param.param_pat_id),
        param.top_level_param,
        param.field_path.clone(),
        remapped_param_var,
        param.param_ty.clone(),
        param.hof_input_is_tuple,
    );

    let hof_name: Rc<str> = Rc::from(format!("{}{{{arg_label}}}", decl_snapshot.name.name));
    let new_decl = CallableDecl {
        span: decl_snapshot.span,
        kind: decl_snapshot.kind,
        name: Ident {
            id: LocalVarId::from(0_u32),
            span: decl_snapshot.name.span,
            name: hof_name,
        },
        generics: decl_snapshot.generics.clone(),
        input: cloned_input,
        output: decl_snapshot.output.clone(),
        functors: decl_snapshot.functors,
        implementation: cloned_impl,
        attrs: decl_snapshot.attrs.clone(),
    };
    (remapped_param, new_decl)
}

/// Transforms the single-parameter spec body to replace the callable parameter
/// with its concrete callee, then removes the now-dead parameter slot.
///
/// A callable's functored specs share one lifted lambda item, so a fresh capture
/// tracker prevents repeated specialization of the same target input binding.
/// Runtime captures prepared for the concrete argument are passed from new
/// HOF input slots to the closure target. A sole statically known callable
/// capture is instead embedded in a private target clone. A fully consumed
/// tuple parameter is dropped rather than retyped to unit when runtime capture
/// slots were appended.
#[allow(clippy::too_many_arguments)]
fn apply_single_param_specialization(
    udts: &UdtMetadata,
    target: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    new_decl: &mut CallableDecl,
    remapped_param: &CallableParam,
    call_site: &CallSite,
    mut closure_info: ClosureInfo,
    assigner: &mut Assigner,
) -> bool {
    if let Some(info) = &mut closure_info {
        if info.callable_capture.is_some() {
            info.target = clone_closure_target(target, info.target, assigner);
        }
        if !specialize_closure_target_callable_capture(
            udts,
            target,
            package_id,
            info.target,
            info.callable_capture.as_ref(),
            assigner,
        ) {
            return false;
        }
    }

    // Share one tracker across this parameter's functored specs so repeated
    // target bindings are recognized even when caller locals or slot indices differ.
    let impl_clone = new_decl.implementation.clone();
    let mut specialized_capture_targets = CaptureSpecializations::new(udts);
    let concrete = if let Some(info) = &closure_info {
        concrete_with_threaded_captures(&call_site.callable_arg, info)
    } else {
        call_site.callable_arg.clone()
    };
    transform_callable_body(
        target,
        package_id,
        destination,
        &impl_clone,
        remapped_param,
        &concrete,
        &[],
        &mut specialized_capture_targets,
        assigner,
    );
    if specialized_capture_targets.failed {
        return false;
    }

    // Whether the removed callable threaded closure captures as new input
    // slots. This governs how a fully consumed tuple parameter is handled below.
    let had_closure_captures = closure_info
        .as_ref()
        .is_some_and(|info| !info.capture_bindings.is_empty());

    if let Some(info) = closure_info
        && !rewrite_closure_target_call_args(
            target,
            &new_decl.implementation,
            package_id,
            destination,
            info.target,
            &info.capture_bindings,
            assigner,
        )
    {
        return false;
    }

    // Remove the callable parameter from the input pattern and update types.
    // When the removed callable was a closure with captures, those captures were
    // threaded as new input slots and the call site drops the consumed slot, so
    // a fully consumed tuple parameter must be dropped rather than retyped to
    // unit.
    remove_callable_param(
        udts,
        target,
        new_decl,
        remapped_param,
        had_closure_captures,
        assigner,
    );
    refresh_rewritten_value_types(target, &new_decl.implementation);
    true
}

/// Rewrites recursive calls retargeted by `FirCloner::set_self_item_remap`.
///
/// The self-remap happens while cloning, before specialization removes callable
/// parameters from the synthesized callable's input. Without this follow-up,
/// a recursive body can call the reduced specialization while still passing the
/// original callable argument, for example `Repeat_H(H, n - 1, q)` instead of
/// `Repeat_H(n - 1, q)`. External call sites go through `rewrite.rs`; cloned
/// recursive self-calls need the same argument-shape reduction locally.
///
/// The cloner retargets *every* recursive self-call to this specialization
/// unconditionally, which is only correct when the self-call forwards the same
/// concrete callable this specialization was generated for. `spec_arg_keys`
/// holds this specialization's per-parameter [`ConcreteCallableKey`] in the same
/// order as `params`. Each self-call's forwarded argument is resolved to its own
/// key and compared: on a full match the callable slots are stripped and the
/// call kept pointed at the specialization; otherwise the cloner's retarget is
/// undone locally by pointing the callee back at `hof_item_id` (the original
/// higher-order function) with its arguments intact, so the fixpoint loop's
/// call-site rewrite routes it to the correct sibling specialization. An
/// argument whose key cannot be statically resolved (`None`) is treated as a
/// mismatch so the decision fails safe. Capturing closures take that path even
/// when their lifted target matches: the normal rewrite must thread their
/// runtime environment, which this slot-removal shortcut does not construct.
#[allow(clippy::too_many_arguments)]
fn rewrite_recursive_self_call_args(
    udts: &UdtMetadata,
    package: &mut Package,
    package_id: PackageId,
    self_item_id: LocalItemId,
    hof_item_id: ItemId,
    spec_arg_keys: &[ConcreteCallableKey],
    decl: &CallableDecl,
    params: &[CallableParam],
    nested_items: &[LocalItemId],
    assigner: &mut Assigner,
) {
    let mut expr_ids = Vec::new();
    let mut seen = FxHashSet::default();
    // The new declaration is not in the package yet, so collect it directly
    // before adding the cloned nested bodies through the shared collector.
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |id, _| {
        if seen.insert(id) {
            expr_ids.push(id);
        }
    });
    assert!(
        nested_items
            .iter()
            .all(|id| package.items.contains_key(*id)),
        "cloned nested items must exist before rewriting recursive calls"
    );
    // Add all specialization bodies of the cloned nested callables. Sharing
    // `seen` collects each expression once without following unrelated callees.
    crate::walk_utils::extend_expr_ids_in_local_callables(
        package,
        nested_items,
        &mut expr_ids,
        &mut seen,
    );
    // The clone map does not promise order; sort for reproducible rewrites.
    expr_ids.sort_unstable();
    // Freeze the recursive call sites before changing any targets or arguments.
    // Only calls to the new specialization qualify, including Adj/Ctl calls.
    let self_calls: Vec<_> = expr_ids
        .into_iter()
        .filter_map(|id| {
            let expr = package.get_expr(id);
            let ExprKind::Call(callee_id, args_id) = expr.kind else {
                return None;
            };
            // Keep the full callee for type refresh and its unwrapped item
            // reference for possible redirection back to the original HOF.
            let (base_id, outer_functor) = peel_body_functors(package, callee_id);
            expr_is_item_callee(package, base_id, package_id, self_item_id).then_some((
                callee_id,
                base_id,
                args_id,
                outer_functor,
            ))
        })
        .collect();

    for (callee_id, base_id, args_id, outer_functor) in self_calls {
        // Resolve the concrete callable each self-call forwards into the
        // specialized callable slots. The conditional self-remap guard consumes
        // these keys to decide whether a self-call actually targets this
        // specialization or a sibling; an unresolved slot (`None`) is treated as
        // "not a match" so the decision fails safe. The keys are computed here so
        // they align with the callable slots stripped just below.
        let self_call_arg_keys: Vec<Option<ConcreteCallableKey>> = params
            .iter()
            .map(|param| {
                let path =
                    super::build_param_input_path(param.hof_input_is_tuple, param, outer_functor);
                resolve_self_call_arg_key(
                    package,
                    package_id,
                    args_id,
                    &path,
                    ItemId {
                        package: package_id,
                        item: self_item_id,
                    },
                )
            })
            .collect();

        // The self-call belongs to this specialization only when every forwarded
        // callable slot resolves to exactly the key this specialization was
        // generated for. Any differing or unresolved slot means the call targets
        // a sibling specialization instead.
        let targets_this_specialization = self_call_arg_keys.len() == spec_arg_keys.len()
            && self_call_arg_keys
                .iter()
                .zip(spec_arg_keys.iter())
                .all(|(resolved, expected)| resolved.as_ref() == Some(expected));

        let input = package.get_pat(decl.input).ty.clone();
        if targets_this_specialization
            && rewrite_recursive_self_call_arg_expr(
                udts,
                package,
                args_id,
                params,
                outer_functor,
                &input,
                assigner,
            )
        {
            refresh_self_call_callee_ty(
                package,
                callee_id,
                decl.input,
                usize::from(outer_functor.controlled),
            );
        } else {
            // This self-call forwards a callable that differs from this
            // specialization's key (or cannot be statically resolved), so
            // keeping the cloner's `hof -> spec` retarget would misroute it into
            // the wrong specialization. Undo that retarget locally by pointing
            // the callee back at the original higher-order function with its
            // arguments intact; the fixpoint loop's call-site rewrite then routes
            // it to the correct sibling specialization on a later iteration. This
            // keeps the shared `FirCloner::remap_res` contract untouched (see
            // `crate::cloner`).
            if let ExprKind::Var(res @ Res::Item(_), _) = &mut package
                .exprs
                .get_mut(base_id)
                .expect("self-call callee expr not found")
                .kind
            {
                *res = Res::Item(hof_item_id);
            }
        }
    }
}

/// Returns whether `expr_id` is a direct reference to `item_id` in `package_id`.
///
/// The caller peels functor wrappers before calling this helper, so matching a
/// bare item reference is enough to identify calls to the synthesized recursive
/// specialization.
fn expr_is_item_callee(
    package: &Package,
    expr_id: ExprId,
    package_id: PackageId,
    item_id: LocalItemId,
) -> bool {
    matches!(
        package.get_expr(expr_id).kind,
        ExprKind::Var(
            Res::Item(ItemId {
                package: callee_package,
                item: callee_item,
            }),
            _
        ) if callee_package == package_id && callee_item == item_id
    )
}

/// Removes every specialized callable argument from one recursive self-call.
///
/// Each [`CallableParam`] describes the callable slot that was removed from the
/// synthesized input. The same paths must be removed from the recursive call's
/// argument expression so the retargeted call matches the reduced callee
/// signature.
#[allow(clippy::too_many_arguments)]
fn rewrite_recursive_self_call_arg_expr(
    udts: &UdtMetadata,
    package: &mut Package,
    args_id: ExprId,
    params: &[CallableParam],
    functor: FunctorApp,
    input: &Ty,
    assigner: &mut Assigner,
) -> bool {
    let params: Vec<_> = params.iter().collect();
    let Some(args) = super::rewrite::build_specialized_call_args(
        udts,
        package,
        args_id,
        input,
        &params,
        &[],
        CaptureScope::Entry,
        usize::from(functor.controlled),
        assigner,
    ) else {
        return false;
    };
    let value = package.get_expr(args.value).clone();
    let args_expr = package
        .exprs
        .get_mut(args_id)
        .expect("recursive args exist");
    args_expr.kind = value.kind;
    args_expr.ty = value.ty;
    super::rewrite::prepend_evaluations(package, args_id, args.prefix, assigner);
    true
}

/// Updates a retargeted recursive callee's arrow type after its args shrink.
///
/// The callee expression was cloned with the original higher-order signature.
/// After recursive self-call arguments are reduced, the arrow input must point
/// at the synthesized callable's final input pattern, preserving any outer
/// controlled functor tuple layers.
fn refresh_self_call_callee_ty(
    package: &mut Package,
    callee_id: ExprId,
    input_pat_id: PatId,
    controlled_layers: usize,
) {
    let original_ty = package.get_expr(callee_id).ty.clone();
    let Ty::Arrow(arrow) = original_ty else {
        return;
    };
    let target_input = package.get_pat(input_pat_id).ty.clone();
    // The callee expression was cloned with the pre-specialization arrow type.
    // After argument reduction, retarget the relevant input slot to the final
    // synthesized input pattern so later type checks see callee and args agree.
    let input = apply_target_input_at_control_path(&arrow.input, &target_input, controlled_layers);
    package
        .exprs
        .get_mut(callee_id)
        .expect("callee expr not found")
        .ty = Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(input),
        output: arrow.output,
        functors: arrow.functors,
    }));
}

/// Finds the single parameter position shared by a forwarded callable array,
/// if the call group qualifies for combined removal.
///
/// Returns `None` when any call site in the group is conditional or dynamic, or
/// when the repeated parameter position is not unique or is not array-typed.
/// The returned `(top_level_param, field_path)` locates the array parameter that
/// combined removal collapses.
fn find_callable_array_group_position(
    remapped_params: &[CallableParam],
    group: &[(&CallSite, &CallableParam)],
) -> Option<(usize, Vec<usize>)> {
    if group.iter().any(|(call_site, _)| {
        !call_site.condition.is_empty()
            || matches!(call_site.callable_arg, ConcreteCallable::Dynamic)
    }) {
        return None;
    }

    let mut positions: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    for param in remapped_params {
        *positions
            .entry((param.top_level_param, param.field_path.clone()))
            .or_default() += 1;
    }
    let repeated = positions
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(position, _)| position)
        .collect::<Vec<_>>();
    let [position] = repeated.as_slice() else {
        return None;
    };
    remapped_params
        .iter()
        .find(|param| (param.top_level_param, param.field_path.clone()) == *position)
        .and_then(|param| matches!(param.param_ty, Ty::Array(_)).then(|| position.clone()))
}

/// Deduplicates callable parameters by their `(top_level_param, field_path)`
/// position, keeping the first occurrence of each.
///
/// Each returned entry pairs the parameter with a flag for whether it is
/// array-typed, which downstream removal uses to tell a forwarded callable
/// array apart from a scalar callable parameter.
fn unique_params_for_removal(params: &[CallableParam]) -> Vec<(&CallableParam, bool)> {
    let mut seen: FxHashSet<(usize, Vec<usize>)> = FxHashSet::default();
    params
        .iter()
        .filter_map(|param| {
            let position = (param.top_level_param, param.field_path.clone());
            seen.insert(position)
                .then_some((param, matches!(param.param_ty, Ty::Array(_))))
        })
        .collect()
}

/// Rebuilds a concrete callable so a forwarded closure carries its captured
/// variables as explicit captures on the specialized callable.
///
/// Global and dynamic callables have no captured environment and are returned
/// unchanged.
fn concrete_with_threaded_captures(
    concrete: &ConcreteCallable,
    info: &ClosureSpecializationInfo,
) -> ConcreteCallable {
    match concrete {
        ConcreteCallable::Closure { functor, .. } => {
            // Thread captures through the specialized callable input so a
            // forwarded closure keeps its runtime environment after the
            // callable parameter that carried it is removed.
            ConcreteCallable::Closure {
                target: info.target,
                captures: info
                    .capture_bindings
                    .iter()
                    .map(|binding| CapturedVar {
                        local: binding.local,
                        ty: binding.ty.clone(),
                        static_callable: None,
                        expr: None,
                        caller_substitutions: Vec::new(),
                    })
                    .collect(),
                functor: *functor,
            }
        }
        ConcreteCallable::Global { .. } | ConcreteCallable::Dynamic => concrete.clone(),
    }
}

/// Identifies whether a closure's only captured value is itself a concrete
/// callable eligible to be baked into the closure target.
///
/// Only a single scalar arrow capture with `static_callable` metadata qualifies.
/// The metadata supplies a global item and functor; this helper does not
/// resolve the initializer or infer a callable from the capture's type.
/// All other captures stay threaded as runtime operands.
fn captured_callable_specialization(
    _package: &Package,
    captures: &[CapturedVar],
) -> Option<CapturedCallableSpecialization> {
    if captures.len() != 1 {
        return None;
    }
    let capture = captures.first()?;
    if !matches!(capture.ty, Ty::Arrow(_)) {
        return None;
    }
    capture
        .static_callable
        .map(|(item_id, functor)| CapturedCallableSpecialization {
            capture_idx: 0,
            capture_ty: capture.ty.clone(),
            concrete: ConcreteCallable::Global { item_id, functor },
        })
}

/// Clones a lifted target before embedding its sole static callable capture.
/// Other occurrences keep the original target and capture signature.
fn clone_closure_target(
    package: &mut Package,
    target: LocalItemId,
    assigner: &mut Assigner,
) -> LocalItemId {
    let source = package.clone();
    let mut cloner = FirCloner::from_assigner(std::mem::take(assigner));
    let target = cloner.clone_nested_item(&source, target, package);
    *assigner = cloner.into_assigner();
    target
}

/// Transforms all specialization bodies in a callable implementation,
/// replacing uses of the callable parameter with direct calls to the concrete
/// callee.
///
/// `specialized_capture_targets` tracks each lifted lambda item and captured
/// callable parameter already specialized. It is supplied by the caller so a
/// multi-argument specialization can share one tracker across every parameter's
/// transform pass; single-argument callers pass a fresh tracker. Slot mappings
/// are updated separately for each closure occurrence as captures are replaced.
#[allow(clippy::too_many_arguments)]
fn transform_callable_body(
    package: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    callable_impl: &CallableImpl,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    specialized_capture_targets: &mut CaptureSpecializations,
    assigner: &mut Assigner,
) {
    let mut alias_set = AliasSet::default();
    match callable_impl {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            transform_block(
                package,
                package_id,
                destination,
                spec_impl.body.block,
                param,
                concrete,
                concrete_group,
                &mut alias_set,
                specialized_capture_targets,
                assigner,
            );
            if let Some(ref adj) = spec_impl.adj {
                transform_block(
                    package,
                    package_id,
                    destination,
                    adj.block,
                    param,
                    concrete,
                    concrete_group,
                    &mut alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
            if let Some(ref ctl) = spec_impl.ctl {
                transform_block(
                    package,
                    package_id,
                    destination,
                    ctl.block,
                    param,
                    concrete,
                    concrete_group,
                    &mut alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
            if let Some(ref ctl_adj) = spec_impl.ctl_adj {
                transform_block(
                    package,
                    package_id,
                    destination,
                    ctl_adj.block,
                    param,
                    concrete,
                    concrete_group,
                    &mut alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
        }
    }
}

/// Recursively walks a block, transforming call expressions that invoke the
/// callable parameter.
#[allow(clippy::too_many_arguments)]
fn transform_block(
    package: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    block_id: qsc_fir::fir::BlockId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    alias_set: &mut AliasSet,
    specialized_capture_targets: &mut CaptureSpecializations,
    assigner: &mut Assigner,
) {
    let block = package
        .blocks
        .get(block_id)
        .expect("block not found")
        .clone();
    for &stmt_id in &block.stmts {
        transform_stmt(
            package,
            package_id,
            destination,
            stmt_id,
            param,
            concrete,
            concrete_group,
            alias_set,
            specialized_capture_targets,
            assigner,
        );
    }
}

/// Walks a pattern tree, returning the `LocalVarId` bound at the given
/// tuple-field path when every intermediate node is a tuple pattern and the
/// leaf is a `Bind`.
fn find_bind_local_at_field_path(
    package: &Package,
    pat_id: PatId,
    field_path: &[usize],
) -> Option<LocalVarId> {
    let pat = package.get_pat(pat_id);
    match field_path.split_first() {
        None => match &pat.kind {
            PatKind::Bind(ident) => Some(ident.id),
            PatKind::Tuple(_) | PatKind::Discard => None,
        },
        Some((index, tail)) => match &pat.kind {
            PatKind::Tuple(sub_pats) => sub_pats
                .get(*index)
                .and_then(|sub_pat_id| find_bind_local_at_field_path(package, *sub_pat_id, tail)),
            PatKind::Bind(_) | PatKind::Discard => None,
        },
    }
}

/// Rewrites one statement in a specialized callable body and updates the alias
/// set used to recognize callable-parameter projections.
///
/// Records aliases introduced by destructuring or direct alias bindings.
/// A recognized destructuring initializer is left for parameter-removal
/// cleanup; other expression-bearing statements are visited for rewriting.
#[allow(clippy::too_many_arguments)]
fn transform_stmt(
    package: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    stmt_id: qsc_fir::fir::StmtId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    alias_set: &mut AliasSet,
    specialized_capture_targets: &mut CaptureSpecializations,
    assigner: &mut Assigner,
) {
    let stmt = package.stmts.get(stmt_id).expect("stmt not found").clone();
    match &stmt.kind {
        qsc_fir::fir::StmtKind::Expr(expr_id) | qsc_fir::fir::StmtKind::Semi(expr_id) => {
            transform_expr(
                package,
                package_id,
                destination,
                *expr_id,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        qsc_fir::fir::StmtKind::Local(mutability, pat_id, expr_id) => {
            // Record aliases introduced by destructuring the tuple-valued
            // parameter down to the callable leaf.
            if *mutability == Mutability::Immutable && !param.field_path.is_empty() {
                let init_expr = package.exprs.get(*expr_id).expect("expr not found");
                if let ExprKind::Var(Res::Local(var), _) = &init_expr.kind {
                    if *var == param.param_var {
                        if let Some(alias_var) =
                            find_bind_local_at_field_path(package, *pat_id, &param.field_path)
                        {
                            alias_set.insert(alias_var);
                            return;
                        }
                    } else if alias_set.contains(var) {
                        let pat = package.pats.get(*pat_id).expect("pat not found");
                        if let PatKind::Bind(ident) = &pat.kind {
                            alias_set.insert(ident.id);
                        }
                    }
                }
            }
            transform_expr(
                package,
                package_id,
                destination,
                *expr_id,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        qsc_fir::fir::StmtKind::Item(_) => {}
    }
}

/// Rewrites an expression subtree in the cloned specialization so callable
/// parameter uses become concrete callees.
///
/// Before, calls may still target `param.param_var`, a tuple-field projection of
/// it, or an alias introduced by destructuring. After, every matching callee is
/// rewritten in place to invoke `concrete`, while nested blocks and control-flow
/// expressions are recursively normalized to the same post-specialization shape.
#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
fn transform_expr(
    package: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    expr_id: ExprId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    alias_set: &mut AliasSet,
    specialized_capture_targets: &mut CaptureSpecializations,
    assigner: &mut Assigner,
) {
    let expr = package.exprs.get(expr_id).expect("expr not found").clone();
    match &expr.kind {
        ExprKind::Call(callee_id, args_id) => {
            let callee_id = *callee_id;
            let args_id = *args_id;

            // Check if the callee is our callable parameter (possibly wrapped
            // in functor applications).
            let (base_id, body_functor) = peel_body_functors(package, callee_id);
            let base_kind = package.get_expr(base_id).kind.clone();

            let replaced = if let Some((array_id, index_id)) = indexed_callable_array_param_source(
                package,
                base_id,
                param.param_var,
                &param.field_path,
            ) {
                // Rewrite nested dispatches before copying this invocation into
                // branches; neither the index nor argument may retain the slot.
                for child in [index_id, args_id] {
                    transform_expr(
                        package,
                        package_id,
                        destination,
                        child,
                        param,
                        concrete,
                        concrete_group,
                        alias_set,
                        specialized_capture_targets,
                        assigner,
                    );
                }
                if !replace_indexed_callable_array_call(
                    package,
                    package_id,
                    destination,
                    expr_id,
                    args_id,
                    array_id,
                    index_id,
                    body_functor,
                    if concrete_group.is_empty() {
                        std::slice::from_ref(concrete)
                    } else {
                        concrete_group
                    },
                    assigner,
                ) {
                    specialized_capture_targets.failed = true;
                }
                return;
            } else if let ExprKind::Var(Res::Local(var), _) = &base_kind
                && *var == param.param_var
                && param.field_path.is_empty()
            {
                // Single-level param: direct use as callee.
                replace_callee(
                    package,
                    package_id,
                    callee_id,
                    body_functor,
                    concrete,
                    assigner,
                );
                true
            } else if expr_matches_param_field_path(
                package,
                base_id,
                param.param_var,
                &param.field_path,
            ) {
                // `expr_matches_param_field_path` already matches an empty
                // field path for a single-field-UDT callee (e.g.
                // `Field(Var(b), [])`), so no separate non-empty guard is
                // needed here; that lets `replace_callee` fire for
                // single-field-UDT callees as well as deeper paths.
                replace_callee(
                    package,
                    package_id,
                    callee_id,
                    body_functor,
                    concrete,
                    assigner,
                );
                true
            } else {
                false
            };

            // Also check alias set for nested params.
            let replaced = if replaced {
                true
            } else if let ExprKind::Var(Res::Local(var), _) = &base_kind
                && alias_set.contains(var)
            {
                replace_callee(
                    package,
                    package_id,
                    callee_id,
                    body_functor,
                    concrete,
                    assigner,
                );
                true
            } else {
                false
            };

            if !replaced {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    callee_id,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            } else if matches!(concrete, ConcreteCallable::Closure { captures, .. } if captures.is_empty())
            {
                let concrete = apply_body_functor_to_concrete(concrete, body_functor);
                if let Some(prefix) = rewrite_indexed_closure_dispatch_args(
                    package,
                    destination,
                    args_id,
                    &concrete,
                    assigner,
                ) {
                    super::rewrite::prepend_evaluations(package, expr_id, prefix, assigner);
                } else {
                    specialized_capture_targets.failed = true;
                }
            }

            // Recurse into the arguments.
            transform_expr(
                package,
                package_id,
                destination,
                args_id,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::Block(block_id) => {
            transform_block(
                package,
                package_id,
                destination,
                *block_id,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::If(cond, body, els) => {
            transform_expr(
                package,
                package_id,
                destination,
                *cond,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
            transform_expr(
                package,
                package_id,
                destination,
                *body,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
            if let Some(els_id) = els {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    *els_id,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
        }
        ExprKind::While(cond, block_id) => {
            transform_expr(
                package,
                package_id,
                destination,
                *cond,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
            transform_block(
                package,
                package_id,
                destination,
                *block_id,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::Tuple(exprs) | ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) => {
            for &e in exprs {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    e,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
        }
        ExprKind::Assign(lhs, rhs)
        | ExprKind::AssignOp(_, lhs, rhs)
        | ExprKind::BinOp(_, lhs, rhs)
        | ExprKind::ArrayRepeat(lhs, rhs)
        | ExprKind::Index(lhs, rhs) => {
            transform_expr(
                package,
                package_id,
                destination,
                *lhs,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
            transform_expr(
                package,
                package_id,
                destination,
                *rhs,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::AssignField(a, _, b) | ExprKind::UpdateField(a, _, b) => {
            transform_expr(
                package,
                package_id,
                destination,
                *a,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
            transform_expr(
                package,
                package_id,
                destination,
                *b,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            transform_expr(
                package,
                package_id,
                destination,
                *a,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
            transform_expr(
                package,
                package_id,
                destination,
                *b,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
            transform_expr(
                package,
                package_id,
                destination,
                *c,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::UnOp(_, inner) | ExprKind::Return(inner) | ExprKind::Fail(inner) => {
            transform_expr(
                package,
                package_id,
                destination,
                *inner,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::Field(inner_id, _) => {
            // For nested callable params, check if this Field expression
            // accesses the arrow element within the param variable.
            if !param.field_path.is_empty() {
                if expr_matches_param_field_path(
                    package,
                    expr_id,
                    param.param_var,
                    &param.field_path,
                ) {
                    // The forwarded value can be a single callable read out of a
                    // struct/tuple param, or a whole callable array nested in
                    // that param and threaded to an inner HOF that indexes it.
                    // Rebuild the array literal so every candidate survives and
                    // any surviving index stays valid; substitute a single
                    // non-array value in place.
                    substitute_forwarded_callable(
                        package,
                        destination,
                        expr_id,
                        concrete,
                        concrete_group,
                        assigner,
                    );
                    return;
                }
                if let Some(path) = collect_field_path_from_param(package, expr_id, param.param_var)
                {
                    if param.field_path.starts_with(&path) {
                        let base = alloc_expr(
                            package,
                            assigner,
                            expr.ty.clone(),
                            expr.kind.clone(),
                            expr.span,
                        );
                        let replacement = reconstruct_specialized_tuple(
                            specialized_capture_targets.udts,
                            package,
                            destination,
                            base,
                            &expr.ty,
                            &param.field_path[path.len()..],
                            concrete,
                            concrete_group,
                            assigner,
                        );
                        let replacement = package.get_expr(replacement).clone();
                        let rewritten = package.exprs.get_mut(expr_id).expect("expression exists");
                        rewritten.kind = replacement.kind;
                        rewritten.ty = replacement.ty;
                    }
                    return;
                }
            } else if collect_field_path_from_param(package, expr_id, param.param_var).is_some() {
                // Empty-path (single-field UDT) callable forwarded by value to
                // an inner HOF: mirror the non-empty branch above and replace
                // the field-access with the concrete value so the fixpoint
                // re-analysis can resolve the inner call site (instead of
                // leaving a forwarded field access that declines to
                // `DynamicCallable`).
                substitute_forwarded_callable(
                    package,
                    destination,
                    expr_id,
                    concrete,
                    concrete_group,
                    assigner,
                );
                return;
            }
            transform_expr(
                package,
                package_id,
                destination,
                *inner_id,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        ExprKind::Range(a, b, c) => {
            if let Some(a) = a {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    *a,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
            if let Some(b) = b {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    *b,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
            if let Some(c) = c {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    *c,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
        }
        ExprKind::String(components) => {
            for comp in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = comp {
                    transform_expr(
                        package,
                        package_id,
                        destination,
                        *e,
                        param,
                        concrete,
                        concrete_group,
                        alias_set,
                        specialized_capture_targets,
                        assigner,
                    );
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    *c,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
            for f in fields {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    f.value,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
        }
        // Substitute the callable parameter variable (or an alias from
        // destructuring) at non-callee positions (e.g., when forwarded as an
        // argument to an inner HOF).
        ExprKind::Var(Res::Local(var), _)
            if (*var == param.param_var && param.field_path.is_empty())
                || alias_set.contains(var) =>
        {
            // A callable parameter forwarded by value is either a single
            // callable or a whole array threaded to an inner HOF that indexes
            // it. For an array, rebuild the concrete array literal so every
            // candidate (with its threaded capture) survives and any surviving
            // index stays valid; a single non-array value is substituted in
            // place.
            substitute_forwarded_callable(
                package,
                destination,
                expr_id,
                concrete,
                concrete_group,
                assigner,
            );
        }
        ExprKind::Var(Res::Local(var), _)
            if *var == param.param_var
                && !param.field_path.is_empty()
                && matches!(expr.ty, Ty::Tuple(_) | Ty::Udt(_)) =>
        {
            let base = alloc_local_var_expr(package, assigner, *var, expr.ty.clone(), expr.span);
            let replacement = reconstruct_specialized_tuple(
                specialized_capture_targets.udts,
                package,
                destination,
                base,
                &expr.ty,
                &param.field_path,
                concrete,
                concrete_group,
                assigner,
            );
            let replacement = package.get_expr(replacement).clone();
            let rewritten = package.exprs.get_mut(expr_id).expect("expression exists");
            rewritten.kind = replacement.kind;
            rewritten.ty = replacement.ty;
        }
        ExprKind::Parallel(limit, body) => {
            for child in limit.iter().chain(std::iter::once(body)) {
                transform_expr(
                    package,
                    package_id,
                    destination,
                    *child,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }
        }
        // When a closure captures the callable parameter being specialized,
        // propagate the specialization into the closure's target callable and
        // remove the capture.
        ExprKind::Closure(captures, target) => {
            if let Some(capture_idx) = captures
                .iter()
                .position(|&c| c == param.param_var || alias_set.contains(&c))
            {
                let target = *target;
                transform_closure_param_capture(
                    package,
                    package_id,
                    destination,
                    expr_id,
                    target,
                    capture_idx,
                    param,
                    concrete,
                    if matches!(param.param_ty, Ty::Array(_)) {
                        if concrete_group.is_empty() {
                            std::slice::from_ref(concrete)
                        } else {
                            concrete_group
                        }
                    } else {
                        &[]
                    },
                    specialized_capture_targets,
                    assigner,
                );
            }
        }
        // Terminals with no sub-expressions.
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Builds the base expression kind, functor application, and inferred arrow
/// type for a concrete callable value. Shared by the in-place single-value
/// substitution ([`replace_callable_value`]) and the allocating per-element
/// reconstruction ([`alloc_callable_value_expr`]) so both emit identical
/// callable-value expressions. `hint_ty` is the arrow type of the slot the
/// value fills — the expression's own type for a single value, or the element
/// type when rebuilding an array. Closures retain that hint type and must have
/// captures in `destination`. Returns `None` for a dynamic callable or a
/// closure with captures owned by another scope.
fn build_callable_value_parts(
    package: &Package,
    destination: CaptureScope,
    concrete: &ConcreteCallable,
    hint_ty: &Ty,
) -> Option<(ExprKind, FunctorApp, Option<Ty>)> {
    match concrete {
        ConcreteCallable::Global { item_id, functor } => {
            let ty = package
                .items
                .get(item_id.item)
                .filter(|_| item_id.package == package.id)
                .and_then(|item| match &item.kind {
                    ItemKind::Callable(decl) => {
                        let mut input = package.get_pat(decl.input).ty.clone();
                        for _ in 0..functor.controlled {
                            input =
                                Ty::Tuple(vec![Ty::Array(Box::new(Ty::Prim(Prim::Qubit))), input]);
                        }
                        Some(Ty::Arrow(Box::new(Arrow {
                            kind: decl.kind,
                            input: Box::new(input),
                            output: Box::new(decl.output.clone()),
                            functors: qsc_fir::ty::FunctorSet::Value(decl.functors),
                        })))
                    }
                    ItemKind::Ty(..) => None,
                });
            Some((ExprKind::Var(Res::Item(*item_id), Vec::new()), *functor, ty))
        }
        ConcreteCallable::Closure {
            target,
            captures,
            functor,
        } => {
            if !captures_belong_to_destination(destination, captures) {
                return None;
            }
            Some((
                ExprKind::Closure(
                    captures.iter().map(|capture| capture.local.var).collect(),
                    *target,
                ),
                *functor,
                Some(hint_ty.clone()),
            ))
        }
        ConcreteCallable::Dynamic => None,
    }
}

/// Replaces a callable-valued expression while preserving closure captures.
/// Callee replacement can collapse a closure to its target item, but forwarded
/// callable values must remain closures so nested HOFs still receive captures.
fn replace_callable_value(
    package: &mut Package,
    destination: CaptureScope,
    expr_id: ExprId,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) {
    let Some((base_kind, functor, base_ty)) = build_callable_value_parts(
        package,
        destination,
        concrete,
        &package.get_expr(expr_id).ty,
    ) else {
        return;
    };

    let expr = package.exprs.get(expr_id).expect("expr not found").clone();
    let new_ty = base_ty.unwrap_or_else(|| expr.ty.clone());
    if !functor.adjoint && functor.controlled == 0 {
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
        expr_mut.kind = base_kind;
        expr_mut.ty = new_ty;
        return;
    }

    let outer_id =
        alloc_functor_wrapped_expr(package, assigner, base_kind, functor, &new_ty, expr.span);

    let outermost_kind = package
        .exprs
        .get(outer_id)
        .expect("expr not found")
        .kind
        .clone();
    let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
    expr_mut.kind = outermost_kind;
    expr_mut.ty = new_ty;
}

/// Allocates a fresh callable-value expression (plus any functor-wrapper nodes)
/// for `concrete` and returns the id of the outermost node. This is the
/// allocating analogue of the in-place substitution [`replace_callable_value`]
/// performs; it is used to materialize each element when rebuilding a forwarded
/// callable array so a multi-candidate array is not collapsed to a single
/// callable. `hint_ty` is the arrow type of the slot the value fills (the array
/// element type) and `span` is applied to every allocated node. Returns `None`
/// for a dynamic callable or a closure whose captures belong to another scope.
fn alloc_callable_value_expr(
    package: &mut Package,
    destination: CaptureScope,
    span: PackageSpan,
    concrete: &ConcreteCallable,
    hint_ty: &Ty,
    assigner: &mut Assigner,
) -> Option<ExprId> {
    let (base_kind, functor, base_ty) =
        build_callable_value_parts(package, destination, concrete, hint_ty)?;
    let new_ty = base_ty.unwrap_or_else(|| hint_ty.clone());

    Some(alloc_functor_wrapped_expr(
        package, assigner, base_kind, functor, &new_ty, span,
    ))
}

/// Rebuilds a forwarded callable array as a concrete array literal of its
/// candidates. An outer HOF that receives a callable array as a flat value can
/// forward it into an inner HOF that indexes it; substituting a single callable
/// there would collapse the whole array to one candidate and drop the other
/// closures' captures. Emitting an `ExprKind::Array` of the concrete callables
/// (each already carrying its own threaded capture) instead lets whole-program
/// re-analysis resolve the inner argument to the full candidate set and
/// specialize the inner indexed dispatch. `expr_id` must be array-typed;
/// elements are emitted in `concrete_group` order.
fn reconstruct_callable_array(
    package: &mut Package,
    destination: CaptureScope,
    expr_id: ExprId,
    concrete_group: &[ConcreteCallable],
    assigner: &mut Assigner,
) {
    let expr = package.exprs.get(expr_id).expect("expr not found").clone();
    let Ty::Array(elem_ty) = &expr.ty else {
        return;
    };
    let elem_ty = elem_ty.as_ref().clone();

    let mut elements = Vec::with_capacity(concrete_group.len());
    for concrete in concrete_group {
        let Some(element_id) = alloc_callable_value_expr(
            package,
            destination,
            expr.span,
            concrete,
            &elem_ty,
            assigner,
        ) else {
            // Dynamic candidates or captures from another scope cannot be
            // materialized. Leave the forwarded expression unchanged.
            return;
        };
        elements.push(element_id);
    }

    let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
    expr_mut.kind = ExprKind::Array(elements);
    expr_mut.ty = expr.ty;
}

/// Substitutes a forwarded callable-parameter use with its concrete value.
///
/// A callable parameter forwarded by value can be a single callable or a whole
/// callable array threaded to an inner HOF. For an array-typed use the array
/// literal is rebuilt via [`reconstruct_callable_array`] so every candidate
/// survives and any surviving index expression (e.g. `arr[0]`) stays a valid
/// array index. A single-candidate specialization carries its value in
/// `concrete` with an empty `concrete_group`; it is still rebuilt as a
/// one-element array rather than collapsed to a scalar, which would leave an
/// index expression indexing a non-array value. A non-array use is replaced in
/// place with the single concrete value.
fn substitute_forwarded_callable(
    package: &mut Package,
    destination: CaptureScope,
    expr_id: ExprId,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    assigner: &mut Assigner,
) {
    if matches!(package.get_expr(expr_id).ty, Ty::Array(_)) {
        let group = if concrete_group.is_empty() {
            std::slice::from_ref(concrete)
        } else {
            concrete_group
        };
        reconstruct_callable_array(package, destination, expr_id, group, assigner);
    } else {
        replace_callable_value(package, destination, expr_id, concrete, assigner);
    }
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_specialized_tuple(
    udts: &UdtMetadata,
    package: &mut Package,
    destination: CaptureScope,
    base: ExprId,
    ty: &Ty,
    path: &[usize],
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    assigner: &mut Assigner,
) -> ExprId {
    if let Ty::Udt(Res::Item(item_id)) = ty {
        let Some(payload_ty) = udts.pure_ty(*item_id).cloned() else {
            return base;
        };
        let span = package.get_expr(base).span;
        let payload = reconstruct_specialized_tuple(
            udts,
            package,
            destination,
            base,
            &payload_ty,
            path,
            concrete,
            concrete_group,
            assigner,
        );
        if let ExprKind::Tuple(values) = &package.get_expr(payload).kind {
            let fields = values
                .iter()
                .enumerate()
                .map(|(index, &value)| qsc_fir::fir::FieldAssign {
                    span,
                    field: Field::Path(FieldPath {
                        indices: vec![index],
                    }),
                    value,
                })
                .collect();
            return alloc_expr(
                package,
                assigner,
                ty.clone(),
                ExprKind::Struct(Res::Item(*item_id), None, fields),
                span,
            );
        }
        let constructor = alloc_item_var_expr(
            package,
            assigner,
            *item_id,
            Ty::Arrow(Box::new(Arrow {
                kind: qsc_fir::fir::CallableKind::Function,
                input: Box::new(payload_ty),
                output: Box::new(ty.clone()),
                functors: FunctorSet::Value(FunctorSetValue::Empty),
            })),
            span,
        );
        return alloc_call_expr(package, assigner, constructor, payload, ty.clone(), span);
    }
    if path.is_empty() {
        substitute_forwarded_callable(
            package,
            destination,
            base,
            concrete,
            concrete_group,
            assigner,
        );
        return base;
    }
    let Ty::Tuple(types) = ty else { return base };
    let span = package.get_expr(base).span;
    let values = types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let field = crate::fir_builder::alloc_field_expr(
                package,
                assigner,
                base,
                index,
                ty.clone(),
                span,
            );
            if index == path[0] {
                reconstruct_specialized_tuple(
                    udts,
                    package,
                    destination,
                    field,
                    ty,
                    &path[1..],
                    concrete,
                    concrete_group,
                    assigner,
                )
            } else {
                field
            }
        })
        .collect();
    crate::fir_builder::alloc_tuple_expr(package, assigner, values, ty.clone(), span)
}

/// Returns true when an expression is a field chain rooted at `param_var`
/// and its collected field path exactly matches `field_path`.
fn expr_matches_param_field_path(
    package: &Package,
    expr_id: ExprId,
    param_var: LocalVarId,
    field_path: &[usize],
) -> bool {
    collect_field_path_from_param(package, expr_id, param_var)
        .is_some_and(|path| path == field_path)
}

/// Recognizes an `array[index]` expression whose array is the callable-array
/// parameter at `field_path`, returning the array and index sub-expressions.
///
/// Returns `None` when the expression is not an index into that parameter.
fn indexed_callable_array_param_source(
    package: &Package,
    expr_id: ExprId,
    param_var: LocalVarId,
    field_path: &[usize],
) -> Option<(ExprId, ExprId)> {
    let ExprKind::Index(array_id, index_id) = package.get_expr(expr_id).kind else {
        return None;
    };
    expr_matches_param_field_path(package, array_id, param_var, field_path)
        .then_some((array_id, index_id))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn replace_indexed_callable_array_call(
    package: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    call_expr_id: ExprId,
    args_id: ExprId,
    array_id: ExprId,
    index_id: ExprId,
    body_functor: FunctorApp,
    concrete_group: &[ConcreteCallable],
    assigner: &mut Assigner,
) -> bool {
    if concrete_group.is_empty() {
        return false;
    }

    let branch_callables: Vec<ConcreteCallable> = concrete_group
        .iter()
        .map(|concrete| apply_body_functor_to_concrete(concrete, body_functor))
        .collect();

    let Ty::Array(_) = package.get_expr(array_id).ty else {
        return false;
    };
    let ExprKind::Call(callee, _) = package.get_expr(call_expr_id).kind else {
        return false;
    };
    let callee_ty = package.get_expr(callee).ty.clone();

    let result_ty = package.get_expr(call_expr_id).ty.clone();
    let span = package.get_expr(call_expr_id).span;
    let original_args = package.get_expr(args_id).clone();

    let hoisted = if expr_is_side_effect_free(package, package_id, index_id) {
        None
    } else {
        let index_ty = package.get_expr(index_id).ty.clone();
        let index_span = package.get_expr(index_id).span;
        let (local_var, let_stmt) = alloc_local_var(
            package,
            assigner,
            "index",
            &index_ty,
            index_id,
            Mutability::Immutable,
        );
        Some((local_var, index_ty, index_span, let_stmt))
    };

    let mut branch_calls = Vec::with_capacity(branch_callables.len());
    for (position, branch_callable) in branch_callables.iter().enumerate() {
        let Some(call_id) = alloc_dispatch_branch_call(
            package,
            package_id,
            destination,
            span,
            &result_ty,
            &callee_ty,
            &original_args,
            branch_callable,
            assigner,
        ) else {
            return false;
        };
        branch_calls.push((position, call_id));
    }

    let mut dispatch_id = branch_calls.last().expect("branch exists").1;
    for (position, call_id) in branch_calls.into_iter().rev().skip(1) {
        let operand = match &hoisted {
            Some((local_var, ty, index_span, _)) => {
                alloc_local_var_expr(package, assigner, *local_var, ty.clone(), *index_span)
            }
            None => index_id,
        };
        let condition_id = super::rewrite::alloc_index_match_expr(
            package,
            operand,
            position,
            branch_callables.len(),
            span,
            assigner,
        );
        dispatch_id = alloc_if_expr(
            package,
            span,
            &result_ty,
            condition_id,
            call_id,
            dispatch_id,
            assigner,
        );
    }

    let bounds_operand = match &hoisted {
        Some((local_var, ty, index_span, _)) => {
            alloc_local_var_expr(package, assigner, *local_var, ty.clone(), *index_span)
        }
        None => index_id,
    };
    let units = (0..branch_callables.len())
        .map(|_| alloc_unit_expr(package, assigner, span))
        .collect();
    let bounds_array = alloc_expr(
        package,
        assigner,
        Ty::Array(Box::new(Ty::UNIT)),
        ExprKind::Array(units),
        span,
    );
    let bounds_check = alloc_expr(
        package,
        assigner,
        Ty::UNIT,
        ExprKind::Index(bounds_array, bounds_operand),
        span,
    );
    let mut stmts = Vec::with_capacity(3);
    if let Some((_, _, _, let_stmt)) = &hoisted {
        stmts.push(*let_stmt);
    }
    stmts.push(alloc_semi_stmt(package, assigner, bounds_check, span));
    stmts.push(alloc_expr_stmt(package, assigner, dispatch_id, span));
    let block_id = alloc_block(package, assigner, stmts, result_ty.clone(), span);
    dispatch_id = alloc_block_expr(package, assigner, block_id, result_ty.clone(), span);

    let dispatch = package.get_expr(dispatch_id).clone();
    let call_expr = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr not found");
    call_expr.kind = dispatch.kind;
    call_expr.ty = dispatch.ty;
    true
}

/// Composes a functor application drawn from a callable value's body onto a
/// concrete callable, folding it into the callable's accumulated functor.
fn apply_body_functor_to_concrete(
    concrete: &ConcreteCallable,
    body_functor: FunctorApp,
) -> ConcreteCallable {
    match concrete {
        ConcreteCallable::Global { item_id, functor } => ConcreteCallable::Global {
            item_id: *item_id,
            functor: compose_functors(functor, &body_functor),
        },
        ConcreteCallable::Closure {
            target,
            captures,
            functor,
        } => ConcreteCallable::Closure {
            target: *target,
            captures: captures.clone(),
            functor: compose_functors(functor, &body_functor),
        },
        ConcreteCallable::Dynamic => ConcreteCallable::Dynamic,
    }
}

#[allow(clippy::too_many_arguments)]
fn alloc_dispatch_branch_call(
    package: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    span: PackageSpan,
    result_ty: &Ty,
    callee_ty: &Ty,
    original_args: &Expr,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) -> Option<ExprId> {
    let (item_id, functor, target) = match concrete {
        ConcreteCallable::Closure {
            target, functor, ..
        } => (
            ItemId {
                package: package_id,
                item: *target,
            },
            *functor,
            Some(*target),
        ),
        ConcreteCallable::Global { item_id, functor } => (*item_id, *functor, None),
        ConcreteCallable::Dynamic => return None,
    };

    let controlled_layers = usize::from(functor.controlled);
    let direct_ty = match concrete {
        ConcreteCallable::Closure { target, .. } => {
            build_direct_target_callee_ty(package, *target, callee_ty, controlled_layers)
                .unwrap_or_else(|| callee_ty.clone())
        }
        _ => target
            .and_then(|target| {
                build_direct_target_callee_ty(package, target, callee_ty, controlled_layers)
            })
            .unwrap_or_else(|| callee_ty.clone()),
    };

    let callee_id = alloc_functor_wrapped_expr(
        package,
        assigner,
        ExprKind::Var(Res::Item(item_id), Vec::new()),
        functor,
        &direct_ty,
        span,
    );

    let args_id = alloc_expr(
        package,
        assigner,
        original_args.ty.clone(),
        original_args.kind.clone(),
        original_args.span,
    );

    let mut prefix = Vec::new();
    if let ConcreteCallable::Closure {
        target, captures, ..
    } = concrete
    {
        let target_input = target_callable_input(package, *target)?;
        prefix = rewrite_closure_dispatch_branch_args(
            package,
            destination,
            args_id,
            captures,
            &target_input,
            controlled_layers,
            CallConvention::DirectItem,
            assigner,
        )?;
    }

    let call = alloc_call_expr(
        package,
        assigner,
        callee_id,
        args_id,
        result_ty.clone(),
        span,
    );
    super::rewrite::prepend_evaluations(package, call, prefix, assigner);
    Some(call)
}

/// Returns the input pattern type of the callable `target`, or `None` when the
/// item is not a callable.
fn target_callable_input(package: &Package, target: LocalItemId) -> Option<Ty> {
    let ItemKind::Callable(decl) = &package.get_item(target).kind else {
        return None;
    };
    Some(package.get_pat(decl.input).ty.clone())
}

/// Threads a closure's captured values into the argument tuple of a call to its
/// dispatch target.
///
/// When a closure is specialized, its captured variables become ordinary
/// leading parameters of the target callable. This rewrites the call's argument
/// expression so the captures are passed first, followed by the original
/// arguments. A non-closure concrete is left unchanged.
fn rewrite_indexed_closure_dispatch_args(
    package: &mut Package,
    destination: CaptureScope,
    args_id: ExprId,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) -> Option<Vec<StmtId>> {
    let ConcreteCallable::Closure {
        target,
        captures,
        functor,
    } = concrete
    else {
        return Some(Vec::new());
    };

    let capture_bindings: Vec<CaptureBinding> = captures
        .iter()
        .map(|capture| CaptureBinding {
            local: capture.local,
            ty: capture.ty.clone(),
        })
        .collect();
    rewrite_closure_target_args(
        package,
        destination,
        args_id,
        *target,
        &capture_bindings,
        usize::from(functor.controlled),
        if captures.is_empty() {
            CallConvention::ClosureValue
        } else {
            CallConvention::DirectItem
        },
        assigner,
    )
}

/// Rewrites a call's argument expression so the closure target receives its
/// captured values alongside the original arguments.
///
/// Unknown targets or incompatible layouts reject the specialization rather
/// than guessing a capture layout.
#[allow(clippy::too_many_arguments)]
fn rewrite_closure_target_args(
    package: &mut Package,
    destination: CaptureScope,
    args_id: ExprId,
    target: LocalItemId,
    capture_bindings: &[CaptureBinding],
    controlled_layers: usize,
    convention: CallConvention,
    assigner: &mut Assigner,
) -> Option<Vec<StmtId>> {
    if let Some(target_input) = target_callable_input(package, target) {
        let captures: Vec<CapturedVar> = capture_bindings
            .iter()
            .map(|binding| CapturedVar {
                local: binding.local,
                ty: binding.ty.clone(),
                static_callable: None,
                expr: None,
                caller_substitutions: Vec::new(),
            })
            .collect();
        rewrite_closure_dispatch_branch_args(
            package,
            destination,
            args_id,
            &captures,
            &target_input,
            controlled_layers,
            convention,
            assigner,
        )
    } else {
        None
    }
}

/// Rewrites `args_id` in place so a specialized closure target receives its
/// captured values followed by the original call arguments, shaped to match the
/// target's input type.
///
/// A `Controlled` functor layer wraps the whole base input as `(controls,
/// base)` without splitting the base tuple. Each layer is peeled while threading
/// the full `target_input` down the recursion, so the captures are spliced in
/// only at the innermost, uncontrolled layer. Descending into `target_input[1]`
/// instead would let the base arguments coincidentally match the target input
/// and drop the captures on the controlled path.
#[allow(clippy::too_many_arguments)]
fn rewrite_closure_dispatch_branch_args(
    package: &mut Package,
    destination: CaptureScope,
    args_id: ExprId,
    captures: &[CapturedVar],
    target_input: &Ty,
    controlled_layers: usize,
    convention: CallConvention,
    assigner: &mut Assigner,
) -> Option<Vec<StmtId>> {
    let original = package.get_expr(args_id).clone();
    let original = alloc_expr(package, assigner, original.ty, original.kind, original.span);
    let rewritten = super::rewrite::build_closure_call_args(
        package,
        original,
        destination,
        captures,
        target_input,
        controlled_layers,
        convention,
        assigner,
    )?;
    let value = package.get_expr(rewritten.value).clone();
    let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
    args_expr.kind = value.kind;
    args_expr.ty = value.ty;
    Some(rewritten.prefix)
}

/// Builds the argument-tuple kind and type for a closure dispatch branch,
/// combining the closure's captured values with the original arguments.
///
/// Test adapter for the shared declaration-directed direct-item packer.
#[cfg(test)]
fn build_closure_dispatch_branch_args_data(
    package: &mut Package,
    destination: CaptureScope,
    args_id: ExprId,
    captures: &[CapturedVar],
    target_input: &Ty,
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let result = super::rewrite::build_closure_call_args(
        package,
        args_id,
        destination,
        captures,
        target_input,
        0,
        CallConvention::DirectItem,
        assigner,
    )?;
    let value = package.get_expr(result.value);
    Some((value.kind.clone(), value.ty.clone()))
}

/// Returns whether an actual capture layout can populate an expected dispatch
/// input. Only callable functor sets use capability matching; all other type
/// structure must match exactly.
pub(super) fn dispatch_layout_types_compatible(actual: &Ty, expected: &Ty) -> bool {
    match (actual, expected) {
        (Ty::Array(actual_item), Ty::Array(expected_item)) => {
            dispatch_layout_types_compatible(actual_item, expected_item)
        }
        (Ty::Arrow(actual_arrow), Ty::Arrow(expected_arrow)) => {
            actual_arrow.kind == expected_arrow.kind
                && dispatch_layout_types_compatible(&actual_arrow.input, &expected_arrow.input)
                && dispatch_layout_types_compatible(&actual_arrow.output, &expected_arrow.output)
                && dispatch_functors_compatible(actual_arrow.functors, expected_arrow.functors)
        }
        (Ty::Tuple(actual_items), Ty::Tuple(expected_items)) => {
            actual_items.len() == expected_items.len()
                && actual_items
                    .iter()
                    .zip(expected_items)
                    .all(|(actual_item, expected_item)| {
                        dispatch_layout_types_compatible(actual_item, expected_item)
                    })
        }
        _ => actual == expected,
    }
}

/// Tests concrete functor capability inclusion: the actual set must contain
/// the expected set. An empty requirement accepts any set; otherwise unresolved
/// sets must be equal. Unlike frontend inference, this adds no constraints.
fn dispatch_functors_compatible(actual: FunctorSet, expected: FunctorSet) -> bool {
    crate::fir_builder::functors_satisfy(actual, expected)
}

/// Synthesizes an `if condition { true_id } else { false_id }` expression with
/// the given result type.
fn alloc_if_expr(
    package: &mut Package,
    span: PackageSpan,
    result_ty: &Ty,
    condition_id: ExprId,
    true_id: ExprId,
    false_id: ExprId,
    assigner: &mut Assigner,
) -> ExprId {
    crate::fir_builder::alloc_if_expr(
        package,
        assigner,
        condition_id,
        true_id,
        Some(false_id),
        result_ty.clone(),
        span,
    )
}

/// Collects field indices from nested `Field(Path)` expressions rooted at `param_var`.
fn collect_field_path_from_param(
    package: &Package,
    expr_id: ExprId,
    param_var: LocalVarId,
) -> Option<Vec<usize>> {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(var), _) if *var == param_var => Some(Vec::new()),
        ExprKind::Field(inner_id, Field::Path(FieldPath { indices })) => {
            let mut path = collect_field_path_from_param(package, *inner_id, param_var)?;
            path.extend(indices.iter().copied());
            Some(path)
        }
        _ => None,
    }
}

/// Replaces the callee expression with a direct reference to the concrete
/// callable, applying the effective functor (composition of creation-site
/// and body-site functors).
///
/// Capture-free concrete closures remain `Closure([], target)` expressions;
/// other concrete callees become item references.
///
/// # Before
/// ```text
/// callee_expr = Var(Local(param_var)) : Arrow   // indirect via callable parameter
/// ```
/// # After
/// ```text
/// callee_expr = Ctl?(Adj?(Var(Item(concrete)))) : Arrow   // direct, with functors
/// ```
///
/// # Mutations
/// - Overwrites `callee_expr_id`'s `ExprKind` and `Ty` in place.
/// - Allocates functor-wrapper `Expr` nodes through `assigner` when the
///   effective functor is non-trivial.
fn replace_callee(
    package: &mut Package,
    package_id: PackageId,
    callee_expr_id: ExprId,
    body_functor: FunctorApp,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) {
    let (target_res, creation_functor) = match concrete {
        ConcreteCallable::Global { item_id, functor } => (Res::Item(*item_id), *functor),
        ConcreteCallable::Closure {
            target, functor, ..
        } => {
            let item_id = ItemId {
                package: package_id,
                item: *target,
            };
            (Res::Item(item_id), *functor)
        }
        ConcreteCallable::Dynamic => return,
    };

    let effective = compose_functors(&creation_functor, &body_functor);

    let callee_expr = package.exprs.get(callee_expr_id).expect("expr not found");
    let original_callee_ty = callee_expr.ty.clone();
    let callee_span = callee_expr.span;
    let callee_ty = match concrete {
        ConcreteCallable::Closure { captures, .. } if captures.is_empty() => {
            original_callee_ty.clone()
        }
        ConcreteCallable::Closure { target, .. } => build_direct_target_callee_ty(
            package,
            *target,
            &original_callee_ty,
            usize::from(effective.controlled),
        )
        .unwrap_or_else(|| original_callee_ty.clone()),
        ConcreteCallable::Global { .. } | ConcreteCallable::Dynamic => original_callee_ty.clone(),
    };

    let base_kind = match concrete {
        ConcreteCallable::Closure {
            target, captures, ..
        } if captures.is_empty() => ExprKind::Closure(Vec::new(), *target),
        _ => ExprKind::Var(target_res, Vec::new()),
    };

    if !effective.adjoint && effective.controlled == 0 {
        // No functor wrapping — replace directly.
        let expr = package
            .exprs
            .get_mut(callee_expr_id)
            .expect("expr not found");
        expr.kind = base_kind;
        expr.ty = callee_ty;
    } else {
        // Allocate fresh expressions for functor wrapper layers.
        let outer_id = alloc_functor_wrapped_expr(
            package,
            assigner,
            base_kind,
            effective,
            &callee_ty,
            callee_span,
        );

        // Copy the outermost node's kind into the original callee expr.
        let outermost_kind = package
            .exprs
            .get(outer_id)
            .expect("expr not found")
            .kind
            .clone();
        let expr = package
            .exprs
            .get_mut(callee_expr_id)
            .expect("expr not found");
        expr.kind = outermost_kind;
        expr.ty = callee_ty;
    }
}

/// Derives the arrow type of the direct-call target from the HOF's
/// indirect-call site arrow type, peeling `controlled_layers` to reach the
/// right nested input slot.
fn build_direct_target_callee_ty(
    package: &Package,
    target_item_id: LocalItemId,
    callee_ty: &Ty,
    controlled_layers: usize,
) -> Option<Ty> {
    let Ty::Arrow(arrow) = callee_ty else {
        return None;
    };

    let ItemKind::Callable(decl) = &package.get_item(target_item_id).kind else {
        return None;
    };

    let target_input = package.get_pat(decl.input).ty.clone();
    let new_input =
        apply_target_input_at_control_path(&arrow.input, &target_input, controlled_layers);

    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}

/// When the HOF body contains a closure that captures the callable parameter
/// being specialized, we must propagate the concrete callable into the
/// closure's target callable and replace the capture. A concrete closure's own
/// captures are rebound to fresh target parameters and spliced into the
/// enclosing closure; a concrete without captures removes the slot.
/// For a callable array, every member's captures are rebound in array order.
///
/// # Before
/// ```text
/// Closure([param_var, ...], target)   // target has a corresponding capture parameter
/// ```
/// # After
/// ```text
/// Closure([concrete_captures..., ...], target)   // target rewritten in place
/// ```
///
/// # Mutations
/// - Transforms the closure target's body via [`transform_callable_body`].
/// - Replaces the target's capture parameter with the concrete's capture
///   parameters, or removes it when no rebinding is needed.
/// - Splices corresponding operands into this occurrence's capture list.
///   Shared target bindings are transformed only on their first tracked use;
///   conflicting embedded callable selections decline the specialization.
#[allow(clippy::too_many_arguments)]
fn transform_closure_param_capture(
    package: &mut Package,
    package_id: PackageId,
    destination: CaptureScope,
    closure_expr_id: ExprId,
    closure_target: LocalItemId,
    capture_idx: usize,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    specialized_capture_targets: &mut CaptureSpecializations,
    assigner: &mut Assigner,
) {
    if let ConcreteCallable::Closure { captures, .. } = concrete
        && !captures_belong_to_destination(destination, captures)
    {
        return;
    }

    // The lambda item is shared across the enclosing callable's functored specs.
    // Specialize each target input binding once, regardless of the caller local
    // supplying it, provided the embedded callable selection agrees. The
    // occurrence map identifies that binding even after slot shifts. Compatible
    // siblings skip the target mutation but still replace their own operands.
    let first_use = specialized_capture_targets.first_use(
        package,
        closure_target,
        closure_expr_id,
        capture_idx,
        concrete,
        concrete_group,
    );
    // first_use can set failed: a false result alone does not distinguish valid
    // reuse from a conflicting selection or an unresolved target binding.
    if specialized_capture_targets.failed {
        return;
    }
    let threaded_operands = if first_use {
        // Rewrite the shared target once, rebinding any nested captures to its
        // parameters and returning this occurrence's corresponding operands.
        let Some(operands) = specialize_closure_target_for_captured_param(
            specialized_capture_targets.udts,
            package,
            package_id,
            closure_target,
            capture_idx,
            &param.param_ty,
            concrete,
            concrete_group,
            assigner,
        ) else {
            // Reject the whole candidate rather than rewriting this occurrence
            // against a target whose capture layout could not be specialized.
            specialized_capture_targets.failed = true;
            return;
        };
        operands
    } else if concrete_group.is_empty() {
        // The target is already specialized, but this occurrence must supply
        // its own runtime capture values, not those of the first occurrence.
        concrete_capture_operands(concrete)
    } else {
        // Match the target's flattened parameter layout: array member order
        // first, then capture order within each member.
        concrete_group
            .iter()
            .flat_map(concrete_capture_operands)
            .collect()
    };

    // Retarget this Closure expression's capture list. A concrete that carried
    // its own captures now expects them as target parameters, so its operands
    // take the removed slot; otherwise the slot simply disappears.
    let closure_expr = package
        .exprs
        .get_mut(closure_expr_id)
        .expect("closure expr not found");
    if let ExprKind::Closure(ref mut captures, _) = closure_expr.kind
        && capture_idx < captures.len()
    {
        captures.splice(
            capture_idx..=capture_idx,
            threaded_operands.iter().map(|binding| binding.local.var),
        );
        // Mirror the splice so later captures still resolve to their own target
        // bindings rather than inheriting the identity of the removed slot.
        specialized_capture_targets.replace_slot(
            closure_expr_id,
            capture_idx,
            threaded_operands.len(),
        );
    }
}

/// Returns the capture operands a concrete callable expects to be passed
/// positionally once its captures have been threaded onto its target.
fn concrete_capture_operands(concrete: &ConcreteCallable) -> Vec<CaptureBinding> {
    match concrete {
        ConcreteCallable::Closure { captures, .. } => captures
            .iter()
            .map(|capture| CaptureBinding {
                local: capture.local,
                ty: capture.ty.clone(),
            })
            .collect(),
        ConcreteCallable::Global { .. } | ConcreteCallable::Dynamic => Vec::new(),
    }
}

/// Specializes the shared closure-target lambda once: replaces uses of the
/// captured callable parameter inside the lambda body with the concrete callee
/// and retires the capture from the lambda's input pattern.
///
/// A concrete callable that is itself a capturing closure cannot be copied into
/// the target body as-is: its capture operands name locals of the scope that
/// built it, which the target body does not bind. Those captures are rebound to
/// fresh parameters of the target, and the returned bindings tell the caller
/// which operands the enclosing `Closure` expression must now carry.
/// Callable arrays flatten every member's environment in array order and retain
/// a separately rebound capture slice for each candidate.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn specialize_closure_target_for_captured_param(
    udts: &UdtMetadata,
    package: &mut Package,
    package_id: PackageId,
    closure_target: LocalItemId,
    capture_idx: usize,
    capture_ty: &Ty,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    assigner: &mut Assigner,
) -> Option<Vec<CaptureBinding>> {
    // Step 1: Find the corresponding binding in the closure target's input pattern.
    let target_item = package.items.get(closure_target);
    let Some(Item {
        kind: ItemKind::Callable(target_decl),
        ..
    }) = target_item
    else {
        return None;
    };
    let target_decl = target_decl.as_ref().clone();

    let target_input_pat = package
        .pats
        .get(target_decl.input)
        .expect("input pat not found")
        .clone();

    // The input pattern should be a Tuple with captures first, then lambda params.
    let capture_param_var = match &target_input_pat.kind {
        PatKind::Tuple(pats) => {
            if capture_idx >= pats.len() {
                return None;
            }
            let capture_pat = package.pats.get(pats[capture_idx]).expect("pat not found");
            match &capture_pat.kind {
                PatKind::Bind(ident) => ident.id,
                _ => return None,
            }
        }
        PatKind::Bind(ident) if capture_idx == 0 => ident.id,
        _ => return None,
    };

    // Step 2: Create a synthetic CallableParam for the closure target's captured param.
    let closure_param = CallableParam::new(
        StoreItemId::from((package_id, closure_target)),
        target_decl.input,
        capture_idx,
        Vec::new(),
        capture_param_var,
        capture_ty.clone(),
        matches!(package.get_pat(target_decl.input).kind, PatKind::Tuple(_)),
    );

    // Step 3: Rebind the concrete's own captures, if any, to fresh parameters of
    // this target so the substituted value names only locals the target binds.
    let rebound = rebind_concrete_captures_to_target_params(
        package,
        closure_target,
        capture_idx,
        concrete,
        concrete_group,
        assigner,
    );
    let local_concrete = rebound
        .as_ref()
        .map_or_else(|| concrete.clone(), |rebound| rebound.concrete.clone());

    let local_group = rebound
        .as_ref()
        .map_or(concrete_group, |rebound| rebound.group.as_slice());
    let existing_calls = if local_group.is_empty()
        && let ConcreteCallable::Closure { target, .. } = &local_concrete
    {
        collect_calls_to_closure_target(package, &target_decl.implementation, package_id, *target)
    } else {
        FxHashSet::default()
    };

    // Step 4: Transform the target callable's body to replace uses of the
    // captured param with the concrete callable. This rewrites a distinct
    // callable, the closure target, so its nested closure occurrences use a
    // fresh capture tracker rather than the enclosing body's slot maps.
    let mut specialized_capture_targets = CaptureSpecializations::new(udts);
    transform_callable_body(
        package,
        package_id,
        CaptureScope::Callable(closure_target),
        &target_decl.implementation,
        &closure_param,
        &local_concrete,
        local_group,
        &mut specialized_capture_targets,
        assigner,
    );
    if specialized_capture_targets.failed {
        return None;
    }

    if local_group.is_empty()
        && let ConcreteCallable::Closure { target, .. } = &local_concrete
    {
        let calls = collect_calls_to_closure_target(
            package,
            &target_decl.implementation,
            package_id,
            *target,
        );
        let mut new_calls: Vec<_> = calls.difference(&existing_calls).copied().collect();
        new_calls.sort_unstable();
        if !prepend_captures_to_calls(
            package,
            &new_calls,
            package_id,
            CaptureScope::Callable(closure_target),
            *target,
            &concrete_capture_operands(&local_concrete),
            assigner,
        ) {
            return None;
        }
    }

    // Step 5: Retire the capture binding. A rebound concrete already replaced
    // the slot with its own capture parameters, so only the plain case removes.
    let operands = if let Some(rebound) = rebound {
        rebound.operands
    } else {
        remove_capture_from_closure_target(package, closure_target, capture_idx);
        Vec::new()
    };
    refresh_callable_types(package, closure_target);

    #[cfg(debug_assertions)]
    if let Some(Item {
        kind: ItemKind::Callable(decl),
        ..
    }) = package.items.get(closure_target)
    {
        crate::invariants::check_local_var_consistency(package, decl);
    }

    Some(operands)
}

/// The result of rebinding a capturing concrete closure onto a target's input.
struct ReboundConcreteCaptures {
    /// The concrete callable with its capture operands renamed to the target's
    /// fresh parameters.
    concrete: ConcreteCallable,
    /// Array candidates with each member's capture slice rebound to the target.
    group: Vec<ConcreteCallable>,
    /// The original caller-scope operands, in the order the target now expects
    /// them, for the enclosing `Closure` expression to carry.
    operands: Vec<CaptureBinding>,
}

/// Replaces the capture parameter at `capture_idx` of `closure_target` with one
/// fresh parameter per capture of a concrete closure or array of closures.
///
/// Returns `None` when no candidate has captures or the target
/// capture slot cannot be replaced. The latter case is not a proof that the
/// concrete is capture-free.
fn rebind_concrete_captures_to_target_params(
    package: &mut Package,
    closure_target: LocalItemId,
    capture_idx: usize,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    assigner: &mut Assigner,
) -> Option<ReboundConcreteCaptures> {
    let candidates = if concrete_group.is_empty() {
        std::slice::from_ref(concrete)
    } else {
        concrete_group
    };
    let captures: Vec<_> = candidates
        .iter()
        .flat_map(|candidate| match candidate {
            ConcreteCallable::Closure { captures, .. } => captures.as_slice(),
            _ => &[],
        })
        .collect();
    if captures.is_empty() {
        return None;
    }
    if !closure_target_capture_slot_is_replaceable(package, closure_target, capture_idx) {
        return None;
    }

    let mut fresh_pat_ids = Vec::with_capacity(captures.len());
    let mut fresh_captures = Vec::with_capacity(captures.len());
    let mut operands = Vec::with_capacity(captures.len());
    for (index, capture) in captures.iter().enumerate() {
        let pat_id = assigner.next_pat();
        let local_var = assigner.next_local();
        package.pats.insert(
            pat_id,
            Pat {
                id: pat_id,
                span: package.synthetic_span(),
                ty: capture.ty.clone(),
                kind: PatKind::Bind(Ident {
                    id: local_var,
                    span: package.synthetic_span(),
                    name: Rc::from(format!("{CAPTURE_NAME_PREFIX}_{index}")),
                }),
            },
        );
        fresh_pat_ids.push(pat_id);
        fresh_captures.push(CapturedVar {
            local: ScopedLocal::new(local_var, CaptureScope::Callable(closure_target)),
            ty: capture.ty.clone(),
            static_callable: capture.static_callable,
            expr: None,
            caller_substitutions: Vec::new(),
        });
        operands.push(CaptureBinding {
            local: capture.local,
            ty: capture.ty.clone(),
        });
    }

    replace_capture_in_closure_target(package, closure_target, capture_idx, &fresh_pat_ids)?;

    let mut offset = 0;
    let rebound: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            if let ConcreteCallable::Closure {
                target,
                captures,
                functor,
            } = candidate
            {
                let end = offset + captures.len();
                let captures = fresh_captures[offset..end].to_vec();
                offset = end;
                ConcreteCallable::Closure {
                    target: *target,
                    captures,
                    functor: *functor,
                }
            } else {
                candidate.clone()
            }
        })
        .collect();
    Some(ReboundConcreteCaptures {
        concrete: rebound[0].clone(),
        group: if concrete_group.is_empty() {
            Vec::new()
        } else {
            rebound
        },
        operands,
    })
}

/// Returns whether the capture slot at `capture_idx` can be replaced by a list
/// of parameters, which is what [`replace_capture_in_closure_target`] requires.
///
/// Checked before any allocation so a target this cannot reshape never leaves
/// orphaned patterns behind.
fn closure_target_capture_slot_is_replaceable(
    package: &Package,
    target_item_id: LocalItemId,
    capture_idx: usize,
) -> bool {
    let Some(ItemKind::Callable(decl)) = package.items.get(target_item_id).map(|item| &item.kind)
    else {
        return false;
    };
    match &package.get_pat(decl.input).kind {
        PatKind::Tuple(pats) => capture_idx < pats.len(),
        PatKind::Bind(_) => capture_idx == 0,
        PatKind::Discard => false,
    }
}

/// Substitutes the capture binding at `capture_idx` of a closure target's input
/// with `replacements`, keeping the surrounding parameters in place.
///
/// Returns `None` when the input is not a shape this can splice, leaving the
/// callable untouched so the caller can fall back to the plain removal path.
fn replace_capture_in_closure_target(
    package: &mut Package,
    target_item_id: LocalItemId,
    capture_idx: usize,
    replacements: &[PatId],
) -> Option<()> {
    let Some(Item {
        kind: ItemKind::Callable(decl),
        ..
    }) = package.items.get(target_item_id)
    else {
        return None;
    };
    let input_pat_id = decl.input;
    let input_pat = package.pats.get(input_pat_id)?.clone();

    let replacement_tys: Vec<Ty> = replacements
        .iter()
        .map(|pat_id| package.get_pat(*pat_id).ty.clone())
        .collect();

    // A sole capture parameter carries no siblings to preserve, so the whole
    // input becomes the replacement list.
    let PatKind::Tuple(pats) = &input_pat.kind else {
        if capture_idx != 0 {
            return None;
        }
        let pat_mut = package.pats.get_mut(input_pat_id)?;
        pat_mut.kind = PatKind::Tuple(replacements.to_vec());
        pat_mut.ty = Ty::Tuple(replacement_tys);
        return Some(());
    };
    if capture_idx >= pats.len() {
        return None;
    }

    let mut new_pats = pats.clone();
    new_pats.splice(capture_idx..=capture_idx, replacements.iter().copied());

    let mut tys = match &input_pat.ty {
        Ty::Tuple(tys) => tys.clone(),
        _ => vec![input_pat.ty.clone(); pats.len()],
    };
    tys.splice(capture_idx..=capture_idx, replacement_tys);

    let pat_mut = package.pats.get_mut(input_pat_id)?;
    pat_mut.kind = PatKind::Tuple(new_pats);
    pat_mut.ty = Ty::Tuple(tys);
    Some(())
}

/// Re-runs the post-transform type-refresh cascade over a callable item's
/// implementation after its body or signature was rewritten in place
/// (e.g. a captured callable was baked in). Does not validate `PostDefunc`.
fn refresh_callable_types(package: &mut Package, item_id: LocalItemId) {
    let Some(Item {
        kind: ItemKind::Callable(decl),
        ..
    }) = package.items.get(item_id)
    else {
        return;
    };
    let implementation = decl.implementation.clone();
    refresh_rewritten_value_types(package, &implementation);
}

/// Specializes a closure target for its captured concrete callable.
///
/// Inlines the recorded [`CapturedCallableSpecialization`] into the target body
/// and drops its capture parameter via
/// [`specialize_closure_target_for_captured_param`].
fn specialize_closure_target_callable_capture(
    udts: &UdtMetadata,
    package: &mut Package,
    package_id: PackageId,
    closure_target: LocalItemId,
    callable_capture: Option<&CapturedCallableSpecialization>,
    assigner: &mut Assigner,
) -> bool {
    let Some(capture) = callable_capture else {
        return true;
    };
    specialize_closure_target_for_captured_param(
        udts,
        package,
        package_id,
        closure_target,
        capture.capture_idx,
        &capture.capture_ty,
        &capture.concrete,
        &[],
        assigner,
    )
    .is_some()
}

/// Removes the capture at `capture_idx` from the closure target callable's
/// input pattern tuple.
///
/// # Before
/// ```text
/// input = (capture_0, capture_1, lambda_param)   // capture_idx = 1
/// ```
/// # After
/// ```text
/// input = (capture_0, lambda_param)   // capture_1 removed
/// ```
///
/// # Mutations
/// - Rewrites the input `Pat` node in place, preserving the outer tuple required
///   by runtime closure calls even when no captures remain.
fn remove_capture_from_closure_target(
    package: &mut Package,
    target_item_id: LocalItemId,
    capture_idx: usize,
) {
    let target_item = package.items.get(target_item_id);
    let Some(Item {
        kind: ItemKind::Callable(decl),
        ..
    }) = target_item
    else {
        return;
    };
    let input_pat_id = decl.input;

    let input_pat = package
        .pats
        .get(input_pat_id)
        .expect("pat not found")
        .clone();
    match &input_pat.kind {
        PatKind::Tuple(pats) => {
            let new_pats: Vec<PatId> = pats
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != capture_idx)
                .map(|(_, &p)| p)
                .collect();

            let tys = match &input_pat.ty {
                Ty::Tuple(tys) => tys.clone(),
                _ => vec![input_pat.ty.clone(); pats.len()],
            };
            let new_tys: Vec<Ty> = tys
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != capture_idx)
                .map(|(_, t)| t.clone())
                .collect();

            let pat_mut = package.pats.get_mut(input_pat_id).expect("pat not found");
            pat_mut.kind = PatKind::Tuple(new_pats);
            pat_mut.ty = if new_tys.is_empty() {
                Ty::UNIT
            } else {
                Ty::Tuple(new_tys)
            };
        }
        PatKind::Bind(_) if capture_idx == 0 => {
            // Only parameter is the capture — replace with unit.
            let pat_mut = package.pats.get_mut(input_pat_id).expect("pat not found");
            pat_mut.kind = PatKind::Tuple(Vec::new());
            pat_mut.ty = Ty::UNIT;
        }
        _ => {}
    }
}

/// When the concrete callable is a closure, its captured variables must be
/// threaded as additional parameters to the specialized callable.
///
/// # Before
/// ```text
/// input = (param_0, param_1)   // original HOF input
/// ```
/// # After
/// ```text
/// input = (param_0, param_1, __capture_0, ..., __capture_N)
/// ```
///
/// # Mutations
/// - Extends the input `Pat` tuple with new `Bind` patterns for each
///   capture, or wraps a scalar input in a tuple.
/// - Allocates new `Pat` and `LocalVarId` nodes through `cloner`.
fn thread_closure_captures(
    cloner: &mut FirCloner,
    package: &mut Package,
    decl: &mut CallableDecl,
    _param: &CallableParam,
    captures: &[CapturedVar],
    name_offset: usize,
    clone_item: LocalItemId,
) -> Vec<CaptureBinding> {
    if captures.is_empty() {
        return Vec::new();
    }

    // Allocate new bindings for each captured variable and build a remap.
    let mut capture_bindings = Vec::with_capacity(captures.len());
    let mut new_pat_ids: Vec<PatId> = Vec::new();
    let mut new_tys: Vec<Ty> = Vec::new();

    for (i, capture) in captures.iter().enumerate() {
        let new_pat_id = cloner.alloc_pat();
        let new_local_var = cloner.alloc_local(capture.local.var);
        capture_bindings.push(CaptureBinding {
            local: ScopedLocal::new(new_local_var, CaptureScope::CloneScope(clone_item)),
            ty: capture.ty.clone(),
        });

        // `name_offset` continues the capture counter across parameters so a
        // multi-argument specialization gets `_.capture_0`, `_.capture_1`, …
        // without collisions; single-argument callers pass `0`, preserving the
        // original spelling.
        let name: Rc<str> = Rc::from(format!("{CAPTURE_NAME_PREFIX}_{}", name_offset + i));
        let new_pat = Pat {
            id: new_pat_id,
            span: package.synthetic_span(),
            ty: capture.ty.clone(),
            kind: PatKind::Bind(Ident {
                id: new_local_var,
                span: package.synthetic_span(),
                name,
            }),
        };
        package.pats.insert(new_pat_id, new_pat);
        new_pat_ids.push(new_pat_id);
        new_tys.push(capture.ty.clone());
    }

    // Extend the input with capture patterns.
    let input_pat = package
        .pats
        .get(decl.input)
        .expect("input pat not found")
        .clone();
    match &input_pat.kind {
        PatKind::Tuple(_) => {
            let input_pat_mut = package
                .pats
                .get_mut(decl.input)
                .expect("input pat not found");
            if let PatKind::Tuple(ref mut pats) = input_pat_mut.kind {
                pats.extend(new_pat_ids);
            }
            if let Ty::Tuple(ref mut tys) = input_pat_mut.ty {
                tys.extend(new_tys);
            }
        }
        PatKind::Bind(_) | PatKind::Discard => {
            // Wrap in a tuple with the captures.
            let old_pat_id = decl.input;
            let tuple_pat_id = cloner.alloc_pat();
            let mut sub_pats = vec![old_pat_id];
            sub_pats.extend(new_pat_ids);

            let mut all_tys = vec![input_pat.ty.clone()];
            all_tys.extend(new_tys);

            let tuple_pat = Pat {
                id: tuple_pat_id,
                span: package.synthetic_span(),
                ty: Ty::Tuple(all_tys),
                kind: PatKind::Tuple(sub_pats),
            };
            package.pats.insert(tuple_pat_id, tuple_pat);
            decl.input = tuple_pat_id;
        }
    }

    capture_bindings
}

/// Returns whether an expression resolves to a direct callee of the given
/// closure target item.
///
/// Matches both a package-qualified `Var(Res::Item)` reference to the target and a
/// capture-free `Closure` over it; any other expression (or a closure still
/// carrying captures) is not treated as a direct call to the target.
fn expr_is_closure_target_callee(
    package: &Package,
    expr_id: ExprId,
    package_id: PackageId,
    closure_target: LocalItemId,
) -> bool {
    match &package.get_expr(expr_id).kind {
        ExprKind::Var(
            Res::Item(ItemId {
                package: callee_package,
                item: callee_item,
            }),
            _,
        ) => *callee_package == package_id && *callee_item == closure_target,
        ExprKind::Closure(captures, target) => captures.is_empty() && *target == closure_target,
        _ => false,
    }
}

/// Updates a callee expression's arrow type to the closure target's direct-call
/// signature after retargeting, peeling `controlled_layers` control tuples to
/// reach the right input slot. No-op when the callee type is not an arrow.
fn refresh_closure_target_callee_expr_ty(
    package: &mut Package,
    callee_id: ExprId,
    closure_target: LocalItemId,
    controlled_layers: usize,
) {
    let original_ty = package.get_expr(callee_id).ty.clone();
    if let Some(new_ty) =
        build_direct_target_callee_ty(package, closure_target, &original_ty, controlled_layers)
    {
        package
            .exprs
            .get_mut(callee_id)
            .expect("callee expr not found")
            .ty = new_ty;
    }
}

/// Read-only collector that records every `Call` expression whose callee
/// resolves to a specific closure-target item in `package_id` after peeling
/// functor wrappers. The same collected set drives argument packing.
struct ClosureTargetCallCollector<'a> {
    package: &'a Package,
    package_id: PackageId,
    closure_target: LocalItemId,
    calls: FxHashSet<ExprId>,
}

impl<'a> Visitor<'a> for ClosureTargetCallCollector<'a> {
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

    fn visit_expr(&mut self, id: ExprId) {
        if let ExprKind::Call(callee_id, _) = self.package.get_expr(id).kind {
            let (base_id, _) = peel_body_functors(self.package, callee_id);
            if expr_is_closure_target_callee(
                self.package,
                base_id,
                self.package_id,
                self.closure_target,
            ) {
                self.calls.insert(id);
            }
        }
        visit::walk_expr(self, id);
    }
}

/// Collects the set of `Call` expressions in a callable implementation whose
/// callee resolves to `closure_target`. Used by [`specialize_many`] to scope a
/// closure's capture-prepend to exactly the calls a parameter retargeted.
fn collect_calls_to_closure_target(
    package: &Package,
    callable_impl: &CallableImpl,
    package_id: PackageId,
    closure_target: LocalItemId,
) -> FxHashSet<ExprId> {
    let mut collector = ClosureTargetCallCollector {
        package,
        package_id,
        closure_target,
        calls: FxHashSet::default(),
    };
    match callable_impl {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            collector.visit_block(spec_impl.body.block);
            if let Some(adj) = &spec_impl.adj {
                collector.visit_block(adj.block);
            }
            if let Some(ctl) = &spec_impl.ctl {
                collector.visit_block(ctl.block);
            }
            if let Some(ctl_adj) = &spec_impl.ctl_adj {
                collector.visit_block(ctl_adj.block);
            }
        }
    }
    collector.calls
}

/// Prepends a closure's captured operands to a specific set of already-located
/// `Call` expressions, the calls a single parameter retargeted, re-peeling each
/// call's functor wrappers to recover its controlled-layer count. This is the
/// scoped counterpart to [`rewrite_closure_target_call_args`], which walks a
/// whole body for one closure target.
fn prepend_captures_to_calls(
    package: &mut Package,
    call_ids: &[ExprId],
    package_id: PackageId,
    destination: CaptureScope,
    closure_target: LocalItemId,
    capture_bindings: &[CaptureBinding],
    assigner: &mut Assigner,
) -> bool {
    // Packing an outer call may replace its argument root, which can itself be
    // another retargeted call. Finish nested invocations before copying them.
    let pending: FxHashSet<_> = call_ids.iter().copied().collect();
    let call_ids: Vec<_> = expressions_in_postorder_from_roots(package, call_ids.iter().copied())
        .into_iter()
        .filter(|id| pending.contains(id))
        .collect();
    for call_id in call_ids {
        let ExprKind::Call(callee_id, args_id) = package.get_expr(call_id).kind else {
            continue;
        };
        let (base_id, outer_functor) = peel_body_functors(package, callee_id);
        if expr_is_closure_target_callee(package, base_id, package_id, closure_target) {
            let convention = if matches!(package.get_expr(base_id).kind, ExprKind::Closure(..)) {
                CallConvention::ClosureValue
            } else {
                CallConvention::DirectItem
            };
            if let Some(prefix) = rewrite_closure_target_args(
                package,
                destination,
                args_id,
                closure_target,
                capture_bindings,
                usize::from(outer_functor.controlled),
                convention,
                assigner,
            ) {
                // Closure values retain their public input; invocation adds the lifted wrapper.
                if matches!(convention, CallConvention::DirectItem) {
                    refresh_closure_target_callee_expr_ty(
                        package,
                        callee_id,
                        closure_target,
                        usize::from(outer_functor.controlled),
                    );
                }
                super::rewrite::prepend_evaluations(package, call_id, prefix, assigner);
            } else {
                return false;
            }
        }
    }
    true
}

/// Rewrites the call-argument expression for a closure target by splicing
/// the captured bindings into the appropriate slot of the call's argument
/// tuple.
///
/// # Before
/// ```text
/// Call(Var(closure_target), original_args)
/// ```
/// # After
/// ```text
/// Call(Var(closure_target), (__capture_0, ..., original_args))
/// ```
///
/// The example shows the grouped layout. When the target input is known,
/// [`rewrite_closure_target_args`] may flatten the original tuple's fields
/// alongside the captures or leave already matching arguments unchanged.
///
/// # Mutations
/// - Packs matching calls across all specialization bodies, innermost first.
fn rewrite_closure_target_call_args(
    package: &mut Package,
    callable_impl: &CallableImpl,
    package_id: PackageId,
    destination: CaptureScope,
    closure_target: LocalItemId,
    capture_bindings: &[CaptureBinding],
    assigner: &mut Assigner,
) -> bool {
    let calls = collect_calls_to_closure_target(package, callable_impl, package_id, closure_target);
    prepend_captures_to_calls(
        package,
        &calls.into_iter().collect::<Vec<_>>(),
        package_id,
        destination,
        closure_target,
        capture_bindings,
        assigner,
    )
}

/// Collects the block ids of every specialization of a callable implementation:
/// `body`, `adj`, `ctl`, and `ctl_adj`.
fn spec_block_ids(callable_impl: &CallableImpl) -> Vec<qsc_fir::fir::BlockId> {
    let mut ids = Vec::new();
    match callable_impl {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            ids.push(spec_impl.body.block);
            if let Some(ref adj) = spec_impl.adj {
                ids.push(adj.block);
            }
            if let Some(ref ctl) = spec_impl.ctl {
                ids.push(ctl.block);
            }
            if let Some(ref ctl_adj) = spec_impl.ctl_adj {
                ids.push(ctl_adj.block);
            }
        }
    }
    ids
}

/// Drops `let <pat> = <param_var>` destructuring statements from every
/// specialization block.
///
/// The combined nested path in [`specialize_many`] removes a tuple-valued
/// parameter slot once all of its arrow fields have been specialized away and
/// their projected calls retargeted. The destructuring that bound those fields
/// is then dead and still references the slot about to be removed, so it must
/// be dropped to avoid a dangling parameter reference. Orphaned statement nodes
/// are reclaimed by the later unreachable-node collection.
///
/// Only statements whose initializer is a direct `Var(Local(param_var))` are
/// removed; tuple fields reached through intermediate alias bindings are not
/// covered, which keeps the combined path scoped to direct destructuring.
fn remove_param_destructuring_stmts(
    package: &mut Package,
    callable_impl: &CallableImpl,
    param_vars: &FxHashSet<LocalVarId>,
) {
    for block_id in spec_block_ids(callable_impl) {
        let stmt_ids = package
            .blocks
            .get(block_id)
            .expect("block not found")
            .stmts
            .clone();
        let mut retained = Vec::with_capacity(stmt_ids.len());
        for stmt_id in stmt_ids {
            let stmt = package.stmts.get(stmt_id).expect("stmt not found");
            let drop_stmt = if let qsc_fir::fir::StmtKind::Local(_, _, expr_id) = &stmt.kind {
                let init = package.exprs.get(*expr_id).expect("expr not found");
                matches!(&init.kind, ExprKind::Var(Res::Local(var), _) if param_vars.contains(var))
            } else {
                false
            };
            if !drop_stmt {
                retained.push(stmt_id);
            }
        }
        package
            .blocks
            .get_mut(block_id)
            .expect("block not found")
            .stmts = retained;
    }
}

/// Removes several callable parameter slots from a specialized callable's input
/// pattern in a single pass, updating the corresponding types.
///
/// # Before
/// ```text
/// input = (param_0, callable_param, param_2)   // a removed slot at index 1
/// ```
/// # After
/// ```text
/// input = (param_0, param_2)   // removed slots dropped
/// ```
///
/// # Mutations
/// - Rewrites the input `Pat` node's `kind` and `ty` in place.
/// - Replaces `decl.input` with a lone surviving pattern only when no captures
///   were appended. Partial nested removals are handled by the caller.
///
/// Used by [`specialize_many`] when more than one arrow parameter is removed at
/// once: filtering the slots one at a time would shift the surviving indices
/// between removals. Each member is removed by its top-level slot. A nested
/// member, one selecting an arrow field of a tuple-valued parameter, only
/// reaches here when its group covers every field of that tuple, as selected by
/// [`remove_combined_callable_params`], so its whole top-level slot is
/// dropped and the dead destructuring of that slot is removed beforehand by
/// [`remove_param_destructuring_stmts`].
///
/// `num_appended_captures` is the count of capture patterns already appended to
/// the input by [`thread_closure_captures`]. The surviving input is flattened to
/// a scalar only when exactly one element remains and no captures were appended,
/// so a lone surviving capture keeps its tuple shape and stays aligned with the
/// call-site rewrite that supplies the capture operand.
fn remove_callable_params(
    udts: &UdtMetadata,
    package: &mut Package,
    decl: &mut CallableDecl,
    params: &[&CallableParam],
    num_appended_captures: usize,
) {
    let input = package.get_pat(decl.input);
    let paths: Vec<_> = if matches!(input.kind, PatKind::Tuple(_)) {
        params
            .iter()
            .map(|param| vec![param.top_level_param])
            .collect()
    } else {
        vec![Vec::new()]
    };
    let paths: Vec<_> = paths.iter().map(Vec::as_slice).collect();
    let Some(mut removal) = InputRemoval::new(udts, &input.ty, &paths) else {
        return;
    };
    if num_appended_captures != 0
        && let InputRemoval::Tuple { collapse, .. } = &mut removal
    {
        *collapse = false;
    }
    remove_pat_with_layout(udts, package, decl.input, &removal);
}

/// Removes a top-level callable parameter from the specialized callable's
/// input pattern, collapsing a singleton tuple to a bare pattern and
/// replacing a bare-bind pattern with `Unit` when the removed parameter was
/// the only one.
///
/// Delegates to [`remove_nested_callable_params`] when `param.field_path` is
/// non-empty (the callable argument is nested inside a tuple parameter).
///
/// # Before
/// ```text
/// input = (a, callable, b)   // top_level_param = 1
/// ```
/// # After
/// ```text
/// input = (a, b)             // callable removed, tuple shrunk
/// ```
fn remove_callable_param(
    udts: &UdtMetadata,
    package: &mut Package,
    decl: &mut CallableDecl,
    param: &CallableParam,
    had_closure_captures: bool,
    assigner: &mut Assigner,
) {
    if !param.field_path.is_empty() {
        remove_nested_callable_params(
            udts,
            package,
            decl,
            param,
            &[&param.field_path],
            had_closure_captures,
            assigner,
        );
        return;
    }

    remove_callable_params(udts, package, decl, &[param], 0);
}

/// Removes a nested callable parameter from the specialized callable's input
/// pattern by navigating into the tuple type at the outer position and removing
/// the arrow element at the inner position. Also rewrites any destructuring
/// patterns in the body that bind the removed element.
///
/// # Before
/// ```text
/// input = (outer: (a, callable, b))   // field_path = [1]
/// ```
/// # After
/// ```text
/// input = (outer: (a, b))   // nested callable removed
/// ```
///
/// # Mutations
/// - Rewrites `Pat.ty` for the sub-pattern and outer tuple in place.
/// - Rewrites destructuring patterns in the body via
///   [`rewrite_destructuring_pat_in_block`].
#[allow(clippy::too_many_arguments)]
fn remove_nested_callable_params(
    udts: &UdtMetadata,
    package: &mut Package,
    decl: &mut CallableDecl,
    param: &CallableParam,
    removals: &[&[usize]],
    had_closure_captures: bool,
    assigner: &mut Assigner,
) {
    let input_pat = package
        .pats
        .get(decl.input)
        .expect("input pat not found")
        .clone();

    let outer_idx = param.top_level_param;

    // Set when the nested removal consumes the parameter's entire tuple, a
    // single-field tuple whose only element was an inlined arrow, and closure
    // captures were threaded as new slots. In that case the parameter's slot and
    // its destructuring are removed rather than retyped to unit, matching the
    // call site that drops the consumed slot and supplies the captures. Without
    // threaded captures the call site keeps the slot as `()`, so the parameter
    // is retyped to unit; the captureless path is unchanged.
    let mut fully_consumed = false;

    // Structural type of the parameter before its callable field is removed.
    // Captured so sibling field accesses in the body can be reindexed against
    // the parameter's pre-removal shape.
    let orig_param_ty = match &input_pat.kind {
        PatKind::Tuple(pats) => {
            // Navigate to the sub-pattern at outer_idx and modify its type.
            let sub_pat_id = pats[outer_idx];
            let sub_pat = package.pats.get(sub_pat_id).expect("pat not found").clone();
            let orig_ty = sub_pat.ty.clone();
            let Some(removal) = InputRemoval::new(udts, &sub_pat.ty, removals) else {
                return;
            };
            let new_ty = removal.reduced_ty(udts, &sub_pat.ty);
            fully_consumed = had_closure_captures && removal.consumes_original();

            if fully_consumed {
                // The removal consumed the parameter's entire tuple, a
                // single-field tuple whose only element was an inlined arrow.
                // The sub-slot carries no surviving input, so drop it from the
                // outer tuple, mirroring the non-nested removal, instead of
                // leaving a phantom unit parameter that the call site does not
                // supply. The dead destructuring of this slot is removed below.
                remove_callable_params(udts, package, decl, &[param], 1);
            } else {
                let sub_pat_mut = package.pats.get_mut(sub_pat_id).expect("pat not found");
                sub_pat_mut.ty = new_ty.clone();

                // Update the outer tuple's type to reflect the changed sub-parameter.
                let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
                if let Ty::Tuple(ref mut tys) = input_pat_mut.ty {
                    tys[outer_idx] = new_ty;
                }
            }
            Some(orig_ty)
        }
        PatKind::Bind(_) => {
            // Single param that is a tuple type — modify the type directly.
            let Some(removal) = InputRemoval::new(udts, &input_pat.ty, removals) else {
                return;
            };
            let new_ty = removal.reduced_ty(udts, &input_pat.ty);
            let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
            input_pat_mut.ty = new_ty;
            Some(input_pat.ty.clone())
        }
        PatKind::Discard => None,
    };

    // Match original destructuring before rebasing can turn a field read into
    // a direct parameter read with an already reduced binding type.
    if !removals.is_empty() {
        if fully_consumed {
            // The parameter's entire tuple was consumed, so its destructuring
            // binding `let (a,) = ops` is dead; the body's calls were already
            // rewritten to the inlined callable. Remove the binding rather than
            // rewriting it to a dangling `let () = ops`.
            let mut param_vars = FxHashSet::default();
            param_vars.insert(param.param_var);
            remove_param_destructuring_stmts(package, &decl.implementation, &param_vars);
        } else if let CallableImpl::Spec(spec_impl) = &decl.implementation {
            rewrite_destructuring_pat_in_block(
                udts,
                package,
                spec_impl.body.block,
                param.param_var,
                removals,
            );
            if let Some(ref adj) = spec_impl.adj {
                rewrite_destructuring_pat_in_block(
                    udts,
                    package,
                    adj.block,
                    param.param_var,
                    removals,
                );
            }
            if let Some(ref ctl) = spec_impl.ctl {
                rewrite_destructuring_pat_in_block(
                    udts,
                    package,
                    ctl.block,
                    param.param_var,
                    removals,
                );
            }
            if let Some(ref ctl_adj) = spec_impl.ctl_adj {
                rewrite_destructuring_pat_in_block(
                    udts,
                    package,
                    ctl_adj.block,
                    param.param_var,
                    removals,
                );
            }
        }
        if let Some(orig_param_ty) = orig_param_ty {
            reindex_sibling_field_access(
                udts,
                package,
                &decl.implementation,
                param.param_var,
                removals,
                &orig_param_ty,
                assigner,
            );
        }
    }
}

/// Rewrites each matching destructuring `let` statement in a block and its nested
/// blocks once. The initializer must be `Var(Local(param_var))`; the sub-pattern
/// at `inner_path` is removed from its tuple pattern.
///
/// # Before
/// ```text
/// let (a, callable, b) = param_var;   // inner_path = [1]
/// ```
/// # After
/// ```text
/// let (a, b) = param_var;   // callable sub-pattern removed
/// ```
///
/// # Mutations
/// - Rewrites `Pat.kind` and `Pat.ty` via [`remove_pat_at_field_paths`].
/// - Updates the init expression's type to match the rewritten pattern.
fn rewrite_destructuring_pat_in_block(
    udts: &UdtMetadata,
    package: &mut Package,
    block_id: qsc_fir::fir::BlockId,
    param_var: LocalVarId,
    paths: &[&[usize]],
) {
    for stmt_id in crate::walk_utils::collect_stmt_ids_in_block(package, block_id) {
        let stmt = package.stmts.get(stmt_id).expect("stmt not found").clone();
        if let qsc_fir::fir::StmtKind::Local(_, pat_id, expr_id) = &stmt.kind {
            let rewrites_param_var = {
                let init_expr = package.exprs.get(*expr_id).expect("expr not found");
                matches!(&init_expr.kind, ExprKind::Var(Res::Local(var), _) if *var == param_var)
            };
            if rewrites_param_var && remove_pat_at_field_paths(udts, package, *pat_id, paths) {
                let new_init_ty = package.pats.get(*pat_id).expect("pat not found").ty.clone();
                let init_mut = package.exprs.get_mut(*expr_id).expect("expr not found");
                init_mut.ty = new_init_ty;
            }
        }
    }
}

/// Removes the sub-pattern at `field_path` from a tuple pattern structure,
/// rewriting the outer pattern type so parameter removal stays type-
/// consistent.
///
/// # Before
/// ```text
/// Pat::Tuple([p0, p1, p2])   // field_path = [1]
/// ```
/// # After
/// ```text
/// Pat::Tuple([p0, p2])   // p1 removed, ty updated
/// ```
///
/// # Mutations
/// - Rewrites `Pat.kind` and `Pat.ty` in place.
/// - Flattens single-element tuples.
fn remove_pat_at_field_paths(
    udts: &UdtMetadata,
    package: &mut Package,
    pat_id: PatId,
    paths: &[&[usize]],
) -> bool {
    let Some(removal) = InputRemoval::new(udts, &package.get_pat(pat_id).ty, paths) else {
        return false;
    };
    remove_pat_with_layout(udts, package, pat_id, &removal)
}

fn remove_pat_with_layout(
    udts: &UdtMetadata,
    package: &mut Package,
    pat_id: PatId,
    removal: &InputRemoval,
) -> bool {
    let pat = package.pats.get(pat_id).expect("pat not found").clone();
    if removal.is_removed() {
        let pat = package.pats.get_mut(pat_id).expect("pat exists");
        pat.kind = PatKind::Tuple(Vec::new());
        pat.ty = Ty::UNIT;
        return true;
    }
    let (PatKind::Tuple(sub_pats), InputRemoval::Tuple { children, collapse }) =
        (&pat.kind, removal)
    else {
        let ty = removal.reduced_ty(udts, &pat.ty);
        let changed = ty != pat.ty;
        package.pats.get_mut(pat_id).expect("pat exists").ty = ty;
        return changed;
    };
    if children.len() != sub_pats.len() {
        return false;
    }
    let mut changed = false;
    let mut remaining = Vec::with_capacity(sub_pats.len());
    for (&child, removal) in sub_pats.iter().zip(children) {
        if removal.is_removed() {
            changed = true;
            continue;
        }
        changed |= remove_pat_with_layout(udts, package, child, removal);
        remaining.push(child);
    }
    if !changed {
        return false;
    }
    let (kind, ty) = if *collapse {
        flattened_tuple_pat(package, &remaining)
    } else {
        let tys = remaining
            .iter()
            .map(|id| package.get_pat(*id).ty.clone())
            .collect();
        (PatKind::Tuple(remaining), Ty::Tuple(tys))
    };
    let pat_mut = package.pats.get_mut(pat_id).expect("pat not found");
    pat_mut.kind = kind;
    pat_mut.ty = ty;
    true
}

/// Builds `(PatKind, Ty)` from surviving patterns: no elements yield unit,
/// one element yields that pattern's kind and type, and multiple elements
/// yield a tuple. A surviving child tuple is not recursively flattened.
fn flattened_tuple_pat(package: &Package, sub_pats: &[PatId]) -> (PatKind, Ty) {
    match sub_pats {
        [] => (PatKind::Tuple(Vec::new()), Ty::UNIT),
        [only_pat_id] => {
            let only_pat = package.get_pat(*only_pat_id);
            (only_pat.kind.clone(), only_pat.ty.clone())
        }
        _ => (
            PatKind::Tuple(sub_pats.to_vec()),
            Ty::Tuple(
                sub_pats
                    .iter()
                    .map(|pat_id| package.get_pat(*pat_id).ty.clone())
                    .collect(),
            ),
        ),
    }
}

/// Rewrites sibling field accesses in a specialized callable's body after a
/// callable field is removed from a tuple-typed parameter.
///
/// All paths are interpreted against `orig_param_ty`. Collect every projection
/// before changing any field chain, then rebase through the shared removal
/// layout. Preserve nominal extraction nodes even when their structural path
/// becomes empty after a singleton collapse.
///
/// # Before
/// ```text
/// // param ty = ((Qubit => Unit), (Qubit => Unit)), removed_path = [0]
/// Field(Var(param), [1])   // selects the second callable
/// ```
/// # After
/// ```text
/// // param ty collapsed to (Qubit => Unit)
/// Var(param)   // the surviving callable is now the parameter itself
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` and `Expr.ty` of affected `Field` expressions in
///   place.
#[allow(clippy::too_many_arguments)]
fn reindex_sibling_field_access(
    udts: &UdtMetadata,
    package: &mut Package,
    callable_impl: &CallableImpl,
    param_var: LocalVarId,
    removed_paths: &[&[usize]],
    orig_param_ty: &Ty,
    assigner: &mut Assigner,
) {
    let Some(removal) = InputRemoval::new(udts, orig_param_ty, removed_paths) else {
        return;
    };
    let reduced = removal.reduced_ty(udts, orig_param_ty);
    let mut rewrites = Vec::new();
    for_each_expr_in_callable_impl(package, callable_impl, &mut |expr_id, expr| {
        if let Some(path) = collect_field_path_from_param(package, expr_id, param_var)
            && let Some(rebased) = removal.rebase(&path)
        {
            let result_ty = if matches!(expr.kind, ExprKind::Var(_, _)) {
                reduced.clone()
            } else {
                let relative: Vec<_> = removed_paths
                    .iter()
                    .filter_map(|removed| removed.strip_prefix(path.as_slice()))
                    .collect();
                let Some(relative) = InputRemoval::new(udts, &expr.ty, &relative) else {
                    return;
                };
                relative.reduced_ty(udts, &expr.ty)
            };
            rewrites.push((expr_id, rebased, result_ty, expr.span));
        }
    });
    for (expr_id, path, result_ty, span) in rewrites {
        let mut kind = ExprKind::Var(Res::Local(param_var), Vec::new());
        let mut ty = reduced.clone();
        for index in path {
            let Ty::Tuple(fields) = udts.underlying_ty(&ty) else {
                unreachable!("rebased path follows validated tuple layout");
            };
            let field_ty = fields[index].clone();
            let base = alloc_expr(package, assigner, ty, kind, span);
            kind = ExprKind::Field(
                base,
                Field::Path(FieldPath {
                    indices: vec![index],
                }),
            );
            ty = field_ty;
        }
        while ty != result_ty {
            let Ty::Udt(Res::Item(item)) = &ty else {
                unreachable!("rebased projection preserves its payload type");
            };
            let Some(payload) = udts.pure_ty(*item).cloned() else {
                unreachable!("projected UDT has declaration metadata");
            };
            let base = alloc_expr(package, assigner, ty, kind, span);
            kind = ExprKind::Field(
                base,
                Field::Path(FieldPath {
                    indices: Vec::new(),
                }),
            );
            ty = payload;
        }
        let expr = package.exprs.get_mut(expr_id).expect("projection exists");
        expr.kind = kind;
        expr.ty = ty;
    }
}

/// Builds a standalone `Package` with a callable's input and implementation
/// nodes, including nested items and lifted closure targets, so cloning can
/// read a disjoint source while mutating the target package. Global item
/// references are retained rather than followed.
fn extract_callable_body(source_pkg: &Package, decl: &CallableDecl) -> Package {
    let mut body_pkg = Package::default();

    extract_pat(source_pkg, decl.input, &mut body_pkg);

    match &decl.implementation {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            extract_spec_decl_body(source_pkg, &spec_impl.body, &mut body_pkg);
            for spec in functored_specs(spec_impl) {
                extract_spec_decl_body(source_pkg, spec, &mut body_pkg);
            }
        }
    }

    body_pkg
}

/// Copies a `SpecDecl`'s input pattern and block into the extraction
/// target package.
fn extract_spec_decl_body(source: &Package, spec: &qsc_fir::fir::SpecDecl, target: &mut Package) {
    if let Some(pat_id) = spec.input {
        extract_pat(source, pat_id, target);
    }
    extract_block(source, spec.block, target);
}

/// Recursively copies a block and every statement it references into the
/// extraction target.
fn extract_block(source: &Package, block_id: qsc_fir::fir::BlockId, target: &mut Package) {
    if target.blocks.contains_key(block_id) {
        return;
    }
    let block = source.get_block(block_id);
    target.blocks.insert(block_id, block.clone());
    for &stmt_id in &block.stmts {
        extract_stmt(source, stmt_id, target);
    }
}

/// Recursively copies a statement and its referenced patterns, expressions,
/// or items into the extraction target.
fn extract_stmt(source: &Package, stmt_id: qsc_fir::fir::StmtId, target: &mut Package) {
    if target.stmts.contains_key(stmt_id) {
        return;
    }
    let stmt = source.get_stmt(stmt_id);
    target.stmts.insert(stmt_id, stmt.clone());
    match &stmt.kind {
        qsc_fir::fir::StmtKind::Expr(e) | qsc_fir::fir::StmtKind::Semi(e) => {
            extract_expr(source, *e, target);
        }
        qsc_fir::fir::StmtKind::Local(_, pat, expr) => {
            extract_pat(source, *pat, target);
            extract_expr(source, *expr, target);
        }
        qsc_fir::fir::StmtKind::Item(item_id) => {
            extract_item(source, *item_id, target);
        }
    }
}

/// Recursively copies an expression's child nodes and lifted closure targets
/// into the extraction target. Global item references are not followed.
///
/// The `ExprKind::Closure` arm extracts the closure's lambda-lifted target item
/// so the cloner relocates the lambda into the specialization target package
/// with a fresh id. This is required for cross-package specialization: a closure
/// carries a bare `LocalItemId` with no package qualifier, so the lambda it
/// references must live in the same package as the closure expression. Named
/// nested functions in `StmtKind::Item` are followed for the same reason.
#[allow(clippy::too_many_lines)]
fn extract_expr(source: &Package, expr_id: ExprId, target: &mut Package) {
    if target.exprs.contains_key(expr_id) {
        return;
    }
    let expr = source.get_expr(expr_id);
    target.exprs.insert(expr_id, expr.clone());
    match &expr.kind {
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                extract_expr(source, e, target);
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
            extract_expr(source, *a, target);
            extract_expr(source, *b, target);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            extract_expr(source, *a, target);
            extract_expr(source, *b, target);
            extract_expr(source, *c, target);
        }
        ExprKind::Block(block_id) => extract_block(source, *block_id, target),
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            extract_expr(source, *e, target);
        }
        ExprKind::If(cond, body, otherwise) => {
            extract_expr(source, *cond, target);
            extract_expr(source, *body, target);
            if let Some(e) = otherwise {
                extract_expr(source, *e, target);
            }
        }
        ExprKind::Range(s, st, e) => {
            for x in [s, st, e].into_iter().flatten() {
                extract_expr(source, *x, target);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                extract_expr(source, *c, target);
            }
            for fa in fields {
                extract_expr(source, fa.value, target);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    extract_expr(source, *e, target);
                }
            }
        }
        ExprKind::While(cond, block) => {
            extract_expr(source, *cond, target);
            extract_block(source, *block, target);
        }
        ExprKind::Closure(_, target_item) => {
            extract_item(source, *target_item, target);
        }
        ExprKind::Parallel(limit, body) => {
            if let Some(limit) = limit {
                extract_expr(source, *limit, target);
            }
            extract_expr(source, *body, target);
        }
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Recursively copies a nested item or lifted closure target and its callable
/// body into the extraction target so that
/// [`FirCloner::clone_nested_item`](crate::cloner::FirCloner) can find it
/// during specialization. Simulatable intrinsics become plain intrinsics;
/// their simulation bodies are not extracted.
fn extract_item(source: &Package, item_id: LocalItemId, target: &mut Package) {
    if target.items.contains_key(item_id) {
        return;
    }
    let item = source.get_item(item_id);
    let mut extracted_item = item.clone();
    if let ItemKind::Callable(decl) = &mut extracted_item.kind
        && matches!(decl.implementation, CallableImpl::SimulatableIntrinsic(_))
    {
        decl.implementation = CallableImpl::Intrinsic;
    }
    target.items.insert(item_id, extracted_item);
    if let ItemKind::Callable(decl) = &item.kind {
        extract_pat(source, decl.input, target);
        match &decl.implementation {
            CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
            CallableImpl::Spec(spec_impl) => {
                extract_spec_decl_body(source, &spec_impl.body, target);
                for spec in functored_specs(spec_impl) {
                    extract_spec_decl_body(source, spec, target);
                }
            }
        }
    }
}

/// Recursively copies a pattern and its sub-patterns into the extraction
/// target.
fn extract_pat(source: &Package, pat_id: PatId, target: &mut Package) {
    if target.pats.contains_key(pat_id) {
        return;
    }
    let pat = source.get_pat(pat_id);
    target.pats.insert(pat_id, pat.clone());
    if let PatKind::Tuple(sub_pats) = &pat.kind {
        for &p in sub_pats {
            extract_pat(source, p, target);
        }
    }
}
