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
//! that flowed those callable values still carry their pre-rewrite arrow
//! types, so a cascade of `refresh_*_types` helpers
//! ([`refresh_rewritten_value_types`], [`refresh_block_types`],
//! [`refresh_stmt_types`], [`refresh_expr_types`]) re-runs type propagation
//! across the cloned body to re-establish the
//! [`crate::invariants::InvariantLevel::PostDefunc`] invariant that no
//! arrow types appear on reachable callable parameters or expressions.

use super::types::{
    AnalysisResult, CallSite, CallableParam, CapturedVar, ConcreteCallable, Error, SpecKey,
    compose_functors, peel_body_functors,
};
use super::{
    build_combined_spec_key, build_combined_spec_key_for_group, build_spec_key,
    has_multiple_forwarded_callable_arrays, is_combined_eligible, partition_mixed_branch_split,
};
use crate::EMPTY_EXEC_RANGE;
use crate::cloner::FirCloner;
use crate::fir_builder::functored_specs;
use crate::package_assigners::PackageAssigners;
use crate::walk_utils::for_each_expr_in_callable_impl;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, Block, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath,
    Functor, Ident, Item, ItemId, ItemKind, Lit, LocalItemId, LocalVarId, Package, PackageId,
    PackageLookup, PackageStore, Pat, PatId, PatKind, Res, Stmt, StmtId, StoreItemId, UnOp,
    Visibility,
};
use qsc_fir::ty::{Arrow, Prim, Ty};
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write as _;
use std::rc::Rc;

/// Maximum number of specializations a single HOF may generate before a
/// warning diagnostic is emitted. Mirrors the LLVM `FuncSpec` `MaxClones`
/// budget, adapted as a diagnostic-only threshold.
const EXCESSIVE_SPECIALIZATION_THRESHOLD: usize = 10;

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
type ClosureInfo = Option<(LocalItemId, Vec<(LocalVarId, Ty)>)>;
type SpecializedCaptureKey = (LocalItemId, LocalVarId);

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
    let mut dedup: FxHashMap<SpecKey, StoreItemId> = FxHashMap::default();
    let mut errors: Vec<Error> = Vec::new();
    let mut recursion_guard: FxHashSet<SpecKey> = FxHashSet::default();

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

    for group in &groups {
        if try_decline_multiple_callable_arrays(store, group, &mut errors) {
            continue;
        }

        // Specialize and rewrite consult the same eligibility predicate so they
        // agree on which call sites are combined. The borrow is scoped here so
        // the combined branch can re-borrow the store mutably below.
        let combined = {
            let package = store.get(group[0].call_pkg_id);
            is_combined_eligible(package, group)
        };
        if combined {
            specialize_combined_group(
                store,
                group,
                &param_by_position,
                &mut dedup,
                &mut errors,
                &mut recursion_guard,
                assigners,
            );
            continue;
        }

        if specialize_mixed_branch_split_group(
            store,
            group,
            &param_by_position,
            &mut dedup,
            &mut errors,
            &mut recursion_guard,
            assigners,
        ) {
            continue;
        }

        specialize_per_row_group(
            store,
            group,
            &param_lookup,
            &param_by_position,
            &mut dedup,
            &mut errors,
            &mut recursion_guard,
            assigners,
        );
    }

    report_excessive_specializations(store, &dedup, &mut errors);

    (dedup, errors)
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
    store: &PackageStore,
    group: &[&CallSite],
    errors: &mut Vec<Error>,
) -> bool {
    let package = store.get(group[0].call_pkg_id);
    if has_multiple_forwarded_callable_arrays(package, group) {
        let span = package.get_expr(group[0].call_expr_id).span;
        if !errors
            .iter()
            .any(|e| matches!(e, Error::UnsupportedMultipleCallableArrays(s) if *s == span))
        {
            errors.push(Error::UnsupportedMultipleCallableArrays(span));
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
/// clones the HOF via [`specialize_many`]. Already-specialized keys and missing
/// HOF items are skipped; a recursive key records a
/// [`Error::RecursiveSpecialization`] diagnostic.
fn specialize_combined_group(
    store: &mut PackageStore,
    group: &[&CallSite],
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    dedup: &mut FxHashMap<SpecKey, StoreItemId>,
    errors: &mut Vec<Error>,
    recursion_guard: &mut FxHashSet<SpecKey>,
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
    if !store
        .get(hof_store_id.package)
        .items
        .contains_key(hof_store_id.item)
    {
        return;
    }

    if recursion_guard.contains(&spec_key) {
        let package = store.get(group[0].call_pkg_id);
        let span = package.get_expr(group[0].call_expr_id).span;
        errors.push(Error::RecursiveSpecialization(span));
        return;
    }
    recursion_guard.insert(spec_key.clone());

    let Some(members) = resolve_group_members(group, hof_store_id, param_by_position) else {
        recursion_guard.remove(&spec_key);
        return;
    };

    let target_pkg_id = group[0].call_pkg_id;
    let new_item_id = assigners.with_package(store, target_pkg_id, |store, assigner| {
        let mut assigner = assigner;
        let result = specialize_many(store, target_pkg_id, &members, &mut assigner);
        (assigner, result)
    });

    if let Some(id) = new_item_id {
        dedup.insert(spec_key.clone(), id);
    }
    recursion_guard.remove(&spec_key);
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
/// Returns `true` when the group is a mixed branch-split (whether or not any
/// candidate produced a spec); `false` leaves the group for the per-row path.
fn specialize_mixed_branch_split_group(
    store: &mut PackageStore,
    group: &[&CallSite],
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    dedup: &mut FxHashMap<SpecKey, StoreItemId>,
    errors: &mut Vec<Error>,
    recursion_guard: &mut FxHashSet<SpecKey>,
    assigners: &mut PackageAssigners,
) -> bool {
    let Some((dispatch, constants)) = partition_mixed_branch_split(group) else {
        return false;
    };
    let hof_item_id = group[0].hof_item_id;
    let hof_store_id = StoreItemId::from((hof_item_id.package, hof_item_id.item));
    if !store
        .get(hof_store_id.package)
        .items
        .contains_key(hof_store_id.item)
    {
        return true;
    }
    for candidate in &dispatch {
        let mut members_cs: Vec<&CallSite> = Vec::with_capacity(constants.len() + 1);
        members_cs.push(*candidate);
        members_cs.extend(constants.iter().copied());
        let spec_key = build_combined_spec_key(hof_item_id, &members_cs);
        if dedup.contains_key(&spec_key) {
            continue;
        }
        if recursion_guard.contains(&spec_key) {
            let package = store.get(candidate.call_pkg_id);
            let span = package.get_expr(candidate.call_expr_id).span;
            errors.push(Error::RecursiveSpecialization(span));
            continue;
        }
        recursion_guard.insert(spec_key.clone());

        let Some(members) = resolve_group_members(&members_cs, hof_store_id, param_by_position)
        else {
            recursion_guard.remove(&spec_key);
            continue;
        };

        let target_pkg_id = candidate.call_pkg_id;
        let new_item_id = assigners.with_package(store, target_pkg_id, |store, assigner| {
            let mut assigner = assigner;
            let result = specialize_many(store, target_pkg_id, &members, &mut assigner);
            (assigner, result)
        });

        if let Some(id) = new_item_id {
            dedup.insert(spec_key.clone(), id);
        }
        recursion_guard.remove(&spec_key);
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
    store: &mut PackageStore,
    group: &[&CallSite],
    param_lookup: &FxHashMap<StoreItemId, &CallableParam>,
    param_by_position: &FxHashMap<(StoreItemId, usize, Vec<usize>), &CallableParam>,
    dedup: &mut FxHashMap<SpecKey, StoreItemId>,
    errors: &mut Vec<Error>,
    recursion_guard: &mut FxHashSet<SpecKey>,
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
        // Confirm the HOF item actually exists in its declared package
        // before proceeding.
        let hof_store_id =
            StoreItemId::from((call_site.hof_item_id.package, call_site.hof_item_id.item));
        if !store
            .get(hof_store_id.package)
            .items
            .contains_key(hof_store_id.item)
        {
            continue;
        }

        // Recursive specialization guard.
        if recursion_guard.contains(&spec_key) {
            let package = store.get(call_site.call_pkg_id);
            let span = package.get_expr(call_site.call_expr_id).span;
            errors.push(Error::RecursiveSpecialization(span));
            continue;
        }
        recursion_guard.insert(spec_key.clone());

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
            recursion_guard.remove(&spec_key);
            continue;
        };

        // Clone the HOF and produce a specialized callable. The spec is
        // allocated into the call site's owning package via that package's
        // own assigner, mirroring monomorphize's specialize-in-place. When
        // the HOF lives in a different package, the cloned body references
        // that package's nodes directly; closures threaded as arguments are
        // local to the call site's package and so remain locally resolvable.
        let target_pkg_id = call_site.call_pkg_id;
        let new_item_id = assigners.with_package(store, target_pkg_id, |store, assigner| {
            let mut assigner = assigner;
            let result = specialize_one(store, target_pkg_id, call_site, param, &mut assigner);
            (assigner, result)
        });

        if let Some(id) = new_item_id {
            dedup.insert(spec_key.clone(), id);
        }

        recursion_guard.remove(&spec_key);
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
            let (name, span) = if let ItemKind::Callable(decl) = &item.kind {
                (decl.name.name.to_string(), decl.name.span)
            } else {
                (format!("Item({hof_id})"), Span::default())
            };
            errors.push(Error::ExcessiveSpecializations(name, *count, span));
        }
    }
}

/// Drives the post-transform retyping cascade across every spec impl of a
/// freshly cloned callable, re-establishing
/// [`crate::invariants::InvariantLevel::PostDefunc`] type consistency after
/// callable references become direct.
///
/// Rewrites `Expr.ty`, `Block.ty`, and `Pat.ty` in place across the entire
/// callable implementation.
fn refresh_rewritten_value_types(package: &mut Package, callable_impl: &CallableImpl) {
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            refresh_block_types(package, spec_impl.body.block);
            for spec in functored_specs(spec_impl) {
                refresh_block_types(package, spec.block);
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            refresh_block_types(package, spec.block);
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

/// Refreshes the type of a single statement and, when it introduces a
/// local binding, retypes the bound pattern to match the rewritten
/// initializer.
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
            }
        }
        qsc_fir::fir::StmtKind::Item(_) => {}
    }
}

/// Recomputes the type of an expression after rewriting, propagating the
/// refreshed type through nested blocks, conditionals, calls, and tuple
/// constructors.
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
fn specialize_many(
    store: &mut PackageStore,
    package_id: PackageId,
    group: &[(&CallSite, &CallableParam)],
    assigner: &mut Assigner,
) -> Option<StoreItemId> {
    if group.len() == 1 {
        let (call_site, param) = group[0];
        return specialize_one(store, package_id, call_site, param, assigner);
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
    let (closure_infos, total_captures) =
        thread_group_closure_captures(&mut cloner, target, &mut new_decl, group, &remapped_params);

    // Recover the assigner from the cloner so all subsequent allocations flow
    // through the shared pipeline assigner.
    *assigner = cloner.into_assigner();

    let callable_array_position = find_callable_array_group_position(&remapped_params, group);

    transform_combined_callable_body(
        target,
        package_id,
        group,
        &remapped_params,
        &closure_infos,
        callable_array_position.as_ref(),
        &new_decl.implementation,
        assigner,
    );

    remove_combined_callable_params(
        target,
        &mut new_decl,
        &remapped_params,
        callable_array_position.as_ref(),
        total_captures,
    );

    let new_item = Item {
        id: new_item_id,
        span: Span::default(),
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
) -> (Vec<ClosureInfo>, usize) {
    let mut closure_infos: Vec<ClosureInfo> = Vec::with_capacity(group.len());
    let mut capture_offset = 0usize;
    for ((call_site, _), remapped_param) in group.iter().zip(remapped_params.iter()) {
        if let ConcreteCallable::Closure {
            ref captures,
            target: closure_target,
            ..
        } = call_site.callable_arg
        {
            let capture_bindings = thread_closure_captures(
                cloner,
                target,
                new_decl,
                remapped_param,
                captures,
                capture_offset,
            );
            capture_offset += capture_bindings.len();
            closure_infos.push(Some((closure_target, capture_bindings)));
        } else {
            closure_infos.push(None);
        }
    }
    (closure_infos, capture_offset)
}

/// Transforms the combined spec body once per specialized parameter, replacing
/// each arrow parameter's calls with its concrete callable.
///
/// A single dedup set is shared across parameters so a lifted lambda captured by
/// more than one parameter is specialized once. For a closure parameter the
/// capture-prepend is scoped to exactly the calls that parameter retargeted (the
/// set difference of calls to the closure target before and after the
/// transform), keeping same-target producer closures from double-prepending each
/// other's captures. Callable-array members skip the scoped prepend because
/// their captures are threaded through the array element rewrite instead.
#[allow(clippy::too_many_arguments)]
fn transform_combined_callable_body(
    target: &mut Package,
    package_id: PackageId,
    group: &[(&CallSite, &CallableParam)],
    remapped_params: &[CallableParam],
    closure_infos: &[ClosureInfo],
    callable_array_position: Option<&(usize, Vec<usize>)>,
    callable_impl: &CallableImpl,
    assigner: &mut Assigner,
) {
    let impl_clone = callable_impl.clone();
    let mut specialized_capture_targets: FxHashSet<SpecializedCaptureKey> = FxHashSet::default();
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
                    if let Some((_, capture_bindings)) = closure_info {
                        concrete_with_threaded_captures(&call_site.callable_arg, capture_bindings)
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
        if let Some((closure_target, capture_bindings)) = closure_info {
            let is_callable_array_member = callable_array_position.is_some_and(|position| {
                (
                    remapped_param.top_level_param,
                    remapped_param.field_path.clone(),
                ) == *position
            });
            let before = if is_callable_array_member {
                FxHashSet::default()
            } else {
                collect_calls_to_closure_target(target, &impl_clone, package_id, *closure_target)
            };
            let concrete =
                concrete_with_threaded_captures(&call_site.callable_arg, capture_bindings);
            transform_callable_body(
                target,
                package_id,
                &impl_clone,
                remapped_param,
                &concrete,
                &concrete_group,
                &mut specialized_capture_targets,
                assigner,
            );
            if !is_callable_array_member {
                let after = collect_calls_to_closure_target(
                    target,
                    &impl_clone,
                    package_id,
                    *closure_target,
                );
                let retargeted: Vec<ExprId> = after.difference(&before).copied().collect();
                prepend_captures_to_calls(
                    target,
                    &retargeted,
                    package_id,
                    *closure_target,
                    capture_bindings,
                    assigner,
                );
            }
        } else {
            transform_callable_body(
                target,
                package_id,
                &impl_clone,
                remapped_param,
                &call_site.callable_arg,
                &concrete_group,
                &mut specialized_capture_targets,
                assigner,
            );
        }
    }
}

/// Removes every specialized arrow parameter slot from the combined spec in a
/// single pass and refreshes value types.
///
/// A tuple-valued parameter specialized through all of its arrow fields leaves
/// a dead `let (a, b, …) = ops` destructuring that still references the slot
/// about to be removed; those statements are dropped first. Callable-array
/// specializations remove their nested slots in descending position order so an
/// earlier removal does not shift a later slot.
fn remove_combined_callable_params(
    target: &mut Package,
    new_decl: &mut CallableDecl,
    remapped_params: &[CallableParam],
    callable_array_position: Option<&(usize, Vec<usize>)>,
    total_captures: usize,
) {
    let nested_param_vars: FxHashSet<LocalVarId> = remapped_params
        .iter()
        .filter(|p| !p.field_path.is_empty())
        .map(|p| p.param_var)
        .collect();
    if !nested_param_vars.is_empty() {
        remove_param_destructuring_stmts(target, &new_decl.implementation, &nested_param_vars);
    }
    if callable_array_position.is_some() {
        let params_for_removal = unique_params_for_removal(remapped_params);
        if params_for_removal
            .iter()
            .all(|(param, _)| !param.field_path.is_empty())
        {
            let mut removal_order = params_for_removal;
            removal_order.sort_by(|(left, _), (right, _)| {
                right
                    .top_level_param
                    .cmp(&left.top_level_param)
                    .then_with(|| right.field_path.cmp(&left.field_path))
            });
            for (param, had_closure_captures) in removal_order {
                remove_nested_callable_param(target, new_decl, param, had_closure_captures);
            }
        } else {
            let param_refs: Vec<&CallableParam> =
                params_for_removal.iter().map(|(param, _)| *param).collect();
            remove_callable_params(target, new_decl, &param_refs, total_captures);
        }
    } else {
        let param_refs: Vec<&CallableParam> = remapped_params.iter().collect();
        remove_callable_params(target, new_decl, &param_refs, total_captures);
    }
    refresh_rewritten_value_types(target, &new_decl.implementation);
}

/// Clones a HOF callable, transforms its body to replace the callable
/// parameter with the concrete callee, and inserts the specialized callable
/// into the target (`package_id`) package. The HOF body is read from
/// `call_site.hof_item_id.package`, which may differ from the target package.
/// Returns the `StoreItemId` of the new item.
fn specialize_one(
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
    let closure_info = if let ConcreteCallable::Closure {
        ref captures,
        target: closure_target,
        ..
    } = call_site.callable_arg
    {
        let capture_bindings = thread_closure_captures(
            &mut cloner,
            target,
            &mut new_decl,
            &remapped_param,
            captures,
            0,
        );
        Some((closure_target, capture_bindings))
    } else {
        None
    };

    // Recover the assigner from the cloner so all subsequent allocations
    // flow through the shared pipeline assigner.
    *assigner = cloner.into_assigner();

    apply_single_param_specialization(
        target,
        package_id,
        &mut new_decl,
        &remapped_param,
        call_site,
        closure_info,
        assigner,
    );

    // Insert the new item.
    let new_item = Item {
        id: new_item_id,
        span: Span::default(),
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
/// A callable's functored specs share one lifted lambda item, so a fresh dedup
/// set guards against re-specializing it across the parameter's specs. When the
/// concrete argument is a closure with captures, those captures are threaded as
/// new input slots and each direct call to the closure target receives the
/// capture operands; a fully consumed tuple parameter is then dropped rather
/// than retyped to unit.
fn apply_single_param_specialization(
    target: &mut Package,
    package_id: PackageId,
    new_decl: &mut CallableDecl,
    remapped_param: &CallableParam,
    call_site: &CallSite,
    closure_info: ClosureInfo,
    assigner: &mut Assigner,
) {
    // A callable's functored specs share one lifted lambda item, so a fresh
    // dedup set guards against re-specializing it across this param's specs.
    let impl_clone = new_decl.implementation.clone();
    let mut specialized_capture_targets: FxHashSet<SpecializedCaptureKey> = FxHashSet::default();
    let concrete = if let Some((_, capture_bindings)) = &closure_info {
        concrete_with_threaded_captures(&call_site.callable_arg, capture_bindings)
    } else {
        call_site.callable_arg.clone()
    };
    transform_callable_body(
        target,
        package_id,
        &impl_clone,
        remapped_param,
        &concrete,
        &[],
        &mut specialized_capture_targets,
        assigner,
    );

    // Whether the removed callable threaded closure captures as new input
    // slots. This governs how a fully consumed tuple parameter is handled below.
    let had_closure_captures = closure_info
        .as_ref()
        .is_some_and(|(_, caps)| !caps.is_empty());

    if let Some((closure_target, capture_bindings)) = closure_info {
        rewrite_closure_target_call_args(
            target,
            &new_decl.implementation,
            package_id,
            closure_target,
            &capture_bindings,
            assigner,
        );
    }

    // Remove the callable parameter from the input pattern and update types.
    // When the removed callable was a closure with captures, those captures were
    // threaded as new input slots and the call site drops the consumed slot, so
    // a fully consumed tuple parameter must be dropped rather than retyped to
    // unit.
    remove_callable_param(target, new_decl, remapped_param, had_closure_captures);
    refresh_rewritten_value_types(target, &new_decl.implementation);
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
    capture_bindings: &[(LocalVarId, Ty)],
) -> ConcreteCallable {
    match concrete {
        ConcreteCallable::Closure {
            target, functor, ..
        } => {
            // Thread captures through the specialized callable input so a
            // forwarded closure keeps its runtime environment after the
            // callable parameter that carried it is removed.
            ConcreteCallable::Closure {
                target: *target,
                captures: capture_bindings
                    .iter()
                    .map(|(var, ty)| CapturedVar {
                        var: *var,
                        ty: ty.clone(),
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

/// Transforms all specialization bodies in a callable implementation,
/// replacing uses of the callable parameter with direct calls to the concrete
/// callee.
///
/// `specialized_capture_targets` tracks each lifted lambda item and captured
/// callable parameter already specialized. It is supplied by the caller so a
/// multi-argument specialization can share one set across every parameter's
/// transform pass; single-argument callers pass a fresh set.
#[allow(clippy::too_many_arguments)]
fn transform_callable_body(
    package: &mut Package,
    package_id: PackageId,
    callable_impl: &CallableImpl,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    specialized_capture_targets: &mut FxHashSet<SpecializedCaptureKey>,
    assigner: &mut Assigner,
) {
    let mut alias_set = AliasSet::default();
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            transform_block(
                package,
                package_id,
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
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            transform_block(
                package,
                package_id,
                spec_decl.block,
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

/// Recursively walks a block, transforming call expressions that invoke the
/// callable parameter.
#[allow(clippy::too_many_arguments)]
fn transform_block(
    package: &mut Package,
    package_id: PackageId,
    block_id: qsc_fir::fir::BlockId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    alias_set: &mut AliasSet,
    specialized_capture_targets: &mut FxHashSet<SpecializedCaptureKey>,
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
/// Before, destructuring locals in `stmt_id` may still hide the callable
/// parameter behind tuple-field aliases. After, any newly introduced aliases are
/// recorded in `alias_set` and all child expressions in the statement have been
/// visited for direct-call rewriting.
#[allow(clippy::too_many_arguments)]
fn transform_stmt(
    package: &mut Package,
    package_id: PackageId,
    stmt_id: qsc_fir::fir::StmtId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    alias_set: &mut AliasSet,
    specialized_capture_targets: &mut FxHashSet<SpecializedCaptureKey>,
    assigner: &mut Assigner,
) {
    let stmt = package.stmts.get(stmt_id).expect("stmt not found").clone();
    match &stmt.kind {
        qsc_fir::fir::StmtKind::Expr(expr_id) | qsc_fir::fir::StmtKind::Semi(expr_id) => {
            transform_expr(
                package,
                package_id,
                *expr_id,
                param,
                concrete,
                concrete_group,
                alias_set,
                specialized_capture_targets,
                assigner,
            );
        }
        qsc_fir::fir::StmtKind::Local(_, pat_id, expr_id) => {
            // Record aliases introduced by destructuring the tuple-valued
            // parameter down to the callable leaf.
            if !param.field_path.is_empty() {
                let init_expr = package.exprs.get(*expr_id).expect("expr not found");
                if let ExprKind::Var(Res::Local(var), _) = &init_expr.kind {
                    if *var == param.param_var {
                        if let Some(alias_var) =
                            find_bind_local_at_field_path(package, *pat_id, &param.field_path)
                        {
                            alias_set.insert(alias_var);
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
    expr_id: ExprId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    concrete_group: &[ConcreteCallable],
    alias_set: &mut AliasSet,
    specialized_capture_targets: &mut FxHashSet<SpecializedCaptureKey>,
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
                replace_indexed_callable_array_call(
                    package,
                    package_id,
                    expr_id,
                    callee_id,
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
                );
                true
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
                    callee_id,
                    param,
                    concrete,
                    concrete_group,
                    alias_set,
                    specialized_capture_targets,
                    assigner,
                );
            }

            // Recurse into the arguments.
            transform_expr(
                package,
                package_id,
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
                        expr_id,
                        concrete,
                        concrete_group,
                        assigner,
                    );
                    return;
                }
            } else if collect_field_path_from_param(package, expr_id, param.param_var).is_some() {
                // Empty-path (single-field UDT) callable forwarded by value to
                // an inner HOF: mirror the non-empty branch above and replace
                // the field-access with the concrete value so the fixpoint
                // re-analysis can resolve the inner call site (instead of
                // leaving a forwarded field access that declines to
                // `DynamicCallable`).
                substitute_forwarded_callable(package, expr_id, concrete, concrete_group, assigner);
                return;
            }
            transform_expr(
                package,
                package_id,
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
            substitute_forwarded_callable(package, expr_id, concrete, concrete_group, assigner);
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
                    expr_id,
                    target,
                    capture_idx,
                    param,
                    concrete,
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
/// type when rebuilding an array — and lets a closure recover its concrete
/// callee type. Returns `None` for a dynamic callable, which has no concrete
/// value to emit.
fn build_callable_value_parts(
    package: &Package,
    concrete: &ConcreteCallable,
    hint_ty: &Ty,
) -> Option<(ExprKind, FunctorApp, Option<Ty>)> {
    match concrete {
        ConcreteCallable::Global { item_id, functor } => {
            let ty = package
                .items
                .get(item_id.item)
                .and_then(|item| match &item.kind {
                    ItemKind::Callable(decl) => Some(Ty::Arrow(Box::new(Arrow {
                        kind: decl.kind,
                        input: Box::new(package.get_pat(decl.input).ty.clone()),
                        output: Box::new(decl.output.clone()),
                        functors: qsc_fir::ty::FunctorSet::Value(decl.functors),
                    }))),
                    ItemKind::Ty(..) => None,
                });
            Some((ExprKind::Var(Res::Item(*item_id), Vec::new()), *functor, ty))
        }
        ConcreteCallable::Closure {
            target,
            captures,
            functor,
        } => {
            let ty = build_direct_target_callee_ty(package, *target, hint_ty, 0).or_else(|| {
                package
                    .items
                    .get(*target)
                    .and_then(|item| match &item.kind {
                        ItemKind::Callable(decl) => Some(Ty::Arrow(Box::new(Arrow {
                            kind: decl.kind,
                            input: Box::new(package.get_pat(decl.input).ty.clone()),
                            output: Box::new(decl.output.clone()),
                            functors: qsc_fir::ty::FunctorSet::Value(decl.functors),
                        }))),
                        ItemKind::Ty(..) => None,
                    })
            });
            Some((
                ExprKind::Closure(
                    captures.iter().map(|capture| capture.var).collect(),
                    *target,
                ),
                *functor,
                ty,
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
    expr_id: ExprId,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) {
    let Some((base_kind, functor, base_ty)) =
        build_callable_value_parts(package, concrete, &package.get_expr(expr_id).ty)
    else {
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

    let mut current_id = assigner.next_expr();
    package.exprs.insert(
        current_id,
        Expr {
            id: current_id,
            span: expr.span,
            ty: new_ty.clone(),
            kind: base_kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    if functor.adjoint {
        let adj_id = assigner.next_expr();
        package.exprs.insert(
            adj_id,
            Expr {
                id: adj_id,
                span: expr.span,
                ty: new_ty.clone(),
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
                span: expr.span,
                ty: new_ty.clone(),
                kind: ExprKind::UnOp(UnOp::Functor(Functor::Ctl), current_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current_id = ctl_id;
    }

    let outermost_kind = package
        .exprs
        .get(current_id)
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
/// for a dynamic callable, which has no concrete value to emit.
fn alloc_callable_value_expr(
    package: &mut Package,
    span: Span,
    concrete: &ConcreteCallable,
    hint_ty: &Ty,
    assigner: &mut Assigner,
) -> Option<ExprId> {
    let (base_kind, functor, base_ty) = build_callable_value_parts(package, concrete, hint_ty)?;
    let new_ty = base_ty.unwrap_or_else(|| hint_ty.clone());

    let mut current_id = assigner.next_expr();
    package.exprs.insert(
        current_id,
        Expr {
            id: current_id,
            span,
            ty: new_ty.clone(),
            kind: base_kind,
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
                ty: new_ty.clone(),
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
                ty: new_ty.clone(),
                kind: ExprKind::UnOp(UnOp::Functor(Functor::Ctl), current_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current_id = ctl_id;
    }

    Some(current_id)
}

/// Rebuilds a forwarded callable array as a concrete array literal of its
/// candidates. An outer HOF that receives a callable array as a flat value can
/// forward it into an inner HOF that indexes it; substituting a single callable
/// there would collapse the whole array to one candidate and drop the other
/// closures' captures. Emitting an `ExprKind::Array` of the concrete callables
/// (each already carrying its own threaded capture) instead lets whole-program
/// re-analysis resolve the inner argument to the full candidate set and
/// specialize the inner indexed dispatch. `expr_id` must be array-typed; its
/// element order is preserved.
fn reconstruct_callable_array(
    package: &mut Package,
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
        let Some(element_id) =
            alloc_callable_value_expr(package, expr.span, concrete, &elem_ty, assigner)
        else {
            // A dynamic candidate cannot be materialized; leave the forwarded
            // parameter in place so re-analysis treats the array as dynamic and
            // falls back to the unspecialized path rather than miscompiling.
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
        reconstruct_callable_array(package, expr_id, group, assigner);
    } else {
        replace_callable_value(package, expr_id, concrete, assigner);
    }
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

#[allow(clippy::too_many_arguments)]
fn replace_indexed_callable_array_call(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    callee_expr_id: ExprId,
    args_id: ExprId,
    array_id: ExprId,
    index_id: ExprId,
    body_functor: FunctorApp,
    concrete_group: &[ConcreteCallable],
    assigner: &mut Assigner,
) {
    let Some(first) = concrete_group.first() else {
        return;
    };

    let branch_callables: Vec<ConcreteCallable> = concrete_group
        .iter()
        .map(|concrete| apply_body_functor_to_concrete(concrete, body_functor))
        .collect();

    if branch_callables.len() == 1 {
        let branch_callable = branch_callables
            .first()
            .expect("branch callable should exist");
        replace_callee(
            package,
            package_id,
            callee_expr_id,
            body_functor,
            first,
            assigner,
        );
        rewrite_indexed_closure_dispatch_args(package, args_id, branch_callable, assigner);
        return;
    }

    let Ty::Array(item_ty) = package.get_expr(array_id).ty.clone() else {
        return;
    };

    let result_ty = package.get_expr(call_expr_id).ty.clone();
    let span = package.get_expr(call_expr_id).span;
    let original_args = package.get_expr(args_id).clone();

    let mut branch_calls = Vec::with_capacity(branch_callables.len());
    for (position, branch_callable) in branch_callables.iter().enumerate() {
        let call_id = alloc_dispatch_branch_call(
            package,
            package_id,
            span,
            &result_ty,
            item_ty.as_ref(),
            &original_args,
            branch_callable,
            assigner,
        );
        branch_calls.push((position, call_id));
    }

    let mut dispatch_id = branch_calls.last().expect("branch exists").1;
    for (position, call_id) in branch_calls.into_iter().rev().skip(1) {
        let condition_id = alloc_index_eq_expr(package, index_id, position, span, assigner);
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

    let dispatch = package.get_expr(dispatch_id).clone();
    let call_expr = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr not found");
    call_expr.kind = dispatch.kind;
    call_expr.ty = dispatch.ty;
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
    span: Span,
    result_ty: &Ty,
    callee_ty: &Ty,
    original_args: &Expr,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) -> ExprId {
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
        ConcreteCallable::Dynamic => return original_args.id,
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

    let callee_id = assigner.next_expr();
    package.exprs.insert(
        callee_id,
        Expr {
            id: callee_id,
            span,
            ty: direct_ty,
            kind: ExprKind::Var(Res::Item(item_id), Vec::new()),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let args_id = assigner.next_expr();
    package.exprs.insert(
        args_id,
        Expr {
            id: args_id,
            span: original_args.span,
            ty: original_args.ty.clone(),
            kind: original_args.kind.clone(),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    if let ConcreteCallable::Closure {
        target, captures, ..
    } = concrete
    {
        if let Some(target_input) = target_callable_input(package, *target) {
            rewrite_closure_dispatch_branch_args(
                package,
                args_id,
                captures,
                &target_input,
                controlled_layers,
                assigner,
            );
        } else {
            let capture_bindings: Vec<(LocalVarId, Ty)> = captures
                .iter()
                .map(|capture| (capture.var, capture.ty.clone()))
                .collect();
            prepend_capture_args_to_call(
                package,
                args_id,
                &capture_bindings,
                controlled_layers,
                assigner,
            );
        }
    }

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
    args_id: ExprId,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) {
    let ConcreteCallable::Closure {
        target,
        captures,
        functor,
    } = concrete
    else {
        return;
    };

    let capture_bindings: Vec<(LocalVarId, Ty)> = captures
        .iter()
        .map(|capture| (capture.var, capture.ty.clone()))
        .collect();
    rewrite_closure_target_args(
        package,
        args_id,
        *target,
        &capture_bindings,
        usize::from(functor.controlled),
        assigner,
    );
}

/// Rewrites a call's argument expression so the closure target receives its
/// captured values alongside the original arguments.
///
/// When the target's input type is known, the arguments are reshaped to match
/// it via [`rewrite_closure_dispatch_branch_args`]; otherwise the captures are
/// simply prepended to the existing argument tuple.
fn rewrite_closure_target_args(
    package: &mut Package,
    args_id: ExprId,
    target: LocalItemId,
    capture_bindings: &[(LocalVarId, Ty)],
    controlled_layers: usize,
    assigner: &mut Assigner,
) {
    if let Some(target_input) = target_callable_input(package, target) {
        let captures: Vec<CapturedVar> = capture_bindings
            .iter()
            .map(|(var, ty)| CapturedVar {
                var: *var,
                ty: ty.clone(),
                expr: None,
                caller_substitutions: Vec::new(),
            })
            .collect();
        rewrite_closure_dispatch_branch_args(
            package,
            args_id,
            &captures,
            &target_input,
            controlled_layers,
            assigner,
        );
    } else {
        prepend_capture_args_to_call(
            package,
            args_id,
            capture_bindings,
            controlled_layers,
            assigner,
        );
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
fn rewrite_closure_dispatch_branch_args(
    package: &mut Package,
    args_id: ExprId,
    captures: &[CapturedVar],
    target_input: &Ty,
    controlled_layers: usize,
    assigner: &mut Assigner,
) {
    if controlled_layers > 0 {
        let inner_id = match package.get_expr(args_id).kind {
            ExprKind::Tuple(ref elements) if elements.len() > 1 => elements[1],
            _ => return,
        };
        // A control layer wraps the ENTIRE base target input as `(ctls, base_input)`;
        // it does NOT split the base input tuple. Thread the full `target_input`
        // through the recursion so the capture prepend at the deepest layer matches the
        // target op's uncontrolled input `captures..., original_args`. Descending into
        // `target_input[1]` here would let the base args coincidentally equal the target
        // input and short-circuit the capture splice, dropping the capture on the
        // controlled path.
        rewrite_closure_dispatch_branch_args(
            package,
            inner_id,
            captures,
            target_input,
            controlled_layers - 1,
            assigner,
        );
        let inner_ty = package.get_expr(inner_id).ty.clone();
        let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
        if let Ty::Tuple(ref mut tys) = args_expr.ty
            && tys.len() > 1
        {
            tys[1] = inner_ty;
        }
        return;
    }

    let Some((kind, ty)) =
        build_closure_dispatch_branch_args_data(package, args_id, captures, target_input, assigner)
    else {
        return;
    };

    let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
    args_expr.kind = kind;
    args_expr.ty = ty;
}

/// Builds the argument-tuple kind and type for a closure dispatch branch,
/// combining the closure's captured values with the original arguments.
///
/// Tries two layouts and returns the first that matches the target's input
/// type: a flattened tuple where captures and the original tuple's fields sit
/// side by side ([`flattened_capture_arg_data`]), then a grouped tuple where the
/// original argument tuple is kept as a single trailing element
/// ([`grouped_capture_arg_data`]). Returns `None` when neither layout applies.
fn build_closure_dispatch_branch_args_data(
    package: &mut Package,
    args_id: ExprId,
    captures: &[CapturedVar],
    target_input: &Ty,
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let original_args = package.get_expr(args_id).clone();
    if original_args.ty == *target_input {
        return Some((original_args.kind, original_args.ty));
    }

    let capture_ids = allocate_capture_exprs(package, original_args.span, captures, assigner);
    let capture_tys: Vec<Ty> = captures.iter().map(|capture| capture.ty.clone()).collect();

    let flattened = flattened_capture_arg_data(
        package,
        &original_args,
        capture_ids.as_slice(),
        &capture_tys,
        target_input,
    );
    if let Some(data) = flattened {
        return Some(data);
    }

    let grouped = grouped_capture_arg_data(
        package,
        &original_args,
        capture_ids.as_slice(),
        &capture_tys,
        target_input,
        assigner,
    );
    grouped.or_else(|| {
        captures
            .is_empty()
            .then_some((original_args.kind, original_args.ty))
    })
}

/// Builds a flattened capture-plus-argument tuple, where the captures and the
/// original tuple's fields become sibling elements.
///
/// Returns `Some` only when `[capture_tys..., arg_tys...]` matches the target
/// input tuple exactly; otherwise `None`, so the caller can try another layout.
fn flattened_capture_arg_data(
    package: &Package,
    original_args: &Expr,
    capture_ids: &[ExprId],
    capture_tys: &[Ty],
    target_input: &Ty,
) -> Option<(ExprKind, Ty)> {
    let Ty::Tuple(target_items) = target_input else {
        return None;
    };
    let ExprKind::Tuple(arg_items) = &original_args.kind else {
        return None;
    };
    let Ty::Tuple(arg_tys) = &original_args.ty else {
        return None;
    };

    let expected_tys: Vec<Ty> = capture_tys
        .iter()
        .cloned()
        .chain(arg_tys.iter().cloned())
        .collect();
    if expected_tys != *target_items {
        return None;
    }

    let elements: Vec<ExprId> = capture_ids
        .iter()
        .copied()
        .chain(arg_items.iter().copied())
        .collect();
    Some(build_expr_data_from_elements(package, elements))
}

/// Builds a grouped capture-plus-argument tuple, where the captures are
/// followed by the original argument tuple as one trailing element.
///
/// Returns `Some` only when `(capture_tys..., original_arg_ty)` matches the
/// target input; otherwise `None`. The original argument expression is copied
/// into a fresh node so it can be reused as the trailing element.
fn grouped_capture_arg_data(
    package: &mut Package,
    original_args: &Expr,
    capture_ids: &[ExprId],
    capture_tys: &[Ty],
    target_input: &Ty,
    assigner: &mut Assigner,
) -> Option<(ExprKind, Ty)> {
    let expected_ty = if capture_tys.is_empty() {
        original_args.ty.clone()
    } else {
        let mut tys = capture_tys.to_vec();
        tys.push(original_args.ty.clone());
        Ty::Tuple(tys)
    };
    if &expected_ty != target_input {
        return None;
    }

    if capture_ids.is_empty() {
        return Some((original_args.kind.clone(), original_args.ty.clone()));
    }

    let preserved_args_id = assigner.next_expr();
    package.exprs.insert(
        preserved_args_id,
        Expr {
            id: preserved_args_id,
            span: original_args.span,
            ty: original_args.ty.clone(),
            kind: original_args.kind.clone(),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let mut elements = capture_ids.to_vec();
    elements.push(preserved_args_id);
    Some(build_expr_data_from_elements(package, elements))
}

/// Allocates one expression per captured variable, to be passed as leading
/// arguments at a specialized call site.
///
/// A capture with a recorded initializer expression reuses it; otherwise a
/// fresh `Var(Res::Local)` reference to the captured variable is synthesized.
fn allocate_capture_exprs(
    package: &mut Package,
    span: Span,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> Vec<ExprId> {
    let mut ids = Vec::with_capacity(captures.len());

    for capture in captures {
        if let Some(expr_id) = capture.expr {
            ids.push(expr_id);
            continue;
        }

        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            Expr {
                id: expr_id,
                span,
                ty: capture.ty.clone(),
                kind: ExprKind::Var(Res::Local(capture.var), Vec::new()),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        ids.push(expr_id);
    }

    ids
}

/// Builds the `ExprKind` and `Ty` for a tuple of the given elements, collapsing
/// the degenerate cases: an empty list becomes `Unit`, and a single element is
/// returned as-is rather than wrapped in a one-tuple.
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

/// Synthesizes the boolean condition `index_expr == index_value`, used to
/// select one arm of an index-dispatch chain.
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

    let condition_id = assigner.next_expr();
    package.exprs.insert(
        condition_id,
        Expr {
            id: condition_id,
            span,
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::BinOp(BinOp::Eq, index_expr_id, lit_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    condition_id
}

/// Synthesizes an `if condition { true_id } else { false_id }` expression with
/// the given result type.
fn alloc_if_expr(
    package: &mut Package,
    span: Span,
    result_ty: &Ty,
    condition_id: ExprId,
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
            kind: ExprKind::If(condition_id, true_id, Some(false_id)),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    if_id
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
        let mut current_id = assigner.next_expr();
        package.exprs.insert(
            current_id,
            Expr {
                id: current_id,
                span: callee_span,
                ty: callee_ty.clone(),
                kind: base_kind,
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );

        if effective.adjoint {
            let adj_id = assigner.next_expr();
            package.exprs.insert(
                adj_id,
                Expr {
                    id: adj_id,
                    span: callee_span,
                    ty: callee_ty.clone(),
                    kind: ExprKind::UnOp(UnOp::Functor(Functor::Adj), current_id),
                    exec_graph_range: EMPTY_EXEC_RANGE,
                },
            );
            current_id = adj_id;
        }

        for _ in 0..effective.controlled {
            let ctl_id = assigner.next_expr();
            package.exprs.insert(
                ctl_id,
                Expr {
                    id: ctl_id,
                    span: callee_span,
                    ty: callee_ty.clone(),
                    kind: ExprKind::UnOp(UnOp::Functor(Functor::Ctl), current_id),
                    exec_graph_range: EMPTY_EXEC_RANGE,
                },
            );
            current_id = ctl_id;
        }

        // Copy the outermost node's kind into the original callee expr.
        let outermost_kind = package
            .exprs
            .get(current_id)
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

/// Replaces the innermost input slot beneath `controlled_layers` nested
/// controlled-operation tuples with `target_input`, returning the rewritten
/// outer type.
///
/// A copy of this helper also lives in
/// `super::rewrite::apply_target_input_at_control_path`; keep the two in
/// sync when changing controlled-layer handling. See the module-level note
/// in `rewrite.rs` for why both copies exist.
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

/// When the HOF body contains a closure that captures the callable parameter
/// being specialized, we must propagate the concrete callable into the
/// closure's target callable and remove the capture so that the `param_var`
/// reference is eliminated.
///
/// # Before
/// ```text
/// Closure([param_var, ...], target)   // target body uses param_var
/// ```
/// # After
/// ```text
/// Closure([...], target')   // param_var capture removed;
///                           // target body uses concrete callee directly
/// ```
///
/// # Mutations
/// - Transforms the closure target's body via [`transform_callable_body`].
/// - Removes the capture from the target's input pattern via
///   [`remove_capture_from_closure_target`].
/// - Removes the capture from the `Closure` expression's capture list.
#[allow(clippy::too_many_arguments)]
fn transform_closure_param_capture(
    package: &mut Package,
    package_id: PackageId,
    closure_expr_id: ExprId,
    closure_target: LocalItemId,
    capture_idx: usize,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    specialized_capture_targets: &mut FxHashSet<SpecializedCaptureKey>,
    assigner: &mut Assigner,
) {
    // The lambda item is shared across the enclosing callable's functored specs.
    // Only the first referring closure specializes it; sibling closures must not
    // re-run that mutation against the already-rewritten lambda. Each closure
    // still drops the capture from its own capture list independently.
    let capture_key = (closure_target, param.param_var);
    if specialized_capture_targets.insert(capture_key) {
        specialize_closure_target_for_captured_param(
            package,
            package_id,
            closure_target,
            capture_idx,
            param,
            concrete,
            assigner,
        );
    }

    // Remove the capture from this Closure expression.
    let closure_expr = package
        .exprs
        .get_mut(closure_expr_id)
        .expect("closure expr not found");
    if let ExprKind::Closure(ref mut captures, _) = closure_expr.kind
        && capture_idx < captures.len()
    {
        captures.remove(capture_idx);
    }
}

/// Specializes the shared closure-target lambda once: replaces uses of the
/// captured callable parameter inside the lambda body with the concrete callee
/// and removes the capture from the lambda's input pattern.
fn specialize_closure_target_for_captured_param(
    package: &mut Package,
    package_id: PackageId,
    closure_target: LocalItemId,
    capture_idx: usize,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    assigner: &mut Assigner,
) {
    // Step 1: Find the corresponding binding in the closure target's input pattern.
    let target_item = package.items.get(closure_target);
    let Some(Item {
        kind: ItemKind::Callable(target_decl),
        ..
    }) = target_item
    else {
        return;
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
                return;
            }
            let capture_pat = package.pats.get(pats[capture_idx]).expect("pat not found");
            match &capture_pat.kind {
                PatKind::Bind(ident) => ident.id,
                _ => return,
            }
        }
        PatKind::Bind(ident) if capture_idx == 0 => ident.id,
        _ => return,
    };

    // Step 2: Create a synthetic CallableParam for the closure target's captured param.
    let closure_param = CallableParam::new(
        StoreItemId::from((package_id, closure_target)),
        target_decl.input,
        capture_idx,
        Vec::new(),
        capture_param_var,
        param.param_ty.clone(),
        matches!(package.get_pat(target_decl.input).kind, PatKind::Tuple(_)),
    );

    // Step 3: Transform the target callable's body to replace uses of the
    // captured param with the concrete callable. This rewrites a distinct
    // callable, the closure target, so it uses its own fresh dedup set.
    let mut specialized_capture_targets: FxHashSet<SpecializedCaptureKey> = FxHashSet::default();
    transform_callable_body(
        package,
        package_id,
        &target_decl.implementation,
        &closure_param,
        concrete,
        &[],
        &mut specialized_capture_targets,
        assigner,
    );

    // Step 4: Remove the capture binding from the target callable's input.
    remove_capture_from_closure_target(package, closure_target, capture_idx);
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
/// - Rewrites the input `Pat` node in place (or replaces `decl.input` when
///   flattening a single-element tuple).
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

            if new_pats.len() == 1 {
                // Flatten single-element tuple.
                let item = package
                    .items
                    .get_mut(target_item_id)
                    .expect("item not found");
                if let ItemKind::Callable(ref mut decl) = item.kind {
                    decl.input = new_pats[0];
                }
            } else {
                let pat_mut = package.pats.get_mut(input_pat_id).expect("pat not found");
                pat_mut.kind = PatKind::Tuple(new_pats);
                pat_mut.ty = if new_tys.is_empty() {
                    Ty::UNIT
                } else {
                    Ty::Tuple(new_tys)
                };
            }
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
) -> Vec<(LocalVarId, Ty)> {
    if captures.is_empty() {
        return Vec::new();
    }

    // Allocate new bindings for each captured variable and build a remap.
    let mut capture_bindings: Vec<(LocalVarId, Ty)> = Vec::with_capacity(captures.len());
    let mut new_pat_ids: Vec<PatId> = Vec::new();
    let mut new_tys: Vec<Ty> = Vec::new();

    for (i, capture) in captures.iter().enumerate() {
        let new_pat_id = cloner.alloc_pat();
        let new_local_var = cloner.alloc_local(capture.var);
        capture_bindings.push((new_local_var, capture.ty.clone()));

        // `name_offset` continues the capture counter across parameters so a
        // multi-argument specialization gets `_.capture_0`, `_.capture_1`, …
        // without collisions; single-argument callers pass `0`, preserving the
        // original spelling.
        let name: Rc<str> = Rc::from(format!("{CAPTURE_NAME_PREFIX}_{}", name_offset + i));
        let new_pat = Pat {
            id: new_pat_id,
            span: Span::default(),
            ty: capture.ty.clone(),
            kind: PatKind::Bind(Ident {
                id: new_local_var,
                span: Span::default(),
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
                span: Span::default(),
                ty: Ty::Tuple(all_tys),
                kind: PatKind::Tuple(sub_pats),
            };
            package.pats.insert(tuple_pat_id, tuple_pat);
            decl.input = tuple_pat_id;
        }
    }

    capture_bindings
}

/// Read-only collector that records every `Call` expression whose callee
/// resolves to a specific closure-target item in `package_id` after peeling
/// functor wrappers. Mirrors the matcher in
/// [`rewrite_closure_target_call_args_in_expr`] so the call sets agree.
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
            if matches!(
                self.package.get_expr(base_id).kind,
                ExprKind::Var(
                    Res::Item(ItemId {
                        package: callee_package,
                        item: callee_item,
                    }),
                    _,
                ) if callee_package == self.package_id && callee_item == self.closure_target
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
        CallableImpl::Intrinsic => {}
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
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            collector.visit_block(spec_decl.block);
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
    closure_target: LocalItemId,
    capture_bindings: &[(LocalVarId, Ty)],
    assigner: &mut Assigner,
) {
    for &call_id in call_ids {
        let ExprKind::Call(callee_id, args_id) = package.get_expr(call_id).kind else {
            continue;
        };
        let (base_id, outer_functor) = peel_body_functors(package, callee_id);
        if matches!(
            package.get_expr(base_id).kind,
            ExprKind::Var(
                Res::Item(ItemId {
                    package: callee_package,
                    item: callee_item,
                }),
                _,
            ) if callee_package == package_id && callee_item == closure_target
        ) {
            rewrite_closure_target_args(
                package,
                args_id,
                closure_target,
                capture_bindings,
                usize::from(outer_functor.controlled),
                assigner,
            );
        }
    }
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
/// The original args expression is preserved as a single element in the
/// new outer tuple, not flattened.
///
/// # Mutations
/// - Delegates to [`rewrite_closure_target_call_args_in_block`] across
///   all specialization bodies.
fn rewrite_closure_target_call_args(
    package: &mut Package,
    callable_impl: &CallableImpl,
    package_id: PackageId,
    closure_target: LocalItemId,
    capture_bindings: &[(LocalVarId, Ty)],
    assigner: &mut Assigner,
) {
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            rewrite_closure_target_call_args_in_block(
                package,
                spec_impl.body.block,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            if let Some(adj) = &spec_impl.adj {
                rewrite_closure_target_call_args_in_block(
                    package,
                    adj.block,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
            if let Some(ctl) = &spec_impl.ctl {
                rewrite_closure_target_call_args_in_block(
                    package,
                    ctl.block,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
            if let Some(ctl_adj) = &spec_impl.ctl_adj {
                rewrite_closure_target_call_args_in_block(
                    package,
                    ctl_adj.block,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
        }
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            rewrite_closure_target_call_args_in_block(
                package,
                spec_decl.block,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
        }
    }
}

/// Walks a block after closure specialization and prepends captured locals to
/// every call that now targets the closure body directly.
///
/// Before, calls to `closure_target` still rely on the closure value to carry
/// its captures implicitly. After, each matching call in `block_id` passes the
/// captured locals explicitly so the rewritten target signature is satisfied.
fn rewrite_closure_target_call_args_in_block(
    package: &mut Package,
    block_id: qsc_fir::fir::BlockId,
    package_id: PackageId,
    closure_target: LocalItemId,
    capture_bindings: &[(LocalVarId, Ty)],
    assigner: &mut Assigner,
) {
    let block = package.get_block(block_id).clone();
    for stmt_id in block.stmts {
        rewrite_closure_target_call_args_in_stmt(
            package,
            stmt_id,
            package_id,
            closure_target,
            capture_bindings,
            assigner,
        );
    }
}

/// Applies closure-capture threading to every expression nested under one
/// statement.
///
/// Before, `stmt_id` may still contain calls whose argument tuple omits the
/// captures now required by `closure_target`. After, all expressions reachable
/// from the statement have been rewritten so those calls pass the captures
/// explicitly.
fn rewrite_closure_target_call_args_in_stmt(
    package: &mut Package,
    stmt_id: qsc_fir::fir::StmtId,
    package_id: PackageId,
    closure_target: LocalItemId,
    capture_bindings: &[(LocalVarId, Ty)],
    assigner: &mut Assigner,
) {
    let stmt = package.get_stmt(stmt_id).clone();
    match stmt.kind {
        qsc_fir::fir::StmtKind::Expr(expr_id)
        | qsc_fir::fir::StmtKind::Semi(expr_id)
        | qsc_fir::fir::StmtKind::Local(_, _, expr_id) => rewrite_closure_target_call_args_in_expr(
            package,
            expr_id,
            package_id,
            closure_target,
            capture_bindings,
            assigner,
        ),
        qsc_fir::fir::StmtKind::Item(_) => {}
    }
}

/// Rewrites an expression subtree so direct calls to a closure target receive
/// explicit capture operands.
///
/// Before, the expression tree may still contain `Call`s whose callee resolves
/// to `closure_target` but whose args tuple omits the captures that were baked
/// into the original closure value. After, every such call prepends those
/// captures, matching the rewritten direct callable signature.
#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
fn rewrite_closure_target_call_args_in_expr(
    package: &mut Package,
    expr_id: ExprId,
    package_id: PackageId,
    closure_target: LocalItemId,
    capture_bindings: &[(LocalVarId, Ty)],
    assigner: &mut Assigner,
) {
    let expr = package.get_expr(expr_id).clone();
    match expr.kind {
        ExprKind::Call(callee_id, args_id) => {
            rewrite_closure_target_call_args_in_expr(
                package,
                callee_id,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            rewrite_closure_target_call_args_in_expr(
                package,
                args_id,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );

            let (base_id, outer_functor) = peel_body_functors(package, callee_id);
            let base_expr = package.get_expr(base_id);
            if matches!(
                base_expr.kind,
                ExprKind::Var(
                    Res::Item(ItemId {
                        package: callee_package,
                        item: callee_item,
                    }),
                    _
                ) if callee_package == package_id && callee_item == closure_target
            ) {
                rewrite_closure_target_args(
                    package,
                    args_id,
                    closure_target,
                    capture_bindings,
                    usize::from(outer_functor.controlled),
                    assigner,
                );
            }
        }
        ExprKind::Block(block_id) => rewrite_closure_target_call_args_in_block(
            package,
            block_id,
            package_id,
            closure_target,
            capture_bindings,
            assigner,
        ),
        ExprKind::If(cond, body, otherwise) => {
            rewrite_closure_target_call_args_in_expr(
                package,
                cond,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            rewrite_closure_target_call_args_in_expr(
                package,
                body,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            if let Some(otherwise) = otherwise {
                rewrite_closure_target_call_args_in_expr(
                    package,
                    otherwise,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
        }
        ExprKind::While(cond, block_id) => {
            rewrite_closure_target_call_args_in_expr(
                package,
                cond,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            rewrite_closure_target_call_args_in_block(
                package,
                block_id,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
        }
        ExprKind::Tuple(exprs) | ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) => {
            for expr_id in exprs {
                rewrite_closure_target_call_args_in_expr(
                    package,
                    expr_id,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
        }
        ExprKind::Assign(lhs, rhs)
        | ExprKind::AssignOp(_, lhs, rhs)
        | ExprKind::BinOp(_, lhs, rhs)
        | ExprKind::ArrayRepeat(lhs, rhs)
        | ExprKind::Index(lhs, rhs)
        | ExprKind::AssignField(lhs, _, rhs)
        | ExprKind::UpdateField(lhs, _, rhs) => {
            rewrite_closure_target_call_args_in_expr(
                package,
                lhs,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            rewrite_closure_target_call_args_in_expr(
                package,
                rhs,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            rewrite_closure_target_call_args_in_expr(
                package,
                a,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            rewrite_closure_target_call_args_in_expr(
                package,
                b,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
            rewrite_closure_target_call_args_in_expr(
                package,
                c,
                package_id,
                closure_target,
                capture_bindings,
                assigner,
            );
        }
        ExprKind::UnOp(_, inner)
        | ExprKind::Return(inner)
        | ExprKind::Fail(inner)
        | ExprKind::Field(inner, _) => rewrite_closure_target_call_args_in_expr(
            package,
            inner,
            package_id,
            closure_target,
            capture_bindings,
            assigner,
        ),
        ExprKind::Range(start, step, end) => {
            if let Some(start) = start {
                rewrite_closure_target_call_args_in_expr(
                    package,
                    start,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
            if let Some(step) = step {
                rewrite_closure_target_call_args_in_expr(
                    package,
                    step,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
            if let Some(end) = end {
                rewrite_closure_target_call_args_in_expr(
                    package,
                    end,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(expr_id) = component {
                    rewrite_closure_target_call_args_in_expr(
                        package,
                        expr_id,
                        package_id,
                        closure_target,
                        capture_bindings,
                        assigner,
                    );
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                rewrite_closure_target_call_args_in_expr(
                    package,
                    copy,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
            for field in fields {
                rewrite_closure_target_call_args_in_expr(
                    package,
                    field.value,
                    package_id,
                    closure_target,
                    capture_bindings,
                    assigner,
                );
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Prepends captured variables as additional arguments ahead of the
/// existing call-site argument tuple (respecting controlled-layer nesting).
///
/// # Before
/// ```text
/// args = (original_args)   // or (ctrl_qubits, (original_args))
/// ```
/// # After
/// ```text
/// args = (__capture_0, ..., __capture_N, original_args)
/// ```
///
/// # Mutations
/// - Rewrites `args_id`'s `ExprKind` and `Ty` in place to a `Tuple`
///   containing capture `Var` expressions followed by the preserved args.
/// - Allocates capture `Var` `Expr` nodes through `assigner`.
fn prepend_capture_args_to_call(
    package: &mut Package,
    args_id: ExprId,
    capture_bindings: &[(LocalVarId, Ty)],
    controlled_layers: usize,
    assigner: &mut Assigner,
) {
    if capture_bindings.is_empty() {
        return;
    }

    if controlled_layers > 0 {
        let inner_id = match package.get_expr(args_id).kind {
            ExprKind::Tuple(ref elements) if elements.len() > 1 => elements[1],
            _ => return,
        };
        prepend_capture_args_to_call(
            package,
            inner_id,
            capture_bindings,
            controlled_layers - 1,
            assigner,
        );
        let inner_ty = package.get_expr(inner_id).ty.clone();
        let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
        if let Ty::Tuple(ref mut tys) = args_expr.ty
            && tys.len() > 1
        {
            tys[1] = inner_ty;
        }
        return;
    }

    let original_args = package.get_expr(args_id).clone();
    let preserved_args_id = assigner.next_expr();
    package.exprs.insert(
        preserved_args_id,
        Expr {
            id: preserved_args_id,
            span: original_args.span,
            ty: original_args.ty.clone(),
            kind: original_args.kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let mut tuple_items = Vec::with_capacity(capture_bindings.len() + 1);
    let mut tuple_tys = Vec::with_capacity(capture_bindings.len() + 1);
    for (capture_var, capture_ty) in capture_bindings {
        let capture_expr_id = assigner.next_expr();
        package.exprs.insert(
            capture_expr_id,
            Expr {
                id: capture_expr_id,
                span: original_args.span,
                ty: capture_ty.clone(),
                kind: ExprKind::Var(Res::Local(*capture_var), Vec::new()),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        tuple_items.push(capture_expr_id);
        tuple_tys.push(capture_ty.clone());
    }
    tuple_items.push(preserved_args_id);
    tuple_tys.push(original_args.ty);

    let args_expr = package.exprs.get_mut(args_id).expect("args expr not found");
    args_expr.kind = ExprKind::Tuple(tuple_items);
    args_expr.ty = Ty::Tuple(tuple_tys);
}

/// Collects the block ids of every specialization of a callable implementation:
/// `body`, `adj`, `ctl`, and `ctl_adj`.
fn spec_block_ids(callable_impl: &CallableImpl) -> Vec<qsc_fir::fir::BlockId> {
    let mut ids = Vec::new();
    match callable_impl {
        CallableImpl::Intrinsic => {}
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
        CallableImpl::SimulatableIntrinsic(spec_decl) => ids.push(spec_decl.block),
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
/// - Flattens single-element tuples.
/// - For nested params, delegates to [`remove_nested_callable_param`].
///
/// Used by [`specialize_many`] when more than one arrow parameter is removed at
/// once: filtering the slots one at a time would shift the surviving indices
/// between removals. Each member is removed by its top-level slot. A nested
/// member, one selecting an arrow field of a tuple-valued parameter, only
/// reaches here when its group covers every field of that tuple, which
/// [`super::is_combined_eligible`] checks, so its whole top-level slot is
/// dropped and the dead destructuring of that slot is removed beforehand by
/// [`remove_param_destructuring_stmts`].
///
/// `num_appended_captures` is the count of capture patterns already appended to
/// the input by [`thread_closure_captures`]. The surviving input is flattened to
/// a scalar only when exactly one element remains and no captures were appended,
/// so a lone surviving capture keeps its tuple shape and stays aligned with the
/// call-site rewrite that supplies the capture operand.
fn remove_callable_params(
    package: &mut Package,
    decl: &mut CallableDecl,
    params: &[&CallableParam],
    num_appended_captures: usize,
) {
    // Every member is removed by its top-level slot. A nested member, one that
    // selects an arrow field of a tuple-valued parameter, reaches here only when
    // its group covers the whole tuple, which `is_combined_eligible` checks, so
    // the entire slot is dropped; the now-dead destructuring of that slot is
    // removed separately by `remove_param_destructuring_stmts`.
    let remove: FxHashSet<usize> = params.iter().map(|p| p.top_level_param).collect();

    let input_pat = package
        .pats
        .get(decl.input)
        .expect("input pat not found")
        .clone();

    match &input_pat.kind {
        PatKind::Tuple(pats) => {
            let tys = match &input_pat.ty {
                Ty::Tuple(tys) => tys.clone(),
                _ => vec![input_pat.ty.clone(); pats.len()],
            };

            let mut new_pats: Vec<PatId> = Vec::new();
            let mut new_tys: Vec<Ty> = Vec::new();
            for (i, (&pat_id, ty)) in pats.iter().zip(tys.iter()).enumerate() {
                if !remove.contains(&i) {
                    new_pats.push(pat_id);
                    new_tys.push(ty.clone());
                }
            }

            if new_pats.len() == 1 && num_appended_captures == 0 {
                // Flatten single-element tuple to the single pattern.
                decl.input = new_pats[0];
            } else {
                let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
                input_pat_mut.kind = PatKind::Tuple(new_pats);
                input_pat_mut.ty = Ty::Tuple(new_tys);
            }
        }
        PatKind::Bind(_) => {
            // A single tuple-valued parameter whose every arrow field is
            // specialized away leaves no surviving input. Captures, when
            // present, are threaded by wrapping the bind in a tuple before this
            // runs, so reaching the bind arm means no captures were appended and
            // the input collapses to unit.
            debug_assert!(
                num_appended_captures == 0,
                "captures wrap the input in a tuple before removal"
            );
            let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
            input_pat_mut.kind = PatKind::Tuple(Vec::new());
            input_pat_mut.ty = Ty::UNIT;
        }
        PatKind::Discard => {
            // A discard input binds nothing to remove.
        }
    }
}

fn remove_callable_param(
    package: &mut Package,
    decl: &mut CallableDecl,
    param: &CallableParam,
    had_closure_captures: bool,
) {
    if !param.field_path.is_empty() {
        remove_nested_callable_param(package, decl, param, had_closure_captures);
        return;
    }

    let input_pat = package
        .pats
        .get(decl.input)
        .expect("input pat not found")
        .clone();

    match &input_pat.kind {
        PatKind::Tuple(pats) => {
            let mut new_pats: Vec<PatId> = Vec::new();
            let mut new_tys: Vec<Ty> = Vec::new();

            let tys = match &input_pat.ty {
                Ty::Tuple(tys) => tys.clone(),
                _ => vec![input_pat.ty.clone(); pats.len()],
            };

            for (i, (&pat_id, ty)) in pats.iter().zip(tys.iter()).enumerate() {
                if i != param.top_level_param {
                    new_pats.push(pat_id);
                    new_tys.push(ty.clone());
                }
            }

            if new_pats.len() == 1 {
                // Flatten single-element tuple to the single pattern.
                decl.input = new_pats[0];
            } else {
                let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
                input_pat_mut.kind = PatKind::Tuple(new_pats);
                input_pat_mut.ty = Ty::Tuple(new_tys);
            }
        }
        PatKind::Bind(_) => {
            // The only parameter IS the callable param — replace with unit.
            let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
            input_pat_mut.kind = PatKind::Tuple(Vec::new());
            input_pat_mut.ty = Ty::UNIT;
        }
        PatKind::Discard => {}
    }
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
fn remove_nested_callable_param(
    package: &mut Package,
    decl: &mut CallableDecl,
    param: &CallableParam,
    had_closure_captures: bool,
) {
    let input_pat = package
        .pats
        .get(decl.input)
        .expect("input pat not found")
        .clone();

    let outer_idx = param.top_level_param;
    let inner_path = param.field_path.as_slice();

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
            let new_ty = remove_ty_at_nested_path(package, &sub_pat.ty, inner_path);
            fully_consumed =
                had_closure_captures && matches!(new_ty, Ty::Tuple(ref t) if t.is_empty());

            if fully_consumed {
                // The removal consumed the parameter's entire tuple, a
                // single-field tuple whose only element was an inlined arrow.
                // The sub-slot carries no surviving input, so drop it from the
                // outer tuple, mirroring the non-nested removal, instead of
                // leaving a phantom unit parameter that the call site does not
                // supply. The dead destructuring of this slot is removed below.
                let tys = match &input_pat.ty {
                    Ty::Tuple(tys) => tys.clone(),
                    _ => vec![input_pat.ty.clone(); pats.len()],
                };
                let mut new_pats: Vec<PatId> = Vec::new();
                let mut new_tys: Vec<Ty> = Vec::new();
                for (i, (&pat_id, ty)) in pats.iter().zip(tys.iter()).enumerate() {
                    if i != outer_idx {
                        new_pats.push(pat_id);
                        new_tys.push(ty.clone());
                    }
                }
                // Keep the surviving slots as a tuple rather than flattening a
                // lone survivor. The call-site rewrite preserves the outer
                // tuple shape, swapping the consumed callable for its appended
                // capture, so the specialized input must stay a tuple to match.
                let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
                input_pat_mut.kind = PatKind::Tuple(new_pats);
                input_pat_mut.ty = Ty::Tuple(new_tys);
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
            let new_ty = remove_ty_at_nested_path(package, &input_pat.ty, inner_path);
            let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
            input_pat_mut.ty = new_ty;
            Some(input_pat.ty.clone())
        }
        PatKind::Discard => None,
    };

    // Removing one callable field from a tuple-typed parameter shifts (and, when
    // only one element survives, collapses) the remaining elements. Field
    // accesses elsewhere in the body that select sibling elements must be
    // rewritten so their indices and shape stay aligned with the parameter's new
    // type. Without this, a later specialization pass would see a stale
    // projection over a parameter that no longer has that tuple structure.
    if let Some(orig_param_ty) = orig_param_ty
        && !inner_path.is_empty()
    {
        reindex_sibling_field_access(
            package,
            &decl.implementation,
            param.param_var,
            inner_path,
            &orig_param_ty,
        );
    }

    // Rewrite destructuring patterns in the body that bind param_var's tuple.
    if !inner_path.is_empty() {
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
                package,
                spec_impl.body.block,
                param.param_var,
                inner_path,
            );
            if let Some(ref adj) = spec_impl.adj {
                rewrite_destructuring_pat_in_block(package, adj.block, param.param_var, inner_path);
            }
            if let Some(ref ctl) = spec_impl.ctl {
                rewrite_destructuring_pat_in_block(package, ctl.block, param.param_var, inner_path);
            }
            if let Some(ref ctl_adj) = spec_impl.ctl_adj {
                rewrite_destructuring_pat_in_block(
                    package,
                    ctl_adj.block,
                    param.param_var,
                    inner_path,
                );
            }
        } else if let CallableImpl::SimulatableIntrinsic(spec_decl) = &decl.implementation {
            rewrite_destructuring_pat_in_block(
                package,
                spec_decl.block,
                param.param_var,
                inner_path,
            );
        }
    }
}

/// Walks a block and rewrites any destructuring `let` statement whose init
/// expression is `Var(Local(param_var))` by removing the sub-pattern at
/// `inner_path` from the tuple pattern.
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
/// - Rewrites `Pat.kind` and `Pat.ty` via [`remove_pat_at_field_path`].
/// - Updates the init expression's type to match the rewritten pattern.
fn rewrite_destructuring_pat_in_block(
    package: &mut Package,
    block_id: qsc_fir::fir::BlockId,
    param_var: LocalVarId,
    inner_path: &[usize],
) {
    let block = package
        .blocks
        .get(block_id)
        .expect("block not found")
        .clone();
    for &stmt_id in &block.stmts {
        let stmt = package.stmts.get(stmt_id).expect("stmt not found").clone();
        if let qsc_fir::fir::StmtKind::Local(_, pat_id, expr_id) = &stmt.kind {
            let rewrites_param_var = {
                let init_expr = package.exprs.get(*expr_id).expect("expr not found");
                matches!(&init_expr.kind, ExprKind::Var(Res::Local(var), _) if *var == param_var)
            };
            if rewrites_param_var && remove_pat_at_field_path(package, *pat_id, inner_path) {
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
fn remove_pat_at_field_path(package: &mut Package, pat_id: PatId, field_path: &[usize]) -> bool {
    let Some((index, tail)) = field_path.split_first() else {
        return false;
    };

    let pat = package.pats.get(pat_id).expect("pat not found").clone();
    let PatKind::Tuple(sub_pats) = &pat.kind else {
        return false;
    };
    if *index >= sub_pats.len() {
        return false;
    }

    if tail.is_empty() {
        let remaining_pats: Vec<PatId> = sub_pats
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != *index)
            .map(|(_, &sub_pat_id)| sub_pat_id)
            .collect();
        let (new_kind, new_ty) = flattened_tuple_pat(package, &remaining_pats);
        let pat_mut = package.pats.get_mut(pat_id).expect("pat not found");
        pat_mut.kind = new_kind;
        pat_mut.ty = new_ty;
        return true;
    }

    let child_pat_id = sub_pats[*index];
    if !remove_pat_at_field_path(package, child_pat_id, tail) {
        return false;
    }

    let new_ty = Ty::Tuple(
        sub_pats
            .iter()
            .map(|sub_pat_id| package.get_pat(*sub_pat_id).ty.clone())
            .collect(),
    );
    let pat_mut = package.pats.get_mut(pat_id).expect("pat not found");
    pat_mut.ty = new_ty;
    true
}

/// Flattens a single-element tuple pattern to its contained pattern (so a
/// one-element tuple never survives pattern removal), returning the
/// resulting `(PatKind, Ty)` for the enclosing pattern slot.
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
/// `removed_path` is the path (relative to `param_var`) of the field that was
/// removed; `orig_param_ty` is the parameter's structural type before removal.
/// When two callable fields live in the same tuple parameter, specialization
/// removes them across successive passes. Removing one element shifts the index
/// of every later sibling down by one and, when only a single element remains,
/// collapses the tuple to that element. This function rewrites the body's
/// projections over `param_var` so they continue to reference the correct
/// element after the parameter's type changes.
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
fn reindex_sibling_field_access(
    package: &mut Package,
    callable_impl: &CallableImpl,
    param_var: LocalVarId,
    removed_path: &[usize],
    orig_param_ty: &Ty,
) {
    enum FieldRewrite {
        // Select the surviving element directly: replace the projection with its
        // inner expression, retyped to the surviving element.
        Collapse(ExprId, Ty),
        // Shift the selected index down by one to account for the removed
        // element.
        Reindex(usize),
    }

    let Some((&removed_idx, parent_path)) = removed_path.split_last() else {
        return;
    };
    let resolved = resolve_udt_ty(package, orig_param_ty);
    let Some(tuple_elems) = tuple_elems_at_path(&resolved, parent_path) else {
        return;
    };
    if removed_idx >= tuple_elems.len() {
        return;
    }
    // Specialization removes exactly one callable field per pass, so this
    // function only ever reconciles a single removed element at a time;
    // multi-element removal would require iterating the shifts below.
    // Removing one element from a two-element tuple leaves a single element, so
    // the tuple slot collapses to that element directly.
    let collapses = tuple_elems.len() == 2;
    let elem_tys: Vec<Ty> = tuple_elems.to_vec();

    let mut rewrites: Vec<(ExprId, FieldRewrite)> = Vec::new();
    for_each_expr_in_callable_impl(package, callable_impl, &mut |expr_id, expr| {
        let ExprKind::Field(inner_id, Field::Path(FieldPath { indices })) = &expr.kind else {
            return;
        };
        if indices.len() != 1 {
            return;
        }
        let selected = indices[0];
        if selected == removed_idx {
            return;
        }
        // The projection must select an element of the affected tuple: the inner
        // expression's path from `param_var` must equal the affected tuple's
        // parent path.
        let Some(base_path) = collect_field_path_from_param(package, *inner_id, param_var) else {
            return;
        };
        if base_path != parent_path {
            return;
        }
        if collapses {
            rewrites.push((
                expr_id,
                FieldRewrite::Collapse(*inner_id, elem_tys[selected].clone()),
            ));
        } else if selected > removed_idx {
            rewrites.push((expr_id, FieldRewrite::Reindex(selected - 1)));
        }
    });

    for (expr_id, rewrite) in rewrites {
        match rewrite {
            FieldRewrite::Reindex(new_idx) => {
                let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
                if let ExprKind::Field(_, Field::Path(path)) = &mut expr_mut.kind {
                    path.indices = vec![new_idx];
                }
            }
            FieldRewrite::Collapse(inner_id, new_ty) => {
                let inner = package.exprs.get(inner_id).expect("expr not found").clone();
                let expr_mut = package.exprs.get_mut(expr_id).expect("expr not found");
                expr_mut.kind = inner.kind;
                expr_mut.ty = new_ty;
            }
        }
    }
}

/// Returns the element types of the tuple reached by navigating `ty` along
/// `path`, or `None` when the path does not resolve to a tuple.
fn tuple_elems_at_path<'a>(ty: &'a Ty, path: &[usize]) -> Option<&'a [Ty]> {
    let mut current = ty;
    for &idx in path {
        let Ty::Tuple(elems) = current else {
            return None;
        };
        current = elems.get(idx)?;
    }
    match current {
        Ty::Tuple(elems) => Some(elems),
        _ => None,
    }
}

/// Removes the element at `path` from a nested tuple type structure.
/// For single-element paths, removes the element at that index from the tuple.
/// For multi-element paths, navigates into the tuple and recursively removes.
fn remove_ty_at_nested_path(package: &Package, ty: &Ty, path: &[usize]) -> Ty {
    if path.is_empty() {
        return Ty::UNIT;
    }
    let ty = resolve_udt_ty(package, ty);
    if let Ty::Tuple(tys) = ty {
        if path.len() == 1 {
            let remaining: Vec<Ty> = tys
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != path[0])
                .map(|(_, t)| t.clone())
                .collect();
            if remaining.is_empty() {
                Ty::UNIT
            } else if remaining.len() == 1 {
                remaining.into_iter().next().expect("single element")
            } else {
                Ty::Tuple(remaining)
            }
        } else {
            let mut new_tys = tys.clone();
            new_tys[path[0]] = remove_ty_at_nested_path(package, &tys[path[0]], &path[1..]);
            Ty::Tuple(new_tys)
        }
    } else {
        Ty::UNIT
    }
}

/// Expands UDT wrappers to the tuple/array/arrow structure that defunctionalization tracks.
///
/// `CallableParam::field_path` is recorded against the pure structural shape of a parameter,
/// but specialization removes the callable parameter before UDT erasure has necessarily run.
/// When the input pattern still has a `Ty::Udt`, `remove_ty_at_nested_path` needs the same
/// structural view that analysis used so a path like `cfg::Inner::Op` can remove the arrow
/// field from the specialized callable's input type. Non-UDT leaves are preserved, and nested
/// tuples, arrays, and arrows are rebuilt with any UDTs inside them expanded as well.
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

/// Builds a standalone `Package` holding every node reachable from a
/// callable body so the cloner can read from a disjoint source while the
/// target package is mutated.
fn extract_callable_body(source_pkg: &Package, decl: &CallableDecl) -> Package {
    let mut body_pkg = Package::default();

    extract_pat(source_pkg, decl.input, &mut body_pkg);

    match &decl.implementation {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            extract_spec_decl_body(source_pkg, &spec_impl.body, &mut body_pkg);
            for spec in functored_specs(spec_impl) {
                extract_spec_decl_body(source_pkg, spec, &mut body_pkg);
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            extract_spec_decl_body(source_pkg, spec, &mut body_pkg);
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

#[allow(clippy::too_many_lines)]
/// Recursively copies an expression and its transitive references into the
/// extraction target.
///
/// The `ExprKind::Closure` arm extracts the closure's lambda-lifted target item
/// so the cloner relocates the lambda into the specialization target package
/// with a fresh id. This is required for cross-package specialization: a closure
/// carries a bare `LocalItemId` with no package qualifier, so the lambda it
/// references must live in the same package as the closure expression. Named
/// nested functions in `StmtKind::Item` are followed for the same reason.
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
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Recursively copies a nested item (named function declared inside a block)
/// and its callable body into the extraction target so that
/// [`FirCloner::clone_nested_item`](crate::cloner::FirCloner) can find it
/// during specialization.
fn extract_item(source: &Package, item_id: LocalItemId, target: &mut Package) {
    if target.items.contains_key(item_id) {
        return;
    }
    let item = source.get_item(item_id);
    target.items.insert(item_id, item.clone());
    if let ItemKind::Callable(decl) = &item.kind {
        extract_pat(source, decl.input, target);
        match &decl.implementation {
            CallableImpl::Intrinsic => {}
            CallableImpl::Spec(spec_impl) => {
                extract_spec_decl_body(source, &spec_impl.body, target);
                for spec in functored_specs(spec_impl) {
                    extract_spec_decl_body(source, spec, target);
                }
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                extract_spec_decl_body(source, spec, target);
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
