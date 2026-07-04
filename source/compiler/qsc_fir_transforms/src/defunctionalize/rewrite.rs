// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rewrite phase of the defunctionalization pass.
//!
//! For each call site where a higher-order function is invoked with a concrete
//! callable argument, this module rewrites the call to invoke the specialized
//! callable directly, removes the callable argument from the call's argument
//! tuple, and threads closure captures as extra arguments when applicable.
//!
//! # Subsystems
//!
//! The module is organized into three cooperating subsystems:
//!
//! - **Dispatch synthesis** — synthesizes `if`/`else` chains that select a
//!   specialized callee per reaching-definition branch for call sites whose
//!   analysis produced a `Multi` lattice with branch conditions (see
//!   [`synthesize_callsite_index_dispatch`],
//!   [`synthesize_direct_index_dispatch`], and the
//!   `synthesize_index_dispatch_plan` family).
//! - **Direct-call dispatch** — rewrites callee expressions, callee types,
//!   and argument tuples so a HOF invocation becomes a direct call to the
//!   specialized target (see [`rewrite_direct_call`],
//!   [`rewrite_direct_callee`], [`rewrite_direct_closure_args`], and
//!   `build_direct_global_callee_ty`).
//! - **Dead-local cleanup** — removes callable-typed locals whose only
//!   remaining uses were direct-call rewrites, keeping `PostDefunc` clean
//!   of arrow-typed residues (see the `prune_*` and
//!   `remove_dead_callable_local_*` helpers).
//!
//! # Notes
//!
//! - A copy of the `apply_target_input_at_control_path` helper also lives
//!   in `super::specialize`. The copy is retained so that specialize and
//!   rewrite can evolve their controlled-layer handling independently
//!   without forcing a shared abstraction boundary; update both copies in
//!   lockstep when controlled-layer semantics change.

use super::types::{
    AnalysisResult, CallSite, CallableParam, CapturedVar, ConcreteCallable, DirectCallSite,
    SpecKey, peel_body_functors,
};
use super::{
    build_combined_spec_key, build_combined_spec_key_for_group, build_spec_key,
    is_combined_eligible, partition_mixed_branch_split, ty_contains_arrow,
};
use crate::EMPTY_EXEC_RANGE;
use crate::walk_utils::{DirectChild, UseClass, classify_block_use, for_each_direct_child};
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, Block, BlockId, CallableImpl, CallableKind, Expr, ExprId, ExprKind, Field, FieldAssign,
    Functor, ItemId, ItemKind, Lit, LocalItemId, LocalVarId, Mutability, Package, PackageId,
    PackageLookup, Pat, PatId, PatKind, Res, Stmt, StmtId, StmtKind, StoreItemId, UnOp,
};
use qsc_fir::ty::{Arrow, Prim, Ty};
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

/// A resolved HOF dispatch target: the `(call site, specialization item, param)`
/// triple produced during branch-split rewriting.
type HofDispatchTarget<'a> = (&'a CallSite, StoreItemId, &'a CallableParam);

/// A HOF dispatch target paired with its guard list (empty list = default branch).
///
/// Guards are stored outermost-first; they are folded into a left-associated
/// `AndL` conjunction at rewrite time.
type ConditionedHofTarget<'a> = (HofDispatchTarget<'a>, Vec<ExprId>);

/// Rewrites call sites in the target package so that higher-order calls are
/// replaced with direct calls to their specialized counterparts.
///
/// For each call site with a matching specialization in `spec_map`:
/// - The callee expression is replaced with a reference to the specialized
///   callable.
/// - The callable argument is removed from the argument tuple.
/// - If the callable argument was a closure, its captured variables are
///   appended as extra arguments.
/// - The callee expression's type is updated to reflect the new signature.
pub(super) fn rewrite(
    package: &mut Package,
    package_id: PackageId,
    analysis: &AnalysisResult,
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    assigner: &mut Assigner,
) {
    let expr_owner_lookup = build_expr_owner_lookup(package);
    let mut rewritten_callable_arg_locals = FxHashSet::default();

    // Source-array locals for closure callable-arrays that a higher-order call
    // forwards (directly, through a struct-literal field, or through `let`
    // aliases) and fully consumes. The bare-`Var` recorder above cannot see the
    // underlying array when it is wrapped in a struct field or aliased, and the
    // array element type keeps `remove_dead_callable_local_from_callable` from
    // pruning it. Tracing the forwarded value back to its source-array local
    // lets the closure-bearing cleanup remove the now-dead binding instead of
    // leaving an array of blanked (unit) closure elements — an arrow-typed
    // block with a unit tail — stranded in a reachable caller.
    let mut hof_consumed_source_arrays = FxHashSet::default();

    // Lowest-index lookup serves the per-row and branch-split paths, where
    // every row of a group resolves the same parameter; it cannot distinguish
    // between separate arrow parameters.
    let param_lookup: FxHashMap<StoreItemId, &CallableParam> = {
        let mut map = FxHashMap::default();
        for p in &analysis.callable_params {
            map.entry(p.callable_id).or_insert(p);
        }
        map
    };

    // Precise lookup keyed by parameter position recovers the exact parameter
    // for each distinct slot of a combined multi-argument call.
    let param_by_position: FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam> = {
        let mut map = FxHashMap::default();
        for p in &analysis.callable_params {
            map.insert((p.callable_id, p.top_level_param, p.field_path.clone()), p);
        }
        map
    };

    // Group this package's static call sites by call expression. A combined
    // multi-argument call contributes one row per arrow parameter; those rows
    // rewrite together so the single call shape matches the one combined
    // specialization. Rows that share an expression but resolve the same
    // parameter, which are the branch-split candidate sets, keep their dispatch
    // path.
    let mut grouped: FxHashMap<ExprId, Vec<&CallSite>> = FxHashMap::default();
    for call_site in &analysis.call_sites {
        // This pass rewrites one package at a time; skip call sites that live
        // in a different package's body.
        if call_site.call_pkg_id != package_id {
            continue;
        }
        // Skip dynamic callables — they have no specialization.
        if matches!(call_site.callable_arg, ConcreteCallable::Dynamic) {
            continue;
        }
        grouped
            .entry(call_site.call_expr_id)
            .or_default()
            .push(call_site);
    }

    for (call_expr_id, group) in &grouped {
        // Combined multi-argument rewrite: specialize and rewrite consult the
        // same predicate so they agree on which call sites are combined.
        if is_combined_eligible(package, group) {
            rewrite_combined_group(
                package,
                *call_expr_id,
                group,
                spec_map,
                &param_by_position,
                &expr_owner_lookup,
                &mut rewritten_callable_arg_locals,
                &mut hof_consumed_source_arrays,
                assigner,
            );
            continue;
        }

        // Per-leaf producer-closure inline. The specialize side built one
        // combined spec per dispatch candidate, formed as `[candidate] +
        // single-valued siblings`, for this mixed branch-split group. Route the
        // synthesized dispatch leaves through those combined specs so each leaf
        // inlines the single-valued producer closure, consumed in the same pass
        // before any later-iteration producer-body clearing.
        if rewrite_mixed_branch_split_group(
            package,
            package_id,
            *call_expr_id,
            group,
            spec_map,
            &param_by_position,
            &expr_owner_lookup,
            &mut rewritten_callable_arg_locals,
            &mut hof_consumed_source_arrays,
            assigner,
        ) {
            continue;
        }

        rewrite_per_row_group(
            package,
            package_id,
            *call_expr_id,
            group,
            spec_map,
            &param_lookup,
            &param_by_position,
            &expr_owner_lookup,
            &mut rewritten_callable_arg_locals,
            assigner,
        );
    }

    rewrite_direct_call_sites(
        package,
        package_id,
        analysis,
        &expr_owner_lookup,
        &mut rewritten_callable_arg_locals,
        assigner,
    );

    prune_dead_callable_arg_locals(
        package,
        &rewritten_callable_arg_locals,
        &hof_consumed_source_arrays,
    );
}

/// Rewrites a combined multi-argument HOF call whose arrow parameters all
/// specialize together under a single combined key.
///
/// Recovers the exact [`CallableParam`] for each row via `param_by_position`,
/// orders the members ascending by parameter position so the rewritten argument
/// tuple lines up with the specialize-side combined input pattern, records the
/// consumed callable-arg locals and source arrays, then dispatches to either
/// the callable-array or the plain multi-argument rewrite. Returns without
/// rewriting when the combined spec is missing or any row's parameter cannot be
/// resolved.
#[allow(clippy::too_many_arguments)]
fn rewrite_combined_group(
    package: &mut Package,
    call_expr_id: ExprId,
    group: &[&CallSite],
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    hof_consumed_source_arrays: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    assigner: &mut Assigner,
) {
    let hof_item_id = group[0].hof_item_id;
    let spec_key = build_combined_spec_key_for_group(hof_item_id, group);
    let Some(&spec_store_id) = spec_map.get(&spec_key) else {
        return;
    };
    let hof_store_id = StoreItemId::from((hof_item_id.package, hof_item_id.item));

    // Recover the exact parameter per row, then order the members
    // ascending by parameter position so the rewritten argument tuple
    // lines up with the specialize-side combined input pattern.
    let mut members: Vec<(&CallSite, &CallableParam)> = Vec::with_capacity(group.len());
    for call_site in group {
        let position_key = (
            hof_store_id,
            call_site.top_level_param,
            call_site.field_path.clone(),
        );
        if let Some(&param) = param_by_position.get(&position_key) {
            members.push((call_site, param));
        } else {
            return;
        }
    }
    members.sort_by(|a, b| {
        a.1.top_level_param
            .cmp(&b.1.top_level_param)
            .then_with(|| a.1.field_path.cmp(&b.1.field_path))
    });

    for (call_site, _) in &members {
        collect_rewritten_callable_arg_local(
            package,
            expr_owner_lookup,
            call_site.call_expr_id,
            call_site.arg_expr_id,
            rewritten_callable_arg_locals,
        );
        collect_hof_consumed_source_array(
            package,
            expr_owner_lookup,
            call_site.call_expr_id,
            call_site.arg_expr_id,
            hof_consumed_source_arrays,
        );
    }
    if callable_array_member_position(&members).is_some()
        && callable_array_member_needs_nested_rewrite(&members)
    {
        rewrite_callable_array_multi(
            package,
            call_expr_id,
            &members,
            spec_store_id,
            expr_owner_lookup,
            assigner,
        );
    } else {
        rewrite_multi(
            package,
            call_expr_id,
            &members,
            spec_store_id,
            expr_owner_lookup,
            assigner,
        );
    }
}

/// Attempts the per-leaf producer-closure inline for a mixed branch-split
/// group, routing each synthesized dispatch leaf through the per-candidate
/// combined spec (`[candidate] + single-valued siblings`) so each leaf inlines
/// the single-valued producer closure in the same pass.
///
/// Returns `true` when the group was rewritten. Returns `false` when the group
/// is not a mixed branch-split, or when a required combined spec or parameter
/// cannot be resolved, so the caller falls through to the per-row path, which
/// surfaces an honest `DynamicCallable` diagnostic instead of wrong QIR.
#[allow(clippy::too_many_arguments)]
fn rewrite_mixed_branch_split_group(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    group: &[&CallSite],
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    hof_consumed_source_arrays: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    assigner: &mut Assigner,
) -> bool {
    let Some((dispatch, constants)) = partition_mixed_branch_split(group) else {
        return false;
    };
    let hof_item_id = group[0].hof_item_id;
    let hof_store_id = StoreItemId::from((hof_item_id.package, hof_item_id.item));

    // Resolve constant sibling params; operand order is handled in the
    // leaf builder `create_combined_branch_call`.
    let mut const_members: Vec<(&CallSite, &CallableParam)> = Vec::with_capacity(constants.len());
    let mut resolved = true;
    for cs in &constants {
        let position_key = (hof_store_id, cs.top_level_param, cs.field_path.clone());
        if let Some(&param) = param_by_position.get(&position_key) {
            const_members.push((*cs, param));
        } else {
            resolved = false;
            break;
        }
    }

    // Build dispatch entries keyed by the per-candidate combined spec.
    let mut entries: Vec<HofDispatchTarget> = Vec::with_capacity(dispatch.len());
    if resolved {
        for candidate in &dispatch {
            let mut members_cs: Vec<&CallSite> = Vec::with_capacity(constants.len() + 1);
            members_cs.push(*candidate);
            members_cs.extend(constants.iter().copied());
            let spec_key = build_combined_spec_key(hof_item_id, &members_cs);
            let position_key = (
                hof_store_id,
                candidate.top_level_param,
                candidate.field_path.clone(),
            );
            if let (Some(&spec_store_id), Some(&param)) = (
                spec_map.get(&spec_key),
                param_by_position.get(&position_key),
            ) {
                entries.push((*candidate, spec_store_id, param));
            } else {
                resolved = false;
                break;
            }
        }
    }

    if resolved && !entries.is_empty() {
        for call_site in group {
            collect_rewritten_callable_arg_local(
                package,
                expr_owner_lookup,
                call_site.call_expr_id,
                call_site.arg_expr_id,
                rewritten_callable_arg_locals,
            );
            collect_hof_consumed_source_array(
                package,
                expr_owner_lookup,
                call_site.call_expr_id,
                call_site.arg_expr_id,
                hof_consumed_source_arrays,
            );
        }
        branch_split_rewrite(
            package,
            package_id,
            call_expr_id,
            &entries,
            &const_members,
            expr_owner_lookup,
            assigner,
        );
        return true;
    }
    // Combined specs not found — fall through to the per-row path.
    false
}

/// Rewrites a HOF call group on the per-row / branch-split path: resolves each
/// row under its single-argument spec key and exact parameter position, then
/// dispatches by candidate count.
///
/// A single resolved entry uses the direct [`rewrite_one`] rewrite; multiple
/// entries synthesize a condition-indexed dispatch via [`branch_split_rewrite`].
/// Rows whose spec or parameter cannot be resolved are skipped.
#[allow(clippy::too_many_arguments)]
fn rewrite_per_row_group(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    group: &[&CallSite],
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    param_lookup: &FxHashMap<StoreItemId, &CallableParam>,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    assigner: &mut Assigner,
) {
    let mut entries: Vec<HofDispatchTarget> = Vec::with_capacity(group.len());
    for call_site in group {
        let spec_key = build_spec_key(call_site);
        let Some(&spec_store_id) = spec_map.get(&spec_key) else {
            continue;
        };
        let hof_store_id =
            StoreItemId::from((call_site.hof_item_id.package, call_site.hof_item_id.item));
        // Resolve the exact parameter for this row's position so a removed
        // sibling drops its own slot rather than the lowest-index slot.
        let position_key = (
            hof_store_id,
            call_site.top_level_param,
            call_site.field_path.clone(),
        );
        let Some(&param) = param_by_position
            .get(&position_key)
            .or_else(|| param_lookup.get(&hof_store_id))
        else {
            continue;
        };
        entries.push((call_site, spec_store_id, param));
    }

    if entries.is_empty() {
        return;
    }

    if entries.len() == 1 {
        let (call_site, spec_store_id, param) = entries[0];
        collect_rewritten_callable_arg_local(
            package,
            expr_owner_lookup,
            call_site.call_expr_id,
            call_site.arg_expr_id,
            rewritten_callable_arg_locals,
        );
        rewrite_one(
            package,
            package_id,
            call_site,
            param,
            spec_store_id,
            expr_owner_lookup,
            assigner,
        );
    } else {
        for (call_site, _, _) in &entries {
            collect_rewritten_callable_arg_local(
                package,
                expr_owner_lookup,
                call_site.call_expr_id,
                call_site.arg_expr_id,
                rewritten_callable_arg_locals,
            );
        }
        branch_split_rewrite(
            package,
            package_id,
            call_expr_id,
            &entries,
            &[],
            expr_owner_lookup,
            assigner,
        );
    }
}

/// Rewrites every direct call site in this package's body, grouping them by
/// call expression.
///
/// A lone unconditional site is rewritten in place by [`rewrite_direct_call`];
/// a group with multiple sites (or a conditional lone site) is lowered to a
/// condition-indexed dispatch via [`branch_split_direct_call_rewrite`].
fn rewrite_direct_call_sites(
    package: &mut Package,
    package_id: PackageId,
    analysis: &AnalysisResult,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    assigner: &mut Assigner,
) {
    let mut grouped_direct: FxHashMap<ExprId, Vec<&DirectCallSite>> = FxHashMap::default();
    for direct_call_site in &analysis.direct_call_sites {
        // Rewrite only the direct call sites that live in this package's body.
        if direct_call_site.call_pkg_id != package_id {
            continue;
        }
        grouped_direct
            .entry(direct_call_site.call_expr_id)
            .or_default()
            .push(direct_call_site);
    }

    for entries in grouped_direct.values() {
        if entries.len() == 1 && entries[0].condition.is_empty() {
            rewrite_direct_call(
                package,
                package_id,
                entries[0],
                expr_owner_lookup,
                rewritten_callable_arg_locals,
                assigner,
            );
        } else {
            let call_expr_id = entries[0].call_expr_id;
            let call_expr = package.get_expr(call_expr_id).clone();
            let ExprKind::Call(callee_id, _) = call_expr.kind else {
                continue;
            };

            collect_rewritten_callable_arg_local(
                package,
                expr_owner_lookup,
                call_expr_id,
                callee_id,
                rewritten_callable_arg_locals,
            );
            branch_split_direct_call_rewrite(
                package,
                package_id,
                call_expr_id,
                entries,
                expr_owner_lookup,
                assigner,
            );
        }
    }
}

/// Rewrites a `DirectCallSite` whose callee was resolved to a specific
/// concrete callable into a direct invocation of that callable, pruning
/// the now-unused callee expression.
///
/// When the call site carries a `def_span` (recorded for a collapsed
/// identity closure), the call expression's span is re-stamped to it so
/// diagnostics point at the original lambda body rather than the wrapper.
fn rewrite_direct_call(
    package: &mut Package,
    package_id: PackageId,
    direct_call_site: &DirectCallSite,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    assigner: &mut Assigner,
) {
    let call_expr = package.get_expr(direct_call_site.call_expr_id).clone();
    let ExprKind::Call(callee_id, args_id) = call_expr.kind else {
        return;
    };
    if let Some(span) = direct_call_site.def_span {
        package
            .exprs
            .get_mut(direct_call_site.call_expr_id)
            .expect("expression should exist")
            .span = span;
    }
    let (_, outer_functor) = peel_body_functors(package, callee_id);
    let controlled_layers = usize::from(outer_functor.controlled);
    let package_direct_lambda = match &direct_call_site.callable {
        ConcreteCallable::Global { item_id, .. } if item_id.package == package_id => {
            direct_lambda_packaged_input(package, item_id.item).is_some_and(|target_input| {
                apply_target_input_at_control_path(
                    &package.get_expr(args_id).ty,
                    &target_input,
                    controlled_layers,
                ) != package.get_expr(args_id).ty
            })
        }
        _ => false,
    };

    collect_rewritten_callable_arg_local(
        package,
        expr_owner_lookup,
        direct_call_site.call_expr_id,
        callee_id,
        rewritten_callable_arg_locals,
    );

    let captures = match &direct_call_site.callable {
        ConcreteCallable::Closure { captures, .. } => {
            resolve_rewrite_captures(package, callee_id, captures)
        }
        _ => Vec::new(),
    };

    rewrite_direct_callee(
        package,
        package_id,
        callee_id,
        &direct_call_site.callable,
        &captures,
        controlled_layers,
        assigner,
    );
    if matches!(direct_call_site.callable, ConcreteCallable::Closure { .. })
        || package_direct_lambda
    {
        rewrite_direct_closure_args(package, args_id, &captures, controlled_layers, assigner);
    }
}

/// Rewrites a direct call whose callee has multiple possible concrete
/// values by synthesizing a condition-indexed dispatch that selects the
/// specialized callee matching the observed branch.
fn branch_split_direct_call_rewrite(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    entries: &[&DirectCallSite],
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    let orig_call = package.get_expr(call_expr_id).clone();
    let ExprKind::Call(orig_callee_id, orig_args_id) = orig_call.kind else {
        return;
    };
    let span = orig_call.span;
    let result_ty = orig_call.ty.clone();

    let mut conditioned: Vec<(&DirectCallSite, Vec<ExprId>)> = Vec::new();
    let mut default = None;
    for &entry in entries {
        if entry.condition.is_empty() {
            if default.is_none() {
                default = Some(entry);
            }
        } else {
            conditioned.push((entry, entry.condition.clone()));
        }
    }

    if conditioned.is_empty()
        && entries.len() > 1
        && let Some((synthetic_conditioned, default_idx)) = synthesize_direct_index_dispatch(
            package,
            expr_owner_lookup,
            call_expr_id,
            entries,
            span,
            assigner,
        )
    {
        conditioned = synthetic_conditioned
            .into_iter()
            .map(|(entry_idx, condition)| (entries[entry_idx], vec![condition]))
            .collect();
        default = Some(entries[default_idx]);
    }

    let default_entry = if let Some(entry) = default {
        entry
    } else {
        if conditioned.is_empty() {
            return;
        }
        conditioned.pop().expect("non-empty conditioned").0
    };

    if conditioned.is_empty() {
        let mut rewritten_callable_arg_locals = FxHashSet::default();
        rewrite_direct_call(
            package,
            package_id,
            default_entry,
            expr_owner_lookup,
            &mut rewritten_callable_arg_locals,
            assigner,
        );
        return;
    }

    let orig_callee = package.get_expr(orig_callee_id).clone();
    let orig_args = package.get_expr(orig_args_id).clone();

    let mut build_call = |package: &mut Package, assigner: &mut Assigner, entry| {
        create_direct_branch_call(
            package,
            package_id,
            &orig_callee,
            &orig_args,
            span,
            &result_ty,
            entry,
            assigner,
        )
    };
    let dispatch_id = build_branch_tree(
        package,
        span,
        &result_ty,
        conditioned,
        default_entry,
        assigner,
        &mut build_call,
    );

    let dispatch = package
        .exprs
        .get(dispatch_id)
        .expect("dispatch expr should exist")
        .clone();
    let orig = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr should exist");
    orig.kind = dispatch.kind;
    orig.ty = dispatch.ty;
}

/// Records a local variable whose call-site rewrite now references a
/// specialized callable, marking it eligible for the dead-local cleanup
/// subsystem.
fn collect_rewritten_callable_arg_local(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    call_expr_id: ExprId,
    expr_id: ExprId,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
) {
    let expr = package.get_expr(expr_id);
    if let ExprKind::Var(Res::Local(var), _) = expr.kind
        && let Some(&callable_id) = expr_owner_lookup.get(&call_expr_id)
    {
        rewritten_callable_arg_locals.insert((callable_id, var));
    }
}

/// Records the source-array local for a closure callable-array that a
/// higher-order call forwards and fully consumes.
///
/// The forwarded argument is not always a bare `Var`: a call may pass the array
/// through a struct-literal field (`f(new Config { Ops = ops })`) or through a
/// chain of `let` aliases (`let a = ops; f(a)`). In every case the underlying
/// value is the same `Var(Res::Local)` source-array local. Recording it lets
/// the closure-bearing cleanup remove the now-dead binding after the call is
/// rewritten, instead of leaving an array of blanked (unit) closure elements —
/// an arrow-typed block with a unit tail — in a reachable caller.
fn collect_hof_consumed_source_array(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    call_expr_id: ExprId,
    arg_expr_id: ExprId,
    hof_consumed_source_arrays: &mut FxHashSet<(LocalItemId, LocalVarId)>,
) {
    let Some(&callable_id) = expr_owner_lookup.get(&call_expr_id) else {
        return;
    };
    trace_forwarded_callable_array_source_locals(package, callable_id, arg_expr_id, &mut |src| {
        hof_consumed_source_arrays.insert((callable_id, src));
    });
}

/// Traces a forwarded argument expression to the callable-array source locals
/// it ultimately references, invoking `record` for each.
///
/// Follows `let`-alias chains (`let a = ops`) to the originating array local and
/// descends into struct-literal fields (`new Config { Ops = ops }`) so a
/// callable array wrapped in a struct is still reached. Only locals whose type
/// is a callable array are recorded; the closure-bearing gate in
/// [`prune_dead_callable_arg_locals`] still decides whether removal is safe, so
/// plain callable-reference arrays are left in place.
fn trace_forwarded_callable_array_source_locals(
    package: &Package,
    owner: LocalItemId,
    expr_id: ExprId,
    record: &mut impl FnMut(LocalVarId),
) {
    match &package.get_expr(expr_id).kind {
        ExprKind::Var(Res::Local(var), _) => {
            let var = *var;
            if !ty_is_callable_array(package, &package.get_expr(expr_id).ty) {
                return;
            }
            // Follow a `let` alias to the original source array; a genuine
            // array-literal (or other non-alias) binding is itself the source.
            if let Some(init) = find_local_init_expr_in_callable(package, owner, var)
                && matches!(package.get_expr(init).kind, ExprKind::Var(Res::Local(_), _))
            {
                trace_forwarded_callable_array_source_locals(package, owner, init, record);
            } else {
                record(var);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                trace_forwarded_callable_array_source_locals(package, owner, *copy, record);
            }
            for field in fields {
                trace_forwarded_callable_array_source_locals(package, owner, field.value, record);
            }
        }
        _ => {}
    }
}

/// Returns `true` when `ty` is an array whose element type contains an arrow.
fn ty_is_callable_array(package: &Package, ty: &Ty) -> bool {
    matches!(resolve_udt_ty(package, ty), Ty::Array(elem) if ty_contains_arrow(&elem))
}

/// Synthesizes an index-dispatch `if`/`else` chain for a HOF call site that
/// resolves to multiple callables via branch-split analysis.
fn synthesize_callsite_index_dispatch(
    package: &mut Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    call_expr_id: ExprId,
    entries: &[HofDispatchTarget],
    span: Span,
    assigner: &mut Assigner,
) -> Option<(Vec<(usize, ExprId)>, usize)> {
    let callables = entries
        .iter()
        .map(|entry| entry.0.callable_arg.clone())
        .collect::<Vec<_>>();
    synthesize_index_dispatch_plan(
        package,
        expr_owner_lookup,
        call_expr_id,
        entries.first()?.0.arg_expr_id,
        &callables,
        span,
        assigner,
    )
}

/// Synthesizes an index-dispatch `if`/`else` chain for a direct-call site
/// whose callee expression resolves to multiple concrete callables.
fn synthesize_direct_index_dispatch(
    package: &mut Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    call_expr_id: ExprId,
    entries: &[&DirectCallSite],
    span: Span,
    assigner: &mut Assigner,
) -> Option<(Vec<(usize, ExprId)>, usize)> {
    let ExprKind::Call(callee_id, _) = package.get_expr(call_expr_id).kind else {
        return None;
    };
    let callables = entries
        .iter()
        .map(|entry| entry.callable.clone())
        .collect::<Vec<_>>();
    synthesize_index_dispatch_plan(
        package,
        expr_owner_lookup,
        call_expr_id,
        callee_id,
        &callables,
        span,
        assigner,
    )
}

/// Plans the branches of an index-dispatch rewrite by pairing each
/// candidate callable with the condition expression that selects it.
fn synthesize_index_dispatch_plan(
    package: &mut Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    dispatch_expr_id: ExprId,
    callables: &[ConcreteCallable],
    span: Span,
    assigner: &mut Assigner,
) -> Option<(Vec<(usize, ExprId)>, usize)> {
    if callables.len() < 2 {
        return None;
    }

    let (index_expr_id, indexed_callables) =
        resolve_index_dispatch_source(package, expr_owner_lookup, owner_expr_id, dispatch_expr_id)?;

    let mut entry_positions = Vec::with_capacity(callables.len());
    for callable in callables {
        let position = indexed_callables
            .iter()
            .position(|candidate| candidate == callable)?;
        entry_positions.push(position);
    }

    let (default_idx, _) = entry_positions
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, position)| *position)?;

    // Non-trivial index expressions (not a plain local read or literal) could
    // repeat side effects if re-evaluated per branch, so hoist into a single
    // shared `let`; each `index == k` comparison then reads the temp. Trivial
    // indices keep referencing the original expression so snapshots stay
    // byte-identical.
    let hoist_index = !matches!(
        package.get_expr(index_expr_id).kind,
        ExprKind::Var(_, _) | ExprKind::Lit(_)
    );
    let hoisted = if hoist_index {
        let index_ty = package.get_expr(index_expr_id).ty.clone();
        let index_span = package.get_expr(index_expr_id).span;
        let block_lookup = build_expr_block_lookup(package);
        hoist_expr_into_let(package, assigner, &block_lookup, index_expr_id, "index")
            .map(|(local_var, _)| (local_var, index_ty, index_span))
    } else {
        None
    };

    let mut conditioned = Vec::with_capacity(callables.len().saturating_sub(1));
    for (entry_idx, position) in entry_positions.into_iter().enumerate() {
        if entry_idx == default_idx {
            continue;
        }
        let operand = match &hoisted {
            Some((local_var, ty, index_span)) => crate::fir_builder::alloc_local_var_expr(
                package,
                assigner,
                *local_var,
                ty.clone(),
                *index_span,
            ),
            None => index_expr_id,
        };
        let condition = alloc_index_eq_expr(package, operand, position, span, assigner);
        conditioned.push((entry_idx, condition));
    }

    Some((conditioned, default_idx))
}

/// Locates the source of a dynamic dispatch (for example the index
/// expression selecting an element in a callable array) that
/// `synthesize_*_index_dispatch` will compare against per-branch values.
fn resolve_index_dispatch_source(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    dispatch_expr_id: ExprId,
) -> Option<(ExprId, Vec<ConcreteCallable>)> {
    let direct_callables = resolve_array_expr_to_callables(
        package,
        expr_owner_lookup,
        owner_expr_id,
        dispatch_expr_id,
    );

    let source_expr_id =
        resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, dispatch_expr_id)?;
    let ExprKind::Index(array_expr_id, index_expr_id) = package.get_expr(source_expr_id).kind
    else {
        return None;
    };

    // Try direct resolution: array elements are callables.
    if let Some(indexed_callables) =
        resolve_array_expr_to_callables(package, expr_owner_lookup, owner_expr_id, array_expr_id)
        && indexed_callables.len() >= 2
    {
        return Some((index_expr_id, indexed_callables));
    }

    if let Some(indexed_callables) = direct_callables
        && indexed_callables.len() >= 2
    {
        return Some((index_expr_id, indexed_callables));
    }

    // Direct resolution failed: array elements may be tuples.
    // Check if the dispatch expression was a local variable bound from a
    // tuple pattern, and try extracting the appropriate field from each
    // array element before resolving.
    let field_path =
        resolve_dispatch_field_path(package, expr_owner_lookup, owner_expr_id, dispatch_expr_id)?;
    let indexed_callables = resolve_array_expr_to_callables_with_field(
        package,
        expr_owner_lookup,
        owner_expr_id,
        array_expr_id,
        &field_path,
    )?;
    if indexed_callables.len() < 2 {
        return None;
    }
    Some((index_expr_id, indexed_callables))
}

/// For a dispatch expression that is a local variable bound from a tuple
/// pattern, returns the field position path within the tuple.
fn resolve_dispatch_field_path(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    dispatch_expr_id: ExprId,
) -> Option<Vec<usize>> {
    let expr = package.get_expr(dispatch_expr_id);
    if let ExprKind::Var(Res::Local(local_var), _) = expr.kind {
        let owner_callable = *expr_owner_lookup.get(&owner_expr_id)?;
        find_var_tuple_field_path_in_callable(package, owner_callable, local_var)
    } else {
        None
    }
}

/// Resolves the expression feeding an index dispatch back to its defining
/// source (literal, local, or field access) so per-branch conditions can
/// compare directly against it.
fn resolve_dispatch_source_expr(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    expr_id: ExprId,
) -> Option<ExprId> {
    let expr = package.get_expr(expr_id);
    match expr.kind {
        ExprKind::Var(Res::Local(local_var), _) => {
            let owner_callable = *expr_owner_lookup.get(&owner_expr_id)?;
            let init_expr_id =
                find_local_init_expr_in_callable(package, owner_callable, local_var)?;
            if init_expr_id == expr_id {
                None
            } else {
                resolve_dispatch_source_expr(
                    package,
                    expr_owner_lookup,
                    owner_expr_id,
                    init_expr_id,
                )
            }
        }
        ExprKind::Block(block_id) => {
            let block = package.get_block(block_id);
            let stmt_id = *block.stmts.last()?;
            let stmt = package.get_stmt(stmt_id);
            #[allow(clippy::manual_let_else)]
            let tail_expr_id = match stmt.kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => expr_id,
                _ => return None,
            };
            resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, tail_expr_id)
        }
        ExprKind::Return(inner_expr_id) => {
            resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, inner_expr_id)
        }
        _ => Some(expr_id),
    }
}

/// Resolves an array-literal expression to the ordered list of concrete
/// callables it contains, used by index-dispatch synthesis.
fn resolve_array_expr_to_callables(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    expr_id: ExprId,
) -> Option<Vec<ConcreteCallable>> {
    let source_expr_id =
        resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, expr_id)?;
    let expr = package.get_expr(source_expr_id);
    let elements = match &expr.kind {
        ExprKind::Array(elements) | ExprKind::ArrayLit(elements) | ExprKind::Tuple(elements) => {
            elements.clone()
        }
        _ => return None,
    };

    let mut callables = Vec::with_capacity(elements.len());
    for elem_expr_id in elements {
        let callable = resolve_expr_to_concrete_callable(
            package,
            expr_owner_lookup,
            owner_expr_id,
            elem_expr_id,
        )?;
        if !callables.contains(&callable) {
            callables.push(callable);
        }
    }

    Some(callables)
}

/// Extracts a nested tuple field from an expression by following a field path.
/// For `field_path = [1]`, returns the second element of a tuple expression.
fn extract_tuple_field(package: &Package, expr_id: ExprId, path: &[usize]) -> Option<ExprId> {
    let mut current = expr_id;
    for &idx in path {
        let expr = package.get_expr(current);
        if let ExprKind::Tuple(fields) = &expr.kind {
            current = *fields.get(idx)?;
        } else {
            return None;
        }
    }
    Some(current)
}

/// Like `resolve_array_expr_to_callables`, but first extracts the tuple field
/// at `field_path` from each array element before resolving to a callable.
fn resolve_array_expr_to_callables_with_field(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    array_expr_id: ExprId,
    field_path: &[usize],
) -> Option<Vec<ConcreteCallable>> {
    let source_expr_id =
        resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, array_expr_id)?;
    let expr = package.get_expr(source_expr_id);
    let elements = match &expr.kind {
        ExprKind::Array(elements) | ExprKind::ArrayLit(elements) | ExprKind::Tuple(elements) => {
            elements.clone()
        }
        _ => return None,
    };

    let mut callables = Vec::with_capacity(elements.len());
    for elem_expr_id in elements {
        let field_expr_id = extract_tuple_field(package, elem_expr_id, field_path)?;
        let callable = resolve_expr_to_concrete_callable(
            package,
            expr_owner_lookup,
            owner_expr_id,
            field_expr_id,
        )?;
        if !callables.contains(&callable) {
            callables.push(callable);
        }
    }

    Some(callables)
}

/// Attempts to resolve an expression to a single concrete callable (global
/// or closure), mirroring the analysis-phase resolution but on the
/// rewritten package.
fn resolve_expr_to_concrete_callable(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    expr_id: ExprId,
) -> Option<ConcreteCallable> {
    let source_expr_id =
        resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, expr_id)?;
    let (base_id, functor) = peel_body_functors(package, source_expr_id);
    let expr = package.get_expr(base_id);
    match expr.kind {
        ExprKind::Var(Res::Item(item_id), _) => Some(ConcreteCallable::Global { item_id, functor }),
        ExprKind::Closure(ref captured_vars, target) => Some(ConcreteCallable::Closure {
            target,
            captures: resolve_concrete_closure_captures(
                package,
                expr_owner_lookup,
                owner_expr_id,
                captured_vars,
            )?,
            functor,
        }),
        _ => None,
    }
}

/// Resolves each captured variable of a concrete closure to a [`CapturedVar`],
/// recovering its type and initializer expression from the owning callable.
///
/// Returns `None` when the owner is unknown or a capture's type cannot be
/// found.
fn resolve_concrete_closure_captures(
    package: &Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    owner_expr_id: ExprId,
    captured_vars: &[LocalVarId],
) -> Option<Vec<CapturedVar>> {
    let owner_callable = *expr_owner_lookup.get(&owner_expr_id)?;
    captured_vars
        .iter()
        .map(|&var| {
            let expr = find_local_init_expr_in_callable(package, owner_callable, var);
            let ty = expr
                .map(|expr_id| package.get_expr(expr_id).ty.clone())
                .or_else(|| find_var_type_in_callable(package, owner_callable, var))?;
            Some(CapturedVar {
                var,
                ty,
                expr,
                caller_substitutions: Vec::new(),
            })
        })
        .collect()
}

/// Drives a [`Visitor`] over a callable implementation's body blocks — the
/// `body` block plus any `adj`/`ctl`/`ctl_adj` specialization blocks.
///
/// Specialization input patterns are intentionally skipped: the `find_*`
/// searches only look at the callable's own input pattern and the statements
/// inside each specialization body.
fn walk_callable_impl_bodies<'a>(vis: &mut impl Visitor<'a>, callable_impl: &CallableImpl) {
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::SimulatableIntrinsic(spec_decl) => vis.visit_block(spec_decl.block),
        CallableImpl::Spec(spec_impl) => {
            vis.visit_block(spec_impl.body.block);
            for spec in [
                spec_impl.adj.as_ref(),
                spec_impl.ctl.as_ref(),
                spec_impl.ctl_adj.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                vis.visit_block(spec.block);
            }
        }
    }
}

/// FIR [`Visitor`] that records the declared type of a target local the first
/// time it reaches the `Bind` pattern that introduces it. Recursion stops once
/// the type is found.
struct VarTypeFinder<'a> {
    package: &'a Package,
    local_var: LocalVarId,
    result: Option<Ty>,
}

impl<'a> Visitor<'a> for VarTypeFinder<'a> {
    fn visit_pat(&mut self, pat: PatId) {
        if self.result.is_some() {
            return;
        }
        let p = self.package.get_pat(pat);
        match &p.kind {
            PatKind::Bind(ident) if ident.id == self.local_var => {
                self.result = Some(p.ty.clone());
            }
            _ => visit::walk_pat(self, pat),
        }
    }

    fn visit_expr(&mut self, expr: ExprId) {
        if self.result.is_some() {
            return;
        }
        visit::walk_expr(self, expr);
    }

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
}

/// Searches a callable's body and input pattern for the declared type of
/// `local_var`, returning `None` when it is not found.
fn find_var_type_in_callable(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> Option<Ty> {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return None;
    };
    let mut finder = VarTypeFinder {
        package,
        local_var,
        result: None,
    };
    finder.visit_pat(decl.input);
    if finder.result.is_none() {
        walk_callable_impl_bodies(&mut finder, &decl.implementation);
    }
    finder.result
}

/// Allocates a `BinOp(Eq, index_expr, Int(index_value))` expression used as
/// the condition guard for index-dispatch branches. Inserts two new `Expr`
/// nodes (literal and comparison) through `assigner`.
fn alloc_index_eq_expr(
    package: &mut Package,
    index_expr_id: ExprId,
    index_value: usize,
    span: Span,
    assigner: &mut Assigner,
) -> ExprId {
    let lit_id = assigner.next_expr();
    let index_value = i64::try_from(index_value).expect("dispatch index should fit in i64");
    package.exprs.insert(
        lit_id,
        Expr {
            id: lit_id,
            span,
            ty: Ty::Prim(Prim::Int),
            kind: ExprKind::Lit(Lit::Int(index_value)),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let cond_id = assigner.next_expr();
    package.exprs.insert(
        cond_id,
        Expr {
            id: cond_id,
            span,
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::BinOp(BinOp::Eq, index_expr_id, lit_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    cond_id
}

/// FIR [`Visitor`] that records the initializer expression of the `Local`
/// binding for a target local the first time it is reached. Recursion stops
/// once the initializer is found.
struct LocalInitFinder<'a> {
    package: &'a Package,
    local_var: LocalVarId,
    result: Option<ExprId>,
}

impl<'a> Visitor<'a> for LocalInitFinder<'a> {
    fn visit_stmt(&mut self, stmt: StmtId) {
        if self.result.is_some() {
            return;
        }
        if let StmtKind::Local(_, pat_id, init_expr_id) = self.package.get_stmt(stmt).kind
            && pat_binds_local_var(self.package, pat_id, self.local_var)
        {
            self.result = Some(init_expr_id);
            return;
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: ExprId) {
        if self.result.is_some() {
            return;
        }
        visit::walk_expr(self, expr);
    }

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
}

/// Locates the initializer expression for a given local variable inside a
/// reachable callable body.
fn find_local_init_expr_in_callable(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> Option<ExprId> {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return None;
    };
    let mut finder = LocalInitFinder {
        package,
        local_var,
        result: None,
    };
    walk_callable_impl_bodies(&mut finder, &decl.implementation);
    finder.result
}

/// Removes callable-typed argument locals whose only remaining uses were
/// rewritten into direct dispatch calls, leaving no arrow-typed residue.
///
/// Removes `Local` binding statements and `Var` references for dead locals
/// via [`remove_dead_callable_local_from_callable`] and
/// [`prune_dead_top_level_callable_locals`].
fn prune_dead_callable_arg_locals(
    package: &mut Package,
    rewritten_callable_arg_locals: &FxHashSet<(LocalItemId, LocalVarId)>,
    hof_consumed_source_arrays: &FxHashSet<(LocalItemId, LocalVarId)>,
) {
    let mut source_arrays: FxHashSet<(LocalItemId, LocalVarId)> = FxHashSet::default();

    // Closure callable-arrays forwarded and fully consumed by a higher-order
    // call are dead source arrays that the direct-path index-read tracer never
    // sees. Seed them here so the same closure-bearing removal below prunes the
    // now-dead binding rather than leaving blanked closure elements behind.
    source_arrays.extend(hof_consumed_source_arrays.iter().copied());

    for &(callable_id, local_var) in rewritten_callable_arg_locals {
        if !local_var_is_used_in_callable(package, callable_id, local_var) {
            // A direct-dispatch callee bound from an indexed read
            // (`let op = ops[i]`) leaves its source array potentially dead once
            // this local is gone. Record it so the source array can be pruned
            // afterward if nothing else reads it.
            if let Some(src) = local_index_source_array_local(package, callable_id, local_var) {
                source_arrays.insert((callable_id, src));
            }
            remove_dead_callable_local_from_callable(package, callable_id, local_var);
        } else if !local_var_is_read_in_callable(package, callable_id, local_var)
            && local_var_has_closure_valued_binding(package, callable_id, local_var)
        {
            // The local is still mentioned, but only as an assignment target:
            // every read was consumed when the call site was rewritten into a
            // direct dispatch. When such a write-only local is initialized from
            // a partial application (`mutable op = Rx(0.0, _)`), its binding
            // holds a closure-tailed block. Left in place, closure cleanup
            // blanks that tail and strands an arrow-typed block with no
            // producing value. Removing the dead binding and its assignments
            // avoids that. Locals bound only to plain callable references
            // (`op = H`) carry no closure tail, so they are left untouched to
            // preserve existing dead-code behavior.
            remove_write_only_callable_local_from_callable(package, callable_id, local_var);
        }
    }

    // Prune callable-array locals that only fed removed direct-dispatch index
    // reads. Restricting removal to a closure-bearing array (`[X, Rx(0.0, _)]`)
    // avoids stranding a blanked closure element as an arrow-typed block with
    // no producing tail, while plain callable-reference arrays are left in
    // place.
    for (callable_id, src_var) in source_arrays {
        if !local_var_is_read_in_callable(package, callable_id, src_var)
            && local_var_has_closure_valued_binding(package, callable_id, src_var)
        {
            remove_write_only_callable_local_from_callable(package, callable_id, src_var);
        }
    }

    prune_dead_top_level_callable_locals(package);
}

/// Builds a map from each expression id to the local callable that owns it, so
/// a rewrite can find the scope an expression belongs to.
fn build_expr_owner_lookup(package: &Package) -> FxHashMap<ExprId, LocalItemId> {
    let mut expr_owner_lookup = FxHashMap::default();

    for (item_id, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, _expr| {
                    expr_owner_lookup.insert(expr_id, item_id);
                },
            );
        }
    }

    expr_owner_lookup
}

/// Builds a map from every `ExprId` to its innermost enclosing block and the
/// index of the top-level statement (within that block) containing it.
///
/// The block-context companion to [`build_expr_owner_lookup`], and a
/// prerequisite for [`hoist_expr_into_let`], which splices a `let` binding
/// immediately before the statement that consumes a hoisted expression. Closure
/// bodies are not traversed (consistent with [`crate::walk_utils::for_each_expr`]).
pub(crate) fn build_expr_block_lookup(package: &Package) -> FxHashMap<ExprId, (BlockId, usize)> {
    let mut lookup = FxHashMap::default();
    for (_item_id, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind {
            match &decl.implementation {
                CallableImpl::Intrinsic => {}
                CallableImpl::Spec(spec_impl) => {
                    record_block_context(package, spec_impl.body.block, &mut lookup);
                    for spec in crate::fir_builder::functored_specs(spec_impl) {
                        record_block_context(package, spec.block, &mut lookup);
                    }
                }
                CallableImpl::SimulatableIntrinsic(spec_decl) => {
                    record_block_context(package, spec_decl.block, &mut lookup);
                }
            }
        }
    }
    lookup
}

/// Records the `(block, stmt index)` context for every expression reachable
/// from the top-level statements of `block_id`.
fn record_block_context(
    package: &Package,
    block_id: BlockId,
    lookup: &mut FxHashMap<ExprId, (BlockId, usize)>,
) {
    let block = package.get_block(block_id);
    for (stmt_index, &stmt_id) in block.stmts.iter().enumerate() {
        let surface = match &package.get_stmt(stmt_id).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
            StmtKind::Item(_) => continue,
        };
        record_expr_context(package, surface, block_id, stmt_index, lookup);
    }
}

/// Records `(block_id, stmt_index)` for `expr_id` and recurses into its
/// children. When a nested block is encountered, recursion re-enters via
/// [`record_block_context`] so descendant expressions are keyed to the
/// innermost enclosing block.
fn record_expr_context(
    package: &Package,
    expr_id: ExprId,
    block_id: BlockId,
    stmt_index: usize,
    lookup: &mut FxHashMap<ExprId, (BlockId, usize)>,
) {
    lookup.insert(expr_id, (block_id, stmt_index));
    for_each_direct_child(&package.get_expr(expr_id).kind, |child| match child {
        // Sibling-scope child expressions share this statement's context.
        DirectChild::Expr(e) => record_expr_context(package, e, block_id, stmt_index, lookup),
        // A nested block re-keys its descendants to its own statement indices.
        DirectChild::Block(inner) => record_block_context(package, inner, lookup),
    });
}

/// Hoists `hoist_expr` into a fresh immutable `let` binding spliced immediately
/// before the statement that consumes it, returning the `(local var, var-read
/// expr)` pair for the temp.
///
/// The caller must rewrite the original occurrence(s) to read the returned
/// `var_expr`. `hoist_expr` is moved into the initializer (not cloned), so its
/// side effect runs exactly once at the binding site. Synthesized statements
/// carry [`EMPTY_EXEC_RANGE`]; `exec_graph_rebuild` repairs ranges later.
///
/// Returns `None` when `hoist_expr` has no recorded block context (not a
/// block-resident expression), leaving the package unchanged.
pub(crate) fn hoist_expr_into_let(
    package: &mut Package,
    assigner: &mut Assigner,
    block_lookup: &FxHashMap<ExprId, (BlockId, usize)>,
    hoist_expr: ExprId,
    temp_name: &str,
) -> Option<(LocalVarId, ExprId)> {
    let &(block_id, stmt_index) = block_lookup.get(&hoist_expr)?;
    let hoist_ty = package.get_expr(hoist_expr).ty.clone();
    let hoist_span = package.get_expr(hoist_expr).span;

    let (local_var, let_stmt) = crate::fir_builder::alloc_local_var(
        package,
        assigner,
        temp_name,
        &hoist_ty,
        hoist_expr,
        Mutability::Immutable,
    );
    let var_expr = crate::fir_builder::alloc_local_var_expr(
        package, assigner, local_var, hoist_ty, hoist_span,
    );

    let block = package
        .blocks
        .get_mut(block_id)
        .expect("block should exist");
    block.stmts.insert(stmt_index, let_stmt);

    Some((local_var, var_expr))
}

/// Reports whether `local_var` is referenced anywhere in the callable's body.
fn local_var_is_used_in_callable(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> bool {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return false;
    };

    let mut used = false;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_expr_id, expr| {
            if matches!(expr.kind, ExprKind::Var(Res::Local(var), _) if var == local_var) {
                used = true;
            }
        },
    );
    used
}

/// Returns `true` when `local_var` is read anywhere in the callable body.
///
/// A read is any `Var(Res::Local(local_var))` reference other than the direct
/// left-hand side of an assignment (`local_var = ...`). This distinguishes a
/// still-referenced-but-write-only local, whose only remaining mentions are
/// assignment targets, from one that is genuinely observed.
fn local_var_is_read_in_callable(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> bool {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return false;
    };

    // Assignment left-hand sides are writes, not reads, so gather them first
    // and exclude those expression positions from the read scan below.
    let mut assign_lhs: FxHashSet<ExprId> = FxHashSet::default();
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_expr_id, expr| {
            if let ExprKind::Assign(lhs, _) = expr.kind {
                assign_lhs.insert(lhs);
            }
        },
    );

    let mut read = false;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |expr_id, expr| {
            if matches!(expr.kind, ExprKind::Var(Res::Local(var), _) if var == local_var)
                && !assign_lhs.contains(&expr_id)
            {
                read = true;
            }
        },
    );
    read
}

/// Returns `true` when `local_var` is bound or assigned from an expression
/// whose subtree contains a `Closure`.
///
/// Partial applications lower to a block whose tail is a `Closure`, so this
/// distinguishes a callable local initialized from a partial application from
/// one bound only to plain callable references (`op = H`), which carry no
/// closure and need no dead-binding removal.
fn local_var_has_closure_valued_binding(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> bool {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return false;
    };

    // The initializer of the `Local` binding.
    if let Some(init_expr_id) = find_local_init_expr_in_callable(package, callable_id, local_var)
        && expr_subtree_contains_closure(package, init_expr_id)
    {
        return true;
    }

    // Any `local_var = <rhs>` assignment whose right-hand side holds a closure.
    let mut found = false;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_expr_id, expr| {
            if let ExprKind::Assign(lhs, rhs) = expr.kind
                && matches!(package.get_expr(lhs).kind, ExprKind::Var(Res::Local(var), _) if var == local_var)
                && expr_subtree_contains_closure(package, rhs)
            {
                found = true;
            }
        },
    );
    found
}

/// Returns `true` when the expression subtree rooted at `expr_id` contains a
/// `Closure` expression.
fn expr_subtree_contains_closure(package: &Package, expr_id: ExprId) -> bool {
    let mut found = false;
    crate::walk_utils::for_each_expr(package, expr_id, &mut |_expr_id, expr| {
        if matches!(expr.kind, ExprKind::Closure(_, _)) {
            found = true;
        }
    });
    found
}

/// Returns the local array variable that a callable local is bound from when
/// its initializer is an indexed read (`let op = ops[i]`).
///
/// Returns `None` when the local has no initializer, or its initializer is not
/// an index into a bare local variable.
fn local_index_source_array_local(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> Option<LocalVarId> {
    let init_expr_id = find_local_init_expr_in_callable(package, callable_id, local_var)?;
    let ExprKind::Index(base, _) = package.get_expr(init_expr_id).kind else {
        return None;
    };
    if let ExprKind::Var(Res::Local(src), _) = package.get_expr(base).kind {
        Some(src)
    } else {
        None
    }
}

/// Removes a write-only callable local from the given callable's body by
/// deleting its binding and every assignment to it, recursing into nested
/// blocks via [`remove_write_only_callable_local_from_block`].
///
/// Unlike [`remove_dead_callable_local_from_callable`], this handles `mutable`
/// bindings and assignment statements, which arise when a callable local is
/// initialized (and possibly reassigned) from partial applications but is only
/// ever consumed through a call site that direct-dispatch rewriting has already
/// replaced.
fn remove_write_only_callable_local_from_callable(
    package: &mut Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return;
    };

    let implementation = decl.implementation.clone();
    match implementation {
        qsc_fir::fir::CallableImpl::Intrinsic => {}
        qsc_fir::fir::CallableImpl::SimulatableIntrinsic(spec_decl) => {
            remove_write_only_callable_local_from_block(package, spec_decl.block, local_var);
        }
        qsc_fir::fir::CallableImpl::Spec(spec_impl) => {
            remove_write_only_callable_local_from_block(package, spec_impl.body.block, local_var);
            for spec in [spec_impl.adj, spec_impl.ctl, spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
                remove_write_only_callable_local_from_block(package, spec.block, local_var);
            }
        }
    }
}

/// Removes the binding and every assignment of a write-only callable local
/// within a block, recursing into nested blocks.
///
/// Drops `Local` bindings (of any mutability) whose pattern is a simple bind
/// of `local_var`, and drops `local_var = ...` assignment statements. Other
/// statements are retained and their nested blocks are walked so assignments in
/// conditional branches or loop bodies are removed as well.
fn remove_write_only_callable_local_from_block(
    package: &mut Package,
    block_id: qsc_fir::fir::BlockId,
    local_var: LocalVarId,
) {
    let stmt_ids = package.get_block(block_id).stmts.clone();
    let mut retained = Vec::with_capacity(stmt_ids.len());

    for stmt_id in stmt_ids {
        let stmt = package.get_stmt(stmt_id);
        let remove_stmt = match &stmt.kind {
            StmtKind::Local(_, pat_id, _) => {
                matches!(&package.get_pat(*pat_id).kind, PatKind::Bind(ident) if ident.id == local_var)
            }
            StmtKind::Semi(expr_id) | StmtKind::Expr(expr_id) => {
                expr_is_assign_to_local(package, *expr_id, local_var)
            }
            StmtKind::Item(_) => false,
        };
        if !remove_stmt {
            retained.push(stmt_id);
        }
    }

    let retained_for_walk = retained.clone();
    package
        .blocks
        .get_mut(block_id)
        .expect("block should exist")
        .stmts = retained;

    for stmt_id in retained_for_walk {
        if let StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) =
            package.get_stmt(stmt_id).kind
        {
            remove_write_only_callable_local_from_expr(package, expr_id, local_var);
        }
    }
}

/// Recurses through an expression subtree removing write-only local assignments
/// found inside nested `Block` and `While` bodies.
fn remove_write_only_callable_local_from_expr(
    package: &mut Package,
    expr_id: ExprId,
    local_var: LocalVarId,
) {
    let expr_kind = package.get_expr(expr_id).kind.clone();
    match expr_kind {
        ExprKind::Block(block_id) => {
            remove_write_only_callable_local_from_block(package, block_id, local_var);
        }
        ExprKind::While(cond, block_id) => {
            remove_write_only_callable_local_from_expr(package, cond, local_var);
            remove_write_only_callable_local_from_block(package, block_id, local_var);
        }
        ExprKind::If(cond, body, otherwise) => {
            remove_write_only_callable_local_from_expr(package, cond, local_var);
            remove_write_only_callable_local_from_expr(package, body, local_var);
            if let Some(otherwise) = otherwise {
                remove_write_only_callable_local_from_expr(package, otherwise, local_var);
            }
        }
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for child in exprs {
                remove_write_only_callable_local_from_expr(package, child, local_var);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b)
        | ExprKind::Assign(a, b) => {
            remove_write_only_callable_local_from_expr(package, a, local_var);
            remove_write_only_callable_local_from_expr(package, b, local_var);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            remove_write_only_callable_local_from_expr(package, a, local_var);
            remove_write_only_callable_local_from_expr(package, b, local_var);
            remove_write_only_callable_local_from_expr(package, c, local_var);
        }
        ExprKind::Fail(inner)
        | ExprKind::Field(inner, _)
        | ExprKind::Return(inner)
        | ExprKind::UnOp(_, inner) => {
            remove_write_only_callable_local_from_expr(package, inner, local_var);
        }
        ExprKind::Range(start, step, end) => {
            for child in [start, step, end].into_iter().flatten() {
                remove_write_only_callable_local_from_expr(package, child, local_var);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(child) = component {
                    remove_write_only_callable_local_from_expr(package, child, local_var);
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                remove_write_only_callable_local_from_expr(package, copy, local_var);
            }
            for field in fields {
                remove_write_only_callable_local_from_expr(package, field.value, local_var);
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Returns `true` when `expr_id` is an assignment whose left-hand side is a
/// bare read of `local_var`.
fn expr_is_assign_to_local(package: &Package, expr_id: ExprId, local_var: LocalVarId) -> bool {
    if let ExprKind::Assign(lhs, _) = package.get_expr(expr_id).kind {
        matches!(package.get_expr(lhs).kind, ExprKind::Var(Res::Local(var), _) if var == local_var)
    } else {
        false
    }
}

/// Removes a specific dead callable local from the given callable's body by
/// deleting its `Local` binding and any references that remain, recursing
/// into nested blocks via [`remove_dead_callable_local_from_block`].
fn remove_dead_callable_local_from_callable(
    package: &mut Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return;
    };

    let implementation = decl.implementation.clone();
    match implementation {
        qsc_fir::fir::CallableImpl::Intrinsic => {}
        qsc_fir::fir::CallableImpl::SimulatableIntrinsic(spec_decl) => {
            remove_dead_callable_local_from_block(package, spec_decl.block, local_var);
        }
        qsc_fir::fir::CallableImpl::Spec(spec_impl) => {
            remove_dead_callable_local_from_block(package, spec_impl.body.block, local_var);
            for spec in [spec_impl.adj, spec_impl.ctl, spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
                remove_dead_callable_local_from_block(package, spec.block, local_var);
            }
        }
    }
}

/// Removes top-level callable-typed locals whose only uses were direct
/// dispatch rewrites, scoped to the package-level entry expression. Filters
/// `Block.stmts` across all callable bodies in the package.
fn prune_dead_top_level_callable_locals(package: &mut Package) {
    let callable_items: Vec<(LocalItemId, qsc_fir::fir::CallableImpl)> = package
        .items
        .iter()
        .filter_map(|(item_id, item)| match &item.kind {
            ItemKind::Callable(decl) => Some((item_id, decl.implementation.clone())),
            ItemKind::Ty(..) => None,
        })
        .collect();

    for (_item_id, implementation) in callable_items {
        match implementation {
            qsc_fir::fir::CallableImpl::Intrinsic => {}
            qsc_fir::fir::CallableImpl::SimulatableIntrinsic(spec_decl) => {
                prune_dead_callable_locals_in_block(package, spec_decl.block);
            }
            qsc_fir::fir::CallableImpl::Spec(spec_impl) => {
                prune_dead_callable_locals_in_block(package, spec_impl.body.block);
                for spec in [spec_impl.adj, spec_impl.ctl, spec_impl.ctl_adj]
                    .into_iter()
                    .flatten()
                {
                    prune_dead_callable_locals_in_block(package, spec.block);
                }
            }
        }
    }
}

/// Walks a block looking for dead callable-typed locals introduced by
/// direct-call rewrites and removes them in place.
///
/// Iterates until no more removals occur so that cascading dead-local chains
/// (e.g. `let a = closure; let b = a;`) are fully pruned in a single call
/// rather than requiring multiple outer fixpoint iterations. Rewrites
/// `Block.stmts` to drop unused `Local` bindings, then recurses into nested
/// blocks.
fn prune_dead_callable_locals_in_block(package: &mut Package, block_id: qsc_fir::fir::BlockId) {
    loop {
        let stmt_ids = package.get_block(block_id).stmts.clone();
        let initial_count = stmt_ids.len();
        let mut retained = Vec::with_capacity(initial_count);

        for stmt_id in stmt_ids {
            let stmt = package.get_stmt(stmt_id);
            let remove_stmt = match stmt.kind {
                StmtKind::Local(Mutability::Immutable, pat_id, _) => {
                    let pat = package.get_pat(pat_id);
                    if local_ty_contains_arrow_through_udts(package, &pat.ty) {
                        let mut bound_vars = Vec::new();
                        collect_bound_pat_vars(package, pat_id, &mut bound_vars);
                        !bound_vars.is_empty()
                            && bound_vars.iter().all(|var| {
                                classify_block_use(package, block_id, *var) == UseClass::Unused
                            })
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if !remove_stmt {
                retained.push(stmt_id);
            }
        }

        package
            .blocks
            .get_mut(block_id)
            .expect("block should exist")
            .stmts
            .clone_from(&retained);

        if retained.len() == initial_count {
            // No removals this pass — walk nested blocks and stop.
            for stmt_id in retained {
                prune_dead_callable_locals_in_stmt(package, stmt_id);
            }
            break;
        }
    }
}

/// Removes a dead callable local scoped to a specific block, including its
/// `Local` binding and any remaining references, recursing into nested
/// blocks via [`remove_dead_callable_local_from_stmt`].
fn remove_dead_callable_local_from_block(
    package: &mut Package,
    block_id: qsc_fir::fir::BlockId,
    local_var: LocalVarId,
) {
    let stmt_ids = package.get_block(block_id).stmts.clone();
    let mut retained = Vec::with_capacity(stmt_ids.len());

    for stmt_id in stmt_ids {
        let stmt = package.get_stmt(stmt_id);
        let remove_stmt = if let StmtKind::Local(Mutability::Immutable, pat_id, _) = stmt.kind
            && local_ty_contains_arrow_through_udts(package, &package.get_pat(pat_id).ty)
            && pat_binds_local_var(package, pat_id, local_var)
        {
            // Only remove when all bound variables in the pattern are
            // unused; a tuple pattern may bind siblings that are still live.
            let mut bound_vars = Vec::new();
            collect_bound_pat_vars(package, pat_id, &mut bound_vars);
            bound_vars
                .iter()
                .all(|&var| classify_block_use(package, block_id, var) == UseClass::Unused)
        } else {
            false
        };

        if !remove_stmt {
            retained.push(stmt_id);
        }
    }

    let retained_for_walk = retained.clone();
    package
        .blocks
        .get_mut(block_id)
        .expect("block should exist")
        .stmts = retained;

    for stmt_id in retained_for_walk {
        remove_dead_callable_local_from_stmt(package, stmt_id, local_var);
    }
}

/// Inspects a single statement for dead callable-local bindings and deletes
/// them when safe, delegating to [`prune_dead_callable_locals_in_expr`] for
/// the statement's inner expression.
fn prune_dead_callable_locals_in_stmt(package: &mut Package, stmt_id: qsc_fir::fir::StmtId) {
    let stmt = package.get_stmt(stmt_id).clone();
    match stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            prune_dead_callable_locals_in_expr(package, expr_id);
        }
        StmtKind::Item(_) => {}
    }
}

/// Descends into an expression subtree looking for dead callable-local
/// bindings introduced by direct-call rewrites, delegating to
/// [`prune_dead_callable_locals_in_block`] for nested `Block` and `While`
/// bodies until all dead bindings are removed.
fn prune_dead_callable_locals_in_expr(package: &mut Package, expr_id: ExprId) {
    let expr = package.get_expr(expr_id).clone();
    match expr.kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for expr_id in exprs {
                prune_dead_callable_locals_in_expr(package, expr_id);
            }
        }
        ExprKind::ArrayRepeat(lhs, rhs)
        | ExprKind::Assign(lhs, rhs)
        | ExprKind::AssignOp(_, lhs, rhs)
        | ExprKind::BinOp(_, lhs, rhs)
        | ExprKind::Call(lhs, rhs)
        | ExprKind::Index(lhs, rhs)
        | ExprKind::AssignField(lhs, _, rhs)
        | ExprKind::UpdateField(lhs, _, rhs) => {
            prune_dead_callable_locals_in_expr(package, lhs);
            prune_dead_callable_locals_in_expr(package, rhs);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            prune_dead_callable_locals_in_expr(package, a);
            prune_dead_callable_locals_in_expr(package, b);
            prune_dead_callable_locals_in_expr(package, c);
        }
        ExprKind::Block(block_id) => prune_dead_callable_locals_in_block(package, block_id),
        ExprKind::Fail(inner)
        | ExprKind::Field(inner, _)
        | ExprKind::Return(inner)
        | ExprKind::UnOp(_, inner) => prune_dead_callable_locals_in_expr(package, inner),
        ExprKind::If(cond, body, otherwise) => {
            prune_dead_callable_locals_in_expr(package, cond);
            prune_dead_callable_locals_in_expr(package, body);
            if let Some(otherwise) = otherwise {
                prune_dead_callable_locals_in_expr(package, otherwise);
            }
        }
        ExprKind::Range(start, step, end) => {
            for expr_id in [start, step, end].into_iter().flatten() {
                prune_dead_callable_locals_in_expr(package, expr_id);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(expr_id) = component {
                    prune_dead_callable_locals_in_expr(package, expr_id);
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                prune_dead_callable_locals_in_expr(package, copy);
            }
            for field in fields {
                prune_dead_callable_locals_in_expr(package, field.value);
            }
        }
        ExprKind::While(cond, block_id) => {
            prune_dead_callable_locals_in_expr(package, cond);
            prune_dead_callable_locals_in_block(package, block_id);
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Removes a specific dead callable local scoped to a single statement,
/// delegating to [`remove_dead_callable_local_from_expr`] for the
/// statement's inner expression.
fn remove_dead_callable_local_from_stmt(
    package: &mut Package,
    stmt_id: qsc_fir::fir::StmtId,
    local_var: LocalVarId,
) {
    let stmt = package.get_stmt(stmt_id).clone();
    match stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            remove_dead_callable_local_from_expr(package, expr_id, local_var);
        }
        StmtKind::Item(_) => {}
    }
}

/// Removes references to a dead callable local inside a given expression
/// subtree, recursing through `Block`, `If`, `While`, and compound
/// expressions to reach every nested block via
/// [`remove_dead_callable_local_from_block`].
fn remove_dead_callable_local_from_expr(
    package: &mut Package,
    expr_id: ExprId,
    local_var: LocalVarId,
) {
    let expr = package.get_expr(expr_id).clone();
    match expr.kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for expr_id in exprs {
                remove_dead_callable_local_from_expr(package, expr_id, local_var);
            }
        }
        ExprKind::ArrayRepeat(lhs, rhs)
        | ExprKind::Assign(lhs, rhs)
        | ExprKind::AssignOp(_, lhs, rhs)
        | ExprKind::BinOp(_, lhs, rhs)
        | ExprKind::Call(lhs, rhs)
        | ExprKind::Index(lhs, rhs)
        | ExprKind::AssignField(lhs, _, rhs)
        | ExprKind::UpdateField(lhs, _, rhs) => {
            remove_dead_callable_local_from_expr(package, lhs, local_var);
            remove_dead_callable_local_from_expr(package, rhs, local_var);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            remove_dead_callable_local_from_expr(package, a, local_var);
            remove_dead_callable_local_from_expr(package, b, local_var);
            remove_dead_callable_local_from_expr(package, c, local_var);
        }
        ExprKind::Block(block_id) => {
            remove_dead_callable_local_from_block(package, block_id, local_var);
        }
        ExprKind::Fail(inner)
        | ExprKind::Field(inner, _)
        | ExprKind::Return(inner)
        | ExprKind::UnOp(_, inner) => {
            remove_dead_callable_local_from_expr(package, inner, local_var);
        }
        ExprKind::If(cond, body, otherwise) => {
            remove_dead_callable_local_from_expr(package, cond, local_var);
            remove_dead_callable_local_from_expr(package, body, local_var);
            if let Some(otherwise) = otherwise {
                remove_dead_callable_local_from_expr(package, otherwise, local_var);
            }
        }
        ExprKind::Range(start, step, end) => {
            for expr_id in [start, step, end].into_iter().flatten() {
                remove_dead_callable_local_from_expr(package, expr_id, local_var);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(expr_id) = component {
                    remove_dead_callable_local_from_expr(package, expr_id, local_var);
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                remove_dead_callable_local_from_expr(package, copy, local_var);
            }
            for field in fields {
                remove_dead_callable_local_from_expr(package, field.value, local_var);
            }
        }
        ExprKind::While(cond, block_id) => {
            remove_dead_callable_local_from_expr(package, cond, local_var);
            remove_dead_callable_local_from_block(package, block_id, local_var);
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Collects the local variables bound by a pattern into `bound_vars`.
fn collect_bound_pat_vars(package: &Package, pat_id: PatId, bound_vars: &mut Vec<LocalVarId>) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => bound_vars.push(ident.id),
        PatKind::Discard => {}
        PatKind::Tuple(pats) => {
            for &sub_pat_id in pats {
                collect_bound_pat_vars(package, sub_pat_id, bound_vars);
            }
        }
    }
}

/// Reports whether the pattern binds `local_var`.
fn pat_binds_local_var(package: &Package, pat_id: PatId, local_var: LocalVarId) -> bool {
    let mut bound_vars = Vec::new();
    collect_bound_pat_vars(package, pat_id, &mut bound_vars);
    bound_vars
        .into_iter()
        .any(|bound_var| bound_var == local_var)
}

/// [`Visitor`] that records the tuple-field path of a target local the
/// first time it reaches a tuple `Local` binding that introduces it. Direct
/// (non-tuple) bindings are ignored because their path is empty. Recursion
/// stops once a path is found.
struct TupleFieldPathFinder<'a> {
    package: &'a Package,
    local_var: LocalVarId,
    result: Option<Vec<usize>>,
}

impl<'a> Visitor<'a> for TupleFieldPathFinder<'a> {
    fn visit_stmt(&mut self, stmt: StmtId) {
        if self.result.is_some() {
            return;
        }
        if let StmtKind::Local(_, pat_id, _) = self.package.get_stmt(stmt).kind
            && let Some(path) = find_var_field_path_in_pat(self.package, pat_id, self.local_var)
            && !path.is_empty()
        {
            self.result = Some(path);
            return;
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: ExprId) {
        if self.result.is_some() {
            return;
        }
        visit::walk_expr(self, expr);
    }

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
}

/// For a local variable bound inside a tuple pattern (e.g.,
/// `let (_, callee, _) = tuple_expr`), returns the field position
/// path (e.g., `[1]` for position 1).
fn find_var_tuple_field_path_in_callable(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> Option<Vec<usize>> {
    let Some(ItemKind::Callable(decl)) = package.items.get(callable_id).map(|item| &item.kind)
    else {
        return None;
    };
    let mut finder = TupleFieldPathFinder {
        package,
        local_var,
        result: None,
    };
    walk_callable_impl_bodies(&mut finder, &decl.implementation);
    finder.result
}

/// Recursively finds the tuple field path for a local variable within a
/// pattern tree. Returns `Some(vec![])` for a direct bind,
/// `Some(vec![1])` for position 1 in a tuple pattern, etc.
fn find_var_field_path_in_pat(
    package: &Package,
    pat_id: PatId,
    local_var: LocalVarId,
) -> Option<Vec<usize>> {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) if ident.id == local_var => Some(Vec::new()),
        PatKind::Bind(_) | PatKind::Discard => None,
        PatKind::Tuple(sub_pats) => {
            for (i, &sub_pat_id) in sub_pats.iter().enumerate() {
                if let Some(mut path) = find_var_field_path_in_pat(package, sub_pat_id, local_var) {
                    path.insert(0, i);
                    return Some(path);
                }
            }
            None
        }
    }
}

/// Rewrites the callee expression of a direct call to reference the
/// specialized target callable and updates its type accordingly.
///
/// # Before
/// ```text
/// Var(original_item) : OldArrow   // callee expr
/// ```
/// # After
/// ```text
/// Var(specialized_item) : NewArrow   // callee replaced and retyped
/// ```
///
/// # Mutations
/// - Overwrites the callee `Expr` node in place via
///   [`rewrite_item_callee_with_functor`].
/// - May allocate functor-wrapper `Expr` nodes through `assigner`.
fn rewrite_direct_callee(
    package: &mut Package,
    package_id: PackageId,
    callee_id: ExprId,
    callable: &ConcreteCallable,
    _captures: &[CapturedVar],
    controlled_layers: usize,
    assigner: &mut Assigner,
) {
    let callee_expr = package.get_expr(callee_id).clone();
    let (item_id, functor, callee_ty) = match callable {
        ConcreteCallable::Global { item_id, functor } => {
            let callee_ty = if item_id.package == package_id
                && direct_lambda_packaged_input(package, item_id.item).is_some()
            {
                build_direct_global_callee_ty(package, *item_id, &callee_expr.ty, controlled_layers)
                    .unwrap_or_else(|| callee_expr.ty.clone())
            } else {
                callee_expr.ty.clone()
            };
            (*item_id, *functor, callee_ty)
        }
        ConcreteCallable::Closure {
            target, functor, ..
        } => {
            let item_id = ItemId {
                package: package_id,
                item: *target,
            };
            (
                item_id,
                *functor,
                build_direct_global_callee_ty(package, item_id, &callee_expr.ty, controlled_layers)
                    .unwrap_or_else(|| callee_expr.ty.clone()),
            )
        }
        ConcreteCallable::Dynamic => return,
    };

    rewrite_item_callee_with_functor(package, callee_id, item_id, callee_ty, functor, assigner);
}

/// Rewrites the argument tuple of a direct call whose callable argument
/// was a closure, splicing captured values into the argument layout.
///
/// # Before
/// ```text
/// original_args : OriginalInputTy
/// ```
/// # After
/// ```text
/// (capture_0, ..., capture_n, original_args) : (CaptureTys..., OriginalInputTy)
/// ```
///
/// # Mutations
/// - Rewrites `args_id`'s `ExprKind` and `Ty` in place to a `Tuple`
///   containing capture expressions followed by the original args.
/// - Allocates capture `Expr` nodes through `assigner`.
/// - For controlled operations, recurses through control-qubit layers.
fn rewrite_direct_closure_args(
    package: &mut Package,
    args_id: ExprId,
    captures: &[CapturedVar],
    controlled_layers: usize,
    assigner: &mut Assigner,
) {
    if controlled_layers > 0 {
        let inner_id = match package.get_expr(args_id).kind {
            ExprKind::Tuple(ref elements) if elements.len() > 1 => elements[1],
            _ => return,
        };
        rewrite_direct_closure_args(package, inner_id, captures, controlled_layers - 1, assigner);
        let inner_ty = package.get_expr(inner_id).ty.clone();
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        if let Ty::Tuple(ref mut tys) = args_mut.ty
            && tys.len() > 1
        {
            tys[1] = inner_ty;
        }
        return;
    }

    let args_expr = package.get_expr(args_id).clone();
    let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();

    let preserved_args_id = assigner.next_expr();
    package.exprs.insert(
        preserved_args_id,
        Expr {
            id: preserved_args_id,
            span: args_expr.span,
            ty: args_expr.ty.clone(),
            kind: args_expr.kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let mut new_elements = capture_ids;
    new_elements.push(preserved_args_id);
    let mut new_tys = capture_tys;
    new_tys.push(args_expr.ty);

    let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
    args_mut.kind = ExprKind::Tuple(new_elements);
    args_mut.ty = Ty::Tuple(new_tys);
}

/// Builds the arrow type for a direct call to a global specialized target,
/// matching the caller's expected signature after controlled-layer peeling.
fn build_direct_global_callee_ty(
    package: &Package,
    item_id: ItemId,
    callee_ty: &Ty,
    controlled_layers: usize,
) -> Option<Ty> {
    let Ty::Arrow(arrow) = callee_ty else {
        return None;
    };
    let ItemKind::Callable(decl) = &package.get_item(item_id.item).kind else {
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

/// Replaces the innermost input slot beneath `controlled_layers` nested
/// controlled-operation tuples with `target_input`, returning the rewritten
/// outer type.
///
/// A copy of this helper also lives in
/// `super::specialize::apply_target_input_at_control_path`; keep the two
/// in sync when changing controlled-layer handling (see the module-level
/// note for why both copies exist).
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

/// Returns the packaged input tuple type for a direct call to a lambda
/// target whose parameters live in a one-element tuple.
///
/// Relies on the naming contract with the producer pass: lifted lambdas
/// that take a single tuple parameter are named with a leading `".lambda"`
/// prefix. Do not rename lambda items without updating this predicate.
fn direct_lambda_packaged_input(package: &Package, item_id: LocalItemId) -> Option<Ty> {
    let ItemKind::Callable(decl) = &package.get_item(item_id).kind else {
        return None;
    };

    let input_ty = package.get_pat(decl.input).ty.clone();
    if decl.name.name.as_ref().starts_with(".lambda")
        && matches!(&input_ty, Ty::Tuple(items) if items.len() == 1)
    {
        Some(input_ty)
    } else {
        None
    }
}

/// Builds a single direct-call branch for index-dispatch synthesis by
/// materializing the callee expression, argument tuple, and capture
/// splicing for one specialized callable.
///
/// # Before
/// ```text
/// (no expression — branch does not yet exist)
/// ```
/// # After
/// ```text
/// Call(Var(specialized_item), (captures..., args)) : result_ty
/// ```
///
/// # Mutations
/// - Allocates callee, args, and call `Expr` nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
fn create_direct_branch_call(
    package: &mut Package,
    package_id: PackageId,
    orig_callee: &Expr,
    orig_args: &Expr,
    span: Span,
    result_ty: &Ty,
    direct_call_site: &DirectCallSite,
    assigner: &mut Assigner,
) -> ExprId {
    let captures = match &direct_call_site.callable {
        ConcreteCallable::Closure { captures, .. } => {
            resolve_rewrite_captures(package, orig_callee.id, captures)
        }
        _ => Vec::new(),
    };
    let (_, outer_functor) = peel_body_functors(package, orig_callee.id);
    let controlled_layers = usize::from(outer_functor.controlled);
    let package_direct_lambda_input = match &direct_call_site.callable {
        ConcreteCallable::Global { item_id, .. } if item_id.package == package_id => {
            direct_lambda_packaged_input(package, item_id.item)
        }
        _ => None,
    };
    let package_direct_lambda = matches!(
        package_direct_lambda_input.as_ref(),
        Some(target_input)
            if apply_target_input_at_control_path(&orig_args.ty, target_input, controlled_layers)
                != orig_args.ty
    );

    let (item_id, functor, callee_ty) = match &direct_call_site.callable {
        ConcreteCallable::Global { item_id, functor } => {
            let callee_ty = if item_id.package == package_id
                && package_direct_lambda_input.is_some()
            {
                build_direct_global_callee_ty(package, *item_id, &orig_callee.ty, controlled_layers)
                    .unwrap_or_else(|| orig_callee.ty.clone())
            } else {
                orig_callee.ty.clone()
            };
            (*item_id, *functor, callee_ty)
        }
        ConcreteCallable::Closure {
            target, functor, ..
        } => {
            let item_id = ItemId {
                package: package_id,
                item: *target,
            };
            (
                item_id,
                *functor,
                build_direct_global_callee_ty(package, item_id, &orig_callee.ty, controlled_layers)
                    .unwrap_or_else(|| orig_callee.ty.clone()),
            )
        }
        ConcreteCallable::Dynamic => return orig_callee.id,
    };

    let callee_id =
        alloc_item_callee_expr_with_functor(package, span, item_id, &callee_ty, functor, assigner);
    let (args_kind, args_ty) = build_direct_branch_args_data(
        package,
        orig_args,
        &captures,
        controlled_layers,
        package_direct_lambda,
        assigner,
    );
    let args_id = assigner.next_expr();
    package.exprs.insert(
        args_id,
        Expr {
            id: args_id,
            span,
            ty: args_ty,
            kind: args_kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let call_id = assigner.next_expr();
    package.exprs.insert(
        call_id,
        Expr {
            id: call_id,
            span,
            ty: result_ty.clone(),
            kind: ExprKind::Call(callee_id, args_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    call_id
}

/// Assembles the argument-tuple expressions for a direct-call branch,
/// including any capture values that must accompany a closure branch.
fn build_direct_branch_args_data(
    package: &mut Package,
    orig_args: &Expr,
    captures: &[CapturedVar],
    controlled_layers: usize,
    package_direct_lambda: bool,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    if controlled_layers > 0 {
        let ExprKind::Tuple(elements) = &orig_args.kind else {
            return build_direct_branch_args_data(
                package,
                orig_args,
                captures,
                0,
                package_direct_lambda,
                assigner,
            );
        };
        let Ty::Tuple(tys) = &orig_args.ty else {
            return build_direct_branch_args_data(
                package,
                orig_args,
                captures,
                0,
                package_direct_lambda,
                assigner,
            );
        };
        if elements.len() < 2 || tys.len() < 2 {
            return build_direct_branch_args_data(
                package,
                orig_args,
                captures,
                0,
                package_direct_lambda,
                assigner,
            );
        }

        let inner_orig = package.get_expr(elements[1]).clone();
        let (inner_kind, inner_ty) = build_direct_branch_args_data(
            package,
            &inner_orig,
            captures,
            controlled_layers - 1,
            package_direct_lambda,
            assigner,
        );

        let inner_id = assigner.next_expr();
        package.exprs.insert(
            inner_id,
            Expr {
                id: inner_id,
                span: inner_orig.span,
                ty: inner_ty.clone(),
                kind: inner_kind,
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );

        return (
            ExprKind::Tuple(vec![elements[0], inner_id]),
            Ty::Tuple(vec![tys[0].clone(), inner_ty]),
        );
    }

    if captures.is_empty() && !package_direct_lambda {
        return (orig_args.kind.clone(), orig_args.ty.clone());
    }

    let capture_ids = allocate_capture_exprs(package, orig_args.span, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();

    let preserved_args_id = assigner.next_expr();
    package.exprs.insert(
        preserved_args_id,
        Expr {
            id: preserved_args_id,
            span: orig_args.span,
            ty: orig_args.ty.clone(),
            kind: orig_args.kind.clone(),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let mut tuple_items = capture_ids;
    tuple_items.push(preserved_args_id);
    let mut tuple_tys = capture_tys;
    tuple_tys.push(orig_args.ty.clone());

    (ExprKind::Tuple(tuple_items), Ty::Tuple(tuple_tys))
}

/// Rewrites a single call site to use the specialized callable.
///
/// # Before
/// ```text
/// Call(Var(hof_item), (callable_arg, other_args))
/// ```
/// # After
/// ```text
/// Call(Var(specialized_item), (other_args, captures...))
/// ```
///
/// # Mutations
/// - Rewrites the callee via [`rewrite_specialized_callee`].
/// - Rewrites args via [`rewrite_args`], removing the callable parameter
///   and appending closure captures.
fn rewrite_one(
    package: &mut Package,
    _package_id: PackageId,
    call_site: &CallSite,
    param: &CallableParam,
    spec_store_id: StoreItemId,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    let call_expr = package.get_expr(call_site.call_expr_id).clone();

    let ExprKind::Call(callee_id, args_id) = call_expr.kind else {
        return;
    };

    // Replace callee with the specialized callable reference
    let spec_item_id = ItemId {
        package: spec_store_id.package,
        item: spec_store_id.item,
    };

    // Build the new callee type: remove the callable param from the arrow input.
    let input_path = callable_param_input_path(package, callee_id, param);
    // Count the outer controlled functor layers wrapping this call so the arg
    // rewrite can nest closure captures INSIDE the control-level input tuple
    // instead of appending them as top-level siblings of the control qubits.
    let (_, outer_functor) = peel_body_functors(package, callee_id);
    let controlled_layers = usize::from(outer_functor.controlled);
    let captures = match &call_site.callable_arg {
        ConcreteCallable::Closure { captures, .. } => {
            resolve_rewrite_captures(package, call_site.arg_expr_id, captures)
        }
        _ => Vec::new(),
    };
    let new_callee_ty = if !param.hof_input_is_tuple && !param.field_path.is_empty() {
        build_specialized_nested_payload_callee_ty(package, callee_id, &input_path, &captures)
    } else {
        build_specialized_callee_ty(package, callee_id, &input_path, &call_site.callable_arg)
    };
    rewrite_specialized_callee(package, callee_id, spec_item_id, new_callee_ty, assigner);

    // Remove the callable argument from the args tuple
    // Insert closure captures as extra arguments.
    //
    // Only the shallow (single field) case short-circuits here: it removes the
    // callable directly from the top-level payload. Deeper paths (len >= 2) must
    // fall through to `rewrite_args`, which routes to the nested deep-removal
    // path (`rewrite_single_arg_nested`) that can strip a callable buried inside
    // a nested aggregate and inline a `Var`-bound local initializer.
    if !param.hof_input_is_tuple && param.field_path.len() == 1 {
        let mut remove_indices = FxHashSet::default();
        if let Some(&field_index) = param.field_path.first() {
            remove_indices.insert(field_index);
        }
        if rewrite_nested_arg_expr_remove_fields_as_payload(
            package,
            expr_owner_lookup.get(&call_site.call_expr_id).copied(),
            args_id,
            &remove_indices,
            &captures,
            assigner,
        ) {
            return;
        }
    }
    rewrite_args(
        package,
        call_site.call_expr_id,
        args_id,
        &input_path,
        controlled_layers,
        &captures,
        expr_owner_lookup,
        assigner,
    );
}

/// Rewrites a single multi-argument higher-order call so it invokes the
/// combined specialization produced on the specialize side.
///
/// Every arrow argument slot is removed from the call's argument tuple in one
/// pass, and each closure argument's captures are appended in ascending
/// parameter order. The resulting argument tuple and callee type mirror the
/// combined specialization's input pattern built by `remove_callable_params` on
/// the specialize side: surviving arguments keep their order, all captures
/// follow in ascending parameter order, and the tuple flattens to a scalar only
/// when a single argument survives and no captures are appended.
///
/// `members` must be ordered ascending by parameter position so the appended
/// captures line up with the specialized input pattern.
fn rewrite_multi(
    package: &mut Package,
    call_expr_id: ExprId,
    members: &[(&CallSite, &CallableParam)],
    spec_store_id: StoreItemId,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    let call_expr = package.get_expr(call_expr_id).clone();
    let ExprKind::Call(callee_id, args_id) = call_expr.kind else {
        return;
    };

    let spec_item_id = ItemId {
        package: spec_store_id.package,
        item: spec_store_id.item,
    };

    // Collect the slots to remove and resolve every closure's captures in
    // ascending parameter order.
    //
    // For a multi-parameter HOF the call argument is a tuple of parameters, so
    // each member's top-level parameter index selects the slot to drop. For a
    // single tuple-valued parameter the whole call argument is that tuple, so
    // each member's immediate field index selects the element to drop instead;
    // the gate guarantees a single-level field path that covers the tuple.
    let uses_tuple_input = members
        .first()
        .is_none_or(|(call_site, _)| call_site.hof_input_is_tuple);
    let mut remove_indices: Vec<usize> = Vec::with_capacity(members.len());
    let mut captures: Vec<CapturedVar> = Vec::new();
    for (call_site, _param) in members {
        let remove_idx = if uses_tuple_input {
            call_site.top_level_param
        } else {
            *call_site
                .field_path
                .first()
                .unwrap_or(&call_site.top_level_param)
        };
        remove_indices.push(remove_idx);
        if let ConcreteCallable::Closure {
            captures: member_captures,
            ..
        } = &call_site.callable_arg
        {
            captures.extend(member_captures.iter().map(|capture| {
                let mut resolved = capture.clone();
                if resolved.expr.is_none() {
                    resolved.expr =
                        resolve_capture_expr_from_arg(package, call_site.arg_expr_id, capture.var);
                }
                resolved
            }));
        }
    }

    // Retarget the callee to the combined specialization with the rebuilt type.
    let new_callee_ty =
        build_specialized_multi_callee_ty(package, callee_id, &remove_indices, &captures);
    rewrite_specialized_callee(package, callee_id, spec_item_id, new_callee_ty, assigner);

    // Rebuild the argument tuple to match the combined input pattern. The
    // owner callable lets a non-inline tuple argument be projected through its
    // local initializer.
    let owner_callable = expr_owner_lookup.get(&call_expr_id).copied();
    rewrite_args_remove_tuple_elements(
        package,
        args_id,
        owner_callable,
        &remove_indices,
        &captures,
        assigner,
    );
}

/// Finds the single parameter position shared by the members of a forwarded
/// callable array, if there is exactly one repeated, array-typed position.
///
/// Returns `None` when no position repeats, more than one does, or the repeated
/// position is not array-typed.
fn callable_array_member_position(
    members: &[(&CallSite, &CallableParam)],
) -> Option<(usize, Vec<usize>)> {
    let mut positions: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    for (_, param) in members {
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
    members
        .iter()
        .find(|(_, param)| (param.top_level_param, param.field_path.clone()) == *position)
        .and_then(|(_, param)| matches!(param.param_ty, Ty::Array(_)).then(|| position.clone()))
}

/// Reports whether the forwarded callable array is nested inside a tuple
/// parameter, in which case the nested-field rewrite path is needed instead of
/// the simpler top-level path.
fn callable_array_member_needs_nested_rewrite(members: &[(&CallSite, &CallableParam)]) -> bool {
    let Some(position) = callable_array_member_position(members) else {
        return false;
    };
    members
        .iter()
        .find(|(_, param)| (param.top_level_param, param.field_path.clone()) == position)
        .is_some_and(|(call_site, param)| {
            !call_site.hof_input_is_tuple || !param.field_path.is_empty()
        })
}

/// Rewrites a call that forwards an array of callables so it targets the
/// specialized clone and no longer passes the callable array.
///
/// Handles the case where several call sites (`members`) share one forwarded
/// callable-array parameter. The callee expression is repointed at
/// `spec_store_id`, the callable-array argument is removed, and any captured
/// values are threaded through as extra arguments.
fn rewrite_callable_array_multi(
    package: &mut Package,
    call_expr_id: ExprId,
    members: &[(&CallSite, &CallableParam)],
    spec_store_id: StoreItemId,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    let call_expr = package.get_expr(call_expr_id).clone();
    let ExprKind::Call(callee_id, args_id) = call_expr.kind else {
        return;
    };

    let spec_item_id = ItemId {
        package: spec_store_id.package,
        item: spec_store_id.item,
    };

    let Some(array_position) = callable_array_member_position(members) else {
        return;
    };
    let Some((first_call_site, param)) = members
        .iter()
        .find(|(_, param)| (param.top_level_param, param.field_path.clone()) == array_position)
        .copied()
    else {
        return;
    };
    let mut captures = Vec::new();
    let mut remove_indices = FxHashSet::default();
    let mut remove_expr_ids = FxHashSet::default();
    for (call_site, _) in members {
        if let Some(&field_index) = call_site.field_path.first() {
            remove_indices.insert(field_index);
        }
        remove_expr_ids.insert(call_site.arg_expr_id);
        if let ConcreteCallable::Closure {
            captures: member_captures,
            ..
        } = &call_site.callable_arg
        {
            captures.extend(resolve_rewrite_captures(
                package,
                call_site.arg_expr_id,
                member_captures,
            ));
        }
    }

    let new_callee_ty = build_nested_callable_array_callee_ty(
        package,
        callee_id,
        first_call_site.hof_input_is_tuple,
        param.top_level_param,
        &remove_indices,
        &captures,
    );
    rewrite_specialized_callee(package, callee_id, spec_item_id, new_callee_ty, assigner);

    rewrite_args_remove_nested_callable_fields(
        package,
        call_expr_id,
        args_id,
        first_call_site.hof_input_is_tuple,
        param.top_level_param,
        &remove_indices,
        &remove_expr_ids,
        &captures,
        expr_owner_lookup,
        assigner,
    );
}

/// Computes the callee arrow type for a combined multi-argument rewrite by
/// removing every arrow input slot in `remove_indices` and appending all
/// capture types in parameter order.
fn build_specialized_multi_callee_ty(
    package: &Package,
    callee_id: ExprId,
    remove_indices: &[usize],
    captures: &[CapturedVar],
) -> Option<Ty> {
    let callee_expr = package.get_expr(callee_id);
    let Ty::Arrow(ref arrow) = callee_expr.ty else {
        return None;
    };
    let new_input = remove_tys_at_indices(package, &arrow.input, remove_indices, captures);
    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}

/// Builds the arrow type of the specialized callee after a forwarded callable
/// array nested inside a tuple parameter is removed and capture types are
/// appended.
///
/// Returns `None` when the callee is not arrow-typed.
fn build_nested_callable_array_callee_ty(
    package: &Package,
    callee_id: ExprId,
    uses_tuple_input: bool,
    top_level_param: usize,
    remove_indices: &FxHashSet<usize>,
    captures: &[CapturedVar],
) -> Option<Ty> {
    let callee_expr = package.get_expr(callee_id);
    let Ty::Arrow(ref arrow) = callee_expr.ty else {
        return None;
    };

    let input_ty = resolve_udt_ty(package, &arrow.input);
    let new_input = if uses_tuple_input {
        let Ty::Tuple(mut top_level_tys) = input_ty else {
            return None;
        };
        top_level_tys[top_level_param] = remove_nested_top_level_fields_from_ty(
            package,
            &top_level_tys[top_level_param],
            remove_indices,
        );
        top_level_tys.extend(captures.iter().map(|capture| capture.ty.clone()));
        Ty::Tuple(top_level_tys)
    } else {
        let mut ty = remove_nested_top_level_fields_from_ty(package, &input_ty, remove_indices);
        if !captures.is_empty() {
            let mut tys = vec![ty];
            tys.extend(captures.iter().map(|capture| capture.ty.clone()));
            ty = Ty::Tuple(tys);
        }
        ty
    };

    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}

/// Removes the tuple element types at `remove_indices` from `ty`, resolving
/// through any UDT wrappers first.
///
/// Collapses the result to `Unit` when nothing remains, and to the sole element
/// when exactly one remains, rather than leaving a one-tuple.
fn remove_nested_top_level_fields_from_ty(
    package: &Package,
    ty: &Ty,
    remove_indices: &FxHashSet<usize>,
) -> Ty {
    let ty = resolve_udt_ty(package, ty);
    let Ty::Tuple(tys) = ty else {
        return ty;
    };
    let remaining: Vec<Ty> = tys
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| !remove_indices.contains(idx))
        .map(|(_, ty)| ty)
        .collect();
    match remaining.as_slice() {
        [] => Ty::UNIT,
        [single] => single.clone(),
        _ => Ty::Tuple(remaining),
    }
}

/// Removes the tuple element types at `remove_indices` and appends the capture
/// types, flattening to a scalar only when a single element survives and no
/// captures are appended, matching the specialize-side input pattern flatten
/// rule in `remove_callable_params`.
fn remove_tys_at_indices(
    package: &Package,
    ty: &Ty,
    remove_indices: &[usize],
    captures: &[CapturedVar],
) -> Ty {
    let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
    let ty = resolve_udt_ty(package, ty);
    let Ty::Tuple(tys) = &ty else {
        // A multi-argument HOF always has a tuple input.
        return ty.clone();
    };
    let remove: FxHashSet<usize> = remove_indices.iter().copied().collect();
    let mut remaining: Vec<Ty> = tys
        .iter()
        .enumerate()
        .filter(|(i, _)| !remove.contains(i))
        .map(|(_, t)| t.clone())
        .collect();
    remaining.extend(capture_tys);
    if remaining.len() == 1 && captures.is_empty() {
        remaining
            .into_iter()
            .next()
            .expect("single element should exist")
    } else {
        Ty::Tuple(remaining)
    }
}

/// Removes the top-level tuple elements at `remove_indices` from a call's
/// argument expression and appends closure captures.
///
/// The rebuilt tuple matches the combined specialization's input pattern:
/// surviving arguments keep their order, captures follow in ascending parameter
/// order, and the tuple flattens to a scalar only when a single argument
/// survives and no captures are appended.
fn rewrite_args_remove_tuple_elements(
    package: &mut Package,
    args_id: ExprId,
    owner_callable: Option<LocalItemId>,
    remove_indices: &[usize],
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) {
    let args_expr = package
        .exprs
        .get(args_id)
        .expect("args expr not found")
        .clone();

    let remove: FxHashSet<usize> = remove_indices.iter().copied().collect();

    if let ExprKind::Tuple(elements) = &args_expr.kind {
        let mut new_elements: Vec<ExprId> = elements
            .iter()
            .enumerate()
            .filter(|(i, _)| !remove.contains(i))
            .map(|(_, &id)| id)
            .collect();

        let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
        new_elements.extend(capture_ids);

        let new_ty = remove_tys_at_indices(package, &args_expr.ty, remove_indices, captures);

        if new_elements.len() == 1 && captures.is_empty() {
            let single_id = new_elements[0];
            let single_expr = package
                .exprs
                .get(single_id)
                .expect("expr not found")
                .clone();
            let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
            args_mut.kind = single_expr.kind;
            args_mut.ty = single_expr.ty;
        } else {
            let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
            args_mut.kind = ExprKind::Tuple(new_elements);
            args_mut.ty = new_ty;
        }
        return;
    }

    // Non-inline argument: a local bound to a tuple or struct literal of
    // callables. The callee was already retargeted to the reduced combined
    // spec, so the arguments must be reduced to match. Resolve the local's
    // initializer and project the surviving slots through the same removal
    // helper the per-row nested path uses, then overwrite the argument
    // expression in place with the rebuilt aggregate. The now-dead initializer
    // binding is pruned later by the dead-callable-local cleanup.
    if let ExprKind::Var(Res::Local(local_var), _) = args_expr.kind
        && let Some(owner_callable) = owner_callable
        && let Some(init_expr_id) =
            find_local_init_expr_in_callable(package, owner_callable, local_var)
        && let Some((kind, ty)) = remove_top_level_field_from_expr_data(
            package,
            init_expr_id,
            &remove,
            captures,
            assigner,
        )
    {
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = kind;
        args_mut.ty = ty;
    }

    // Any other shape, for example a function-returning-tuple result, is
    // outside the combined rewrite's scope and is left untouched; the
    // defunctionalization fixpoint surfaces an honest dynamic-callable
    // diagnostic for shapes the analysis cannot project.
}

/// Removes the callable argument selected by `param` from the call arguments
/// and appends closure captures when needed.
///
/// # Before
/// ```text
/// (callable_arg, arg1, arg2)
/// ```
/// # After
/// ```text
/// (arg1, arg2, capture0, ..., captureN)   // callable_arg removed, captures appended
/// ```
///
/// # Mutations
/// - Rewrites `args_id`'s `ExprKind` and `Ty` in place.
/// - Allocates capture `Expr` nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
fn rewrite_args(
    package: &mut Package,
    call_expr_id: ExprId,
    args_id: ExprId,
    input_path: &[usize],
    controlled_layers: usize,
    captures: &[CapturedVar],
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    let args_expr = package
        .exprs
        .get(args_id)
        .expect("args expr not found")
        .clone();

    if input_path.is_empty() {
        rewrite_single_arg_root(package, args_id, captures, assigner);
    } else if matches!(args_expr.kind, ExprKind::Tuple(_)) {
        let owner_callable = expr_owner_lookup.get(&call_expr_id).copied();
        if input_path.len() == 1 {
            rewrite_args_remove_tuple_element(package, args_id, input_path[0], captures, assigner);
        } else {
            rewrite_args_nested_tuple_input(
                package,
                owner_callable,
                args_id,
                input_path[0],
                &input_path[1..],
                controlled_layers,
                captures,
                assigner,
            );
        }
    } else {
        rewrite_single_arg_nested(
            package,
            call_expr_id,
            args_id,
            input_path,
            captures,
            expr_owner_lookup,
            assigner,
        );
    }
}

/// Removes a top-level element from a tuple-structured args expression and
/// appends any closure captures.
///
/// # Before
/// ```text
/// (arg0, callable_arg, arg2)   // param_index = 1
/// ```
/// # After
/// ```text
/// (arg0, arg2, capture0, ...)   // element removed, captures appended
/// ```
///
/// # Mutations
/// - Rewrites `args_id`'s `ExprKind` and `Ty` in place.
/// - Flattens single-element tuples to scalars.
/// - Allocates capture `Expr` nodes through `assigner`.
fn rewrite_args_remove_tuple_element(
    package: &mut Package,
    args_id: ExprId,
    param_index: usize,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) {
    let args_expr = package
        .exprs
        .get(args_id)
        .expect("args expr not found")
        .clone();

    match &args_expr.kind {
        ExprKind::Tuple(elements) => {
            let mut new_elements: Vec<ExprId> = elements
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != param_index)
                .map(|(_, &id)| id)
                .collect();

            // Append capture expressions.
            let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
            new_elements.extend(capture_ids);

            // Rebuild the type.
            let new_ty =
                build_tuple_ty_without_path(package, &args_expr.ty, &[param_index], captures);

            if new_elements.len() == 1 && captures.is_empty() {
                // Flatten single-element tuple to match remove_callable_param
                // which flattens the declaration's input pattern.
                let single_id = new_elements[0];
                let single_expr = package
                    .exprs
                    .get(single_id)
                    .expect("expr not found")
                    .clone();
                let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
                args_mut.kind = single_expr.kind;
                args_mut.ty = single_expr.ty;
            } else {
                let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
                args_mut.kind = ExprKind::Tuple(new_elements);
                args_mut.ty = new_ty;
            }
        }
        _ => {
            rewrite_single_arg_root(package, args_id, captures, assigner);
        }
    }
}

/// Rewrites args for a nested callable inside a top-level tuple input slot.
///
/// For an uncontrolled call (`controlled_layers == 0`) any closure captures are
/// appended to the top-level args tuple as siblings of the surviving elements.
///
/// # Before
/// ```text
/// (ctrl_qubits, (callable_arg, inner_arg))   // field_path = [0]
/// ```
/// # After
/// ```text
/// (ctrl_qubits, (inner_arg), capture0, ...)   // nested element removed
/// ```
///
/// For a controlled call (`controlled_layers > 0`) each `Controlled` functor
/// wraps the whole input as `([ctls], base_input)` without splitting the base
/// input tuple. Appending captures at the top level would produce a malformed
/// tuple such as `([ctls], (inner_arg), capture0)` whose control-level element
/// is no longer a 2-tuple, which `split_controls_and_input` in `qsc_rca`
/// rejects. Instead the captures are nested INSIDE the deepest input tuple via
/// [`append_captures_beneath_control_layers`], yielding
/// `([ctls], (inner_arg, capture0, ...))`. This lockstep with
/// `rewrite_closure_dispatch_branch_args` in [`super::specialize`] keeps the
/// caller arg shape aligned with the specialized callee's uncontrolled input
/// pattern.
///
/// # Mutations
/// - Rewrites the inner element via [`rewrite_local_single_arg_nested`] or
///   [`remove_element_at_path`], then updates the outer tuple's type.
/// - Allocates capture `Expr` nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
fn rewrite_args_nested_tuple_input(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    args_id: ExprId,
    top_level_param: usize,
    field_path: &[usize],
    controlled_layers: usize,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) {
    let args_expr = package
        .exprs
        .get(args_id)
        .expect("args expr not found")
        .clone();

    if let ExprKind::Tuple(elements) = &args_expr.kind {
        let inner_id = elements[top_level_param];
        if !rewrite_local_single_arg_nested(
            package,
            owner_callable,
            inner_id,
            field_path,
            &[],
            assigner,
        ) {
            // Remove the nested element from the inner tuple.
            remove_element_at_path(package, inner_id, field_path);
        }

        // Under one or more control layers, nest the captures inside the base
        // input tuple beneath the control qubits rather than appending them as
        // top-level siblings, refreshing each control tuple's input-slot type on
        // the way out.
        if !captures.is_empty() && controlled_layers > 0 {
            append_captures_beneath_control_layers(
                package,
                args_id,
                controlled_layers,
                captures,
                assigner,
            );
            return;
        }

        // Read the updated inner type before mutably borrowing the outer.
        let inner_ty = package
            .exprs
            .get(inner_id)
            .expect("expr not found")
            .ty
            .clone();

        // Append captures to the top-level tuple if any.
        if captures.is_empty() {
            // Update the outer tuple's type for the modified inner element.
            let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
            if let Ty::Tuple(ref mut tys) = args_mut.ty {
                tys[top_level_param] = inner_ty;
            }
        } else {
            let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
            let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
            let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
            if let ExprKind::Tuple(ref mut elems) = args_mut.kind {
                elems.extend(capture_ids);
            }
            if let Ty::Tuple(ref mut tys) = args_mut.ty {
                tys[top_level_param] = inner_ty;
                tys.extend(capture_tys);
            }
        }
    }
}

/// Appends closure capture expressions and types into the input tuple nested
/// beneath `controlled_layers` control functor layers.
///
/// Each `Controlled` functor wraps the whole input as `([ctls], base_input)`
/// (it never splits the base input tuple), so descending into `elements[1]`
/// once per layer reaches the callable's uncontrolled input tuple. The captures
/// are appended LAST inside that base tuple, and each enclosing control tuple's
/// input-slot type (`tys[1]`) is refreshed on the way out so the control-level
/// argument stays a valid 2-tuple.
///
/// # Before (single control layer)
/// ```text
/// ([ctls], (inner_arg))
/// ```
/// # After (single control layer)
/// ```text
/// ([ctls], (inner_arg, capture0, ...))
/// ```
///
/// # Mutations
/// - Rewrites the deepest input tuple's `ExprKind` and `Ty` in place.
/// - Refreshes each enclosing control tuple's input-slot `Ty`.
/// - Allocates capture `Expr` nodes through `assigner`.
fn append_captures_beneath_control_layers(
    package: &mut Package,
    tuple_id: ExprId,
    controlled_layers: usize,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) {
    if controlled_layers == 0 {
        let span = package.get_expr(tuple_id).span;
        let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
        let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
        let tuple_mut = package
            .exprs
            .get_mut(tuple_id)
            .expect("args expr not found");
        if let ExprKind::Tuple(ref mut elems) = tuple_mut.kind {
            elems.extend(capture_ids);
        }
        if let Ty::Tuple(ref mut tys) = tuple_mut.ty {
            tys.extend(capture_tys);
        }
        return;
    }

    let inner_id = match package.get_expr(tuple_id).kind {
        ExprKind::Tuple(ref elements) if elements.len() > 1 => elements[1],
        _ => return,
    };
    append_captures_beneath_control_layers(
        package,
        inner_id,
        controlled_layers - 1,
        captures,
        assigner,
    );
    let inner_ty = package.get_expr(inner_id).ty.clone();
    let tuple_mut = package
        .exprs
        .get_mut(tuple_id)
        .expect("args expr not found");
    if let Ty::Tuple(ref mut tys) = tuple_mut.ty
        && tys.len() > 1
    {
        tys[1] = inner_ty;
    }
}

/// Rewrites args when the callable is nested inside the single argument value.
///
/// # Before
/// ```text
/// args = local_udt   // UDT/tuple containing callable at field_path
/// ```
/// # After
/// ```text
/// args = (remaining_fields, captures...)   // callable field removed
/// ```
///
/// # Mutations
/// - Delegates to [`rewrite_local_single_arg_nested`] when the arg is a
///   local whose initializer can be decomposed, otherwise falls back to
///   [`remove_element_at_path`].
/// - Allocates capture `Expr` nodes through `assigner`.
fn rewrite_single_arg_nested(
    package: &mut Package,
    call_expr_id: ExprId,
    args_id: ExprId,
    field_path: &[usize],
    captures: &[CapturedVar],
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    if rewrite_local_single_arg_nested(
        package,
        expr_owner_lookup.get(&call_expr_id).copied(),
        args_id,
        field_path,
        captures,
        assigner,
    ) {
        return;
    }

    if field_path.len() == 1 {
        let mut remove_indices = FxHashSet::default();
        remove_indices.insert(field_path[0]);
        // A top-level callable field can sit inside a struct or tuple literal.
        // Rebuild that aggregate so the remaining fields keep the same order as
        // the specialized callee's reduced input pattern.
        if rewrite_nested_arg_expr_remove_fields_as_payload(
            package,
            expr_owner_lookup.get(&call_expr_id).copied(),
            args_id,
            &remove_indices,
            captures,
            assigner,
        ) {
            return;
        }
        if let Some((kind, ty)) = remove_top_level_field_from_expr_data(
            package,
            args_id,
            &remove_indices,
            captures,
            assigner,
        ) {
            let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
            args_expr.kind = kind;
            args_expr.ty = ty;
            return;
        }
    }

    remove_element_at_path(package, args_id, field_path);
    if !captures.is_empty() {
        let span = package.get_expr(args_id).span;
        let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
        let modified_expr = package.exprs.get(args_id).expect("expr not found").clone();
        let mut new_elements = if let ExprKind::Tuple(elems) = &modified_expr.kind {
            elems.clone()
        } else {
            // Non-`Tuple` arg (e.g. an unresolved `Var`): copy the current
            // payload into a FRESH expr and reference that instead of
            // `args_id`. Referencing `args_id` here would make the rewritten
            // `Tuple([args_id, ...])` contain itself, producing a
            // self-referential expr cycle that overflows any later
            // expression-tree walk.
            let payload_id = assigner.next_expr();
            package.exprs.insert(
                payload_id,
                Expr {
                    id: payload_id,
                    span,
                    ty: modified_expr.ty.clone(),
                    kind: modified_expr.kind.clone(),
                    exec_graph_range: EMPTY_EXEC_RANGE,
                },
            );
            vec![payload_id]
        };
        new_elements.extend(capture_ids);
        let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
        let mut new_tys = match &modified_expr.ty {
            Ty::Tuple(tys) => tys.clone(),
            ty => vec![ty.clone()],
        };
        new_tys.extend(capture_tys);
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = ExprKind::Tuple(new_elements);
        args_mut.ty = Ty::Tuple(new_tys);
    }
}

#[allow(clippy::too_many_arguments)]
fn rewrite_args_remove_nested_callable_fields(
    package: &mut Package,
    call_expr_id: ExprId,
    args_id: ExprId,
    uses_tuple_input: bool,
    top_level_param: usize,
    remove_indices: &FxHashSet<usize>,
    remove_expr_ids: &FxHashSet<ExprId>,
    captures: &[CapturedVar],
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    let args_expr = package.get_expr(args_id).clone();
    let owner_callable = expr_owner_lookup.get(&call_expr_id).copied();

    if uses_tuple_input {
        if let ExprKind::Tuple(elements) = args_expr.kind {
            let inner_id = elements[top_level_param];
            if rewrite_nested_arg_expr_remove_fields(
                package,
                owner_callable,
                inner_id,
                remove_indices,
                remove_expr_ids,
                &[],
                assigner,
            ) {
                let inner_ty = package.get_expr(inner_id).ty.clone();
                let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
                if let Ty::Tuple(ref mut tys) = args_mut.ty {
                    tys[top_level_param] = inner_ty;
                }
                if !captures.is_empty() {
                    let capture_ids =
                        allocate_capture_exprs(package, args_expr.span, captures, assigner);
                    let capture_tys: Vec<Ty> =
                        captures.iter().map(|capture| capture.ty.clone()).collect();
                    let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
                    if let ExprKind::Tuple(ref mut elems) = args_mut.kind {
                        elems.extend(capture_ids);
                    }
                    if let Ty::Tuple(ref mut tys) = args_mut.ty {
                        tys.extend(capture_tys);
                    }
                }
            }
        }
        return;
    }

    let _ = rewrite_nested_arg_expr_remove_fields(
        package,
        owner_callable,
        args_id,
        remove_indices,
        remove_expr_ids,
        captures,
        assigner,
    );
}

/// Rewrites a call's argument expression to drop the removed callable field and
/// thread captured values through, keeping the surviving fields as a single
/// payload element.
///
/// Returns `true` when the argument was rewritten. When removing the field
/// empties the payload, only the captures are emitted so the argument arity
/// matches the specialized callee's input pattern.
fn rewrite_nested_arg_expr_remove_fields_as_payload(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    args_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> bool {
    let args_expr = package.get_expr(args_id).clone();
    let source_id = if let ExprKind::Var(Res::Local(local_var), _) = args_expr.kind
        && let Some(owner_callable) = owner_callable
        && let Some(init_expr_id) =
            find_local_init_expr_in_callable(package, owner_callable, local_var)
    {
        init_expr_id
    } else {
        args_id
    };

    let Some((payload_kind, payload_ty)) =
        remove_top_level_field_from_expr_data(package, source_id, remove_indices, &[], assigner)
    else {
        return false;
    };

    if captures.is_empty() {
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = payload_kind;
        args_mut.ty = payload_ty;
        return true;
    }

    // Removing the sole tuple field leaves an empty payload. The specialized
    // callee drops that emptied slot and keeps only the threaded captures, so
    // prepending the empty payload here would produce a longer argument tuple
    // than the callee's input pattern. Emit only the captures in that case so
    // both sides agree on arity; otherwise keep the surviving payload ahead of
    // the captures.
    let payload_is_empty = matches!(&payload_kind, ExprKind::Tuple(fields) if fields.is_empty());

    let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();

    let (mut elements, mut tys) = if payload_is_empty {
        (Vec::new(), Vec::new())
    } else {
        let payload_id = assigner.next_expr();
        package.exprs.insert(
            payload_id,
            Expr {
                id: payload_id,
                span: args_expr.span,
                ty: payload_ty.clone(),
                kind: payload_kind,
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        (vec![payload_id], vec![payload_ty])
    };
    elements.extend(capture_ids);
    tys.extend(capture_tys);

    let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
    args_mut.kind = ExprKind::Tuple(elements);
    args_mut.ty = Ty::Tuple(tys);
    true
}

/// Rewrites a call's argument expression to drop the removed callable fields,
/// identified by index or by expression id, and thread captured values through
/// as sibling elements.
///
/// Returns `true` when the argument was rewritten.
fn rewrite_nested_arg_expr_remove_fields(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    args_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    remove_expr_ids: &FxHashSet<ExprId>,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> bool {
    let source_id = if let ExprKind::Var(Res::Local(local_var), _) = package.get_expr(args_id).kind
        && let Some(owner_callable) = owner_callable
        && let Some(init_expr_id) =
            find_local_init_expr_in_callable(package, owner_callable, local_var)
    {
        init_expr_id
    } else {
        args_id
    };

    let Some((kind, ty)) = remove_top_level_field_from_expr_data_with_exprs(
        package,
        source_id,
        remove_indices,
        remove_expr_ids,
        captures,
        assigner,
    ) else {
        return false;
    };

    let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
    args_expr.kind = kind;
    args_expr.ty = ty;
    true
}

/// Rewrites a single local UDT/tuple argument by replacing the argument use with
/// the local initializer after removing the specialized callable field.
///
/// # Before
/// ```text
/// args = Var(local_udt)   // bound to (field0, callable, field2)
/// ```
/// # After
/// ```text
/// args = (field0, field2, captures...)   // callable field removed
/// ```
///
/// # Mutations
/// - Overwrites `args_id`'s `ExprKind` and `Ty` in place.
/// - Allocates capture `Expr` nodes through `assigner`.
fn rewrite_local_single_arg_nested(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    args_id: ExprId,
    field_path: &[usize],
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> bool {
    if field_path.len() == 1 {
        let mut remove_indices = FxHashSet::default();
        remove_indices.insert(field_path[0]);
        return rewrite_nested_arg_expr_remove_fields_as_payload(
            package,
            owner_callable,
            args_id,
            &remove_indices,
            captures,
            assigner,
        );
    }

    // Deep path (`field_path.len() >= 2`): the callable lives inside a nested
    // aggregate reached through a `Var`-bound local. Resolve the local's
    // initializer and deep-strip the callable field, then inline the
    // deep-stripped value at the call site. The original local binding becomes
    // dead and is removed by later cleanup, so it no longer retains an
    // arrow-typed field.
    let args_expr = package.get_expr(args_id).clone();
    let source_id = if let ExprKind::Var(Res::Local(local_var), _) = args_expr.kind
        && let Some(owner_callable) = owner_callable
        && let Some(init_expr_id) =
            find_local_init_expr_in_callable(package, owner_callable, local_var)
    {
        init_expr_id
    } else {
        // Non-`Var` / unresolved args fall back to the caller's existing
        // behavior (the inline-literal deep path is handled there).
        return false;
    };

    let Some((kind, ty)) = build_removed_nested_expr_data(package, source_id, field_path, assigner)
    else {
        return false;
    };

    if captures.is_empty() {
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = kind;
        args_mut.ty = ty;
        return true;
    }

    // Capture-carrying deep local: wrap the deep-stripped payload and append the
    // closure captures (capture-LAST flatten), mirroring the len == 1 handling in
    // `rewrite_nested_arg_expr_remove_fields_as_payload`. The payload is a FRESH
    // expr id, so the rewritten arg never references itself; that avoids the
    // self-referential `Tuple([args_id, ...])` cycle the plain fallback in
    // `rewrite_single_arg_nested` would otherwise build for a non-`Tuple` `Var`
    // arg carrying captures.
    let payload_id = assigner.next_expr();
    package.exprs.insert(
        payload_id,
        Expr {
            id: payload_id,
            span: args_expr.span,
            ty: ty.clone(),
            kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();
    let mut elements = vec![payload_id];
    elements.extend(capture_ids);
    let mut tys = vec![ty];
    tys.extend(capture_tys);
    let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
    args_mut.kind = ExprKind::Tuple(elements);
    args_mut.ty = Ty::Tuple(tys);
    true
}

/// Builds replacement expression data for a call-argument aggregate after the
/// top-level callable fields have been removed.
///
/// Before, the tuple or struct represented by `expr_id` still contains the
/// callable-valued fields selected by `remove_indices`. After, the returned
/// `ExprKind`/`Ty` pair describes the same aggregate with those fields removed,
/// collapsed when only one element remains, and widened with any closure
/// captures that must become explicit call arguments.
fn remove_top_level_field_from_expr_data(
    package: &mut Package,
    expr_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    remove_top_level_field_from_expr_data_with_exprs(
        package,
        expr_id,
        remove_indices,
        &FxHashSet::default(),
        captures,
        assigner,
    )
}

/// Builds replacement expression data for a call-argument aggregate after
/// removing the callable fields at `remove_indices` or `remove_expr_ids` and
/// appending capture expressions.
///
/// Recurses through a `Call` to its argument tuple, and handles both tuple and
/// struct aggregates. Returns `None` for a shape it does not rewrite.
fn remove_top_level_field_from_expr_data_with_exprs(
    package: &mut Package,
    expr_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    remove_expr_ids: &FxHashSet<ExprId>,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let expr = package.get_expr(expr_id).clone();
    let mut remaining = match &expr.kind {
        ExprKind::Call(_, args_id) => {
            return remove_top_level_field_from_expr_data_with_exprs(
                package,
                *args_id,
                remove_indices,
                remove_expr_ids,
                captures,
                assigner,
            );
        }
        ExprKind::Tuple(elements) => elements
            .iter()
            .enumerate()
            .filter(|(idx, expr_id)| {
                !remove_indices.contains(idx) && !remove_expr_ids.contains(expr_id)
            })
            .map(|(_, &expr_id)| expr_id)
            .collect::<Vec<_>>(),
        ExprKind::Struct(_, _, fields) => fields
            .iter()
            .filter_map(|field| match &field.field {
                Field::Path(path)
                    if path
                        .indices
                        .first()
                        .is_none_or(|idx| !remove_indices.contains(idx))
                        && !remove_expr_ids.contains(&field.value) =>
                {
                    Some(field.value)
                }
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => return None,
    };

    remaining.extend(allocate_capture_exprs(
        package, expr.span, captures, assigner,
    ));

    Some(build_expr_data_from_elements(package, remaining))
}

/// Builds the `ExprKind` and `Ty` for a tuple of the given elements, collapsing
/// an empty list to `Unit` and a single element to itself rather than a
/// one-tuple.
fn build_expr_data_from_elements(package: &Package, elements: Vec<ExprId>) -> (ExprKind, Ty) {
    match elements.as_slice() {
        [] => (ExprKind::Tuple(Vec::new()), Ty::UNIT),
        [single] => {
            let expr = package.get_expr(*single);
            (expr.kind.clone(), expr.ty.clone())
        }
        _ => {
            let tys = elements
                .iter()
                .map(|&expr_id| package.get_expr(expr_id).ty.clone())
                .collect();
            (ExprKind::Tuple(elements), Ty::Tuple(tys))
        }
    }
}

/// Builds replacement expression data for a call-argument aggregate after
/// removing the callable-valued field reachable at `field_path`, without
/// mutating any existing expression node.
///
/// This is the deep (`field_path.len() >= 1`) analogue of
/// [`remove_top_level_field_from_expr_data`]. For each path segment it unwraps a
/// UDT-constructor `Call(ctor, args)` to its argument aggregate, descends into
/// the selected tuple element, and rebuilds each intermediate tuple with a
/// freshly allocated `Expr` for the stripped child while sharing the untouched
/// sibling expression ids unchanged.
///
/// # Before (`field_path = [0, 0]`, `expr = Config(OpBox(callable, 1), 5)`)
/// ```text
/// Config(OpBox(callable, 1), 5)
/// ```
/// # After
/// ```text
/// ((1), 5)   // the callable at Inner[0] removed, OpBox collapsed to its Id
/// ```
///
/// Returns `None` for a `Struct` literal or an out-of-range index. A
/// `Struct`-literal-bound deep local is intentionally left to decline: analysis
/// cannot resolve a concrete callable through a `Struct` literal, so such a call
/// site is already reported as `Qsc.Defunctionalize.DynamicCallable` upstream of
/// this rewrite and never reaches here. Extending this helper to recurse through
/// `Struct` fields would therefore be dead code unless analysis is separately
/// taught to resolve concrete callables through struct literals.
fn build_removed_nested_expr_data(
    package: &mut Package,
    expr_id: ExprId,
    field_path: &[usize],
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let expr = package.get_expr(expr_id).clone();

    // Unwrap a UDT-constructor `Call(ctor, args)` to its argument aggregate.
    if let ExprKind::Call(_, inner_args_id) = &expr.kind {
        return build_removed_nested_expr_data(package, *inner_args_id, field_path, assigner);
    }

    let (&index, rest) = field_path.split_first()?;

    let ExprKind::Tuple(elements) = &expr.kind else {
        // `Struct` literals (and any other aggregate form) decline here; see the
        // doc comment above for why this is a correct, bounded decline.
        return None;
    };
    if index >= elements.len() {
        return None;
    }
    let elements = elements.clone();

    if rest.is_empty() {
        // Terminal segment: drop the selected element, collapsing/retyping via
        // the shared element builder.
        let remaining: Vec<ExprId> = elements
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != index)
            .map(|(_, &id)| id)
            .collect();
        return Some(build_expr_data_from_elements(package, remaining));
    }

    // Deeper segment: recursively strip the nested aggregate at `index`, then
    // splice a FRESH `Expr` for it back into a rebuilt tuple so the original
    // nested nodes are left untouched.
    let (child_kind, child_ty) =
        build_removed_nested_expr_data(package, elements[index], rest, assigner)?;
    let child_span = package.get_expr(elements[index]).span;
    let child_id = assigner.next_expr();
    package.exprs.insert(
        child_id,
        Expr {
            id: child_id,
            span: child_span,
            ty: child_ty,
            kind: child_kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    let mut new_elements = elements;
    new_elements[index] = child_id;
    let new_tys: Vec<Ty> = new_elements
        .iter()
        .map(|&id| package.get_expr(id).ty.clone())
        .collect();
    Some((ExprKind::Tuple(new_elements), Ty::Tuple(new_tys)))
}

/// Rewrites a single-parameter call's args expression after the callable
/// argument has been removed.
///
/// Before, `args_id` evaluates to the callable argument itself. After, it
/// evaluates to `()` for a plain global callee or to `(captures...)` when the
/// rewritten direct call must thread closure captures explicitly.
fn rewrite_single_arg_root(
    package: &mut Package,
    args_id: ExprId,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) {
    let args_expr = package
        .exprs
        .get(args_id)
        .expect("args expr not found")
        .clone();

    if captures.is_empty() {
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = ExprKind::Tuple(Vec::new());
        args_mut.ty = Ty::UNIT;
    } else if captures.len() == 1 {
        // A single capture flattens to a scalar arg expression, matching the
        // single-element-flatten convention in `remove_callable_param`.
        let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
        let single = package
            .exprs
            .get(capture_ids[0])
            .expect("capture expr not found")
            .clone();
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = single.kind;
        args_mut.ty = single.ty;
    } else {
        let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
        let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = ExprKind::Tuple(capture_ids);
        args_mut.ty = Ty::Tuple(capture_tys);
    }
}

/// Removes the callable argument at `path` from a tuple-valued args expression
/// in place.
///
/// Before, the tuple nesting rooted at `expr_id` still matches the original
/// higher-order callable input. After, the selected element is removed, empty
/// tuples become unit, and one-element tuples collapse so the remaining shape
/// matches the specialized callee's input.
fn remove_element_at_path(package: &mut Package, expr_id: ExprId, path: &[usize]) {
    if path.is_empty() {
        return;
    }
    let expr = package.exprs.get(expr_id).expect("expr not found").clone();

    if path.len() == 1 {
        if let ExprKind::Tuple(elements) = &expr.kind {
            let new_elements: Vec<ExprId> = elements
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != path[0])
                .map(|(_, &id)| id)
                .collect();
            let new_tys: Vec<Ty> = if let Ty::Tuple(tys) = &expr.ty {
                tys.iter()
                    .enumerate()
                    .filter(|(i, _)| *i != path[0])
                    .map(|(_, t)| t.clone())
                    .collect()
            } else {
                Vec::new()
            };

            if new_elements.len() == 1 {
                // Flatten single-element tuple.
                let single = package
                    .exprs
                    .get(new_elements[0])
                    .expect("expr not found")
                    .clone();
                let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
                expr_mut.kind = single.kind;
                expr_mut.ty = single.ty;
            } else if new_elements.is_empty() {
                let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
                expr_mut.kind = ExprKind::Tuple(Vec::new());
                expr_mut.ty = Ty::UNIT;
            } else {
                let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
                expr_mut.kind = ExprKind::Tuple(new_elements);
                expr_mut.ty = Ty::Tuple(new_tys);
            }
        }
    } else if let ExprKind::Tuple(elements) = &expr.kind {
        let inner_id = elements[path[0]];
        remove_element_at_path(package, inner_id, &path[1..]);
        // Update the outer tuple's type for the modified inner element.
        let inner_expr = package.exprs.get(inner_id).expect("expr not found");
        let inner_ty = inner_expr.ty.clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
        if let Ty::Tuple(ref mut tys) = expr_mut.ty {
            tys[path[0]] = inner_ty;
        }
    }
}

/// Materializes the capture operands that must be appended to rewritten call
/// arguments.
///
/// Before, each capture is represented only by analysis metadata: an optional
/// existing `ExprId` and the local it denotes. After, every capture has a
/// concrete `ExprId` that can be spliced into a tuple, reusing the recorded
/// expression when possible and otherwise synthesizing `Var(Local(_))` nodes.
fn allocate_capture_exprs(
    package: &mut Package,
    span: Span,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> Vec<ExprId> {
    if captures.is_empty() {
        return Vec::new();
    }

    let mut ids = Vec::with_capacity(captures.len());

    for capture in captures {
        if let Some(expr_id) = capture.expr {
            if capture.caller_substitutions.is_empty() {
                ids.push(expr_id);
            } else {
                // The capture's initializer is a producer-scope compound
                // literal whose inner leaves reference the producing function's
                // parameters. Deep-clone it into caller scope, rebinding each
                // recorded producer leaf to its caller-scope argument, so no
                // unbound producer local is spliced into the caller.
                let substitutions: FxHashMap<LocalVarId, ExprId> =
                    capture.caller_substitutions.iter().copied().collect();
                let rebound = clone_capture_literal_with_substitutions(
                    package,
                    expr_id,
                    &substitutions,
                    assigner,
                );
                ids.push(rebound);
            }
            continue;
        }

        let new_id = assigner.next_expr();
        let new_expr = Expr {
            id: new_id,
            span,
            ty: capture.ty.clone(),
            kind: ExprKind::Var(Res::Local(capture.var), Vec::new()),
            exec_graph_range: EMPTY_EXEC_RANGE,
        };
        package.exprs.insert(new_id, new_expr);
        ids.push(new_id);
    }

    ids
}

/// Deep-clones a producer-scope compound-literal capture into caller scope,
/// rebinding its inner producer-parameter leaves to caller-scope arguments.
///
/// Before, the literal's inner `Var(Res::Local(_))` leaves reference the
/// producing function's parameters, which are unbound in the caller. After,
/// every safe, referentially-transparent node (struct/tuple/array constructors,
/// pure `function` calls, binary/unary operators, field and index accessors,
/// index/field updates, and ranges) is re-allocated with a fresh `ExprId`, each
/// producer leaf recorded in `substitutions` is replaced by the caller-scope
/// argument expression bound to that parameter at the call site (reused as-is,
/// not re-cloned), and every other leaf is cloned verbatim, so the
/// reconstructed literal is rooted entirely in caller-scope values. The set of
/// kinds recursed here must stay harmonized with
/// `collect_compound_capture_substitutions` and the residual-leak decline guard
/// in analysis; a non-pure `Call` (operation callee) is intentionally excluded
/// so its call is not relocated.
#[allow(clippy::too_many_lines)]
fn clone_capture_literal_with_substitutions(
    package: &mut Package,
    expr_id: ExprId,
    substitutions: &FxHashMap<LocalVarId, ExprId>,
    assigner: &mut Assigner,
) -> ExprId {
    let expr = package.get_expr(expr_id).clone();

    // A substituted producer-parameter leaf resolves directly to its already
    // caller-scope argument expression, which is reused unchanged.
    if let ExprKind::Var(Res::Local(var), _) = &expr.kind
        && let Some(&caller_expr) = substitutions.get(var)
    {
        return caller_expr;
    }

    let new_kind = match &expr.kind {
        ExprKind::Tuple(elements) => {
            let mut cloned = Vec::with_capacity(elements.len());
            for &elem in elements {
                cloned.push(clone_capture_literal_with_substitutions(
                    package,
                    elem,
                    substitutions,
                    assigner,
                ));
            }
            ExprKind::Tuple(cloned)
        }
        ExprKind::Array(elements) => {
            let mut cloned = Vec::with_capacity(elements.len());
            for &elem in elements {
                cloned.push(clone_capture_literal_with_substitutions(
                    package,
                    elem,
                    substitutions,
                    assigner,
                ));
            }
            ExprKind::Array(cloned)
        }
        ExprKind::ArrayLit(elements) => {
            let mut cloned = Vec::with_capacity(elements.len());
            for &elem in elements {
                cloned.push(clone_capture_literal_with_substitutions(
                    package,
                    elem,
                    substitutions,
                    assigner,
                ));
            }
            ExprKind::ArrayLit(cloned)
        }
        ExprKind::ArrayRepeat(value, size) => {
            let value =
                clone_capture_literal_with_substitutions(package, *value, substitutions, assigner);
            let size =
                clone_capture_literal_with_substitutions(package, *size, substitutions, assigner);
            ExprKind::ArrayRepeat(value, size)
        }
        ExprKind::Struct(name, copy, fields) => {
            let copy = (*copy).map(|copy_id| {
                clone_capture_literal_with_substitutions(package, copy_id, substitutions, assigner)
            });
            let mut cloned_fields = Vec::with_capacity(fields.len());
            for field in fields {
                let value = clone_capture_literal_with_substitutions(
                    package,
                    field.value,
                    substitutions,
                    assigner,
                );
                cloned_fields.push(FieldAssign {
                    span: field.span,
                    field: field.field.clone(),
                    value,
                });
            }
            ExprKind::Struct(*name, copy, cloned_fields)
        }
        // A `Call` is rebuilt only when its callee is a pure `function`; a
        // non-pure operation call falls to the verbatim `other => other` arm so
        // its call is never relocated (analysis declines such a capture to a
        // dynamic call site before rewrite runs, so this arm is effectively
        // unreached for operation callees).
        ExprKind::Call(callee, arg) if callee_is_pure_function(package, *callee) => {
            let callee =
                clone_capture_literal_with_substitutions(package, *callee, substitutions, assigner);
            let arg =
                clone_capture_literal_with_substitutions(package, *arg, substitutions, assigner);
            ExprKind::Call(callee, arg)
        }
        ExprKind::BinOp(op, lhs, rhs) => {
            let lhs =
                clone_capture_literal_with_substitutions(package, *lhs, substitutions, assigner);
            let rhs =
                clone_capture_literal_with_substitutions(package, *rhs, substitutions, assigner);
            ExprKind::BinOp(*op, lhs, rhs)
        }
        ExprKind::UnOp(op, operand) => {
            let operand = clone_capture_literal_with_substitutions(
                package,
                *operand,
                substitutions,
                assigner,
            );
            ExprKind::UnOp(*op, operand)
        }
        ExprKind::Field(base, field) => {
            let base =
                clone_capture_literal_with_substitutions(package, *base, substitutions, assigner);
            ExprKind::Field(base, field.clone())
        }
        ExprKind::Index(base, index) => {
            let base =
                clone_capture_literal_with_substitutions(package, *base, substitutions, assigner);
            let index =
                clone_capture_literal_with_substitutions(package, *index, substitutions, assigner);
            ExprKind::Index(base, index)
        }
        ExprKind::UpdateIndex(container, index, value) => {
            let container = clone_capture_literal_with_substitutions(
                package,
                *container,
                substitutions,
                assigner,
            );
            let index =
                clone_capture_literal_with_substitutions(package, *index, substitutions, assigner);
            let value =
                clone_capture_literal_with_substitutions(package, *value, substitutions, assigner);
            ExprKind::UpdateIndex(container, index, value)
        }
        ExprKind::UpdateField(record, field, value) => {
            let record =
                clone_capture_literal_with_substitutions(package, *record, substitutions, assigner);
            let value =
                clone_capture_literal_with_substitutions(package, *value, substitutions, assigner);
            ExprKind::UpdateField(record, field.clone(), value)
        }
        ExprKind::Range(start, step, end) => {
            let clone_opt = |package: &mut Package,
                             part: Option<ExprId>,
                             assigner: &mut Assigner| {
                part.map(|part| {
                    clone_capture_literal_with_substitutions(package, part, substitutions, assigner)
                })
            };
            let start = clone_opt(package, *start, assigner);
            let step = clone_opt(package, *step, assigner);
            let end = clone_opt(package, *end, assigner);
            ExprKind::Range(start, step, end)
        }
        _ => expr.kind.clone(),
    };

    let new_id = assigner.next_expr();
    let new_expr = Expr {
        id: new_id,
        span: expr.span,
        ty: expr.ty.clone(),
        kind: new_kind,
        exec_graph_range: EMPTY_EXEC_RANGE,
    };
    package.exprs.insert(new_id, new_expr);
    new_id
}

/// Reports whether a `Call`'s callee resolves to a pure `function`.
///
/// A Q# `function` is guaranteed side-effect free and its arrow type cannot
/// bear functors, so it is referentially transparent and its call may be
/// relocated or duplicated into caller-scope argument construction. An
/// `operation` may have observable side effects and ordering, so its call must
/// not be relocated. This mirrors the identical gate applied in analysis so the
/// collect / clone / decline-guard sites recurse the same set of `Call` nodes.
fn callee_is_pure_function(package: &Package, callee: ExprId) -> bool {
    matches!(
        &package.get_expr(callee).ty,
        Ty::Arrow(arrow) if arrow.kind == CallableKind::Function
    )
}

/// Computes the callee arrow type that corresponds to a rewritten direct call.
///
/// Before, the callee type still includes the callable-valued parameter from
/// the original higher-order signature. After, the returned arrow removes that
/// input slot and appends any closure capture types so the callee type matches
/// the rewritten args expression.
fn build_specialized_callee_ty(
    package: &Package,
    callee_id: ExprId,
    input_path: &[usize],
    concrete: &ConcreteCallable,
) -> Option<Ty> {
    let callee_expr = package.get_expr(callee_id);
    let Ty::Arrow(ref arrow) = callee_expr.ty else {
        return None;
    };

    let captures = match concrete {
        ConcreteCallable::Closure { captures, .. } => captures.as_slice(),
        _ => &[],
    };

    let new_input = remove_ty_at_path(package, &arrow.input, input_path, captures);
    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}

/// Builds the arrow type of the specialized callee after the callable value at
/// `input_path` is removed from the input and capture types are appended.
///
/// Returns `None` when the callee is not arrow-typed.
fn build_specialized_nested_payload_callee_ty(
    package: &Package,
    callee_id: ExprId,
    input_path: &[usize],
    captures: &[CapturedVar],
) -> Option<Ty> {
    let callee_expr = package.get_expr(callee_id);
    let Ty::Arrow(ref arrow) = callee_expr.ty else {
        return None;
    };

    let payload = remove_ty_at_path(package, &arrow.input, input_path, &[]);
    let new_input = if captures.is_empty() {
        payload
    } else {
        let mut tys = vec![payload];
        tys.extend(captures.iter().map(|capture| capture.ty.clone()));
        Ty::Tuple(tys)
    };

    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}

/// Removes the type at a given path from a tuple type and appends capture types.
/// For single-element paths, removes the element at that index from the tuple.
/// For multi-element paths, navigates into nested tuples to remove the element.
/// An empty path removes the entire root value. If the type is not a tuple,
/// it represents the single callable-param case, so the result is either Unit
/// or a tuple of capture types.
fn remove_ty_at_path(package: &Package, ty: &Ty, path: &[usize], captures: &[CapturedVar]) -> Ty {
    let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();

    if path.is_empty() {
        return match capture_tys.len() {
            0 => Ty::UNIT,
            // A single capture flattens to a scalar arrow input, matching the
            // single-element-flatten convention in `remove_callable_param`.
            1 => capture_tys.into_iter().next().expect("one capture type"),
            _ => Ty::Tuple(capture_tys),
        };
    }

    let ty = resolve_udt_ty(package, ty);

    if path.len() == 1 {
        if let Ty::Tuple(tys) = &ty {
            let mut remaining: Vec<Ty> = tys
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != path[0])
                .map(|(_, t)| t.clone())
                .collect();
            remaining.extend(capture_tys);
            if remaining.is_empty() {
                Ty::UNIT
            } else if remaining.len() == 1 && captures.is_empty() {
                // Flatten single-element tuple to match pattern flattening.
                remaining
                    .into_iter()
                    .next()
                    .expect("single element should exist")
            } else {
                Ty::Tuple(remaining)
            }
        } else {
            // Single param is the callable — result is captures or unit.
            if capture_tys.is_empty() {
                Ty::UNIT
            } else {
                Ty::Tuple(capture_tys)
            }
        }
    } else {
        // Navigate deeper: modify the sub-type at path[0], then rebuild.
        if let Ty::Tuple(tys) = &ty {
            let mut new_tys = tys.clone();
            // Remove nested element without captures at inner level.
            new_tys[path[0]] = remove_ty_at_path(package, &tys[path[0]], &path[1..], &[]);
            // Append captures at the top level.
            new_tys.extend(capture_tys);
            Ty::Tuple(new_tys)
        } else {
            // Single param that is a tuple type — remove from within.
            let modified = remove_ty_at_path(package, &ty, &path[1..], &[]);
            if capture_tys.is_empty() {
                modified
            } else {
                let mut all = vec![modified];
                all.extend(capture_tys);
                Ty::Tuple(all)
            }
        }
    }
}

/// Builds the tuple type for the args expression after removing the element at
/// `param_path` and appending capture types.
fn build_tuple_ty_without_path(
    package: &Package,
    ty: &Ty,
    param_path: &[usize],
    captures: &[CapturedVar],
) -> Ty {
    remove_ty_at_path(package, ty, param_path, captures)
}

/// Reports whether `ty` contains an arrow type after resolving through any UDT
/// wrappers.
fn local_ty_contains_arrow_through_udts(package: &Package, ty: &Ty) -> bool {
    ty_contains_arrow(&resolve_udt_ty(package, ty))
}

/// Resolves a type through user-defined-type wrappers to its underlying
/// structural type, recursing into tuples, arrays, and arrow inputs and
/// outputs.
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
        Ty::Arrow(arrow) => Ty::Arrow(Box::new(Arrow {
            kind: arrow.kind,
            input: Box::new(resolve_udt_ty(package, &arrow.input)),
            output: Box::new(resolve_udt_ty(package, &arrow.output)),
            functors: arrow.functors,
        })),
        _ => ty.clone(),
    }
}

/// Computes the argument-tuple path that locates `param` at the given call
/// site, accounting for any functor shell around the callee.
fn callable_param_input_path(
    package: &Package,
    callee_id: ExprId,
    param: &CallableParam,
) -> Vec<usize> {
    let (_, outer_functor) = peel_body_functors(package, callee_id);
    let uses_tuple = param.hof_input_is_tuple;
    super::build_param_input_path(uses_tuple, param, outer_functor)
}

/// Replaces `callee_id` with a reference to the specialized callable while
/// preserving any outer functor shell.
///
/// Before, the callee subtree still refers to the original higher-order item.
/// After, the same root `ExprId` evaluates the specialized callable and carries
/// the rewritten arrow type expected by the direct-call args.
fn rewrite_specialized_callee(
    package: &mut Package,
    callee_id: ExprId,
    spec_item_id: ItemId,
    new_callee_ty: Option<Ty>,
    assigner: &mut Assigner,
) {
    let (_, outer_functor) = peel_body_functors(package, callee_id);
    let callee_expr = package.get_expr(callee_id).clone();
    let callee_ty = new_callee_ty.unwrap_or_else(|| callee_expr.ty.clone());

    rewrite_item_callee_with_functor(
        package,
        callee_id,
        spec_item_id,
        callee_ty,
        outer_functor,
        assigner,
    );
}

/// Overwrites `callee_id` so it names `item_id`, rebuilding any `Adj`/`Ctl`
/// wrapper chain around a fresh inner `Var` expression.
///
/// # Before
/// ```text
/// Ctl(Adj(Var(original_item))) : OldArrow
/// ```
/// # After
/// ```text
/// Ctl(Adj(Var(specialized_item))) : NewArrow
/// ```
///
/// # Mutations
/// - Rewrites `callee_id`'s `ExprKind` and `Ty` in place.
/// - Allocates fresh inner `Var` and functor-wrapper `Expr` nodes through
///   `assigner` when the functor chain is non-trivial.
fn rewrite_item_callee_with_functor(
    package: &mut Package,
    callee_id: ExprId,
    item_id: ItemId,
    callee_ty: Ty,
    functor: FunctorApp,
    assigner: &mut Assigner,
) {
    let callee_expr = package.get_expr(callee_id).clone();

    if !functor.adjoint && functor.controlled == 0 {
        let expr = package
            .exprs
            .get_mut(callee_id)
            .expect("callee expr not found");
        expr.kind = ExprKind::Var(Res::Item(item_id), Vec::new());
        expr.ty = callee_ty;
        return;
    }

    // Rebuild the functor wrapper chain from the inside out, then copy the
    // outermost node back into the original callee slot.
    let mut current_id = assigner.next_expr();
    package.exprs.insert(
        current_id,
        Expr {
            id: current_id,
            span: callee_expr.span,
            ty: callee_ty.clone(),
            kind: ExprKind::Var(Res::Item(item_id), Vec::new()),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    if functor.adjoint {
        let adj_id = assigner.next_expr();
        package.exprs.insert(
            adj_id,
            Expr {
                id: adj_id,
                span: callee_expr.span,
                ty: callee_ty.clone(),
                kind: ExprKind::UnOp(UnOp::Functor(Functor::Adj), current_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current_id = adj_id;
    }

    for _ in 0..functor.controlled {
        let ctl_id = assigner.next_expr();
        package.exprs.insert(
            ctl_id,
            Expr {
                id: ctl_id,
                span: callee_expr.span,
                ty: callee_ty.clone(),
                kind: ExprKind::UnOp(UnOp::Functor(Functor::Ctl), current_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current_id = ctl_id;
    }

    let outermost_kind = package
        .exprs
        .get(current_id)
        .expect("specialized callee wrapper should exist")
        .kind
        .clone();
    let expr = package
        .exprs
        .get_mut(callee_id)
        .expect("callee expr not found");
    expr.kind = outermost_kind;
    expr.ty = callee_ty;
}

/// Restricts a mixed branch-split candidate set to the single parameter
/// position whose callable is selected by the loop index.
///
/// A per-row group can mix a dispatched parameter, the same position carrying
/// two or more candidates such as `[H, X]` at slot 0, with siblings at other
/// positions such as a global `Y` at slot 1. Only the dispatched parameter
/// should drive the index dispatch; the siblings stay in the original call
/// arguments and are threaded as runtime values by each specialized leaf. When
/// exactly one position has two or more candidates and at least one sibling
/// exists, the result keeps only that position's entries. The input is returned
/// unchanged when no position is dispatched, or when two or more positions form
/// a genuine product of dispatched parameters.
fn restrict_to_dispatched_parameter<'a>(
    entries: &[HofDispatchTarget<'a>],
) -> Vec<HofDispatchTarget<'a>> {
    let mut candidates_per_position: FxHashMap<(usize, Vec<usize>), usize> = FxHashMap::default();
    for entry in entries {
        *candidates_per_position
            .entry((entry.0.top_level_param, entry.0.field_path.clone()))
            .or_default() += 1;
    }
    let dispatched_positions: Vec<(usize, Vec<usize>)> = candidates_per_position
        .iter()
        .filter(|(_, count)| **count >= 2)
        .map(|(position, _)| position.clone())
        .collect();
    if dispatched_positions.len() != 1 {
        return entries.to_vec();
    }
    let kept_position = &dispatched_positions[0];
    let filtered: Vec<HofDispatchTarget> = entries
        .iter()
        .filter(|entry| (entry.0.top_level_param, entry.0.field_path.clone()) == *kept_position)
        .copied()
        .collect();
    if filtered.len() == entries.len() {
        entries.to_vec()
    } else {
        filtered
    }
}

/// Rewrites a call site that has multiple callee candidates (from branch-split
/// analysis) into an if/elif/else dispatch chain where each branch calls the
/// appropriate specialization.
///
/// # Before
/// ```text
/// Call(Var(hof), (callable_arg, other_args))
/// ```
/// # After
/// ```text
/// if cond_0 { Call(Var(spec_0), args_0) }
/// elif cond_1 { Call(Var(spec_1), args_1) }
/// else { Call(Var(spec_default), args_default) }
/// ```
///
/// # Mutations
/// - Replaces `call_expr_id`'s `ExprKind` with the dispatch chain.
/// - Allocates per-branch `Call`, callee, args, and `If` `Expr` nodes
///   through `assigner`.
fn branch_split_rewrite(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    entries: &[HofDispatchTarget],
    constants: &[(&CallSite, &CallableParam)],
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    let orig_call = package.get_expr(call_expr_id).clone();
    let ExprKind::Call(orig_callee_id, orig_args_id) = orig_call.kind else {
        return;
    };
    let span = orig_call.span;
    let result_ty = orig_call.ty.clone();

    // A per-row mixed group, built when defunctionalization cannot prove a
    // combined specialization, can mix a dispatched parameter, the same
    // position carrying two or more empty-condition candidates such as `[H, X]`
    // at slot 0, with a sibling at a different position such as a global `Y` at
    // slot 1. The sibling is not selected by the loop index, so it must not
    // enter the index-dispatch candidate set; if it does,
    // `synthesize_callsite_index_dispatch` cannot locate it among the indexed
    // callables, aborts, and the call collapses to a single default, dropping
    // the real candidates.
    //
    // Restrict the candidate set to the single dispatched parameter. The
    // sibling at the other position stays in the original arguments, so each
    // specialized leaf threads it as a runtime argument in its original
    // position through `create_branch_call`, which removes only the dispatch
    // slot, preserving call order.
    let restricted = restrict_to_dispatched_parameter(entries);
    let entries: &[HofDispatchTarget] = &restricted;

    let Some((conditioned, default_entry)) = partition_branch_split_targets(
        package,
        expr_owner_lookup,
        call_expr_id,
        entries,
        span,
        assigner,
    ) else {
        return;
    };

    if conditioned.is_empty() {
        // Single effective entry. For a combined group the entry's spec id is a
        // per-candidate combined spec whose input pattern removes every member
        // slot, so the single-slot `create_branch_call` would mis-shape the
        // call. Bail instead so the fixpoint surfaces an honest diagnostic
        // rather than emitting wrong QIR.
        if !constants.is_empty() {
            return;
        }
        // Single effective entry — use normal rewrite.
        rewrite_one(
            package,
            package_id,
            default_entry.0,
            default_entry.2,
            default_entry.1,
            expr_owner_lookup,
            assigner,
        );
        return;
    }

    install_branch_split_dispatch(
        package,
        package_id,
        call_expr_id,
        orig_callee_id,
        orig_args_id,
        span,
        &result_ty,
        conditioned,
        default_entry,
        constants,
        assigner,
    );
}

/// Partitions branch-split dispatch entries into conditioned targets and a
/// single default target for the `else` arm.
///
/// Entries carrying an explicit condition become conditioned targets; the first
/// empty-condition entry becomes the default. When no entry carries a condition
/// but more than one candidate exists, a synthetic index dispatch is derived
/// from the callee's runtime-selected index via
/// [`synthesize_callsite_index_dispatch`]. If no default is found, the last
/// conditioned target is promoted to serve as the `else` arm. Returns `None`
/// when there is no entry to dispatch at all.
fn partition_branch_split_targets<'a>(
    package: &mut Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    call_expr_id: ExprId,
    entries: &[HofDispatchTarget<'a>],
    span: Span,
    assigner: &mut Assigner,
) -> Option<(Vec<ConditionedHofTarget<'a>>, HofDispatchTarget<'a>)> {
    let mut conditioned: Vec<ConditionedHofTarget> = Vec::new();
    let mut default: Option<HofDispatchTarget> = None;
    for &entry in entries {
        if entry.0.condition.is_empty() {
            if default.is_none() {
                default = Some(entry);
            }
        } else {
            conditioned.push((entry, entry.0.condition.clone()));
        }
    }

    if conditioned.is_empty()
        && entries.len() > 1
        && let Some((synthetic_conditioned, default_idx)) = synthesize_callsite_index_dispatch(
            package,
            expr_owner_lookup,
            call_expr_id,
            entries,
            span,
            assigner,
        )
    {
        conditioned = synthetic_conditioned
            .into_iter()
            .map(|(entry_idx, condition)| (entries[entry_idx], vec![condition]))
            .collect();
        default = Some(entries[default_idx]);
    }

    // Must have a default for the else branch; steal last conditioned if needed.
    let default_entry = if let Some(d) = default {
        d
    } else {
        conditioned.pop()?.0
    };
    Some((conditioned, default_entry))
}

/// Builds the if/elif/else dispatch chain for a branch-split rewrite and
/// installs it in place of the original call expression.
///
/// Each conditioned target and the default target are lowered to a specialized
/// call — via [`create_branch_call`] for a per-row group or
/// [`create_combined_branch_call`] when constant sibling parameters are present
/// — and assembled into a nested `if` tree by [`build_branch_tree`]. The
/// resulting dispatch expression's kind and type overwrite `call_expr_id`.
#[allow(clippy::too_many_arguments)]
fn install_branch_split_dispatch<'a>(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    orig_callee_id: ExprId,
    orig_args_id: ExprId,
    span: Span,
    result_ty: &Ty,
    conditioned: Vec<ConditionedHofTarget<'a>>,
    default_entry: HofDispatchTarget<'a>,
    constants: &[(&CallSite, &CallableParam)],
    assigner: &mut Assigner,
) {
    // Clone original callee and args expressions before modifications.
    let orig_callee = package.get_expr(orig_callee_id).clone();
    let orig_args = package.get_expr(orig_args_id).clone();

    let mut build_call = |package: &mut Package, assigner: &mut Assigner, (cs, spec_id, param)| {
        if constants.is_empty() {
            create_branch_call(
                package,
                package_id,
                &orig_callee,
                &orig_args,
                span,
                result_ty,
                cs,
                param,
                spec_id,
                assigner,
            )
        } else {
            create_combined_branch_call(
                package,
                &orig_callee,
                &orig_args,
                span,
                result_ty,
                cs,
                param,
                constants,
                spec_id,
                assigner,
            )
        }
    };
    let dispatch_id = build_branch_tree(
        package,
        span,
        result_ty,
        conditioned,
        default_entry,
        assigner,
        &mut build_call,
    );

    // Replace the original call expression with the dispatch chain.
    let dispatch = package
        .exprs
        .get(dispatch_id)
        .expect("dispatch expr should exist")
        .clone();
    let orig = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr should exist");
    orig.kind = dispatch.kind;
    orig.ty = dispatch.ty;
}

/// Creates a single branch's specialised call expression, returning its
/// [`ExprId`]. The callee is replaced with the specialization, the callable
/// argument is removed from the args, and closure captures are appended.
///
/// # Before
/// ```text
/// (no expression — branch does not yet exist)
/// ```
/// # After
/// ```text
/// Call(Var(spec_item), (remaining_args, captures...)) : result_ty
/// ```
///
/// # Mutations
/// - Allocates callee, args, and call `Expr` nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
fn create_branch_call(
    package: &mut Package,
    _package_id: PackageId,
    orig_callee: &Expr,
    orig_args: &Expr,
    span: Span,
    result_ty: &Ty,
    call_site: &CallSite,
    param: &CallableParam,
    spec_store_id: StoreItemId,
    assigner: &mut Assigner,
) -> ExprId {
    let spec_item_id = ItemId {
        package: spec_store_id.package,
        item: spec_store_id.item,
    };

    // Specialised callee type.
    let input_path = callable_param_input_path(package, orig_callee.id, param);
    let new_callee_ty = build_specialized_callee_ty_from_expr(
        package,
        orig_callee,
        &input_path,
        &call_site.callable_arg,
    );
    let callee_id = alloc_specialized_callee_expr(
        package,
        orig_callee,
        spec_item_id,
        &new_callee_ty.unwrap_or_else(|| orig_callee.ty.clone()),
        assigner,
    );

    // Build args: remove callable param + append captures.
    let captures = match &call_site.callable_arg {
        ConcreteCallable::Closure { captures, .. } => {
            resolve_rewrite_captures(package, call_site.arg_expr_id, captures)
        }
        _ => Vec::new(),
    };
    let (args_kind, args_ty) =
        build_branch_args_data(package, orig_args, &input_path, &captures, span, assigner);

    let args_id = assigner.next_expr();
    package.exprs.insert(
        args_id,
        Expr {
            id: args_id,
            span,
            ty: args_ty,
            kind: args_kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    // Call expression.
    let call_id = assigner.next_expr();
    package.exprs.insert(
        call_id,
        Expr {
            id: call_id,
            span,
            ty: result_ty.clone(),
            kind: ExprKind::Call(callee_id, args_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    call_id
}

/// Creates a single dispatch leaf for the combined branch-split path, returning
/// its [`ExprId`]. The leaf calls one per-candidate combined specialization
/// formed as `[dispatch candidate] + single-valued siblings`. Every member's
/// argument slot is removed from the call's argument tuple in one pass and each
/// closure member's captures are appended in ascending parameter order,
/// mirroring [`rewrite_multi`] so the leaf's argument shape matches the combined
/// spec's input pattern. The single-valued producer closures are therefore
/// inlined into the leaf in this pass, consumed before any later-iteration body
/// clearing.
///
/// # Mutations
/// - Allocates callee, args, and call `Expr` nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
fn create_combined_branch_call(
    package: &mut Package,
    orig_callee: &Expr,
    orig_args: &Expr,
    span: Span,
    result_ty: &Ty,
    candidate: &CallSite,
    candidate_param: &CallableParam,
    constants: &[(&CallSite, &CallableParam)],
    spec_store_id: StoreItemId,
    assigner: &mut Assigner,
) -> ExprId {
    let spec_item_id = ItemId {
        package: spec_store_id.package,
        item: spec_store_id.item,
    };

    // Order members ascending by parameter position so the removed slots and
    // the appended captures line up with the combined spec's input pattern.
    let mut members: Vec<(&CallSite, &CallableParam)> = Vec::with_capacity(constants.len() + 1);
    members.push((candidate, candidate_param));
    members.extend(constants.iter().copied());
    members.sort_by(|a, b| {
        a.1.top_level_param
            .cmp(&b.1.top_level_param)
            .then_with(|| a.1.field_path.cmp(&b.1.field_path))
    });

    // Collect the slots to remove and resolve every closure's captures in
    // ascending parameter order, mirroring `rewrite_multi`.
    let uses_tuple_input = members.first().is_none_or(|(cs, _)| cs.hof_input_is_tuple);
    let mut remove_indices: Vec<usize> = Vec::with_capacity(members.len());
    let mut captures: Vec<CapturedVar> = Vec::new();
    for (cs, _param) in &members {
        let remove_idx = if uses_tuple_input {
            cs.top_level_param
        } else {
            *cs.field_path.first().unwrap_or(&cs.top_level_param)
        };
        remove_indices.push(remove_idx);
        if let ConcreteCallable::Closure {
            captures: member_captures,
            ..
        } = &cs.callable_arg
        {
            captures.extend(resolve_rewrite_captures(
                package,
                cs.arg_expr_id,
                member_captures,
            ));
        }
    }

    // Combined specialized callee type and expression.
    let new_callee_ty =
        build_specialized_multi_callee_ty(package, orig_callee.id, &remove_indices, &captures);
    let callee_id = alloc_specialized_callee_expr(
        package,
        orig_callee,
        spec_item_id,
        &new_callee_ty.unwrap_or_else(|| orig_callee.ty.clone()),
        assigner,
    );

    // Build the leaf argument tuple: drop every member slot and append captures.
    let (args_kind, args_ty) = if uses_tuple_input {
        build_combined_branch_args_data(
            package,
            orig_args,
            &remove_indices,
            &captures,
            span,
            assigner,
        )
    } else {
        build_combined_nested_branch_args_data(
            package,
            orig_args,
            &remove_indices,
            &captures,
            span,
            assigner,
        )
    };
    let args_id = assigner.next_expr();
    package.exprs.insert(
        args_id,
        Expr {
            id: args_id,
            span,
            ty: args_ty,
            kind: args_kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let call_id = assigner.next_expr();
    package.exprs.insert(
        call_id,
        Expr {
            id: call_id,
            span,
            ty: result_ty.clone(),
            kind: ExprKind::Call(callee_id, args_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    call_id
}

/// Builds the `(ExprKind, Ty)` for a combined dispatch leaf's argument tuple:
/// every element at `remove_indices` is dropped and the resolved capture
/// expressions are appended, flattening to a scalar only when a single element
/// survives and no captures are appended, matching the combined spec's input
/// pattern in [`remove_tys_at_indices`]. Surviving element expressions are
/// reused, mirroring the single-slot [`build_branch_args_data`] branch-split
/// behavior.
fn build_combined_branch_args_data(
    package: &mut Package,
    orig_args: &Expr,
    remove_indices: &[usize],
    captures: &[CapturedVar],
    span: Span,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    let new_ty = remove_tys_at_indices(package, &orig_args.ty, remove_indices, captures);
    match &orig_args.kind {
        ExprKind::Tuple(elements) => {
            let remove: FxHashSet<usize> = remove_indices.iter().copied().collect();
            let mut new_elements: Vec<ExprId> = elements
                .iter()
                .enumerate()
                .filter(|(i, _)| !remove.contains(i))
                .map(|(_, &id)| id)
                .collect();
            let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
            new_elements.extend(capture_ids);
            if new_elements.len() == 1 && captures.is_empty() {
                let single_id = new_elements[0];
                let single_expr = package.exprs.get(single_id).expect("expr not found");
                (single_expr.kind.clone(), single_expr.ty.clone())
            } else {
                (ExprKind::Tuple(new_elements), new_ty)
            }
        }
        // A combined multi-argument HOF always has a tuple argument; fall back
        // to the original kind defensively.
        _ => (orig_args.kind.clone(), new_ty),
    }
}

/// Builds the argument expression data for one branch of a combined
/// multi-argument dispatch, removing the callable fields at `remove_indices` and
/// appending capture expressions as a trailing group.
fn build_combined_nested_branch_args_data(
    package: &mut Package,
    orig_args: &Expr,
    remove_indices: &[usize],
    captures: &[CapturedVar],
    span: Span,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    let remove: FxHashSet<usize> = remove_indices.iter().copied().collect();
    if let Some((payload_kind, payload_ty)) =
        remove_top_level_field_from_expr_data(package, orig_args.id, &remove, &[], assigner)
    {
        if captures.is_empty() {
            return (payload_kind, payload_ty);
        }

        let payload_id = assigner.next_expr();
        package.exprs.insert(
            payload_id,
            Expr {
                id: payload_id,
                span,
                ty: payload_ty.clone(),
                kind: payload_kind,
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );

        let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
        let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();
        let mut elements = vec![payload_id];
        elements.extend(capture_ids);
        let mut tys = vec![payload_ty];
        tys.extend(capture_tys);
        return (ExprKind::Tuple(elements), Ty::Tuple(tys));
    }

    let new_ty = remove_nested_top_level_fields_from_ty(package, &orig_args.ty, &remove);
    if captures.is_empty() {
        (orig_args.kind.clone(), new_ty)
    } else {
        let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
        let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();
        let mut elements = match &orig_args.kind {
            ExprKind::Tuple(elements) => elements.clone(),
            _ => vec![orig_args.id],
        };
        elements.extend(capture_ids);
        let mut tys = vec![new_ty];
        tys.extend(capture_tys);
        (ExprKind::Tuple(elements), Ty::Tuple(tys))
    }
}

/// Resolves the defining expressions for the captures referenced in a
/// direct-call rewrite, using the combined call-argument and block-scope
/// lookups.
fn resolve_rewrite_captures(
    package: &Package,
    arg_expr_id: ExprId,
    captures: &[CapturedVar],
) -> Vec<CapturedVar> {
    captures
        .iter()
        .map(|capture| {
            let mut resolved = capture.clone();
            if resolved.expr.is_none() {
                resolved.expr = resolve_capture_expr_from_arg(package, arg_expr_id, capture.var);
            }
            resolved
        })
        .collect()
}

/// Resolves a capture expression by inspecting the call's argument tuple,
/// used when the capture was passed in directly at the call site.
fn resolve_capture_expr_from_arg(
    package: &Package,
    arg_expr_id: ExprId,
    capture_var: LocalVarId,
) -> Option<ExprId> {
    let expr = package.get_expr(arg_expr_id);
    match &expr.kind {
        ExprKind::Block(block_id) => {
            resolve_capture_expr_from_block(package, *block_id, capture_var)
        }
        ExprKind::If(_, body, otherwise) => {
            resolve_capture_expr_from_arg(package, *body, capture_var).or_else(|| {
                otherwise.and_then(|else_id| {
                    resolve_capture_expr_from_arg(package, else_id, capture_var)
                })
            })
        }
        ExprKind::UnOp(_, inner) => resolve_capture_expr_from_arg(package, *inner, capture_var),
        _ => None,
    }
}

/// Resolves a capture expression by looking up the capture's defining
/// binding in the enclosing block's local-expression map.
fn resolve_capture_expr_from_block(
    package: &Package,
    block_id: qsc_fir::fir::BlockId,
    capture_var: LocalVarId,
) -> Option<ExprId> {
    let block = package.get_block(block_id);
    let mut bindings = FxHashMap::default();

    for stmt_id in &block.stmts {
        let stmt = package.get_stmt(*stmt_id);
        if let StmtKind::Local(_, pat_id, init_expr_id) = &stmt.kind {
            collect_block_local_exprs(package, *pat_id, *init_expr_id, &mut bindings);
        }
    }

    let mut current = capture_var;
    for _ in 0..32 {
        let &expr_id = bindings.get(&current)?;
        let expr = package.get_expr(expr_id);
        if let ExprKind::Var(Res::Local(next_var), _) = &expr.kind
            && *next_var != current
            && bindings.contains_key(next_var)
        {
            current = *next_var;
            continue;
        }
        return Some(expr_id);
    }

    None
}

/// Builds a `LocalVarId => ExprId` map from a block's statements, capturing
/// the initializer expressions for every immutable local binding.
fn collect_block_local_exprs(
    package: &Package,
    pat_id: qsc_fir::fir::PatId,
    init_expr_id: ExprId,
    bindings: &mut FxHashMap<LocalVarId, ExprId>,
) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            bindings.insert(ident.id, init_expr_id);
        }
        PatKind::Discard => {}
        PatKind::Tuple(pats) => {
            for &sub_pat_id in pats {
                collect_block_local_exprs(package, sub_pat_id, init_expr_id, bindings);
            }
        }
    }
}

/// Builds the args `ExprKind` and `Ty` for a branch call by removing the
/// callable parameter and appending closure captures.
fn build_branch_args_data(
    package: &mut Package,
    orig_args: &Expr,
    input_path: &[usize],
    captures: &[CapturedVar],
    span: Span,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    if input_path.is_empty() {
        // Single-param HOF: the argument IS the callable param.
        if captures.is_empty() {
            (ExprKind::Tuple(Vec::new()), Ty::UNIT)
        } else {
            let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
            let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
            (ExprKind::Tuple(capture_ids), Ty::Tuple(capture_tys))
        }
    } else if matches!(orig_args.kind, ExprKind::Tuple(_)) {
        match &orig_args.kind {
            ExprKind::Tuple(elements) => {
                if input_path.len() == 1 {
                    let mut new_elements: Vec<ExprId> = elements
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != input_path[0])
                        .map(|(_, &id)| id)
                        .collect();
                    let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
                    new_elements.extend(capture_ids);
                    let new_ty =
                        build_tuple_ty_without_path(package, &orig_args.ty, input_path, captures);
                    // Flatten single-element tuple to match the flattening in
                    // rewrite_args_remove_tuple_element so the partial evaluator
                    // receives a scalar expression rather than a malformed 1-tuple.
                    if new_elements.len() == 1 && captures.is_empty() {
                        let single_id = new_elements[0];
                        let single_expr = package.exprs.get(single_id).expect("expr not found");
                        (single_expr.kind.clone(), single_expr.ty.clone())
                    } else {
                        (ExprKind::Tuple(new_elements), new_ty)
                    }
                } else {
                    let new_ty =
                        build_tuple_ty_without_path(package, &orig_args.ty, input_path, captures);
                    let mut new_kind = orig_args.kind.clone();
                    if let ExprKind::Tuple(ref mut elems) = new_kind {
                        if let Some(outer_elem_id) = elems.get(input_path[0]).copied() {
                            remove_element_at_path(package, outer_elem_id, &input_path[1..]);
                        }
                        let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
                        elems.extend(capture_ids);
                    }
                    (new_kind, new_ty)
                }
            }
            _ => (
                orig_args.kind.clone(),
                build_tuple_ty_without_path(package, &orig_args.ty, input_path, captures),
            ),
        }
    } else if input_path.len() == 1 {
        let param_index = input_path[0];
        match &orig_args.kind {
            ExprKind::Tuple(elements) => {
                let mut new_elements: Vec<ExprId> = elements
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != param_index)
                    .map(|(_, &id)| id)
                    .collect();
                let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
                new_elements.extend(capture_ids);
                let new_ty =
                    build_tuple_ty_without_path(package, &orig_args.ty, input_path, captures);
                // Flatten single-element tuple to match the flattening in
                // rewrite_args_remove_tuple_element so the partial evaluator
                // receives a scalar expression rather than a malformed 1-tuple.
                if new_elements.len() == 1 && captures.is_empty() {
                    let single_id = new_elements[0];
                    let single_expr = package.exprs.get(single_id).expect("expr not found");
                    (single_expr.kind.clone(), single_expr.ty.clone())
                } else {
                    (ExprKind::Tuple(new_elements), new_ty)
                }
            }
            _ => (
                orig_args.kind.clone(),
                build_tuple_ty_without_path(package, &orig_args.ty, input_path, captures),
            ),
        }
    } else {
        // Nested path: rebuild both the args type and expression with the
        // nested element removed.
        remove_element_at_path(package, orig_args.id, input_path);
        let new_ty = build_tuple_ty_without_path(package, &orig_args.ty, input_path, captures);
        let modified_args = package.get_expr(orig_args.id).clone();
        let new_kind = if captures.is_empty() {
            modified_args.kind
        } else {
            let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
            if let ExprKind::Tuple(mut elems) = modified_args.kind {
                elems.extend(capture_ids);
                ExprKind::Tuple(elems)
            } else {
                let mut elems = vec![orig_args.id];
                elems.extend(capture_ids);
                ExprKind::Tuple(elems)
            }
        };
        (new_kind, new_ty)
    }
}

/// Allocates a fresh `Var` expression that references a specialized callable
/// item, returning its new `ExprId`. Delegates to
/// [`alloc_item_callee_expr_with_functor`], which inserts the `Var` and any
/// functor-wrapper `Expr` nodes.
fn alloc_specialized_callee_expr(
    package: &mut Package,
    orig_callee: &Expr,
    spec_item_id: ItemId,
    callee_ty: &Ty,
    assigner: &mut Assigner,
) -> ExprId {
    let (_, outer_functor) = peel_body_functors(package, orig_callee.id);
    alloc_item_callee_expr_with_functor(
        package,
        orig_callee.span,
        spec_item_id,
        callee_ty,
        outer_functor,
        assigner,
    )
}

/// Allocates a fresh callee expression that wraps an item reference with the
/// requested functor applications (`Adj` and/or `Ctl` layers). Inserts one
/// `Var` `Expr` plus zero or more functor-wrapper `Expr` nodes through
/// `assigner`.
fn alloc_item_callee_expr_with_functor(
    package: &mut Package,
    span: Span,
    item_id: ItemId,
    callee_ty: &Ty,
    functor: FunctorApp,
    assigner: &mut Assigner,
) -> ExprId {
    let mut current_id = assigner.next_expr();
    package.exprs.insert(
        current_id,
        Expr {
            id: current_id,
            span,
            ty: callee_ty.clone(),
            kind: ExprKind::Var(Res::Item(item_id), Vec::new()),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    if functor.adjoint {
        let adj_id = assigner.next_expr();
        package.exprs.insert(
            adj_id,
            Expr {
                id: adj_id,
                span,
                ty: callee_ty.clone(),
                kind: ExprKind::UnOp(UnOp::Functor(Functor::Adj), current_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current_id = adj_id;
    }

    for _ in 0..functor.controlled {
        let ctl_id = assigner.next_expr();
        package.exprs.insert(
            ctl_id,
            Expr {
                id: ctl_id,
                span,
                ty: callee_ty.clone(),
                kind: ExprKind::UnOp(UnOp::Functor(Functor::Ctl), current_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current_id = ctl_id;
    }

    current_id
}

/// Allocates a new `ExprKind::If` expression and inserts it into the package
/// through `assigner`.
fn alloc_if_expr(
    package: &mut Package,
    span: Span,
    result_ty: &Ty,
    cond_id: ExprId,
    true_id: ExprId,
    false_id: ExprId,
    assigner: &mut Assigner,
) -> ExprId {
    let if_id = assigner.next_expr();
    package.exprs.insert(
        if_id,
        Expr {
            id: if_id,
            span,
            ty: result_ty.clone(),
            kind: ExprKind::If(cond_id, true_id, Some(false_id)),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    if_id
}

/// Builds a nested `if`/`else` tree selecting one of several specialized calls,
/// reconstructed from the flat outermost-first guard lists by contiguous
/// leading-guard factoring. Each condition `ExprId` is referenced exactly once,
/// so a side-effecting condition runs at most once per dynamic path.
///
/// `conditioned` holds the guarded entries in front-to-back (outermost-first)
/// order, each paired with its guard list; `default_entry` supplies the
/// outermost `else` call. The `build_call` closure materializes the
/// specialized `Call` expression for a given entry, receiving the package and
/// assigner so it can allocate nodes without `&mut` aliasing; it is passed by
/// `&mut` so the same closure threads through the recursion.
///
/// Returns the `ExprId` of the root dispatch expression; callers write its
/// `kind`/`ty` back into the original call expression.
fn build_branch_tree<E: Copy>(
    package: &mut Package,
    span: Span,
    result_ty: &Ty,
    conditioned: Vec<(E, Vec<ExprId>)>,
    default_entry: E,
    assigner: &mut Assigner,
    build_call: &mut impl FnMut(&mut Package, &mut Assigner, E) -> ExprId,
) -> ExprId {
    // Base case: nothing left to guard -> emit the (outer) default call.
    if conditioned.is_empty() {
        return build_call(package, assigner, default_entry);
    }

    // The leading guard `g` of the first entry is this level's `if` condition
    // (the first entry is non-empty; the default arm is split out by the
    // caller). Take the maximal contiguous leading run whose first guard == `g`.
    let g = conditioned[0].1[0];
    let run_len = conditioned
        .iter()
        .take_while(|(_, guards)| guards.first().copied() == Some(g))
        .count();

    let mut iter = conditioned.into_iter();
    let run: Vec<(E, Vec<ExprId>)> = (&mut iter).take(run_len).collect();
    let rest: Vec<(E, Vec<ExprId>)> = iter.collect();

    // Contiguity invariant: the leading guard `g` must not reappear in a later
    // run. Holds for every lattice the current joins produce; assert in debug
    // builds to catch a future join that violates it.
    debug_assert!(
        !rest
            .iter()
            .any(|(_, guards)| guards.first().copied() == Some(g)),
        "build_branch_tree: leading guard reappears in a non-contiguous run"
    );

    // Strip the shared leading `g` from every run entry; the entry that strips
    // to empty is the group's inner default (exactly one is guaranteed by the
    // one-default-per-group invariant below).
    let mut inner_default: Option<E> = None;
    let mut inner_conditioned: Vec<(E, Vec<ExprId>)> = Vec::with_capacity(run.len());
    for (entry, mut guards) in run {
        guards.remove(0);
        if guards.is_empty() {
            inner_default = Some(entry);
        } else {
            inner_conditioned.push((entry, guards));
        }
    }

    // One-default-per-group invariant: a leading-guard run must contain exactly
    // one entry that strips to empty. Holds for every lattice the current joins
    // produce; assert in debug builds, and in release fall through to the outer
    // default rather than crashing on user code.
    debug_assert!(
        inner_default.is_some(),
        "build_branch_tree: leading-guard run has no inner default"
    );
    let inner_default = inner_default.unwrap_or(default_entry);

    // Then subtree: recurse on the stripped run, defaulting to the group else.
    let then_id = build_branch_tree(
        package,
        span,
        result_ty,
        inner_conditioned,
        inner_default,
        assigner,
        build_call,
    );

    // Else subtree: recurse on the rest, keeping the outer default.
    let else_id = build_branch_tree(
        package,
        span,
        result_ty,
        rest,
        default_entry,
        assigner,
        build_call,
    );

    // `g` is referenced once here -- no AndL, no re-evaluation.
    alloc_if_expr(package, span, result_ty, g, then_id, else_id, assigner)
}

/// Builds the specialised callee type from a saved callee expression snapshot.
fn build_specialized_callee_ty_from_expr(
    package: &Package,
    callee_expr: &Expr,
    input_path: &[usize],
    concrete: &ConcreteCallable,
) -> Option<Ty> {
    let Ty::Arrow(ref arrow) = callee_expr.ty else {
        return None;
    };
    let captures = match concrete {
        ConcreteCallable::Closure { captures, .. } => captures.as_slice(),
        _ => &[],
    };
    let new_input = remove_ty_at_path(package, &arrow.input, input_path, captures);
    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}
