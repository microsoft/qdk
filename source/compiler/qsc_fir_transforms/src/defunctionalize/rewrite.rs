// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rewrite phase of the defunctionalization pass.
//!
//! For eligible call sites where a higher-order function is invoked with a concrete
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
//! - **Dead-local cleanup** — removes eligible unused callable-typed locals
//!   after direct-call rewrites. Other arrow-typed residues may remain
//!   (see the `prune_*` and
//!   `remove_dead_callable_local_*` helpers).
//!
//! # Notes
//!
//! - Controlled input substitution is shared with specialization through
//!   [`super::apply_target_input_at_control_path`].

use super::captures::{CaptureDestination, allocate_capture_exprs, captures_belong_to_destination};
use super::types::{
    AnalysisResult, CallSite, CallableParam, CaptureScope, CapturedVar, ConcreteCallable,
    DirectCallSite, ScopedLocal, SpecKey, peel_body_functors,
};
use super::{
    apply_target_input_at_control_path, build_combined_spec_key, build_combined_spec_key_for_group,
    build_spec_key, dispatched_precedes_detached_static, is_combined_eligible,
    partition_mixed_branch_split, ty_contains_arrow,
};
use crate::fir_builder::{
    alloc_bin_op_expr, alloc_block, alloc_block_expr, alloc_call_expr, alloc_expr, alloc_expr_stmt,
    alloc_functor_wrapped_expr, alloc_int_lit, alloc_semi_stmt, alloc_unit_expr,
};
use crate::walk_utils::{
    DirectChild, UseClass, classify_block_use, expr_is_safe_to_discard,
    expr_is_safe_to_discard_with_total_foreign, expr_is_side_effect_free, for_each_direct_child,
};
use qsc_data_structures::functors::FunctorApp;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::PackageSpan;
use qsc_fir::fir::{
    BinOp, Block, BlockId, CallableImpl, Expr, ExprId, ExprKind, Field, FieldAssign, FieldPath,
    ItemId, ItemKind, Lit, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup,
    Pat, PatId, PatKind, Res, Stmt, StmtId, StmtKind, StoreItemId,
};
use qsc_fir::ty::{Arrow, Prim, Ty};
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

/// A resolved HOF dispatch target: the `(call site, specialization item, param)`
/// triple produced during branch-split rewriting.
type HofDispatchTarget<'a> = (&'a CallSite, StoreItemId, &'a CallableParam);

/// A HOF dispatch target paired with its guard list (empty list = default branch).
///
/// Guards are stored outermost-first; shared leading guards are factored into
/// nested `if` expressions by [`build_branch_tree`].
type ConditionedHofTarget<'a> = (HofDispatchTarget<'a>, Vec<ExprId>);
struct IndexDispatchPlan {
    conditioned: Vec<(usize, ExprId)>,
    default_index: usize,
    bounds_check: ExprId,
}

struct RewriteOnePlan {
    callee_id: ExprId,
    original_args_id: ExprId,
    input_path: Vec<usize>,
    controlled_layers: usize,
    captures: Vec<CapturedVar>,
    destination: CaptureScope,
    new_callee_ty: Option<Ty>,
}

#[derive(Clone, Copy)]
struct CaptureWriterContext {
    owner_callable: Option<LocalItemId>,
    destination: CaptureScope,
}

/// Rewrites call sites in the target package so that higher-order calls are
/// replaced with direct calls to their specialized counterparts.
///
/// For eligible call sites with a matching specialization in `spec_map`:
/// - The callee expression is replaced with a reference to the specialized
///   callable.
/// - The callable argument is removed from the argument tuple.
/// - If the callable argument was a closure, its runtime captures are
///   appended as extra arguments.
/// - The callee expression's type is updated to reflect the new signature.
pub(super) fn rewrite(
    package: &mut Package,
    package_id: PackageId,
    analysis: &AnalysisResult,
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    specialized_items: &FxHashSet<StoreItemId>,
    assigner: &mut Assigner,
    total_foreign: &FxHashSet<ItemId>,
) {
    let expr_owner_lookup = build_expr_owner_lookup(package, package_id, specialized_items);
    let mut rewritten_callable_arg_locals = FxHashSet::default();

    // Source-array locals for closure callable-arrays that a higher-order call
    // forwards (directly, through a struct-literal field, or through `let`
    // aliases) and fully consumes. The bare-`Var` recorder cannot see the
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

        // Specialize declined to build a spec for this shape, so there is
        // nothing to rewrite to; leave the callable first-class.
        if dispatched_precedes_detached_static(group) {
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
        package_id,
        &rewritten_callable_arg_locals,
        &hof_consumed_source_arrays,
        total_foreign,
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
    expr_owner_lookup: &ExprOwnerLookup,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    hof_consumed_source_arrays: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    assigner: &mut Assigner,
) {
    // Every row in this group came from the same higher-order call, so they all
    // point at the same HOF. Grab it and look up the one specialization that was
    // generated for this exact combination of callable arguments. If we never
    // built that specialization, there is nothing to rewrite to, so bail out.
    let hof_item_id = group[0].hof_item_id;
    let spec_key = build_combined_spec_key_for_group(hof_item_id, group);
    let Some(&spec_store_id) = spec_map.get(&spec_key) else {
        return;
    };
    let hof_store_id = StoreItemId::from((hof_item_id.package, hof_item_id.item));

    // Recover the exact parameter per row, then order the members
    // ascending by parameter position so the rewritten argument tuple
    // lines up with the specialize-side combined input pattern.
    //
    // Each row knows which slot of the call it filled (its top-level position
    // plus the path into any nested tuple/struct). Use that to find the exact
    // parameter it corresponds to on the HOF. If any row's parameter can't be
    // matched, we can't safely rewrite the whole call, so give up entirely.
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
    // Put the rows back in the order the parameters appear in the HOF signature,
    // so the new argument list we build matches the specialization's expected
    // input shape one-for-one.
    members.sort_by(|a, b| {
        a.1.top_level_param
            .cmp(&b.1.top_level_param)
            .then_with(|| a.1.field_path.cmp(&b.1.field_path))
    });

    if members.iter().any(|(call_site, _)| {
        dispatch_source_is_indexed(
            package,
            expr_owner_lookup,
            call_expr_id,
            call_site.arg_expr_id,
        ) && !dispatch_source_index_is_statically_in_bounds(
            package,
            expr_owner_lookup,
            call_expr_id,
            call_site.arg_expr_id,
        )
    }) {
        return;
    }

    // Before we change the call, note the callable arguments that are about to
    // be baked into the specialization. Recording them (and any backing arrays
    // that get fully consumed) lets a later cleanup pass delete the now-unused
    // local variables that used to hold those callables.
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
    // Finally, actually rewrite the call. If one of the arguments is an array of
    // callables that itself needs to be rebuilt element-by-element, take the
    // specialized array path; otherwise do the plain multi-argument rewrite that
    // just drops the callable args and calls the specialization directly.
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
/// Returns `true` when the resolved group is handed to [`branch_split_rewrite`],
/// which may still decline the rewrite. Returns `false` when the group is not
/// a mixed branch-split or a required combined spec or parameter cannot be
/// resolved, allowing the caller to try the per-row path.
#[allow(clippy::too_many_arguments)]
fn rewrite_mixed_branch_split_group(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    group: &[&CallSite],
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    expr_owner_lookup: &ExprOwnerLookup,
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
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn rewrite_per_row_group(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    group: &[&CallSite],
    spec_map: &FxHashMap<SpecKey, StoreItemId>,
    param_lookup: &FxHashMap<StoreItemId, &CallableParam>,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    expr_owner_lookup: &ExprOwnerLookup,
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
        let Some(rewrite_plan) = plan_rewrite_one(package, call_site, param, expr_owner_lookup)
        else {
            return;
        };
        let indexed_source = dispatch_source_is_indexed(
            package,
            expr_owner_lookup,
            call_expr_id,
            call_site.arg_expr_id,
        );
        let statically_in_bounds = dispatch_source_index_is_statically_in_bounds(
            package,
            expr_owner_lookup,
            call_expr_id,
            call_site.arg_expr_id,
        );
        let bounds_check = if statically_in_bounds {
            None
        } else {
            synthesize_callsite_index_dispatch(
                package,
                package_id,
                expr_owner_lookup,
                call_expr_id,
                &entries,
                assigner,
            )
            .map(|plan| {
                let ExprKind::Call(callee, args) = package.get_expr(call_expr_id).kind else {
                    unreachable!("dispatch owner must remain a call");
                };
                let path = callable_param_input_path(package, callee, entries[0].2);
                let prefix = materialize_argument_prefix(package, args, &path, &[], assigner);
                prepend_evaluations(package, plan.bounds_check, prefix, assigner);
                plan.bounds_check
            })
        };
        if indexed_source && !statically_in_bounds && bounds_check.is_none() {
            return;
        }
        let mut pending_rewritten_locals = FxHashSet::default();
        collect_rewritten_callable_arg_local(
            package,
            expr_owner_lookup,
            call_site.call_expr_id,
            call_site.arg_expr_id,
            &mut pending_rewritten_locals,
        );
        if rewrite_one(
            package,
            package_id,
            call_site,
            param,
            spec_store_id,
            rewrite_plan,
            expr_owner_lookup,
            assigner,
        ) {
            rewritten_callable_arg_locals.extend(pending_rewritten_locals);
            if let Some(bounds_check) = bounds_check {
                prepend_bounds_check_to_rewritten_call(
                    package,
                    assigner,
                    call_expr_id,
                    bounds_check,
                );
            }
        }
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

/// Groups direct call sites in this package's body by call expression and
/// rewrites those whose captures belong to the destination scope.
///
/// A lone unconditional site is rewritten in place by [`rewrite_direct_call`];
/// a group with multiple sites (or a conditional lone site) is lowered to a
/// condition-indexed dispatch via [`branch_split_direct_call_rewrite`].
fn rewrite_direct_call_sites(
    package: &mut Package,
    package_id: PackageId,
    analysis: &AnalysisResult,
    expr_owner_lookup: &ExprOwnerLookup,
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
            if !direct_call_rewrite_is_valid(package, entries[0], expr_owner_lookup) {
                continue;
            }
            let ExprKind::Call(callee_id, _) = package.get_expr(entries[0].call_expr_id).kind
            else {
                continue;
            };
            let indexed_source = dispatch_source_is_indexed(
                package,
                expr_owner_lookup,
                entries[0].call_expr_id,
                callee_id,
            );
            let bounds_check = synthesize_direct_index_dispatch(
                package,
                package_id,
                expr_owner_lookup,
                entries[0].call_expr_id,
                entries,
                assigner,
            )
            .map(|plan| plan.bounds_check);
            if indexed_source && bounds_check.is_none() {
                continue;
            }
            rewrite_direct_call(
                package,
                package_id,
                entries[0],
                expr_owner_lookup,
                rewritten_callable_arg_locals,
                assigner,
            );
            if let Some(bounds_check) = bounds_check {
                prepend_bounds_check_to_rewritten_call(
                    package,
                    assigner,
                    entries[0].call_expr_id,
                    bounds_check,
                );
            }
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

fn dispatch_source_is_indexed(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
    owner_expr_id: ExprId,
    dispatch_expr_id: ExprId,
) -> bool {
    resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, dispatch_expr_id)
        .is_some_and(|source| matches!(package.get_expr(source).kind, ExprKind::Index(..)))
}

fn dispatch_source_index_is_statically_in_bounds(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
    owner_expr_id: ExprId,
    dispatch_expr_id: ExprId,
) -> bool {
    resolve_index_dispatch_source(package, expr_owner_lookup, owner_expr_id, dispatch_expr_id)
        .is_some_and(|(_, index_expr_id, callables)| {
            matches!(
                package.get_expr(strip_transparent_block_expr(package, index_expr_id)).kind,
                ExprKind::Lit(Lit::Int(index))
                    if usize::try_from(index).is_ok_and(|index| index < callables.len())
            )
        })
}

fn direct_call_rewrite_is_valid(
    package: &Package,
    direct_call_site: &DirectCallSite,
    expr_owner_lookup: &ExprOwnerLookup,
) -> bool {
    let ExprKind::Call(callee_id, _) = package.get_expr(direct_call_site.call_expr_id).kind else {
        return false;
    };
    let captures = match &direct_call_site.callable {
        ConcreteCallable::Closure { captures, .. } => {
            resolve_rewrite_captures(package, callee_id, captures)
        }
        ConcreteCallable::Global { .. } => {
            resolve_rewrite_captures(package, callee_id, direct_call_site.captures.as_slice())
        }
        ConcreteCallable::Dynamic => Vec::new(),
    };
    expr_owner_lookup
        .scope(&direct_call_site.call_expr_id)
        .is_some_and(|destination| captures_belong_to_destination(destination, &captures))
}

/// Rewrites a `DirectCallSite` whose callee was resolved to a specific
/// concrete callable into a direct invocation of that callable, overwriting
/// the callee expression and recording its local for later cleanup.
///
/// When the call site carries a `def_span` (recorded for a collapsed
/// identity closure), the call expression's span is re-stamped to it so
/// diagnostics point at the original lambda body rather than the wrapper.
fn rewrite_direct_call(
    package: &mut Package,
    package_id: PackageId,
    direct_call_site: &DirectCallSite,
    expr_owner_lookup: &ExprOwnerLookup,
    rewritten_callable_arg_locals: &mut FxHashSet<(LocalItemId, LocalVarId)>,
    assigner: &mut Assigner,
) {
    let call_expr = package.get_expr(direct_call_site.call_expr_id).clone();
    let ExprKind::Call(callee_id, args_id) = call_expr.kind else {
        return;
    };
    if let Some(span) = direct_call_site.def_span {
        let span = package.span(span);
        package
            .exprs
            .get_mut(direct_call_site.call_expr_id)
            .expect("expression should exist")
            .span = span;
    }
    let (_, outer_functor) = peel_body_functors(package, callee_id);
    let controlled_layers = usize::from(outer_functor.controlled);
    let captures = match &direct_call_site.callable {
        ConcreteCallable::Closure { captures, .. } => {
            resolve_rewrite_captures(package, callee_id, captures)
        }
        ConcreteCallable::Global { .. } => {
            resolve_rewrite_captures(package, callee_id, direct_call_site.captures.as_slice())
        }
        ConcreteCallable::Dynamic => Vec::new(),
    };
    let Some(destination) = expr_owner_lookup.scope(&direct_call_site.call_expr_id) else {
        return;
    };
    if !captures_belong_to_destination(destination, &captures) {
        return;
    }
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

    rewrite_direct_callee(
        package,
        package_id,
        callee_id,
        &direct_call_site.callable,
        &captures,
        controlled_layers,
        assigner,
    );
    // The target callable's declared input, used to shape the call arguments.
    // For a closure this is the (already capture-flattened) target signature;
    // for a directly-called lambda whose parameters live in a one-element tuple
    // it is the packaged one-tuple input.
    let target_input = match &direct_call_site.callable {
        ConcreteCallable::Closure { target, .. } => match &package.get_item(*target).kind {
            ItemKind::Callable(decl) => Some(package.get_pat(decl.input).ty.clone()),
            ItemKind::Ty(..) => None,
        },
        ConcreteCallable::Global { item_id, .. } if item_id.package == package_id => {
            direct_lambda_packaged_input(package, item_id.item)
        }
        ConcreteCallable::Global { .. } if !captures.is_empty() => Some(Ty::Tuple(
            captures
                .iter()
                .map(|capture| capture.ty.clone())
                .chain(std::iter::once(package.get_expr(args_id).ty.clone()))
                .collect(),
        )),
        _ => None,
    };
    if let Some(target_input) = target_input
        && (matches!(direct_call_site.callable, ConcreteCallable::Closure { .. })
            || package_direct_lambda
            || !captures.is_empty())
    {
        rewrite_direct_closure_args(
            package,
            args_id,
            destination,
            &captures,
            &target_input,
            controlled_layers,
            assigner,
        );
    }
}

/// Move invocation inside callee control flow before collecting callable facts.
/// Blocks retain their effects and capture bindings; branches select once before
/// arguments run. Each step consumes a block or conditional on the callee spine.
/// Each invocation receives a deep argument copy so later normalization cannot
/// share branch-local temporaries across alternatives. Returns whether new calls
/// were created and need capture normalization.
pub(super) fn normalize_direct_callee_control_flow(
    package: &mut Package,
    mut pending: Vec<ExprId>,
    assigner: &mut Assigner,
) -> bool {
    let mut changed = false;
    while let Some(id) = pending.pop() {
        let call = package.get_expr(id).clone();
        let ExprKind::Call(callee, args) = call.kind else {
            continue;
        };
        let callee_ty = package.get_expr(callee).ty.clone();
        let (base, functor) = peel_body_functors(package, callee);
        let (branches, prefix, condition) = match package.get_expr(base).kind {
            ExprKind::Block(block) => {
                let Some((&tail, prefix)) = package.get_block(block).stmts.split_last() else {
                    continue;
                };
                let StmtKind::Expr(value) = package.get_stmt(tail).kind else {
                    continue;
                };
                (vec![value], prefix.to_vec(), None)
            }
            ExprKind::If(condition, then, Some(otherwise)) => {
                (vec![then, otherwise], Vec::new(), Some(condition))
            }
            _ => continue,
        };

        let mut calls = Vec::new();
        for branch in branches {
            let value = package.get_expr(branch).clone();
            let callee = alloc_functor_wrapped_expr(
                package, assigner, value.kind, functor, &callee_ty, value.span,
            );
            let args = crate::cloner::clone_expr_within_package(package, args, assigner);
            let invocation =
                alloc_call_expr(package, assigner, callee, args, call.ty.clone(), call.span);
            pending.push(invocation);
            calls.push(invocation);
        }
        let kind = if let Some(condition) = condition {
            ExprKind::If(condition, calls[0], Some(calls[1]))
        } else {
            let mut statements = prefix;
            statements.push(alloc_expr_stmt(package, assigner, calls[0], call.span));
            ExprKind::Block(alloc_block(
                package, assigner, statements, call.ty, call.span,
            ))
        };
        package.exprs.get_mut(id).expect("call exists").kind = kind;
        changed = true;
    }
    changed
}

/// Rewrites a direct call whose callee has multiple possible concrete
/// values by synthesizing a condition-indexed dispatch that selects the
/// specialized callee matching the observed branch.
#[allow(clippy::too_many_lines)]
fn branch_split_direct_call_rewrite(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    entries: &[&DirectCallSite],
    expr_owner_lookup: &ExprOwnerLookup,
    assigner: &mut Assigner,
) {
    let orig_call = package.get_expr(call_expr_id).clone();
    let ExprKind::Call(orig_callee_id, orig_args_id) = orig_call.kind else {
        return;
    };
    let span = orig_call.span;
    let result_ty = orig_call.ty.clone();
    if !dispatch_source_has_discriminator(
        package,
        expr_owner_lookup,
        call_expr_id,
        orig_callee_id,
        entries
            .iter()
            .map(|entry| (&entry.callable, !entry.condition.is_empty())),
    ) {
        return;
    }
    let Some(destination) = expr_owner_lookup.scope(&call_expr_id) else {
        return;
    };
    for entry in entries {
        let capture_source = match &entry.callable {
            ConcreteCallable::Closure { captures, .. } => captures.as_slice(),
            ConcreteCallable::Global { .. } => entry.captures.as_slice(),
            ConcreteCallable::Dynamic => &[],
        };
        let captures = resolve_rewrite_captures(package, orig_callee_id, capture_source);
        if !captures_belong_to_destination(destination, &captures) {
            return;
        }
    }

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

    let mut bounds_check = None;
    if conditioned.is_empty()
        && !entries.is_empty()
        && let Some(plan) = synthesize_direct_index_dispatch(
            package,
            package_id,
            expr_owner_lookup,
            call_expr_id,
            entries,
            assigner,
        )
    {
        conditioned = plan
            .conditioned
            .into_iter()
            .map(|(entry_idx, condition)| (entries[entry_idx], vec![condition]))
            .collect();
        default = Some(entries[plan.default_index]);
        bounds_check = Some(plan.bounds_check);
    }
    if conditioned.is_empty() && entries.len() > 1 {
        return;
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
        if let Some(bounds_check) = bounds_check {
            prepend_bounds_check_to_rewritten_call(package, assigner, call_expr_id, bounds_check);
        }
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
            destination,
            entry,
            assigner,
        )
    };
    let mut dispatch_id = build_branch_tree(
        package,
        span,
        &result_ty,
        conditioned,
        default_entry,
        assigner,
        &mut build_call,
    );
    if let Some(bounds_check) = bounds_check {
        dispatch_id = prepend_bounds_check(
            package,
            assigner,
            bounds_check,
            dispatch_id,
            result_ty.clone(),
            span,
        );
    }

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
    expr_owner_lookup: &ExprOwnerLookup,
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
/// rewritten, instead of leaving an array of neutralized closure elements in a
/// reachable caller.
fn collect_hof_consumed_source_array(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
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
/// [`prune_dead_callable_arg_locals`] still decides whether removal is allowed, so
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

/// Returns `true` for an array whose element has an arrow at its root or under
/// tuple fields, after UDT expansion. Nested arrays remain opaque.
fn ty_is_callable_array(package: &Package, ty: &Ty) -> bool {
    matches!(resolve_udt_ty(package, ty), Ty::Array(elem) if ty_contains_arrow(&elem))
}

/// Plans index-dispatch guards and a default candidate for a HOF call site
/// that resolves to multiple callables via branch-split analysis.
fn synthesize_callsite_index_dispatch(
    package: &mut Package,
    package_id: PackageId,
    expr_owner_lookup: &ExprOwnerLookup,
    call_expr_id: ExprId,
    entries: &[HofDispatchTarget],
    assigner: &mut Assigner,
) -> Option<IndexDispatchPlan> {
    let callables = entries
        .iter()
        .map(|entry| entry.0.callable_arg.clone())
        .collect::<Vec<_>>();
    let plan = synthesize_index_dispatch_plan(
        package,
        package_id,
        expr_owner_lookup,
        (call_expr_id, entries.first()?.0.arg_expr_id),
        &callables,
        assigner,
    )?;
    Some(plan)
}

/// Saves tuple operands that precede a callable operand before dispatch or
/// captures move out of the argument tuple. Inline struct capture operands are
/// first bound in source order by the prepass. Discard safety alone does not
/// permit moving a local read across a later write.
fn materialize_argument_prefix(
    package: &mut Package,
    args: ExprId,
    input_path: &[usize],
    consumed: &[ExprId],
    assigner: &mut Assigner,
) -> Vec<StmtId> {
    let mut prefix = Vec::new();
    let mut operand = args;
    for &position in input_path {
        let ExprKind::Tuple(elements) = &package.get_expr(operand).kind else {
            break;
        };
        prefix.extend(elements.iter().take(position).copied());
        operand = elements[position];
    }
    let mut statements = Vec::new();
    for operand in prefix {
        if consumed.contains(&operand) {
            continue;
        }
        let expression = package.get_expr(operand).clone();
        if matches!(
            expression.kind,
            ExprKind::Lit(_) | ExprKind::Var(Res::Item(_), _)
        ) {
            continue;
        }
        let initializer = alloc_expr(
            package,
            assigner,
            expression.ty.clone(),
            expression.kind,
            expression.span,
        );
        let (local, statement) = crate::fir_builder::alloc_local_var(
            package,
            assigner,
            "argument",
            &expression.ty,
            initializer,
            Mutability::Immutable,
        );
        package
            .exprs
            .get_mut(operand)
            .expect("operand must exist")
            .kind = ExprKind::Var(Res::Local(local), Vec::new());
        statements.push(statement);
    }
    statements
}

fn prepend_evaluations(
    package: &mut Package,
    call: ExprId,
    mut statements: Vec<StmtId>,
    assigner: &mut Assigner,
) {
    if statements.is_empty() {
        return;
    }
    let expr = package.get_expr(call).clone();
    let value = alloc_expr(package, assigner, expr.ty.clone(), expr.kind, expr.span);
    statements.push(alloc_expr_stmt(package, assigner, value, expr.span));
    let block = alloc_block(package, assigner, statements, expr.ty, expr.span);
    package.exprs.get_mut(call).expect("call exists").kind = ExprKind::Block(block);
}

fn materialize_capture_operands(
    package: &mut Package,
    destination: CaptureScope,
    captures: &mut [CapturedVar],
    statements: &mut Vec<StmtId>,
    assigner: &mut Assigner,
) {
    for capture in captures {
        let Some(expression) = capture.expr else {
            continue;
        };
        if matches!(
            package.get_expr(expression).kind,
            ExprKind::Lit(_) | ExprKind::Var(Res::Item(_), _)
        ) {
            continue;
        }
        let span = package.get_expr(expression).span;
        let operand = allocate_capture_exprs(
            package,
            span,
            destination,
            std::slice::from_ref(capture),
            assigner,
        )[0];
        let (local, statement) = crate::fir_builder::alloc_local_var(
            package,
            assigner,
            "_.capture",
            &capture.ty,
            operand,
            Mutability::Immutable,
        );
        statements.push(statement);
        capture.expr = Some(crate::fir_builder::alloc_local_var_expr(
            package,
            assigner,
            local,
            capture.ty.clone(),
            span,
        ));
        capture.caller_substitutions.clear();
    }
}

/// Plans index-dispatch guards and a default candidate for a direct-call site
/// whose callee expression resolves to multiple concrete callables.
fn synthesize_direct_index_dispatch(
    package: &mut Package,
    package_id: PackageId,
    expr_owner_lookup: &ExprOwnerLookup,
    call_expr_id: ExprId,
    entries: &[&DirectCallSite],
    assigner: &mut Assigner,
) -> Option<IndexDispatchPlan> {
    let ExprKind::Call(callee_id, _) = package.get_expr(call_expr_id).kind else {
        return None;
    };
    let callables = entries
        .iter()
        .map(|entry| entry.callable.clone())
        .collect::<Vec<_>>();
    synthesize_index_dispatch_plan(
        package,
        package_id,
        expr_owner_lookup,
        (call_expr_id, callee_id),
        &callables,
        assigner,
    )
}

/// Unguarded alternatives need a recoverable index, even when their source
/// is hidden behind an alias or a conditional rather than an `Index` node.
fn dispatch_source_has_discriminator<'a>(
    package: &Package,
    owners: &ExprOwnerLookup,
    owner: ExprId,
    source: ExprId,
    candidates: impl Iterator<Item = (&'a ConcreteCallable, bool)>,
) -> bool {
    let mut first = None;
    let mut distinct = false;
    for (callable, guarded) in candidates {
        if guarded {
            return true;
        }
        if let Some(first) = first {
            distinct |= callable != first;
        } else {
            first = Some(callable);
        }
    }
    (!distinct && !dispatch_source_is_indexed(package, owners, owner, source))
        || resolve_index_dispatch_source(package, owners, owner, source).is_some()
}

/// Plans the branches of an index-dispatch rewrite by pairing each
/// candidate callable with the condition expression that selects it.
fn synthesize_index_dispatch_plan(
    package: &mut Package,
    package_id: PackageId,
    expr_owner_lookup: &ExprOwnerLookup,
    dispatch_source: (ExprId, ExprId),
    callables: &[ConcreteCallable],
    assigner: &mut Assigner,
) -> Option<IndexDispatchPlan> {
    let (owner_expr_id, dispatch_expr_id) = dispatch_source;
    let (source_index_expr_id, index_expr_id, indexed_callables) =
        resolve_index_dispatch_source(package, expr_owner_lookup, owner_expr_id, dispatch_expr_id)?;
    let span = package.get_expr(owner_expr_id).span;

    let mut entry_positions = Vec::with_capacity(callables.len());
    let mut used_positions = vec![false; indexed_callables.len()];
    for callable in callables {
        let position = indexed_callables
            .iter()
            .enumerate()
            .position(|(position, candidate)| {
                !used_positions[position] && indexed_callable_matches(candidate, callable)
            })?;
        used_positions[position] = true;
        entry_positions.push(position);
    }

    let (default_idx, _) = entry_positions
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, position)| *position)?;

    // Try to hoist indices that fail the side-effect check into a shared `let`
    // when block context is available. Accepted indices are reused directly;
    // the check does not prove they cannot trap.
    let hoist_index = !expr_is_side_effect_free(package, package_id, index_expr_id);
    let hoist_plan = if hoist_index {
        let source_scope = expr_owner_lookup.scope(&source_index_expr_id)?;
        let owner_scope = expr_owner_lookup.scope(&owner_expr_id)?;
        if source_scope != owner_scope {
            return None;
        }
        let ExprKind::Index(array_expr_id, current_index_expr_id) =
            package.get_expr(source_index_expr_id).kind
        else {
            return None;
        };
        if current_index_expr_id != index_expr_id {
            return None;
        }
        let index_ty = package.get_expr(index_expr_id).ty.clone();
        let index_span = package.get_expr(index_expr_id).span;
        let block_lookup = build_expr_block_lookup(package);
        if !index_can_move_before_statement(
            package,
            package_id,
            source_index_expr_id,
            *block_lookup.get(&index_expr_id)?,
        ) {
            return None;
        }
        Some((array_expr_id, index_ty, index_span, block_lookup))
    } else {
        None
    };

    // Every operation above this point is fallible and leaves the package
    // unchanged. Once the hoist starts, the remaining allocations and source
    // edge replacement are guaranteed to complete.
    let hoisted = hoist_plan.map(|(array_expr_id, index_ty, index_span, block_lookup)| {
        let (local_var, replacement_expr_id) =
            hoist_expr_into_let(package, assigner, &block_lookup, index_expr_id, "index")
                .expect("validated index expression should have block context");
        package
            .exprs
            .get_mut(source_index_expr_id)
            .expect("validated indexed dispatch source should exist")
            .kind = ExprKind::Index(array_expr_id, replacement_expr_id);
        (local_var, index_ty, index_span)
    });

    let mut conditioned = Vec::with_capacity(callables.len().saturating_sub(1));
    for (entry_idx, position) in entry_positions.into_iter().enumerate() {
        if entry_idx == default_idx {
            continue;
        }
        let condition = alloc_index_dispatch_guard(
            package,
            assigner,
            hoisted.as_ref(),
            index_expr_id,
            position,
            indexed_callables.len(),
            span,
        );
        conditioned.push((entry_idx, condition));
    }

    let bounds_operand = match &hoisted {
        Some((local_var, ty, index_span)) => crate::fir_builder::alloc_local_var_expr(
            package,
            assigner,
            *local_var,
            ty.clone(),
            *index_span,
        ),
        None => index_expr_id,
    };
    let bounds_check = alloc_index_bounds_check(
        package,
        assigner,
        bounds_operand,
        indexed_callables.len(),
        span,
    );

    Some(IndexDispatchPlan {
        conditioned,
        default_index: default_idx,
        bounds_check,
    })
}

/// Statement-wide staging must not cross earlier eager operands or lazy guards.
fn index_can_move_before_statement(
    package: &Package,
    package_id: PackageId,
    source: ExprId,
    (block, statement): (BlockId, usize),
) -> bool {
    let stmt = package.get_stmt(package.get_block(block).stmts[statement]);
    let root = match stmt.kind {
        StmtKind::Local(_, _, root) | StmtKind::Expr(root) | StmtKind::Semi(root) => root,
        StmtKind::Item(_) => return false,
    };
    let first_evaluated = root == source
        || matches!(package.get_expr(root).kind, ExprKind::Call(callee, _) if callee == source);
    first_evaluated
        && matches!(package.get_expr(source).kind, ExprKind::Index(array, _)
            if expr_is_side_effect_free(package, package_id, array))
}

fn alloc_index_dispatch_guard(
    package: &mut Package,
    assigner: &mut Assigner,
    hoisted: Option<&(LocalVarId, Ty, PackageSpan)>,
    index_expr_id: ExprId,
    position: usize,
    length: usize,
    span: PackageSpan,
) -> ExprId {
    let operand = match hoisted {
        Some((local_var, ty, index_span)) => crate::fir_builder::alloc_local_var_expr(
            package,
            assigner,
            *local_var,
            ty.clone(),
            *index_span,
        ),
        None => strip_transparent_block_expr(package, index_expr_id),
    };
    alloc_index_match_expr(package, operand, position, length, span, assigner)
}

fn alloc_index_bounds_check(
    package: &mut Package,
    assigner: &mut Assigner,
    index_expr_id: ExprId,
    length: usize,
    span: PackageSpan,
) -> ExprId {
    let units = (0..length)
        .map(|_| alloc_unit_expr(package, assigner, span))
        .collect();
    let array = alloc_expr(
        package,
        assigner,
        Ty::Array(Box::new(Ty::UNIT)),
        ExprKind::Array(units),
        span,
    );
    alloc_expr(
        package,
        assigner,
        Ty::UNIT,
        ExprKind::Index(array, index_expr_id),
        span,
    )
}

fn prepend_bounds_check(
    package: &mut Package,
    assigner: &mut Assigner,
    bounds_check: ExprId,
    dispatch: ExprId,
    result_ty: Ty,
    span: PackageSpan,
) -> ExprId {
    let mut statements = if let ExprKind::Block(block) = package.get_expr(bounds_check).kind {
        package.get_block(block).stmts.clone()
    } else {
        vec![alloc_semi_stmt(package, assigner, bounds_check, span)]
    };
    let dispatch_stmt = alloc_expr_stmt(package, assigner, dispatch, span);
    statements.push(dispatch_stmt);
    let block = alloc_block(package, assigner, statements, result_ty.clone(), span);
    alloc_block_expr(package, assigner, block, result_ty, span)
}

fn prepend_bounds_check_to_rewritten_call(
    package: &mut Package,
    assigner: &mut Assigner,
    call_expr_id: ExprId,
    bounds_check: ExprId,
) {
    let rewritten = package.get_expr(call_expr_id).clone();
    let result_ty = rewritten.ty.clone();
    let span = rewritten.span;
    let rewritten_id = alloc_expr(package, assigner, rewritten.ty, rewritten.kind, span);
    let dispatch_id = prepend_bounds_check(
        package,
        assigner,
        bounds_check,
        rewritten_id,
        result_ty,
        span,
    );
    let dispatch = package.get_expr(dispatch_id).clone();
    let call = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr should exist");
    call.kind = dispatch.kind;
    call.ty = dispatch.ty;
}

fn indexed_callable_matches(reconstructed: &ConcreteCallable, analyzed: &ConcreteCallable) -> bool {
    match (reconstructed, analyzed) {
        (
            ConcreteCallable::Closure {
                target: reconstructed_target,
                captures: reconstructed_captures,
                functor: reconstructed_functor,
            },
            ConcreteCallable::Closure {
                target: analyzed_target,
                captures: analyzed_captures,
                functor: analyzed_functor,
            },
        ) => {
            reconstructed_target == analyzed_target
                && reconstructed_functor == analyzed_functor
                && reconstructed_captures.len() == analyzed_captures.len()
                && reconstructed_captures.iter().zip(analyzed_captures).all(
                    |(reconstructed, analyzed)| {
                        reconstructed.local == analyzed.local
                            && reconstructed.ty == analyzed.ty
                            && (analyzed.expr.is_none() || reconstructed.expr == analyzed.expr)
                            && reconstructed.caller_substitutions == analyzed.caller_substitutions
                    },
                )
        }
        _ => reconstructed == analyzed,
    }
}

/// Removes nested `Block([Expr(tail)])` wrappers from an index expression used
/// in synthesized dispatch guards.
///
/// This keeps pure block indices such as `ops[{ i }]` from rendering as
/// `if { i } == 0`, while leaving blocks with locals or multiple statements
/// untouched so they still use the normal hoist path when needed.
fn strip_transparent_block_expr(package: &Package, expr_id: ExprId) -> ExprId {
    let ExprKind::Block(block_id) = package.get_expr(expr_id).kind else {
        return expr_id;
    };
    let block = package.get_block(block_id);
    let [stmt_id] = block.stmts.as_slice() else {
        return expr_id;
    };
    let StmtKind::Expr(tail_expr_id) = package.get_stmt(*stmt_id).kind else {
        return expr_id;
    };
    strip_transparent_block_expr(package, tail_expr_id)
}

/// Locates the source of a dynamic dispatch (for example the index
/// expression selecting an element in a callable array) that
/// `synthesize_*_index_dispatch` will compare against per-branch values.
fn resolve_index_dispatch_source(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
    owner_expr_id: ExprId,
    dispatch_expr_id: ExprId,
) -> Option<(ExprId, ExprId, Vec<ConcreteCallable>)> {
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
        && !indexed_callables.is_empty()
    {
        return Some((source_expr_id, index_expr_id, indexed_callables));
    }

    if let Some(indexed_callables) = direct_callables
        && !indexed_callables.is_empty()
    {
        return Some((source_expr_id, index_expr_id, indexed_callables));
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
    if indexed_callables.is_empty() {
        return None;
    }
    Some((source_expr_id, index_expr_id, indexed_callables))
}

/// For a dispatch expression that is a local variable bound from a tuple
/// pattern, returns the field position path within the tuple.
fn resolve_dispatch_field_path(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
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

/// Follows local initializers, block tails, and returns to the expression
/// feeding an index dispatch. Other expressions, including field accesses,
/// are returned unchanged.
fn resolve_dispatch_source_expr(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
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
        ExprKind::Field(record_expr_id, Field::Path(ref field_path)) => {
            resolve_dispatch_struct_field_expr(
                package,
                expr_owner_lookup,
                owner_expr_id,
                record_expr_id,
                field_path,
            )
        }
        _ => Some(expr_id),
    }
}

fn resolve_dispatch_struct_field_expr(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
    owner_expr_id: ExprId,
    record_expr_id: ExprId,
    field_path: &FieldPath,
) -> Option<ExprId> {
    let record_source =
        resolve_dispatch_source_expr(package, expr_owner_lookup, owner_expr_id, record_expr_id)?;
    match &package.get_expr(record_source).kind {
        ExprKind::Struct(_, copy, fields) => fields
            .iter()
            .find_map(|field| {
                matches!(&field.field, Field::Path(path) if path == field_path)
                    .then_some(field.value)
            })
            .or_else(|| {
                copy.and_then(|copy| {
                    resolve_dispatch_struct_field_expr(
                        package,
                        expr_owner_lookup,
                        owner_expr_id,
                        copy,
                        field_path,
                    )
                })
            }),
        ExprKind::Tuple(_) => extract_tuple_field(package, record_source, &field_path.indices),
        _ => None,
    }
}

/// Resolves an array or tuple literal to the ordered list of concrete
/// callables it contains, used by index-dispatch synthesis.
fn resolve_array_expr_to_callables(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
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
        callables.push(callable);
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
    expr_owner_lookup: &ExprOwnerLookup,
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
        callables.push(callable);
    }

    Some(callables)
}

/// Attempts to resolve an expression to a single concrete callable (global
/// or closure), mirroring the analysis-phase resolution but on the
/// rewritten package.
fn resolve_expr_to_concrete_callable(
    package: &Package,
    expr_owner_lookup: &ExprOwnerLookup,
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
    expr_owner_lookup: &ExprOwnerLookup,
    owner_expr_id: ExprId,
    captured_vars: &[LocalVarId],
) -> Option<Vec<CapturedVar>> {
    let owner_callable = *expr_owner_lookup.get(&owner_expr_id)?;
    let owner_scope = expr_owner_lookup.scope(&owner_expr_id)?;
    captured_vars
        .iter()
        .map(|&var| {
            let expr = find_local_init_expr_in_callable(package, owner_callable, var);
            let ty = expr
                .map(|expr_id| package.get_expr(expr_id).ty.clone())
                .or_else(|| find_var_type_in_callable(package, owner_callable, var))?;
            Some(CapturedVar {
                local: ScopedLocal::new(var, owner_scope),
                ty,
                static_callable: None,
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
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
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
    span: PackageSpan,
    assigner: &mut Assigner,
) -> ExprId {
    let index_value = i64::try_from(index_value).expect("dispatch index should fit in i64");
    let lit_id = alloc_int_lit(package, assigner, index_value, span);
    alloc_bin_op_expr(
        package,
        assigner,
        BinOp::Eq,
        index_expr_id,
        lit_id,
        Ty::Prim(Prim::Bool),
        span,
    )
}

pub(super) fn alloc_index_match_expr(
    package: &mut Package,
    index_expr_id: ExprId,
    position: usize,
    length: usize,
    span: PackageSpan,
    assigner: &mut Assigner,
) -> ExprId {
    let positive = alloc_index_eq_expr(package, index_expr_id, position, span, assigner);
    let position = i64::try_from(position).expect("dispatch position should fit in i64");
    let length = i64::try_from(length).expect("dispatch length should fit in i64");
    let negative = alloc_int_lit(package, assigner, position - length, span);
    let negative = alloc_bin_op_expr(
        package,
        assigner,
        BinOp::Eq,
        index_expr_id,
        negative,
        Ty::Prim(Prim::Bool),
        span,
    );
    alloc_bin_op_expr(
        package,
        assigner,
        BinOp::OrL,
        positive,
        negative,
        Ty::Prim(Prim::Bool),
        span,
    )
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

/// Discard safety alone cannot justify replay: a pure read of a mutable local
/// may produce a different value now than when the aggregate was stored.
/// Keep the initializer shortcut only for immutable bindings whose initializer
/// is unobservable and refers exclusively to stable locals.
fn replayable_local_initializer(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> Option<ExprId> {
    let init = find_local_init_expr_in_callable(package, callable_id, local_var)?;
    if !expr_is_safe_to_discard(package, package.id, init) {
        return None;
    }
    let ItemKind::Callable(decl) = &package.get_item(callable_id).kind else {
        return None;
    };
    let mut mutable_locals = FxHashSet::default();
    crate::walk_utils::for_each_node_in_callable(package, decl, &mut |node| {
        if let crate::walk_utils::CallableNode::Stmt(id) = node
            && let StmtKind::Local(Mutability::Mutable, pat, _) = package.get_stmt(id).kind
        {
            let mut bindings = Vec::new();
            collect_bound_pat_vars(package, pat, &mut bindings);
            mutable_locals.extend(bindings);
        }
    });
    if mutable_locals.contains(&local_var) {
        return None;
    }
    let mut reads_mutable = false;
    crate::walk_utils::for_each_expr(package, init, &mut |_, expr| {
        reads_mutable |= match &expr.kind {
            ExprKind::Var(Res::Local(var), _) => mutable_locals.contains(var),
            ExprKind::Closure(captures, _) => {
                captures.iter().any(|var| mutable_locals.contains(var))
            }
            _ => false,
        };
    });
    (!reads_mutable).then_some(init)
}

/// Prunes eligible callable-argument locals and closure-bearing source arrays
/// after their reads have been consumed by call-site rewrites.
///
/// Removes dead immutable bindings and selected write-only bindings and
/// assignments, then runs general dead-callable-local cleanup. This does not
/// eliminate every arrow-typed residue.
fn prune_dead_callable_arg_locals(
    package: &mut Package,
    package_id: PackageId,
    rewritten_callable_arg_locals: &FxHashSet<(LocalItemId, LocalVarId)>,
    hof_consumed_source_arrays: &FxHashSet<(LocalItemId, LocalVarId)>,
    total_foreign: &FxHashSet<ItemId>,
) {
    let mut source_arrays: FxHashSet<(LocalItemId, LocalVarId)> = FxHashSet::default();

    // Closure callable-arrays forwarded and fully consumed by a higher-order
    // call are dead source arrays that the direct-path index-read tracer never
    // sees. Seed them here so the same closure-bearing removal below prunes the
    // now-dead binding rather than leaving neutralized closure elements behind.
    source_arrays.extend(hof_consumed_source_arrays.iter().copied());

    for &(callable_id, local_var) in rewritten_callable_arg_locals {
        if !local_var_is_used_in_callable(package, callable_id, local_var) {
            // Every use was consumed by the call-site rewrite, so only the
            // initializer's own evaluation is still observable. Deleting the
            // binding is allowed only when that evaluation is not observable.
            if !consumed_callable_local_disposition(
                package,
                package_id,
                callable_id,
                local_var,
                total_foreign,
            )
            .allows_removal()
            {
                continue;
            }
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
            // neutralizes that tail into a reference no one produces a value
            // for. Removing the dead binding and its assignments avoids that.
            // Locals bound only to plain callable references
            // (`op = H`) carry no closure tail, so they are left untouched to
            // preserve existing dead-code behavior.
            remove_write_only_callable_local_from_callable(package, callable_id, local_var);
        }
    }

    // Prune callable-array locals that only fed removed direct-dispatch index
    // reads. Restricting removal to a closure-bearing array (`[X, Rx(0.0, _)]`)
    // avoids stranding a neutralized closure element in a dead binding, while
    // plain callable-reference arrays are left in place.
    for (callable_id, src_var) in source_arrays {
        if !local_var_is_read_in_callable(package, callable_id, src_var)
            && local_var_has_closure_valued_binding(package, callable_id, src_var)
        {
            remove_write_only_callable_local_from_callable(package, callable_id, src_var);
        }
    }

    prune_dead_top_level_callable_locals(package, package_id);
}

struct ExprOwnerLookup {
    callable: FxHashMap<ExprId, LocalItemId>,
    scope: FxHashMap<ExprId, CaptureScope>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
impl ExprOwnerLookup {
    fn get(&self, expr_id: &ExprId) -> Option<&LocalItemId> {
        self.callable.get(expr_id)
    }

    fn callable(&self, expr_id: &ExprId) -> Option<LocalItemId> {
        self.callable.get(expr_id).copied()
    }

    fn scope(&self, expr_id: &ExprId) -> Option<CaptureScope> {
        self.scope.get(expr_id).copied()
    }
}

/// Builds maps from each expression id to its callable owner and exact capture
/// scope. Entry expressions have no callable owner but belong to `Entry`.
fn build_expr_owner_lookup(
    package: &Package,
    package_id: PackageId,
    specialized_items: &FxHashSet<StoreItemId>,
) -> ExprOwnerLookup {
    let mut lookup = ExprOwnerLookup {
        callable: FxHashMap::default(),
        scope: FxHashMap::default(),
    };

    for (item_id, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind {
            let store_id = StoreItemId::from((package_id, item_id));
            let scope = if specialized_items.contains(&store_id) {
                CaptureScope::CloneScope(item_id)
            } else {
                CaptureScope::Callable(item_id)
            };
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, _expr| {
                    lookup.callable.insert(expr_id, item_id);
                    lookup.scope.insert(expr_id, scope);
                },
            );
        }
    }

    if let Some(entry_expr_id) = package.entry {
        crate::walk_utils::for_each_expr(package, entry_expr_id, &mut |expr_id, _expr| {
            lookup.scope.insert(expr_id, CaptureScope::Entry);
        });
    }

    lookup
}

/// Maps block-resident expressions in callable implementations to their
/// innermost enclosing block and the index of the statement containing them.
/// The package entry expression is not traversed separately.
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
                CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
                CallableImpl::Spec(spec_impl) => {
                    record_block_context(package, spec_impl.body.block, &mut lookup);
                    for spec in crate::fir_builder::functored_specs(spec_impl) {
                        record_block_context(package, spec.block, &mut lookup);
                    }
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
/// carry [`crate::EMPTY_EXEC_RANGE`]; `exec_graph_rebuild` repairs ranges later.
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
        qsc_fir::fir::CallableImpl::Intrinsic
        | qsc_fir::fir::CallableImpl::SimulatableIntrinsic(_) => {}
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
        ExprKind::Parallel(limit, body) => {
            if let Some(limit) = limit {
                remove_write_only_callable_local_from_expr(package, limit, local_var);
            }
            remove_write_only_callable_local_from_expr(package, body, local_var);
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

/// Where a consumed callable expression's evaluation lives once rewriting has
/// finished.
///
/// Defunctionalization repeatedly faces the same question in two places: a
/// `let` binding whose callable value was consumed by a call-site rewrite, and
/// the callable argument expression that same rewrite removes from the call.
/// Both ask whether *deleting the expression* also *drops its evaluation*.
/// Those are different questions. A binding whose captures have already been
/// relocated into the specialized call can still look observable; retaining it
/// would evaluate the capture initializer twice and apply a gate twice.
///
/// Naming the four outcomes keeps that distinction visible. Only
/// [`Self::Retained`] forbids removal; the other three each describe a
/// *different reason* removal is sound, and each is separately testable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EvaluationDisposition {
    /// The evaluation is genuinely dropped, and dropping it is provably
    /// unobservable: the expression is side-effect free and total, so no
    /// effect, failure, or state change disappears with it.
    ///
    /// The vacuous case lands here too — when there is no binding at all there
    /// is no evaluation to drop.
    Discarded,

    /// The evaluation moved into the rewritten call rather than disappearing.
    ///
    /// Partial application lowers to bindings followed by a closure. Rewriting
    /// must evaluate the captured operands at the original callable operand
    /// position, before later arguments, even when the specialized signature
    /// appends captures after those arguments. Materialized temporaries preserve
    /// that order; retaining the consumed capture initializer would duplicate it.
    Relocated,

    /// The evaluation is reproduced by the dispatch the rewrite generated.
    ///
    /// Dispatch preserves the selected path and any required index bounds
    /// check. Rewriting must also preserve the original operand timing.
    Replayed,

    /// The evaluation is observable and nothing above reproduces it, so the
    /// expression must survive.
    Retained,
}

impl EvaluationDisposition {
    /// Returns whether the expression may be deleted.
    ///
    /// Every disposition except [`Self::Retained`] carries a reason the
    /// evaluation still happens (or provably never mattered).
    fn allows_removal(self) -> bool {
        !matches!(self, Self::Retained)
    }
}

/// Which of the two consumption sites is asking for a disposition.
///
/// Both sites delete an expression, but the rewrite gives them different
/// guarantees, and one rule depends on which is asking. See
/// [`consumed_callable_expr_disposition`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConsumptionSite {
    /// A `let` binding whose callable value every use consumed.
    Binding,

    /// The callable argument expression that the rewrite removes from the call
    /// it is specializing.
    Argument,
}

/// Classifies where a consumed callable *expression's* evaluation lives after
/// rewriting.
///
/// This is the single decision that both consumption sites share. It replaces
/// the four predicates that used to be OR'd together at the binding gate, and
/// it is applied to the argument expression itself by
/// [`super::analysis`] before a call site is accepted for specialization, so
/// the argument-removal family (`remove_element_at_path`,
/// `rewrite_args_remove_tuple_element`, `rewrite_single_arg_root`,
/// `remove_top_level_field_from_expr_data`, and the branch-dispatch arg
/// builders) never has to re-derive it. Those sites delete the expression
/// outright with no purity guard of their own; the guarantee they rely on is
/// established here.
///
/// One rule is positional. At [`ConsumptionSite::Argument`] the expression
/// being deleted *is* the selection the analysis statically resolved, and the
/// specialized call re-expresses that selection directly, so a statically
/// resolved indexed read is [`EvaluationDisposition::Replayed`]. Rewriting must
/// preserve its bounds check or prove the index statically in bounds. At
/// [`ConsumptionSite::Binding`] the array may still be read elsewhere, so that
/// rule does not apply and the narrower closure-bearing test governs instead.
///
/// `total_foreign` lets the discard proof see through calls into other
/// packages, both the named total intrinsics and the factories
/// [`crate::walk_utils::extend_with_discardable_foreign_callables`] proved pure
/// on their home ground.
pub(super) fn consumed_callable_expr_disposition(
    package: &Package,
    package_id: PackageId,
    expr_id: ExprId,
    site: ConsumptionSite,
    total_foreign: &FxHashSet<ItemId>,
) -> EvaluationDisposition {
    if expr_is_safe_to_discard_with_total_foreign(package, package_id, expr_id, total_foreign) {
        EvaluationDisposition::Discarded
    } else if is_replayed_callable_selection(package, expr_id)
        || (site == ConsumptionSite::Argument
            && is_replayed_index_selection(package, package_id, expr_id, total_foreign))
    {
        EvaluationDisposition::Replayed
    } else if captures_relocated_into_call(package, expr_id) {
        EvaluationDisposition::Relocated
    } else {
        EvaluationDisposition::Retained
    }
}

/// Returns whether the expression is an indexed read whose operands are
/// themselves safe to discard.
///
/// The rewrite replaces the selection with direct dispatch, retaining a bounds
/// check unless the index is statically in bounds. Discard-safe operands ensure
/// deleting the original selection does not lose their effects or failures.
///
/// Evidence for [`EvaluationDisposition::Replayed`]; see
/// [`consumed_callable_expr_disposition`].
fn is_replayed_index_selection(
    package: &Package,
    package_id: PackageId,
    expr_id: ExprId,
    total_foreign: &FxHashSet<ItemId>,
) -> bool {
    let ExprKind::Index(array_id, index_id) = package.get_expr(expr_id).kind else {
        return false;
    };
    expr_is_safe_to_discard_with_total_foreign(package, package_id, array_id, total_foreign)
        && expr_is_safe_to_discard_with_total_foreign(package, package_id, index_id, total_foreign)
}

/// Classifies where a consumed callable *local's* initializer evaluation lives
/// after rewriting.
///
/// The local itself is already unused, so the only remaining question is what
/// happened to its initializer's evaluation. That is
/// [`consumed_callable_expr_disposition`], plus one binding-shaped case the
/// expression alone cannot see: a local bound from an indexed read of a
/// closure-bearing callable array, whose selection the generated index dispatch
/// replays. That case mirrors the closure-bearing gate in
/// [`prune_dead_callable_arg_locals`] — an array of plain callable references
/// carries no closure to strand, so it stays protected by discard safety.
fn consumed_callable_local_disposition(
    package: &Package,
    package_id: PackageId,
    callable_id: LocalItemId,
    local_var: LocalVarId,
    total_foreign: &FxHashSet<ItemId>,
) -> EvaluationDisposition {
    let Some(init_expr_id) = find_local_init_expr_in_callable(package, callable_id, local_var)
    else {
        // No `let` binding to delete, so there is no evaluation to preserve.
        return EvaluationDisposition::Discarded;
    };

    let disposition = consumed_callable_expr_disposition(
        package,
        package_id,
        init_expr_id,
        ConsumptionSite::Binding,
        total_foreign,
    );
    if disposition.allows_removal() {
        return disposition;
    }

    if reads_closure_bearing_callable_array(package, callable_id, local_var) {
        return EvaluationDisposition::Replayed;
    }

    EvaluationDisposition::Retained
}

/// Returns whether the local is bound from an indexed read of a callable array
/// that still holds a partial-application closure.
///
/// The index dispatch that replaced this local's use enumerates the array's
/// elements and preserves its bounds behavior. Once no use remains, this permits
/// removal of the redundant initializer instead of retaining a dead binding
/// over closure elements whose payloads have moved into specialized calls.
///
/// Evidence for [`EvaluationDisposition::Replayed`]; see
/// [`consumed_callable_local_disposition`].
fn reads_closure_bearing_callable_array(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> bool {
    local_index_source_array_local(package, callable_id, local_var)
        .is_some_and(|source| local_var_has_closure_valued_binding(package, callable_id, source))
}

/// Removes a specific dead callable local from the given callable's body by
/// deleting eligible unused immutable `Local` bindings, recursing
/// into nested blocks via [`remove_dead_callable_local_from_block`].
///
/// The caller decides whether removal is allowed; see
/// [`consumed_callable_local_disposition`].
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
        qsc_fir::fir::CallableImpl::Intrinsic
        | qsc_fir::fir::CallableImpl::SimulatableIntrinsic(_) => {}
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

/// Recognizes a closure or a chain of blocks whose final expression yields one.
///
/// This recognizes the capture-producing shape, not permission to move its
/// evaluation across other operands. Argument rewrites materialize captures
/// before later arguments; cleanup must not evaluate those initializers again.
///
/// Only the closure-yielding shape qualifies. A binding whose initializer
/// merely produces a callable some other way, such as a call to an effectful
/// producer, relocates nothing and stays protected by discard safety.
///
/// Evidence for [`EvaluationDisposition::Relocated`]; see
/// [`consumed_callable_expr_disposition`].
fn captures_relocated_into_call(package: &Package, expr_id: ExprId) -> bool {
    match package.get_expr(expr_id).kind {
        ExprKind::Closure(_, _) => true,
        ExprKind::Block(block_id) => {
            package
                .get_block(block_id)
                .stmts
                .last()
                .is_some_and(|&tail_stmt_id| {
                    matches!(
                        package.get_stmt(tail_stmt_id).kind,
                        StmtKind::Expr(tail_expr_id)
                            if captures_relocated_into_call(package, tail_expr_id)
                    )
                })
        }
        _ => false,
    }
}

/// Prunes unused, discard-safe immutable callable-typed bindings across callable
/// bodies and the package-level entry expression, including nested blocks.
///
/// The entry expression needs the same treatment as a callable body, and for a
/// stronger reason: it is the reachability root, so a dead callable binding
/// left there can never be removed by item DCE. The synthetic entry that
/// `qsc::codegen` builds for callable-argument code generation binds each
/// concrete callable argument to a `let` before the target call, and those
/// bindings are exactly what direct-dispatch rewriting makes dead.
fn prune_dead_top_level_callable_locals(package: &mut Package, package_id: PackageId) {
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
            qsc_fir::fir::CallableImpl::Intrinsic
            | qsc_fir::fir::CallableImpl::SimulatableIntrinsic(_) => {}
            qsc_fir::fir::CallableImpl::Spec(spec_impl) => {
                prune_dead_callable_locals_in_block(package, package_id, spec_impl.body.block);
                for spec in [spec_impl.adj, spec_impl.ctl, spec_impl.ctl_adj]
                    .into_iter()
                    .flatten()
                {
                    prune_dead_callable_locals_in_block(package, package_id, spec.block);
                }
            }
        }
    }

    if let Some(entry_id) = package.entry {
        prune_dead_callable_locals_in_expr(package, package_id, entry_id);
    }
}

/// Removes unused immutable callable-typed bindings with discard-safe
/// initializers from a block.
///
/// Repeats removal within this block to prune cascading dead-local chains,
/// then recurses into nested blocks. Removals in nested blocks can expose
/// further opportunities for a later outer fixpoint iteration.
fn prune_dead_callable_locals_in_block(
    package: &mut Package,
    package_id: PackageId,
    block_id: qsc_fir::fir::BlockId,
) {
    loop {
        let stmt_ids = package.get_block(block_id).stmts.clone();
        let initial_count = stmt_ids.len();
        let mut retained = Vec::with_capacity(initial_count);

        for stmt_id in stmt_ids {
            let stmt = package.get_stmt(stmt_id);
            let remove_stmt = match stmt.kind {
                StmtKind::Local(Mutability::Immutable, pat_id, init_expr_id) => {
                    let pat = package.get_pat(pat_id);
                    if local_ty_contains_arrow_through_udts(package, &pat.ty) {
                        let mut bound_vars = Vec::new();
                        collect_bound_pat_vars(package, pat_id, &mut bound_vars);
                        !bound_vars.is_empty()
                            && expr_is_safe_to_discard(package, package_id, init_expr_id)
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
                prune_dead_callable_locals_in_stmt(package, package_id, stmt_id);
            }
            break;
        }
    }
}

/// Removes an immutable callable-typed binding containing `local_var` when all
/// variables it binds are unused in the block, then recurses into nested blocks
/// via [`remove_dead_callable_local_from_stmt`]. Does not rewrite `Var` nodes.
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

/// Recognizes a static callable-selection tree eligible for cleanup after
/// branch dispatch. Leaves must be item references and block wrappers must
/// contain only their tail expression.
///
/// This checks the initializer's shape, not whether the generated dispatch
/// retained every condition or its original evaluation point.
///
/// Evidence for [`EvaluationDisposition::Replayed`]; see
/// [`consumed_callable_expr_disposition`].
fn is_replayed_callable_selection(package: &Package, expr_id: ExprId) -> bool {
    match package.get_expr(expr_id).kind {
        ExprKind::If(_, then_expr_id, Some(else_expr_id)) => {
            is_replayed_callable_selection(package, then_expr_id)
                && is_replayed_callable_selection(package, else_expr_id)
        }
        ExprKind::Block(block_id) => {
            let block = package.get_block(block_id);
            matches!(block.stmts.as_slice(), [stmt_id] if matches!(package.get_stmt(*stmt_id).kind, StmtKind::Expr(tail) if is_replayed_callable_selection(package, tail)))
        }
        ExprKind::Var(Res::Item(_), _) => true,
        _ => false,
    }
}

/// Visits a statement's expression to prune eligible bindings in nested
/// blocks via [`prune_dead_callable_locals_in_expr`]. The statement itself
/// is not removed here.
fn prune_dead_callable_locals_in_stmt(
    package: &mut Package,
    package_id: PackageId,
    stmt_id: qsc_fir::fir::StmtId,
) {
    let stmt = package.get_stmt(stmt_id).clone();
    match stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            prune_dead_callable_locals_in_expr(package, package_id, expr_id);
        }
        StmtKind::Item(_) => {}
    }
}

/// Descends into an expression subtree looking for dead callable-local
/// bindings introduced by direct-call rewrites, delegating to
/// [`prune_dead_callable_locals_in_block`] for nested `Block` and `While`
/// bodies.
fn prune_dead_callable_locals_in_expr(
    package: &mut Package,
    package_id: PackageId,
    expr_id: ExprId,
) {
    let expr = package.get_expr(expr_id).clone();
    match expr.kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for expr_id in exprs {
                prune_dead_callable_locals_in_expr(package, package_id, expr_id);
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
            prune_dead_callable_locals_in_expr(package, package_id, lhs);
            prune_dead_callable_locals_in_expr(package, package_id, rhs);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            prune_dead_callable_locals_in_expr(package, package_id, a);
            prune_dead_callable_locals_in_expr(package, package_id, b);
            prune_dead_callable_locals_in_expr(package, package_id, c);
        }
        ExprKind::Block(block_id) => {
            prune_dead_callable_locals_in_block(package, package_id, block_id);
        }
        ExprKind::Fail(inner)
        | ExprKind::Field(inner, _)
        | ExprKind::Return(inner)
        | ExprKind::UnOp(_, inner) => {
            prune_dead_callable_locals_in_expr(package, package_id, inner);
        }
        ExprKind::If(cond, body, otherwise) => {
            prune_dead_callable_locals_in_expr(package, package_id, cond);
            prune_dead_callable_locals_in_expr(package, package_id, body);
            if let Some(otherwise) = otherwise {
                prune_dead_callable_locals_in_expr(package, package_id, otherwise);
            }
        }
        ExprKind::Range(start, step, end) => {
            for expr_id in [start, step, end].into_iter().flatten() {
                prune_dead_callable_locals_in_expr(package, package_id, expr_id);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(expr_id) = component {
                    prune_dead_callable_locals_in_expr(package, package_id, expr_id);
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                prune_dead_callable_locals_in_expr(package, package_id, copy);
            }
            for field in fields {
                prune_dead_callable_locals_in_expr(package, package_id, field.value);
            }
        }
        ExprKind::While(cond, block_id) => {
            prune_dead_callable_locals_in_expr(package, package_id, cond);
            prune_dead_callable_locals_in_block(package, package_id, block_id);
        }
        ExprKind::Parallel(limit, expr) => {
            if let Some(l) = limit {
                prune_dead_callable_locals_in_expr(package, package_id, l);
            }
            prune_dead_callable_locals_in_expr(package, package_id, expr);
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

/// Removes eligible bindings of a dead callable local inside a given expression
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
        ExprKind::Parallel(limit, expr) => {
            if let Some(l) = limit {
                remove_dead_callable_local_from_expr(package, l, local_var);
            }
            remove_dead_callable_local_from_expr(package, expr, local_var);
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
    let retain_evaluation = consumed_callable_expr_disposition(
        package,
        package_id,
        callee_id,
        ConsumptionSite::Argument,
        &FxHashSet::default(),
    ) == EvaluationDisposition::Retained;
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
    if retain_evaluation {
        let original = alloc_expr(
            package,
            assigner,
            callee_expr.ty,
            callee_expr.kind,
            callee_expr.span,
        );
        let statement = alloc_semi_stmt(package, assigner, original, callee_expr.span);
        prepend_evaluations(package, callee_id, vec![statement], assigner);
    }
}

/// Rewrites the argument tuple of a direct call whose callee
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
/// When the current arguments already match the target callable's declared
/// input (`target_input`) they are left unchanged. This guards the degenerate
/// case where the closure has no runtime captures and the target's
/// input was flattened to the bare parameter tuple, so grouping the arguments
/// into a one-element tuple would produce an over-wrapped `((a, b),)` that no
/// longer matches the callee.
///
/// # Mutations
/// - Rewrites `args_id`'s `ExprKind` and `Ty` in place to a `Tuple`
///   containing capture expressions followed by the original args.
/// - Allocates capture `Expr` nodes through `assigner`.
/// - For controlled operations, recurses through control-qubit layers,
///   threading the full `target_input` so the base-input match is checked at
///   the innermost, uncontrolled layer.
fn rewrite_direct_closure_args(
    package: &mut Package,
    args_id: ExprId,
    destination: CaptureScope,
    captures: &[CapturedVar],
    target_input: &Ty,
    controlled_layers: usize,
    assigner: &mut Assigner,
) {
    if controlled_layers > 0 {
        let inner_id = match package.get_expr(args_id).kind {
            ExprKind::Tuple(ref elements) if elements.len() > 1 => elements[1],
            _ => {
                rewrite_direct_closure_args(
                    package,
                    args_id,
                    destination,
                    captures,
                    target_input,
                    0,
                    assigner,
                );
                return;
            }
        };
        rewrite_direct_closure_args(
            package,
            inner_id,
            destination,
            captures,
            target_input,
            controlled_layers - 1,
            assigner,
        );
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

    // The arguments already match the target's declared input, so there is
    // nothing to splice. This covers the no-capture case where wrapping the
    // arguments in a one-element tuple would over-wrap a shape the callee
    // already expects flat.
    if args_expr.ty == *target_input {
        return;
    }

    let capture_ids =
        allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();

    let preserved_args_id = alloc_expr(
        package,
        assigner,
        args_expr.ty.clone(),
        args_expr.kind,
        args_expr.span,
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

/// Returns the packaged input tuple type for a direct call to a lambda
/// target whose parameters live in a one-element tuple.
///
/// Recognizes lifted lambdas by the `".lambda"` prefix and separately checks
/// that their declared input type is a one-element tuple.
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
    span: PackageSpan,
    result_ty: &Ty,
    destination: CaptureScope,
    direct_call_site: &DirectCallSite,
    assigner: &mut Assigner,
) -> ExprId {
    let capture_source = match &direct_call_site.callable {
        ConcreteCallable::Closure { captures, .. } => captures.as_slice(),
        ConcreteCallable::Global { .. } => direct_call_site.captures.as_slice(),
        ConcreteCallable::Dynamic => &[],
    };
    let captures = resolve_rewrite_captures(package, orig_callee.id, capture_source);
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
        destination,
        &captures,
        controlled_layers,
        package_direct_lambda,
        assigner,
    );
    let args_id = alloc_expr(package, assigner, args_ty, args_kind, span);

    alloc_call_expr(
        package,
        assigner,
        callee_id,
        args_id,
        result_ty.clone(),
        span,
    )
}

/// Assembles the argument-tuple expressions for a direct-call branch,
/// including any capture values that must accompany a closure branch.
fn build_direct_branch_args_data(
    package: &mut Package,
    orig_args: &Expr,
    destination: CaptureScope,
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
                destination,
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
                destination,
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
                destination,
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
            destination,
            captures,
            controlled_layers - 1,
            package_direct_lambda,
            assigner,
        );

        let inner_id = alloc_expr(
            package,
            assigner,
            inner_ty.clone(),
            inner_kind,
            inner_orig.span,
        );

        return (
            ExprKind::Tuple(vec![elements[0], inner_id]),
            Ty::Tuple(vec![tys[0].clone(), inner_ty]),
        );
    }

    if captures.is_empty() && !package_direct_lambda {
        return (orig_args.kind.clone(), orig_args.ty.clone());
    }

    let capture_ids =
        allocate_capture_exprs(package, orig_args.span, destination, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();

    let preserved_args_id = alloc_expr(
        package,
        assigner,
        orig_args.ty.clone(),
        orig_args.kind.clone(),
        orig_args.span,
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
fn plan_rewrite_one(
    package: &Package,
    call_site: &CallSite,
    param: &CallableParam,
    expr_owner_lookup: &ExprOwnerLookup,
) -> Option<RewriteOnePlan> {
    let ExprKind::Call(callee_id, original_args_id) = package.get_expr(call_site.call_expr_id).kind
    else {
        return None;
    };
    let input_path = callable_param_input_path(package, callee_id, param);
    let (_, outer_functor) = peel_body_functors(package, callee_id);
    let controlled_layers = usize::from(outer_functor.controlled);
    let captures = match &call_site.callable_arg {
        ConcreteCallable::Closure { captures, .. } => filter_threaded_rewrite_captures(
            package,
            resolve_rewrite_captures(package, call_site.arg_expr_id, captures),
        ),
        _ => Vec::new(),
    };
    let destination = expr_owner_lookup.scope(&call_site.call_expr_id)?;
    if !captures_belong_to_destination(destination, &captures) {
        return None;
    }
    let new_callee_ty =
        if controlled_layers > 0 && !param.hof_input_is_tuple && !param.field_path.is_empty() {
            // The controlled single-payload rewrite reads the generated declaration
            // rather than appending captures outside the control wrappers.
            None
        } else if !param.hof_input_is_tuple && !param.field_path.is_empty() {
            build_specialized_nested_payload_callee_ty(package, callee_id, &input_path, &captures)
        } else {
            build_specialized_callee_ty(package, callee_id, &input_path, &call_site.callable_arg)
        };
    Some(RewriteOnePlan {
        callee_id,
        original_args_id,
        input_path,
        controlled_layers,
        captures,
        destination,
        new_callee_ty,
    })
}

#[allow(clippy::too_many_arguments)]
fn rewrite_one(
    package: &mut Package,
    package_id: PackageId,
    call_site: &CallSite,
    param: &CallableParam,
    spec_store_id: StoreItemId,
    mut plan: RewriteOnePlan,
    expr_owner_lookup: &ExprOwnerLookup,
    assigner: &mut Assigner,
) -> bool {
    let mut statements = if plan.captures.iter().any(|capture| capture.expr.is_some()) {
        materialize_argument_prefix(
            package,
            plan.original_args_id,
            &plan.input_path,
            &[],
            assigner,
        )
    } else {
        Vec::new()
    };
    materialize_capture_operands(
        package,
        plan.destination,
        &mut plan.captures,
        &mut statements,
        assigner,
    );
    let rewritten = rewrite_one_with_captures(
        package,
        package_id,
        call_site,
        param,
        spec_store_id,
        plan,
        expr_owner_lookup,
        assigner,
    );
    prepend_evaluations(package, call_site.call_expr_id, statements, assigner);
    rewritten
}

#[allow(clippy::too_many_arguments)]
fn rewrite_one_with_captures(
    package: &mut Package,
    _package_id: PackageId,
    call_site: &CallSite,
    param: &CallableParam,
    spec_store_id: StoreItemId,
    plan: RewriteOnePlan,
    expr_owner_lookup: &ExprOwnerLookup,
    assigner: &mut Assigner,
) -> bool {
    let original_args = package.get_expr(plan.original_args_id).clone();
    let args_id = alloc_expr(
        package,
        assigner,
        original_args.ty,
        original_args.kind,
        original_args.span,
    );
    let call_expr = package
        .exprs
        .get_mut(call_site.call_expr_id)
        .expect("call expression should exist");
    let ExprKind::Call(_, call_args_id) = &mut call_expr.kind else {
        unreachable!("call expression should remain a call");
    };
    *call_args_id = args_id;

    // Replace callee with the specialized callable reference
    let spec_item_id = ItemId {
        package: spec_store_id.package,
        item: spec_store_id.item,
    };

    // A controlled single-payload call must reduce the payload beneath every
    // control wrapper. Reuse the branch builder rather than treating the outer
    // (controls, payload) tuple as the payload itself.
    if plan.controlled_layers > 0 && !param.hof_input_is_tuple && !param.field_path.is_empty() {
        let callee_ty = build_direct_global_callee_ty(
            package,
            spec_item_id,
            &package.get_expr(plan.callee_id).ty,
            plan.controlled_layers,
        );
        rewrite_specialized_callee(package, plan.callee_id, spec_item_id, callee_ty, assigner);
        let args = package.get_expr(args_id).clone();
        let (kind, ty) = build_branch_args_data(
            package,
            &args,
            plan.destination,
            &plan.input_path,
            plan.controlled_layers,
            param.hof_input_is_tuple,
            &plan.captures,
            args.span,
            assigner,
        );
        let args = package
            .exprs
            .get_mut(args_id)
            .expect("args expression exists");
        args.kind = kind;
        args.ty = ty;
        return true;
    }

    // Build the new callee type: remove the callable param from the arrow input.
    rewrite_specialized_callee(
        package,
        plan.callee_id,
        spec_item_id,
        plan.new_callee_ty,
        assigner,
    );

    // Remove the callable argument from the args tuple
    // Insert closure captures as extra arguments.
    //
    if !param.hof_input_is_tuple && param.field_path.len() == 1 {
        let mut remove_indices = FxHashSet::default();
        if let Some(&field_index) = param.field_path.first() {
            remove_indices.insert(field_index);
        }
        if rewrite_nested_arg_expr_remove_fields_as_payload(
            package,
            expr_owner_lookup.callable(&call_site.call_expr_id),
            plan.destination.into(),
            args_id,
            &remove_indices,
            &plan.captures,
            assigner,
        ) {
            return true;
        }
    }
    // The argument is one aggregate payload, even if its expression is a tuple.
    // Remove the deeply nested callable field while keeping the reduced payload
    // together, then append any closure captures as separate arguments. Routing
    // through the generic tuple-input rewrite would treat payload fields as
    // separate parameters. Controlled calls fall through so the control-aware
    // rewrite below can preserve the wrappers around the base input.
    if !param.hof_input_is_tuple
        && param.field_path.len() > 1
        && plan.controlled_layers == 0
        && rewrite_local_single_arg_nested(
            package,
            expr_owner_lookup.callable(&call_site.call_expr_id),
            plan.destination.into(),
            args_id,
            &param.field_path,
            &plan.captures,
            assigner,
        )
    {
        return true;
    }
    rewrite_args(
        package,
        call_site.call_expr_id,
        args_id,
        &plan.input_path,
        plan.controlled_layers,
        &plan.captures,
        expr_owner_lookup,
        assigner,
    );
    true
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
    expr_owner_lookup: &ExprOwnerLookup,
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
    let mut capture_ranges = Vec::new();
    for (call_site, param) in members {
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
            let start = captures.len();
            captures.extend(filter_threaded_rewrite_captures(
                package,
                member_captures
                    .iter()
                    .map(|capture| {
                        let mut resolved = capture.clone();
                        if resolved.expr.is_none() {
                            resolved.expr = resolve_capture_expr_from_arg(
                                package,
                                call_site.arg_expr_id,
                                capture.local.var,
                            );
                        }
                        resolved
                    })
                    .collect(),
            ));
            capture_ranges.push((param, start..captures.len()));
        }
    }

    let Some(destination) = expr_owner_lookup.scope(&call_expr_id) else {
        return;
    };
    if !captures_belong_to_destination(destination, &captures) {
        return;
    }

    let mut statements = Vec::new();
    let consumed: Vec<_> = members.iter().map(|(site, _)| site.arg_expr_id).collect();
    for (param, range) in capture_ranges {
        if captures[range.clone()]
            .iter()
            .any(|capture| capture.expr.is_some())
        {
            let path = callable_param_input_path(package, callee_id, param);
            statements.extend(materialize_argument_prefix(
                package, args_id, &path, &consumed, assigner,
            ));
            materialize_capture_operands(
                package,
                destination,
                &mut captures[range],
                &mut statements,
                assigner,
            );
        }
    }

    // Retarget the callee to the combined specialization with the rebuilt type.
    let new_callee_ty =
        build_specialized_multi_callee_ty(package, callee_id, &remove_indices, &captures);
    rewrite_specialized_callee(package, callee_id, spec_item_id, new_callee_ty, assigner);

    // Rebuild the argument tuple to match the combined input pattern. The
    // owner callable lets a non-inline tuple argument be projected through its
    // local initializer.
    rewrite_args_remove_tuple_elements(
        package,
        args_id,
        expr_owner_lookup.callable(&call_expr_id),
        destination,
        &remove_indices,
        &captures,
        assigner,
    );
    prepend_evaluations(package, call_expr_id, statements, assigner);
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
    expr_owner_lookup: &ExprOwnerLookup,
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

    let Some(destination) = expr_owner_lookup.scope(&call_expr_id) else {
        return;
    };
    if !captures_belong_to_destination(destination, &captures) {
        return;
    }

    if !first_call_site.hof_input_is_tuple && param.field_path.is_empty() {
        rewrite_single_arg_root(package, destination, args_id, &captures, assigner);
        let new_callee_ty = if let Ty::Arrow(arrow) = &package.get_expr(callee_id).ty {
            let mut arrow = arrow.clone();
            arrow.input = Box::new(package.get_expr(args_id).ty.clone());
            Some(Ty::Arrow(arrow))
        } else {
            None
        };
        rewrite_specialized_callee(package, callee_id, spec_item_id, new_callee_ty, assigner);
        return;
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
        destination,
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
    let (_, functor) = peel_body_functors(package, callee_id);
    let mut input = arrow.input.as_ref();
    for _ in 0..functor.controlled {
        let Ty::Tuple(items) = input else {
            return None;
        };
        input = items.get(1)?;
    }
    let new_input = remove_tys_at_indices(package, input, remove_indices, captures);
    let new_input = apply_target_input_at_control_path(
        &arrow.input,
        &new_input,
        usize::from(functor.controlled),
    );
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
    destination: CaptureScope,
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

        let capture_ids =
            allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
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

    // Struct literals and stored aggregates use the same field-removal helper,
    // which also appends captures to match the retargeted callee input.
    if let Some((kind, ty)) = remove_top_level_field_from_expr_data(
        package,
        owner_callable,
        args_id,
        &remove,
        captures,
        destination.into(),
        assigner,
    ) {
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = kind;
        args_mut.ty = ty;
    }

    // If none of the supported projections applies, leave the argument
    // unchanged. This helper does not undo the callee retargeting.
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
    expr_owner_lookup: &ExprOwnerLookup,
    assigner: &mut Assigner,
) {
    let owner_callable = expr_owner_lookup.get(&call_expr_id).copied();
    let Some(destination) = expr_owner_lookup.scope(&call_expr_id) else {
        return;
    };
    let args_expr = package
        .exprs
        .get(args_id)
        .expect("args expr not found")
        .clone();

    if input_path.is_empty() {
        rewrite_single_arg_root(package, destination, args_id, captures, assigner);
    } else if matches!(args_expr.kind, ExprKind::Tuple(_)) {
        if input_path.len() == 1 {
            rewrite_args_remove_tuple_element(
                package,
                destination,
                args_id,
                input_path[0],
                captures,
                assigner,
            );
        } else {
            rewrite_args_nested_tuple_input(
                package,
                owner_callable,
                destination,
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
    destination: CaptureScope,
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
            let capture_ids =
                allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
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
            rewrite_single_arg_root(package, destination, args_id, captures, assigner);
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
    destination: CaptureScope,
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
            destination.into(),
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
                destination,
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
            let capture_ids =
                allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
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
    destination: CaptureScope,
    controlled_layers: usize,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) {
    if controlled_layers == 0 {
        let original = package.get_expr(tuple_id).clone();
        let span = original.span;
        let capture_ids = allocate_capture_exprs(package, span, destination, captures, assigner);
        let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
        if !matches!(original.kind, ExprKind::Tuple(_)) {
            let payload_id =
                alloc_expr(package, assigner, original.ty.clone(), original.kind, span);
            let mut elements = vec![payload_id];
            elements.extend(capture_ids);
            let mut tys = vec![original.ty];
            tys.extend(capture_tys);
            let tuple_mut = package
                .exprs
                .get_mut(tuple_id)
                .expect("args expr not found");
            tuple_mut.kind = ExprKind::Tuple(elements);
            tuple_mut.ty = Ty::Tuple(tys);
            return;
        }
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
        destination,
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
    expr_owner_lookup: &ExprOwnerLookup,
    assigner: &mut Assigner,
) {
    if rewrite_local_single_arg_nested(
        package,
        expr_owner_lookup.get(&call_expr_id).copied(),
        expr_owner_lookup
            .scope(&call_expr_id)
            .map_or(CaptureDestination::Unknown, CaptureDestination::Known),
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
            expr_owner_lookup
                .scope(&call_expr_id)
                .map_or(CaptureDestination::Unknown, CaptureDestination::Known),
            args_id,
            &remove_indices,
            captures,
            assigner,
        ) {
            return;
        }
        if let Some((kind, ty)) = remove_top_level_field_from_expr_data(
            package,
            expr_owner_lookup.get(&call_expr_id).copied(),
            args_id,
            &remove_indices,
            captures,
            expr_owner_lookup
                .scope(&call_expr_id)
                .map_or(CaptureDestination::Unknown, CaptureDestination::Known),
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
        let capture_ids = allocate_capture_exprs(
            package,
            span,
            expr_owner_lookup
                .scope(&call_expr_id)
                .map_or(CaptureDestination::Unknown, CaptureDestination::Known),
            captures,
            assigner,
        );
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
            let payload_id = alloc_expr(
                package,
                assigner,
                modified_expr.ty.clone(),
                modified_expr.kind.clone(),
                span,
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

/// Removes the forwarded callable-array fields from a call's argument tuple and
/// threads any closure captures through, for a call routed to a combined
/// callable-array specialization by [`rewrite_callable_array_multi`].
///
/// The callable array sits in one of two argument shapes, selected by
/// `uses_tuple_input`:
/// - A multi-parameter HOF whose input is a tuple of parameters, where the
///   array lives in the `top_level_param` slot of that tuple.
/// - A single tuple-valued parameter whose whole argument *is* that tuple.
///
/// Fields to drop are identified positionally by `remove_indices` and by the
/// exact forwarded element expression via `remove_expr_ids`, matching the
/// callee-type reduction already applied by
/// [`build_nested_callable_array_callee_ty`].
///
/// # Before (`uses_tuple_input`, `top_level_param = 1`)
/// ```text
/// (other_arg, (callable0, callable1, keep), more_args)
/// ```
/// # After
/// ```text
/// (other_arg, (keep), more_args, capture0, ...)   // array fields removed, captures appended
/// ```
///
/// # Mutations
/// - Rewrites the inner tuple slot (or `args_id` itself) in place via
///   [`rewrite_nested_arg_expr_remove_fields`].
/// - Refreshes the outer tuple's slot type and appends closure captures.
/// - Allocates capture `Expr` nodes through `assigner`.
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
    destination: CaptureScope,
    expr_owner_lookup: &ExprOwnerLookup,
    assigner: &mut Assigner,
) {
    let args_expr = package.get_expr(args_id).clone();
    let owner_callable = expr_owner_lookup.get(&call_expr_id).copied();

    if uses_tuple_input {
        // Multi-parameter HOF: the callable array occupies one slot of the
        // top-level argument tuple. Descend into `elements[top_level_param]`,
        // strip the callable fields there, then refresh that slot's type and
        // append any captures as top-level siblings of the surviving arguments.
        if let ExprKind::Tuple(elements) = args_expr.kind {
            let inner_id = elements[top_level_param];
            if rewrite_nested_arg_expr_remove_fields(
                package,
                owner_callable,
                inner_id,
                remove_indices,
                remove_expr_ids,
                // No captures here: they belong at the top level (siblings of the
                // other parameters), not inside this one parameter's slot, so they
                // are appended to the outer `args_id` tuple below instead.
                &[],
                destination.into(),
                assigner,
            ) {
                // Refresh the outer tuple's type for the now-reduced inner slot.
                let inner_ty = package.get_expr(inner_id).ty.clone();
                let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
                if let Ty::Tuple(ref mut tys) = args_mut.ty {
                    tys[top_level_param] = inner_ty;
                }
                if !captures.is_empty() {
                    // Append closure captures (and their types) as top-level
                    // siblings, matching the combined callee's input pattern.
                    let capture_ids = allocate_capture_exprs(
                        package,
                        args_expr.span,
                        destination,
                        captures,
                        assigner,
                    );
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

    // Single tuple-valued parameter: the whole argument is the tuple holding the
    // callable array, so strip the fields directly from `args_id` and thread the
    // captures in the same pass. The returned success flag is unused because
    // there is no enclosing slot type to refresh afterward.
    let _ = rewrite_nested_arg_expr_remove_fields(
        package,
        owner_callable,
        args_id,
        remove_indices,
        remove_expr_ids,
        captures,
        destination.into(),
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
    destination: CaptureDestination,
    args_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> bool {
    let args_expr = package.get_expr(args_id).clone();
    let Some((payload_kind, payload_ty)) = remove_top_level_field_from_expr_data_with_exprs(
        package,
        owner_callable,
        args_id,
        remove_indices,
        &FxHashSet::default(),
        &[],
        destination,
        assigner,
    ) else {
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

    let capture_ids =
        allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();

    let (mut elements, mut tys) = if payload_is_empty {
        (Vec::new(), Vec::new())
    } else {
        let payload_id = alloc_expr(
            package,
            assigner,
            payload_ty.clone(),
            payload_kind,
            args_expr.span,
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
#[allow(clippy::too_many_arguments)]
fn rewrite_nested_arg_expr_remove_fields(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    args_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    remove_expr_ids: &FxHashSet<ExprId>,
    captures: &[CapturedVar],
    destination: CaptureDestination,
    assigner: &mut Assigner,
) -> bool {
    let Some((kind, ty)) = remove_top_level_field_from_expr_data_with_exprs(
        package,
        owner_callable,
        args_id,
        remove_indices,
        remove_expr_ids,
        captures,
        destination,
        assigner,
    ) else {
        return false;
    };

    let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
    args_expr.kind = kind;
    args_expr.ty = ty;
    true
}

/// Rewrites a single local UDT/tuple argument by removing the specialized
/// callable field. Stored values are projected unless replaying the initializer
/// is proven to preserve both evaluation and value.
///
/// # Before
/// ```text
/// args = Var(local_udt)   // bound to (field0, callable, field2)
/// ```
/// # After
/// ```text
/// args = ((field0, field2), captures...)   // surviving fields form one payload
/// ```
///
/// Without runtime captures, the surviving payload is used directly.
///
/// # Mutations
/// - Overwrites `args_id`'s `ExprKind` and `Ty` in place.
/// - Allocates capture `Expr` nodes through `assigner`.
fn rewrite_local_single_arg_nested(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    destination: CaptureDestination,
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
            destination,
            args_id,
            &remove_indices,
            captures,
            assigner,
        );
    }

    // Reconstruct only stable, unobservable initializers; otherwise use the
    // stored value so later writes cannot change its fields retroactively.
    let args_expr = package.get_expr(args_id).clone();
    let source_id = if let ExprKind::Var(Res::Local(local_var), _) = args_expr.kind
        && let Some(owner_callable) = owner_callable
        && let Some(init_expr_id) = replayable_local_initializer(package, owner_callable, local_var)
    {
        init_expr_id
    } else if matches!(
        args_expr.kind,
        ExprKind::Tuple(_) | ExprKind::Struct(_, _, _) | ExprKind::Var(Res::Local(_), _)
    ) || type_constructor_args(package, &args_expr).is_some()
    {
        args_id
    } else {
        // Other unresolved args fall back to the caller's existing
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
    // closure captures, mirroring the len == 1 handling in
    // `rewrite_nested_arg_expr_remove_fields_as_payload`. The payload is a FRESH
    // expr id, avoiding a self-referential `Tuple([args_id, ...])` cycle.
    let payload_id = alloc_expr(package, assigner, ty.clone(), kind, args_expr.span);
    let capture_ids =
        allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
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
    owner_callable: Option<LocalItemId>,
    expr_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    captures: &[CapturedVar],
    destination: CaptureDestination,
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    remove_top_level_field_from_expr_data_with_exprs(
        package,
        owner_callable,
        expr_id,
        remove_indices,
        &FxHashSet::default(),
        captures,
        destination,
        assigner,
    )
}

/// Builds replacement expression data for a call-argument aggregate after
/// removing the callable fields at `remove_indices` or `remove_expr_ids` and
/// appending capture expressions.
///
/// Handles tuple and struct aggregates, projecting stored locals when their
/// initializers cannot safely be replayed. Only type constructors are unwrapped
/// to their arguments; ordinary calls used as copy sources retain their returned
/// values. Surviving struct fields follow declaration order even when their
/// initializers use a different evaluation order. Returns `None` for a shape it
/// does not rewrite.
#[allow(clippy::too_many_arguments)]
fn remove_top_level_field_from_expr_data_with_exprs(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    expr_id: ExprId,
    remove_indices: &FxHashSet<usize>,
    remove_expr_ids: &FxHashSet<ExprId>,
    captures: &[CapturedVar],
    destination: CaptureDestination,
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let expr = package.get_expr(expr_id).clone();
    let (statements, expr) = if struct_argument_needs_materialization(package, &expr) {
        materialize_struct_argument(package, &expr, assigner)
    } else {
        (Vec::new(), expr)
    };
    let mut remaining = match &expr.kind {
        ExprKind::Var(Res::Local(local), _) => {
            if let Some(init) = owner_callable
                .and_then(|owner| replayable_local_initializer(package, owner, *local))
            {
                return remove_top_level_field_from_expr_data_with_exprs(
                    package,
                    owner_callable,
                    init,
                    remove_indices,
                    remove_expr_ids,
                    captures,
                    destination,
                    assigner,
                );
            }
            tuple_argument_elements(package, &expr, assigner)?
                .into_iter()
                .enumerate()
                .filter(|(index, _)| !remove_indices.contains(index))
                .map(|(_, field)| field)
                .collect()
        }
        ExprKind::Call(..) => {
            let args_id = type_constructor_args(package, &expr)?;
            return remove_top_level_field_from_expr_data_with_exprs(
                package,
                owner_callable,
                args_id,
                remove_indices,
                remove_expr_ids,
                captures,
                destination,
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
        ExprKind::Struct(_, copy_id, fields) => collect_struct_fields_after_removal(
            package,
            owner_callable,
            &expr,
            *copy_id,
            fields,
            remove_indices,
            remove_expr_ids,
            assigner,
        )?,
        _ => return None,
    };

    remaining.extend(allocate_capture_exprs(
        package,
        expr.span,
        destination,
        captures,
        assigner,
    ));

    let (kind, ty) = build_expr_data_from_elements(package, remaining);
    Some(wrap_argument_statements(
        package, statements, kind, ty, expr.span, assigner,
    ))
}

/// Rebuilds the surviving field values of a `Struct` expression
/// after removing selected fields, so a callable field spliced out of a struct
/// literal leaves a well-formed argument aggregate.
///
/// Explicitly-assigned fields are taken from `fields`; every other field is
/// materialized from the optional base copy expression via
/// [`materialize_struct_copy_field`]. Fields at `remove_indices`, and any value
/// in `remove_expr_ids`, are dropped. Results follow declaration order, not
/// field-initializer order. Returns `None` for a missing field without a copy
/// source or a shape it cannot decompose.
#[allow(clippy::too_many_arguments)]
fn collect_struct_fields_after_removal(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    expr: &Expr,
    copy_id: Option<ExprId>,
    fields: &[FieldAssign],
    remove_indices: &FxHashSet<usize>,
    remove_expr_ids: &FxHashSet<ExprId>,
    assigner: &mut Assigner,
) -> Option<Vec<ExprId>> {
    let field_tys = struct_top_level_field_tys(package, &expr.ty)?;
    let mut explicit_fields: FxHashMap<usize, ExprId> = FxHashMap::default();
    for field in fields {
        let Field::Path(path) = &field.field else {
            return None;
        };
        let Some(&top_level_index) = path.indices.first() else {
            continue;
        };
        if path.indices.len() != 1 {
            return None;
        }
        explicit_fields.insert(top_level_index, field.value);
    }

    let mut remaining = Vec::new();
    for (index, ty) in field_tys.into_iter().enumerate() {
        if remove_indices.contains(&index) {
            continue;
        }
        if let Some(&field_id) = explicit_fields.get(&index) {
            if !remove_expr_ids.contains(&field_id) {
                remaining.push(field_id);
            }
            continue;
        }
        let field_id = materialize_struct_copy_field(
            package,
            owner_callable,
            copy_id?,
            index,
            ty,
            expr.span,
            assigner,
        )?;
        if !remove_expr_ids.contains(&field_id) {
            remaining.push(field_id);
        }
    }
    Some(remaining)
}

/// Returns the top-level field types of a struct/UDT type, resolving UDT
/// wrappers to their underlying tuple; `None` when the type is not a tuple.
fn struct_top_level_field_tys(package: &Package, ty: &Ty) -> Option<Vec<Ty>> {
    match resolve_udt_ty(package, ty) {
        Ty::Tuple(tys) => Some(tys),
        _ => None,
    }
}

/// Produces an expression for field `field_index` of a struct copy source.
///
/// Follows the copy expression to its underlying value — a proven replayable
/// local initializer, a tuple element, a type constructor's arguments, or a
/// nested struct field — returning the concrete sub-expression when one is
/// found. Ordinary calls and other opaque values get a synthesized `Field`
/// projection of their result; missing tuple/struct fields return `None`.
fn materialize_struct_copy_field(
    package: &mut Package,
    owner_callable: Option<LocalItemId>,
    copy_id: ExprId,
    field_index: usize,
    field_ty: Ty,
    span: PackageSpan,
    assigner: &mut Assigner,
) -> Option<ExprId> {
    let copy_expr = package.get_expr(copy_id).clone();
    if let Some(args_id) = type_constructor_args(package, &copy_expr) {
        return materialize_struct_copy_field(
            package,
            owner_callable,
            args_id,
            field_index,
            field_ty,
            span,
            assigner,
        );
    }
    match copy_expr.kind {
        ExprKind::Var(Res::Local(local_var), _)
            if let Some(owner_callable) = owner_callable
                && let Some(init_expr_id) =
                    replayable_local_initializer(package, owner_callable, local_var) =>
        {
            materialize_struct_copy_field(
                package,
                Some(owner_callable),
                init_expr_id,
                field_index,
                field_ty,
                span,
                assigner,
            )
        }
        ExprKind::Tuple(elements) => elements.get(field_index).copied(),
        ExprKind::Struct(_, nested_copy, fields) => {
            for field in fields {
                let Field::Path(path) = &field.field else {
                    return None;
                };
                if path.indices.as_slice() == [field_index] {
                    return Some(field.value);
                }
            }
            if let Some(nested_copy) = nested_copy {
                materialize_struct_copy_field(
                    package,
                    owner_callable,
                    nested_copy,
                    field_index,
                    field_ty,
                    span,
                    assigner,
                )
            } else {
                None
            }
        }
        _ => Some(alloc_expr(
            package,
            assigner,
            field_ty,
            ExprKind::Field(
                copy_id,
                Field::Path(FieldPath {
                    indices: vec![field_index],
                }),
            ),
            span,
        )),
    }
}

/// Constructor callees name their result type's item, including foreign types.
/// An ordinary function returning that type must retain its computed result.
fn type_constructor_args(package: &Package, expr: &Expr) -> Option<ExprId> {
    let ExprKind::Call(callee, args) = expr.kind else {
        return None;
    };
    match (&package.get_expr(callee).kind, &expr.ty) {
        (ExprKind::Var(Res::Item(callee_item), _), Ty::Udt(Res::Item(type_item)))
            if callee_item == type_item =>
        {
            Some(args)
        }
        _ => None,
    }
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
/// (1, 5)   // OpBox's callable field removed and its sole surviving field unwrapped
/// ```
///
/// Struct literals use declaration-ordered fields, including fields inherited
/// from a copy source. Tuple-valued locals and their fields are projected without re-evaluating
/// their initializers. Returns `None` for an empty path, an unsupported shape,
/// or an out-of-range index.
fn build_removed_nested_expr_data(
    package: &mut Package,
    expr_id: ExprId,
    field_path: &[usize],
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let expr = package.get_expr(expr_id).clone();

    if struct_argument_needs_materialization(package, &expr) {
        return build_ordered_struct_argument(package, &expr, field_path, assigner);
    }

    // Unwrap a UDT-constructor `Call(ctor, args)` to its argument aggregate.
    if let Some(inner_args_id) = type_constructor_args(package, &expr) {
        return build_removed_nested_expr_data(package, inner_args_id, field_path, assigner);
    }

    let (&index, rest) = field_path.split_first()?;

    let elements = tuple_argument_elements(package, &expr, assigner)?;
    if index >= elements.len() {
        return None;
    }

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
    let child_id = alloc_expr(package, assigner, child_ty, child_kind, child_span);
    let mut new_elements = elements;
    new_elements[index] = child_id;
    let new_tys: Vec<Ty> = new_elements
        .iter()
        .map(|&id| package.get_expr(id).ty.clone())
        .collect();
    Some((ExprKind::Tuple(new_elements), Ty::Tuple(new_tys)))
}

/// Reuses literal aggregate children or projects a stored tuple value. Following
/// a local's initializer instead could duplicate effects or reconstruct a value
/// from operands that have since changed. Struct fields are returned in
/// declaration order so tuple paths remain valid for reordered initializers.
fn tuple_argument_elements(
    package: &mut Package,
    expr: &Expr,
    assigner: &mut Assigner,
) -> Option<Vec<ExprId>> {
    if let ExprKind::Tuple(elements) = &expr.kind {
        return Some(elements.clone());
    }
    if let ExprKind::Struct(_, copy, fields) = &expr.kind {
        return collect_struct_fields_after_removal(
            package,
            None,
            expr,
            *copy,
            fields,
            &FxHashSet::default(),
            &FxHashSet::default(),
            assigner,
        );
    }
    let mut root = expr.id;
    while let ExprKind::Field(base, _) = package.get_expr(root).kind {
        root = base;
    }
    if !matches!(package.get_expr(root).kind, ExprKind::Var(Res::Local(_), _)) {
        return None;
    }
    let Ty::Tuple(types) = resolve_udt_ty(package, &expr.ty) else {
        return None;
    };
    // Callers may overwrite the argument root with the rebuilt payload. Keep
    // projections rooted in a separate read to avoid a self-referential graph.
    let stored = alloc_expr(
        package,
        assigner,
        expr.ty.clone(),
        expr.kind.clone(),
        expr.span,
    );
    Some(
        types
            .into_iter()
            .enumerate()
            .map(|(index, ty)| {
                crate::fir_builder::alloc_field_expr(
                    package, assigner, stored, index, ty, expr.span,
                )
            })
            .collect(),
    )
}

fn struct_argument_needs_materialization(package: &Package, expr: &Expr) -> bool {
    let ExprKind::Struct(_, copy, _) = expr.kind else {
        return false;
    };
    copy.is_some_and(|id| {
        let value = package.get_expr(id);
        matches!(value.kind, ExprKind::Call(..)) && type_constructor_args(package, value).is_none()
    }) || !expr_is_safe_to_discard(package, package.id, expr.id)
}

/// Field layout follows declaration order, but evaluation follows source order.
/// Capture the copy value and initializers once before reducing their layout;
/// otherwise projections can duplicate a factory call and reordered fields can
/// reorder effects or change which failure occurs.
fn materialize_struct_argument(
    package: &mut Package,
    expr: &Expr,
    assigner: &mut Assigner,
) -> (Vec<StmtId>, Expr) {
    let ExprKind::Struct(name, copy, fields) = &expr.kind else {
        unreachable!("ordered struct arguments require a Struct expression");
    };
    let mut statements = Vec::new();
    let mut bind_value = |package: &mut Package, id: ExprId| {
        let value = package.get_expr(id);
        let ty = value.ty.clone();
        let span = value.span;
        let (local, statement) = crate::fir_builder::alloc_local_var(
            package,
            assigner,
            "_.branch_field",
            &ty,
            id,
            Mutability::Immutable,
        );
        statements.push(statement);
        crate::fir_builder::alloc_local_var_expr(package, assigner, local, ty, span)
    };
    let copy = copy.map(|id| bind_value(package, id));
    let fields = fields
        .iter()
        .map(|field| FieldAssign {
            value: bind_value(package, field.value),
            ..field.clone()
        })
        .collect();
    let stored = Expr {
        kind: ExprKind::Struct(*name, copy, fields),
        ..expr.clone()
    };
    (statements, stored)
}

fn build_ordered_struct_argument(
    package: &mut Package,
    expr: &Expr,
    field_path: &[usize],
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let (statements, stored) = materialize_struct_argument(package, expr, assigner);
    let elements = tuple_argument_elements(package, &stored, assigner)?;
    let ty = Ty::Tuple(
        elements
            .iter()
            .map(|id| package.get_expr(*id).ty.clone())
            .collect(),
    );
    let tuple = alloc_expr(package, assigner, ty, ExprKind::Tuple(elements), expr.span);
    let (kind, ty) = build_removed_nested_expr_data(package, tuple, field_path, assigner)?;
    Some(wrap_argument_statements(
        package, statements, kind, ty, expr.span, assigner,
    ))
}

fn wrap_argument_statements(
    package: &mut Package,
    mut statements: Vec<StmtId>,
    kind: ExprKind,
    ty: Ty,
    span: PackageSpan,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    if statements.is_empty() {
        return (kind, ty);
    }
    let value = alloc_expr(package, assigner, ty.clone(), kind, span);
    statements.push(crate::fir_builder::alloc_expr_stmt(
        package, assigner, value, span,
    ));
    let block = crate::fir_builder::alloc_block(package, assigner, statements, ty.clone(), span);
    (ExprKind::Block(block), ty)
}

/// Rewrites a single-parameter call's args expression after the callable
/// argument has been removed.
///
/// Before, `args_id` evaluates to the callable argument itself. After, it
/// evaluates to `()` with no runtime captures, the capture value itself with
/// one capture, or a tuple with multiple captures.
fn rewrite_single_arg_root(
    package: &mut Package,
    destination: CaptureScope,
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
        let capture_ids =
            allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
        let single = package
            .exprs
            .get(capture_ids[0])
            .expect("capture expr not found")
            .clone();
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = single.kind;
        args_mut.ty = single.ty;
    } else {
        let capture_ids =
            allocate_capture_exprs(package, args_expr.span, destination, captures, assigner);
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
///
/// # Discarded evaluation
///
/// The selected element is deleted outright, with no purity guard here. That
/// is sound only because [`super::analysis`] already classified this call
/// site's callable argument through
/// [`consumed_callable_expr_disposition`] and declined the call site when the
/// disposition was [`EvaluationDisposition::Retained`]. This function is one
/// member of a family that shares that single guarantee —
/// [`rewrite_args_remove_tuple_element`], [`rewrite_single_arg_root`],
/// [`remove_top_level_field_from_expr_data`], and the branch-dispatch argument
/// builders drop the same expression by other routes. Adding a guard here
/// alone would leave the rest of the family unprotected, so the decision stays
/// at the one point that covers all of them.
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
    build_specialized_callee_ty_from_expr(
        package,
        package.get_expr(callee_id),
        input_path,
        concrete,
    )
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
/// An empty path replaces the entire root with `Unit`, a single capture type,
/// or a tuple of capture types. With a one-element path and a non-tuple input,
/// the result is `Unit` or a tuple of capture types, without singleton flattening.
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

/// Reports whether `ty` has an arrow at its root or under tuple fields after
/// UDT expansion. Arrays remain opaque to this cleanup predicate.
fn local_ty_contains_arrow_through_udts(package: &Package, ty: &Ty) -> bool {
    ty_contains_arrow(&resolve_udt_ty(package, ty))
}

/// Resolves a type through user-defined-type wrappers to its underlying
/// structural type, recursing into tuples, arrays, and arrow inputs and
/// outputs.
///
/// Unguarded UDT recursion; terminates only because the frontend rejects cyclic UDTs.
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
    let span = package.get_expr(callee_id).span;
    let base_kind = ExprKind::Var(Res::Item(item_id), Vec::new());

    if !functor.adjoint && functor.controlled == 0 {
        let expr = package
            .exprs
            .get_mut(callee_id)
            .expect("callee expr not found");
        expr.kind = base_kind;
        expr.ty = callee_ty;
        return;
    }

    // Rebuild the functor wrapper chain from the inside out, then copy the
    // outermost node back into the original callee slot.
    let outer_id =
        alloc_functor_wrapped_expr(package, assigner, base_kind, functor, &callee_ty, span);
    let outermost_kind = package
        .exprs
        .get(outer_id)
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
#[allow(clippy::too_many_lines)]
fn branch_split_rewrite(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    entries: &[HofDispatchTarget],
    constants: &[(&CallSite, &CallableParam)],
    expr_owner_lookup: &ExprOwnerLookup,
    assigner: &mut Assigner,
) {
    let orig_call = package.get_expr(call_expr_id).clone();
    let ExprKind::Call(orig_callee_id, orig_args_id) = orig_call.kind else {
        return;
    };
    let span = orig_call.span;
    let result_ty = orig_call.ty.clone();

    // Dispatch chooses among alternatives for one parameter. Static siblings
    // are threaded or consumed by each leaf's specialization; they must not
    // enter the index-dispatch candidate set.
    let restricted = restrict_to_dispatched_parameter(entries);
    let entries: &[HofDispatchTarget] = &restricted;
    if entries.first().is_some_and(|first| {
        !dispatch_source_has_discriminator(
            package,
            expr_owner_lookup,
            call_expr_id,
            first.0.arg_expr_id,
            entries
                .iter()
                .map(|entry| (&entry.0.callable_arg, !entry.0.condition.is_empty())),
        )
    }) {
        return;
    }
    let Some(destination) = expr_owner_lookup.scope(&call_expr_id) else {
        return;
    };
    for (call_site, _, _) in entries {
        let captures = match &call_site.callable_arg {
            ConcreteCallable::Closure { captures, .. } => {
                resolve_rewrite_captures(package, call_site.arg_expr_id, captures)
            }
            _ => Vec::new(),
        };
        if !captures_belong_to_destination(destination, &captures) {
            return;
        }
    }
    for (call_site, _) in constants {
        let captures = match &call_site.callable_arg {
            ConcreteCallable::Closure { captures, .. } => {
                resolve_rewrite_captures(package, call_site.arg_expr_id, captures)
            }
            _ => Vec::new(),
        };
        if !captures_belong_to_destination(destination, &captures) {
            return;
        }
    }

    let Some((conditioned, default_entry, bounds_check)) = partition_branch_split_targets(
        package,
        package_id,
        expr_owner_lookup,
        call_expr_id,
        entries,
        assigner,
    ) else {
        return;
    };

    if conditioned.is_empty() {
        // Single effective entry. For a combined group the entry's spec id is a
        // per-candidate combined spec whose input pattern removes every member
        // slot, so the single-slot `create_branch_call` would mis-shape the
        // call. Leave the call unchanged rather than apply a single-slot rewrite.
        if !constants.is_empty() {
            return;
        }
        // Single effective entry — use normal rewrite.
        let Some(rewrite_plan) =
            plan_rewrite_one(package, default_entry.0, default_entry.2, expr_owner_lookup)
        else {
            return;
        };
        let prefix = if bounds_check.is_some() {
            let path = callable_param_input_path(package, orig_callee_id, default_entry.2);
            materialize_argument_prefix(package, orig_args_id, &path, &[], assigner)
        } else {
            Vec::new()
        };
        let _ = rewrite_one(
            package,
            package_id,
            default_entry.0,
            default_entry.2,
            default_entry.1,
            rewrite_plan,
            expr_owner_lookup,
            assigner,
        );
        if let Some(bounds_check) = bounds_check {
            prepend_evaluations(package, bounds_check, prefix, assigner);
            prepend_bounds_check_to_rewritten_call(package, assigner, call_expr_id, bounds_check);
        }
        return;
    }

    let input_path = callable_param_input_path(package, orig_callee_id, entries[0].2);
    let consumed: Vec<_> = constants
        .iter()
        .map(|(site, _)| site.arg_expr_id)
        .chain(entries.iter().map(|(site, _, _)| site.arg_expr_id))
        .collect();
    let mut owned_constants: Vec<_> = constants
        .iter()
        .map(|(site, param)| ((*site).clone(), *param))
        .collect();
    let mut statements = materialize_early_mixed_captures(
        package,
        orig_callee_id,
        orig_args_id,
        &input_path,
        &consumed,
        &mut owned_constants,
        destination,
        assigner,
    );
    statements.extend(materialize_argument_prefix(
        package,
        orig_args_id,
        &input_path,
        &consumed,
        assigner,
    ));
    let constants: Vec<_> = owned_constants
        .iter()
        .map(|(site, param)| (site, *param))
        .collect();
    install_branch_split_dispatch(
        package,
        package_id,
        call_expr_id,
        orig_callee_id,
        orig_args_id,
        span,
        &result_ty,
        CaptureWriterContext {
            owner_callable: expr_owner_lookup.callable(&call_expr_id),
            destination,
        },
        conditioned,
        default_entry,
        bounds_check,
        &constants,
        assigner,
    );
    prepend_evaluations(package, call_expr_id, statements, assigner);
}

#[allow(clippy::too_many_arguments)]
fn materialize_early_mixed_captures(
    package: &mut Package,
    callee: ExprId,
    args: ExprId,
    dispatch_path: &[usize],
    consumed: &[ExprId],
    constants: &mut [(CallSite, &CallableParam)],
    destination: CaptureScope,
    assigner: &mut Assigner,
) -> Vec<StmtId> {
    constants.sort_by_key(|(_, param)| callable_param_input_path(package, callee, param));
    let mut statements = Vec::new();
    for (site, param) in constants {
        let path = callable_param_input_path(package, callee, param);
        if path.as_slice() >= dispatch_path {
            continue;
        }
        if let ConcreteCallable::Closure { captures, .. } = &mut site.callable_arg {
            *captures = resolve_rewrite_captures(package, site.arg_expr_id, captures);
            statements.extend(materialize_argument_prefix(
                package, args, &path, consumed, assigner,
            ));
            materialize_capture_operands(package, destination, captures, &mut statements, assigner);
        }
    }
    statements
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
    package_id: PackageId,
    expr_owner_lookup: &ExprOwnerLookup,
    call_expr_id: ExprId,
    entries: &[HofDispatchTarget<'a>],
    assigner: &mut Assigner,
) -> Option<(
    Vec<ConditionedHofTarget<'a>>,
    HofDispatchTarget<'a>,
    Option<ExprId>,
)> {
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

    let mut bounds_check = None;
    if conditioned.is_empty()
        && entries.len() > 1
        && let Some(plan) = synthesize_callsite_index_dispatch(
            package,
            package_id,
            expr_owner_lookup,
            call_expr_id,
            entries,
            assigner,
        )
    {
        conditioned = plan
            .conditioned
            .into_iter()
            .map(|(entry_idx, condition)| (entries[entry_idx], vec![condition]))
            .collect();
        default = Some(entries[plan.default_index]);
        bounds_check = Some(plan.bounds_check);
    }
    if conditioned.is_empty() && entries.len() > 1 {
        return None;
    }

    // Must have a default for the else branch; steal last conditioned if needed.
    let default_entry = if let Some(d) = default {
        d
    } else {
        conditioned.pop()?.0
    };
    Some((conditioned, default_entry, bounds_check))
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
    span: PackageSpan,
    result_ty: &Ty,
    writer: CaptureWriterContext,
    conditioned: Vec<ConditionedHofTarget<'a>>,
    default_entry: HofDispatchTarget<'a>,
    bounds_check: Option<ExprId>,
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
                writer.destination,
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
                writer,
                cs,
                param,
                constants,
                spec_id,
                assigner,
            )
        }
    };
    let mut dispatch_id = build_branch_tree(
        package,
        span,
        result_ty,
        conditioned,
        default_entry,
        assigner,
        &mut build_call,
    );
    if let Some(bounds_check) = bounds_check {
        dispatch_id = prepend_bounds_check(
            package,
            assigner,
            bounds_check,
            dispatch_id,
            result_ty.clone(),
            span,
        );
    }

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
    package_id: PackageId,
    orig_callee: &Expr,
    orig_args: &Expr,
    span: PackageSpan,
    result_ty: &Ty,
    destination: CaptureScope,
    call_site: &CallSite,
    param: &CallableParam,
    spec_store_id: StoreItemId,
    assigner: &mut Assigner,
) -> ExprId {
    let spec_item_id = ItemId {
        package: spec_store_id.package,
        item: spec_store_id.item,
    };

    let (_, functor) = peel_body_functors(package, orig_callee.id);
    let controlled_layers = usize::from(functor.controlled);
    let input_path = callable_param_input_path(package, orig_callee.id, param);
    assert_eq!(
        spec_store_id.package, package_id,
        "branch specialization should be local"
    );
    let new_callee_ty =
        build_direct_global_callee_ty(package, spec_item_id, &orig_callee.ty, controlled_layers)
            .expect("specialized branch should have a callable signature");
    let callee_id =
        alloc_specialized_callee_expr(package, orig_callee, spec_item_id, &new_callee_ty, assigner);

    // Build args: remove callable param + append captures.
    let mut captures = match &call_site.callable_arg {
        ConcreteCallable::Closure { captures, .. } => filter_threaded_rewrite_captures(
            package,
            resolve_rewrite_captures(package, call_site.arg_expr_id, captures),
        ),
        _ => Vec::new(),
    };
    let mut statements = Vec::new();
    materialize_capture_operands(
        package,
        destination,
        &mut captures,
        &mut statements,
        assigner,
    );
    let (args_kind, args_ty) = build_branch_args_data(
        package,
        orig_args,
        destination,
        &input_path,
        controlled_layers,
        param.hof_input_is_tuple,
        &captures,
        span,
        assigner,
    );

    let args_id = alloc_expr(package, assigner, args_ty, args_kind, span);

    // Call expression.
    let call = alloc_call_expr(
        package,
        assigner,
        callee_id,
        args_id,
        result_ty.clone(),
        span,
    );
    prepend_evaluations(package, call, statements, assigner);
    call
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
/// # Before
/// ```text
/// // one branch's slice of the original HOF call, dispatched candidate `H` at
/// // slot 0 plus a producer-closure sibling at slot 1:
/// Call(Var(hof), (H, makeOp(), other_args)) : result_ty
/// ```
/// # After
/// ```text
/// // both callable slots removed, the closure's captures appended:
/// Call(Var(combined_spec), (other_args, captures...)) : result_ty
/// ```
///
/// # Mutations
/// - Allocates callee, args, and call `Expr` nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
fn create_combined_branch_call(
    package: &mut Package,
    orig_callee: &Expr,
    orig_args: &Expr,
    span: PackageSpan,
    result_ty: &Ty,
    writer: CaptureWriterContext,
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

    // Gather this leaf's members: the dispatched candidate plus every
    // single-valued sibling. Order them ascending by parameter position so the
    // removed slots and the appended captures line up with the combined spec's
    // input pattern.
    let mut members: Vec<(&CallSite, &CallableParam)> = Vec::with_capacity(constants.len() + 1);
    members.push((candidate, candidate_param));
    members.extend(constants.iter().copied());
    members.sort_by(|a, b| {
        a.1.top_level_param
            .cmp(&b.1.top_level_param)
            .then_with(|| a.1.field_path.cmp(&b.1.field_path))
    });
    // Prefix materialization rewrites argument roots. Give each dispatch leaf
    // its own tuple tree so a temporary defined in one branch cannot leak into
    // another branch's arguments.
    let mut argument_copies = FxHashMap::default();
    let args_id = copy_branch_argument_tuple(package, orig_args.id, &mut argument_copies, assigner);
    let consumed: Vec<_> = members
        .iter()
        .filter_map(|(site, _)| argument_copies.get(&site.arg_expr_id).copied())
        .collect();

    // Walk the members to record which argument-tuple slots to drop and, for
    // each closure member, resolve the capture expressions to append (in the
    // same ascending order), mirroring `rewrite_multi`.
    let uses_tuple_input = members.first().is_none_or(|(cs, _)| cs.hof_input_is_tuple);
    let mut remove_indices: Vec<usize> = Vec::with_capacity(members.len());
    let mut captures: Vec<CapturedVar> = Vec::new();
    let mut statements = Vec::new();
    for (cs, param) in &members {
        // A tuple-input HOF removes the whole top-level slot; a single
        // tuple-valued parameter removes the immediate field instead.
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
            let mut member_captures = filter_threaded_rewrite_captures(
                package,
                resolve_rewrite_captures(package, cs.arg_expr_id, member_captures),
            );
            if member_captures.iter().any(|capture| capture.expr.is_some()) {
                let path = callable_param_input_path(package, orig_callee.id, param);
                statements.extend(materialize_argument_prefix(
                    package, args_id, &path, &consumed, assigner,
                ));
                materialize_capture_operands(
                    package,
                    writer.destination,
                    &mut member_captures,
                    &mut statements,
                    assigner,
                );
            }
            captures.extend(member_captures);
        }
    }

    // Build the specialized callee: its arrow type drops the removed slots and
    // appends the capture types, and the callee expression names the combined
    // spec item. Fall back to the original callee type if the new one cannot be
    // computed.
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
    // The tuple-input and single-tuple-parameter shapes are handled separately.
    let (_, functor) = peel_body_functors(package, orig_callee.id);
    let orig_args = package.get_expr(args_id).clone();
    let (args_kind, args_ty) = build_combined_controlled_branch_args(
        package,
        &orig_args,
        writer,
        &remove_indices,
        &captures,
        uses_tuple_input,
        usize::from(functor.controlled),
        span,
        assigner,
    );
    // Allocate the new args expression node.
    let args_id = alloc_expr(package, assigner, args_ty, args_kind, span);

    // Allocate the call expression that invokes the combined spec with the
    // rewritten args, and hand back its id as this dispatch leaf.
    let call = alloc_call_expr(
        package,
        assigner,
        callee_id,
        args_id,
        result_ty.clone(),
        span,
    );
    prepend_evaluations(package, call, statements, assigner);
    call
}

fn copy_branch_argument_tuple(
    package: &mut Package,
    id: ExprId,
    copies: &mut FxHashMap<ExprId, ExprId>,
    assigner: &mut Assigner,
) -> ExprId {
    let expression = package.get_expr(id).clone();
    let kind = if let ExprKind::Tuple(elements) = expression.kind {
        ExprKind::Tuple(
            elements
                .iter()
                .map(|&element| copy_branch_argument_tuple(package, element, copies, assigner))
                .collect(),
        )
    } else {
        expression.kind
    };
    let copy = alloc_expr(package, assigner, expression.ty, kind, expression.span);
    copies.insert(id, copy);
    copy
}

/// Callable positions refer to the original input, inside every control layer.
/// Keep the control expressions and rebuild only that innermost input.
#[allow(clippy::too_many_arguments)]
fn build_combined_controlled_branch_args(
    package: &mut Package,
    orig_args: &Expr,
    writer: CaptureWriterContext,
    remove_indices: &[usize],
    captures: &[CapturedVar],
    uses_tuple_input: bool,
    controlled_layers: usize,
    span: PackageSpan,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    if controlled_layers > 0 {
        let ExprKind::Tuple(elements) = &orig_args.kind else {
            panic!("controlled call arguments must be a tuple");
        };
        let controls = elements[0];
        let input = package.get_expr(elements[1]).clone();
        let (kind, ty) = build_combined_controlled_branch_args(
            package,
            &input,
            writer,
            remove_indices,
            captures,
            uses_tuple_input,
            controlled_layers - 1,
            span,
            assigner,
        );
        let input = alloc_expr(package, assigner, ty.clone(), kind, span);
        return (
            ExprKind::Tuple(vec![controls, input]),
            Ty::Tuple(vec![package.get_expr(controls).ty.clone(), ty]),
        );
    }
    if uses_tuple_input {
        build_combined_branch_args_data(
            package,
            orig_args,
            writer.destination,
            remove_indices,
            captures,
            span,
            assigner,
        )
    } else {
        build_combined_nested_branch_args_data(
            package,
            orig_args,
            writer,
            remove_indices,
            captures,
            span,
            assigner,
        )
    }
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
    destination: CaptureScope,
    remove_indices: &[usize],
    captures: &[CapturedVar],
    span: PackageSpan,
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
            let capture_ids =
                allocate_capture_exprs(package, span, destination, captures, assigner);
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
    writer: CaptureWriterContext,
    remove_indices: &[usize],
    captures: &[CapturedVar],
    span: PackageSpan,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    let remove: FxHashSet<usize> = remove_indices.iter().copied().collect();
    if let Some((payload_kind, payload_ty)) = remove_top_level_field_from_expr_data_with_exprs(
        package,
        writer.owner_callable,
        orig_args.id,
        &remove,
        &FxHashSet::default(),
        &[],
        writer.destination.into(),
        assigner,
    ) {
        if captures.is_empty() {
            return (payload_kind, payload_ty);
        }

        let payload_id = alloc_expr(package, assigner, payload_ty.clone(), payload_kind, span);

        let capture_ids =
            allocate_capture_exprs(package, span, writer.destination, captures, assigner);
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
        let capture_ids =
            allocate_capture_exprs(package, span, writer.destination, captures, assigner);
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
                resolved.expr =
                    resolve_capture_expr_from_arg(package, arg_expr_id, capture.local.var);
            }
            resolved
        })
        .collect()
}

/// Drops a closure's sole capture from the runtime-threaded set when that
/// capture is a statically-known callable.
///
/// Such a capture is baked directly into the closure target during
/// specialization rather than passed as a runtime operand, so it must not also
/// be threaded as a call argument. Multi-capture closures are returned
/// unchanged.
fn filter_threaded_rewrite_captures(
    _package: &Package,
    captures: Vec<CapturedVar>,
) -> Vec<CapturedVar> {
    if captures.len() != 1 {
        return captures;
    }
    captures
        .into_iter()
        .filter(|capture| capture.static_callable.is_none())
        .collect()
}

/// Looks for a capture initializer in the supplied argument or callee
/// expression, following blocks, conditional arms, and unary wrappers.
/// Tuple elements are not traversed.
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

/// Maps every local bound by `pat_id` to `init_expr_id`.
/// Tuple sub-patterns share the whole initializer id; no projection is created,
/// and the caller supplies bindings of either mutability.
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

/// Builds a branch's reduced argument without changing the source expression.
/// Sibling dispatch arms share that source, so each changed tuple along the path
/// needs a fresh node. Captures belong beneath every control-register wrapper.
#[allow(clippy::too_many_arguments)]
fn build_branch_args_data(
    package: &mut Package,
    orig_args: &Expr,
    destination: CaptureScope,
    input_path: &[usize],
    controlled_layers: usize,
    hof_input_is_tuple: bool,
    captures: &[CapturedVar],
    span: PackageSpan,
    assigner: &mut Assigner,
) -> (ExprKind, Ty) {
    if controlled_layers > 0 {
        let elements = tuple_argument_elements(package, orig_args, assigner)
            .expect("controlled branch arguments should be a tuple");
        let [controls, input] = elements.as_slice() else {
            panic!("controlled branch arguments should contain controls and input");
        };
        assert_eq!(
            input_path.first(),
            Some(&1),
            "callable path should descend through controls"
        );
        let input = package.get_expr(*input).clone();
        let (kind, ty) = build_branch_args_data(
            package,
            &input,
            destination,
            &input_path[1..],
            controlled_layers - 1,
            hof_input_is_tuple,
            captures,
            span,
            assigner,
        );
        let input = alloc_expr(package, assigner, ty.clone(), kind, input.span);
        return (
            ExprKind::Tuple(vec![*controls, input]),
            Ty::Tuple(vec![package.get_expr(*controls).ty.clone(), ty]),
        );
    }

    if input_path.is_empty() {
        // Match the specialized input: no captures become Unit, one becomes a
        // scalar, and multiple captures remain a tuple.
        let capture_ids = allocate_capture_exprs(package, span, destination, captures, assigner);
        build_expr_data_from_elements(package, capture_ids)
    } else if !hof_input_is_tuple {
        let (kind, ty) =
            build_removed_nested_expr_data(package, orig_args.id, input_path, assigner)
                .expect("nested branch argument should contain the callable field");
        if captures.is_empty() {
            return (kind, ty);
        }
        let mut elements = Vec::new();
        if !matches!(&kind, ExprKind::Tuple(items) if items.is_empty()) {
            elements.push(alloc_expr(package, assigner, ty, kind, span));
        }
        elements.extend(allocate_capture_exprs(
            package,
            span,
            destination,
            captures,
            assigner,
        ));
        let types = elements
            .iter()
            .map(|id| package.get_expr(*id).ty.clone())
            .collect();
        (ExprKind::Tuple(elements), Ty::Tuple(types))
    } else {
        let elements = tuple_argument_elements(package, orig_args, assigner)
            .expect("tuple-input branch should have tuple arguments");
        let (index, rest) = input_path.split_first().expect("nonempty callable path");
        let mut elements = elements;
        if rest.is_empty() {
            elements.remove(*index);
        } else {
            let (kind, ty) =
                build_removed_nested_expr_data(package, elements[*index], rest, assigner)
                    .expect("nested branch parameter should contain the callable field");
            if !captures.is_empty() && matches!(&kind, ExprKind::Tuple(items) if items.is_empty()) {
                elements.remove(*index);
            } else {
                elements[*index] = alloc_expr(package, assigner, ty, kind, span);
            }
        }
        elements.extend(allocate_capture_exprs(
            package,
            span,
            destination,
            captures,
            assigner,
        ));
        if rest.is_empty() {
            build_expr_data_from_elements(package, elements)
        } else {
            let types = elements
                .iter()
                .map(|id| package.get_expr(*id).ty.clone())
                .collect();
            (ExprKind::Tuple(elements), Ty::Tuple(types))
        }
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
    span: PackageSpan,
    item_id: ItemId,
    callee_ty: &Ty,
    functor: FunctorApp,
    assigner: &mut Assigner,
) -> ExprId {
    alloc_functor_wrapped_expr(
        package,
        assigner,
        ExprKind::Var(Res::Item(item_id), Vec::new()),
        functor,
        callee_ty,
        span,
    )
}

/// Allocates a new `ExprKind::If` expression and inserts it into the package
/// through `assigner`.
fn alloc_if_expr(
    package: &mut Package,
    span: PackageSpan,
    result_ty: &Ty,
    cond_id: ExprId,
    true_id: ExprId,
    false_id: ExprId,
    assigner: &mut Assigner,
) -> ExprId {
    crate::fir_builder::alloc_if_expr(
        package,
        assigner,
        cond_id,
        true_id,
        Some(false_id),
        result_ty.clone(),
        span,
    )
}

/// Builds a nested `if`/`else` tree selecting one of several specialized calls,
/// reconstructed from the flat outermost-first guard lists by contiguous
/// leading-guard factoring. Each shared leading guard is emitted once for its
/// contiguous run rather than repeated in a conjunction for each leaf.
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
    span: PackageSpan,
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

    // The input is expected to have one inner default per leading-guard run.
    // Assert that one was found; release builds use the outer default if absent.
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
        ConcreteCallable::Closure { captures, .. } => {
            filter_threaded_rewrite_captures(package, captures.clone())
        }
        _ => Vec::new(),
    };
    let new_input = remove_ty_at_path(package, &arrow.input, input_path, &captures);
    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}
