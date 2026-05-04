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
//! Cloning a HOF body replaces one or more indirect callable references
//! (typed as arrow) with direct item references (typed as the callable's
//! concrete signature). The surrounding expressions, statements, and blocks
//! that flowed those callable values still carry their pre-rewrite arrow
//! types, so a cascade of `refresh_*_types` helpers
//! ([`refresh_rewritten_value_types`], [`refresh_block_types`],
//! [`refresh_stmt_types`], [`refresh_expr_types`]) re-runs type propagation
//! across the cloned body to re-establish the
//! [`crate::invariants::InvariantLevel::PostDefunc`] invariant that no
//! arrow types appear on reachable callable parameters or expressions.

use super::build_spec_key;
use super::types::{
    AnalysisResult, CallSite, CallableParam, CapturedVar, ConcreteCallable, Error, SpecKey,
    compose_functors, peel_body_functors,
};
use crate::EMPTY_EXEC_RANGE;
use crate::cloner::FirCloner;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath, Functor, Ident, Item,
    ItemId, ItemKind, LocalItemId, LocalVarId, NodeId, Package, PackageId, PackageLookup,
    PackageStore, Pat, PatId, PatKind, Res, UnOp, Visibility,
};
use qsc_fir::ty::{Arrow, Ty};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// Maximum number of specializations a single HOF may generate before a
/// warning diagnostic is emitted. Mirrors the LLVM `FuncSpec` `MaxClones`
/// budget, adapted as a diagnostic-only threshold.
const EXCESSIVE_SPECIALIZATION_THRESHOLD: usize = 10;

/// Set of `LocalVarId`s that alias a nested callable parameter after
/// destructuring (e.g. `let (op, _) = pair;` makes `op` an alias).
type AliasSet = FxHashSet<LocalVarId>;

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
/// Returns a map from `SpecKey` to the `LocalItemId` of the newly created
/// specialized callable in the target package.
pub(super) fn specialize(
    store: &mut PackageStore,
    package_id: PackageId,
    analysis: &AnalysisResult,
    assigner: &mut Assigner,
) -> (FxHashMap<SpecKey, LocalItemId>, Vec<Error>) {
    let mut dedup: FxHashMap<SpecKey, LocalItemId> = FxHashMap::default();
    let mut errors: Vec<Error> = Vec::new();
    let mut recursion_guard: FxHashSet<SpecKey> = FxHashSet::default();

    // Build a lookup from LocalItemId → CallableParam for quick access.
    // Use entry().or_insert() to keep the first (lowest-index) param per
    // callable, ensuring deterministic behavior when a callable has multiple
    // arrow params.
    let mut param_lookup: FxHashMap<LocalItemId, &CallableParam> = FxHashMap::default();
    for p in &analysis.callable_params {
        param_lookup.entry(p.callable_id).or_insert(p);
    }

    for call_site in &analysis.call_sites {
        let spec_key = build_spec_key(call_site);

        // Already specialized — skip.
        if dedup.contains_key(&spec_key) {
            continue;
        }

        // Dynamic callables cannot be specialized — emit an error with the
        // call-site span so the user gets an actionable diagnostic instead of
        // the generic `FixpointNotReached` convergence error.
        if matches!(call_site.callable_arg, ConcreteCallable::Dynamic) {
            let package = store.get(package_id);
            let span = package.get_expr(call_site.call_expr_id).span;
            errors.push(Error::DynamicCallable(span));
            continue;
        }

        // Skip cross-package HOFs that were NOT cloned into the user
        // package by monomorphization. Cross-package HOFs that WERE cloned
        // (e.g. generic std lib callables monomorphized with concrete types)
        // now exist in the user package's items map and should be processed.
        if call_site.hof_item_id.package != package_id {
            let pkg = store.get(package_id);
            if !pkg.items.contains_key(call_site.hof_item_id.item) {
                continue;
            }
        }

        // Recursive specialization guard.
        if recursion_guard.contains(&spec_key) {
            let package = store.get(package_id);
            let span = package.get_expr(call_site.call_expr_id).span;
            errors.push(Error::RecursiveSpecialization(span));
            continue;
        }
        recursion_guard.insert(spec_key.clone());

        let hof_local_item = call_site.hof_item_id.item;

        // Look up the callable parameter for this HOF.
        let Some(param) = param_lookup.get(&hof_local_item).copied() else {
            recursion_guard.remove(&spec_key);
            continue;
        };

        // Clone the HOF and produce a specialized callable.
        let new_item_id = specialize_one(store, package_id, call_site, param, assigner);

        if let Some(id) = new_item_id {
            dedup.insert(spec_key.clone(), id);
        }

        recursion_guard.remove(&spec_key);
    }

    // Count specializations per HOF and emit a warning when the threshold
    // is exceeded. Group dedup entries by the HOF callable_id embedded in
    // each SpecKey.
    let mut specs_per_hof: FxHashMap<LocalItemId, usize> = FxHashMap::default();
    for key in dedup.keys() {
        *specs_per_hof.entry(key.hof_id).or_default() += 1;
    }
    for (hof_id, count) in &specs_per_hof {
        if *count > EXCESSIVE_SPECIALIZATION_THRESHOLD {
            let package = store.get(package_id);
            let item = package.get_item(*hof_id);
            let (name, span) = if let ItemKind::Callable(decl) = &item.kind {
                (decl.name.name.to_string(), decl.name.span)
            } else {
                (format!("Item({hof_id})"), Span::default())
            };
            errors.push(Error::ExcessiveSpecializations(name, *count, span));
        }
    }

    (dedup, errors)
}

/// Drives the post-transform retyping cascade across every spec impl of a
/// freshly cloned callable, re-establishing [`crate::invariants::InvariantLevel::PostDefunc`]
/// type consistency after callable references become direct.
fn refresh_rewritten_value_types(package: &mut Package, callable_impl: &CallableImpl) {
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            refresh_block_types(package, spec_impl.body.block);
            for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
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
/// local binding, updates the bound pattern's type to match the rewritten
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

/// Clones a HOF callable, transforms its body to replace the callable
/// parameter with the concrete callee, and inserts the specialized callable
/// into the package. Returns the `LocalItemId` of the new item.
#[allow(clippy::too_many_lines)]
fn specialize_one(
    store: &mut PackageStore,
    package_id: PackageId,
    call_site: &CallSite,
    param: &CallableParam,
    assigner: &mut Assigner,
) -> Option<LocalItemId> {
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

    // Clone input BEFORE impl so that `local_map` contains input parameter
    // mappings when the callable body is walked.
    let cloned_input = cloner.clone_pat(&body_pkg, decl_snapshot.input, target);
    let cloned_impl = cloner.clone_callable_impl(&body_pkg, &decl_snapshot.implementation, target);

    // Input is cloned BEFORE the body (above), so `local_map` always
    // contains the mapping for the original parameter variable.
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
    );

    let hof_name: Rc<str> = Rc::from(format!("{}{{{arg_label}}}", decl_snapshot.name.name));
    let mut new_decl = CallableDecl {
        id: NodeId::from(0_u32),
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

    // Thread closure captures BEFORE recovering the assigner, since
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
        );
        Some((closure_target, capture_bindings))
    } else {
        None
    };

    // Recover the assigner from the cloner so all subsequent allocations
    // flow through the shared pipeline assigner.
    *assigner = cloner.into_assigner();

    // Transform the body to replace callable param with the concrete callee.
    let impl_clone = new_decl.implementation.clone();
    transform_callable_body(
        target,
        package_id,
        &impl_clone,
        &remapped_param,
        &call_site.callable_arg,
        assigner,
    );

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
    remove_callable_param(target, &mut new_decl, &remapped_param);
    refresh_rewritten_value_types(target, &new_decl.implementation);

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

    Some(new_item_id)
}

/// Transforms all specialization bodies in a callable implementation,
/// replacing uses of the callable parameter with direct calls to the concrete
/// callee.
fn transform_callable_body(
    package: &mut Package,
    package_id: PackageId,
    callable_impl: &CallableImpl,
    param: &CallableParam,
    concrete: &ConcreteCallable,
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
                &mut alias_set,
                assigner,
            );
            if let Some(ref adj) = spec_impl.adj {
                transform_block(
                    package,
                    package_id,
                    adj.block,
                    param,
                    concrete,
                    &mut alias_set,
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
                    &mut alias_set,
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
                    &mut alias_set,
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
                &mut alias_set,
                assigner,
            );
        }
    }
}

/// Recursively walks a block, transforming call expressions that invoke the
/// callable parameter.
fn transform_block(
    package: &mut Package,
    package_id: PackageId,
    block_id: qsc_fir::fir::BlockId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    alias_set: &mut AliasSet,
    assigner: &mut Assigner,
) {
    let block = package
        .blocks
        .get(block_id)
        .expect("block not found")
        .clone();
    for &stmt_id in &block.stmts {
        transform_stmt(
            package, package_id, stmt_id, param, concrete, alias_set, assigner,
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

fn transform_stmt(
    package: &mut Package,
    package_id: PackageId,
    stmt_id: qsc_fir::fir::StmtId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    alias_set: &mut AliasSet,
    assigner: &mut Assigner,
) {
    let stmt = package.stmts.get(stmt_id).expect("stmt not found").clone();
    match &stmt.kind {
        qsc_fir::fir::StmtKind::Expr(expr_id) | qsc_fir::fir::StmtKind::Semi(expr_id) => {
            transform_expr(
                package, package_id, *expr_id, param, concrete, alias_set, assigner,
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
                package, package_id, *expr_id, param, concrete, alias_set, assigner,
            );
        }
        qsc_fir::fir::StmtKind::Item(_) => {}
    }
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
fn transform_expr(
    package: &mut Package,
    package_id: PackageId,
    expr_id: ExprId,
    param: &CallableParam,
    concrete: &ConcreteCallable,
    alias_set: &mut AliasSet,
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

            let replaced = if let ExprKind::Var(Res::Local(var), _) = &base_kind
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
            } else if !param.field_path.is_empty()
                && expr_matches_param_field_path(
                    package,
                    base_id,
                    param.param_var,
                    &param.field_path,
                )
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
                    package, package_id, callee_id, param, concrete, alias_set, assigner,
                );
            }

            // Recurse into the arguments.
            transform_expr(
                package, package_id, args_id, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::Block(block_id) => {
            transform_block(
                package, package_id, *block_id, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::If(cond, body, els) => {
            transform_expr(
                package, package_id, *cond, param, concrete, alias_set, assigner,
            );
            transform_expr(
                package, package_id, *body, param, concrete, alias_set, assigner,
            );
            if let Some(els_id) = els {
                transform_expr(
                    package, package_id, *els_id, param, concrete, alias_set, assigner,
                );
            }
        }
        ExprKind::While(cond, block_id) => {
            transform_expr(
                package, package_id, *cond, param, concrete, alias_set, assigner,
            );
            transform_block(
                package, package_id, *block_id, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::Tuple(exprs) | ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) => {
            for &e in exprs {
                transform_expr(package, package_id, e, param, concrete, alias_set, assigner);
            }
        }
        ExprKind::Assign(lhs, rhs)
        | ExprKind::AssignOp(_, lhs, rhs)
        | ExprKind::BinOp(_, lhs, rhs)
        | ExprKind::ArrayRepeat(lhs, rhs)
        | ExprKind::Index(lhs, rhs) => {
            transform_expr(
                package, package_id, *lhs, param, concrete, alias_set, assigner,
            );
            transform_expr(
                package, package_id, *rhs, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::AssignField(a, _, b) | ExprKind::UpdateField(a, _, b) => {
            transform_expr(
                package, package_id, *a, param, concrete, alias_set, assigner,
            );
            transform_expr(
                package, package_id, *b, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            transform_expr(
                package, package_id, *a, param, concrete, alias_set, assigner,
            );
            transform_expr(
                package, package_id, *b, param, concrete, alias_set, assigner,
            );
            transform_expr(
                package, package_id, *c, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::UnOp(_, inner) | ExprKind::Return(inner) | ExprKind::Fail(inner) => {
            transform_expr(
                package, package_id, *inner, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::Field(inner_id, _) => {
            // For nested callable params, check if this Field expression
            // accesses the arrow element within the param variable.
            if !param.field_path.is_empty()
                && expr_matches_param_field_path(
                    package,
                    expr_id,
                    param.param_var,
                    &param.field_path,
                )
            {
                replace_callee(
                    package,
                    package_id,
                    expr_id,
                    FunctorApp::default(),
                    concrete,
                    assigner,
                );
                return;
            }
            transform_expr(
                package, package_id, *inner_id, param, concrete, alias_set, assigner,
            );
        }
        ExprKind::Range(a, b, c) => {
            if let Some(a) = a {
                transform_expr(
                    package, package_id, *a, param, concrete, alias_set, assigner,
                );
            }
            if let Some(b) = b {
                transform_expr(
                    package, package_id, *b, param, concrete, alias_set, assigner,
                );
            }
            if let Some(c) = c {
                transform_expr(
                    package, package_id, *c, param, concrete, alias_set, assigner,
                );
            }
        }
        ExprKind::String(components) => {
            for comp in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = comp {
                    transform_expr(
                        package, package_id, *e, param, concrete, alias_set, assigner,
                    );
                }
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                transform_expr(
                    package, package_id, *c, param, concrete, alias_set, assigner,
                );
            }
            for f in fields {
                transform_expr(
                    package, package_id, f.value, param, concrete, alias_set, assigner,
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
            replace_callee(
                package,
                package_id,
                expr_id,
                FunctorApp::default(),
                concrete,
                assigner,
            );
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
                    assigner,
                );
            }
        }
        // Terminals with no sub-expressions.
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
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
/// [`super::rewrite::apply_target_input_at_control_path`]; keep the two in
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
/// Closure captures map 1-1 to the first N parameters of the closure target
/// callable's input pattern (in order). To specialize:
/// 1. Look up the capture's corresponding binding in the target callable.
/// 2. Walk the target callable's body, replacing `Var(Local(capture_param))`
///    with a direct reference to the concrete callable.
/// 3. Remove the capture binding from the target callable's input pattern.
/// 4. Remove the capture from the `Closure` expression's capture list.
#[allow(clippy::too_many_arguments)]
fn transform_closure_param_capture(
    package: &mut Package,
    package_id: PackageId,
    closure_expr_id: ExprId,
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
        closure_target,
        target_decl.input,
        capture_idx,
        Vec::new(),
        capture_param_var,
        param.param_ty.clone(),
    );

    // Step 3: Transform the target callable's body to replace uses of the
    // captured param with the concrete callable.
    transform_callable_body(
        package,
        package_id,
        &target_decl.implementation,
        &closure_param,
        concrete,
        assigner,
    );

    // Step 4: Remove the capture binding from the target callable's input.
    remove_capture_from_closure_target(package, closure_target, capture_idx);

    // Step 5: Remove the capture from the Closure expression.
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

/// Removes the capture at `capture_idx` from the closure target callable's
/// input pattern tuple.
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
fn thread_closure_captures(
    cloner: &mut FirCloner,
    package: &mut Package,
    decl: &mut CallableDecl,
    _param: &CallableParam,
    captures: &[CapturedVar],
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

        let name: Rc<str> = Rc::from(format!("__capture_{i}"));
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

/// Rewrites the call-argument expression for a closure target by splicing
/// the captured bindings into the appropriate slot of the call's argument
/// tuple.
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
                prepend_capture_args_to_call(
                    package,
                    args_id,
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
fn prepend_capture_args_to_call(
    package: &mut Package,
    args_id: ExprId,
    capture_bindings: &[(LocalVarId, Ty)],
    controlled_layers: usize,
    assigner: &mut Assigner,
) {
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

/// Removes the callable parameter from the specialized callable's input
/// pattern and updates the corresponding types.
fn remove_callable_param(package: &mut Package, decl: &mut CallableDecl, param: &CallableParam) {
    if !param.field_path.is_empty() {
        remove_nested_callable_param(package, decl, param);
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
fn remove_nested_callable_param(
    package: &mut Package,
    decl: &mut CallableDecl,
    param: &CallableParam,
) {
    let input_pat = package
        .pats
        .get(decl.input)
        .expect("input pat not found")
        .clone();

    let outer_idx = param.top_level_param;
    let inner_path = param.field_path.as_slice();

    match &input_pat.kind {
        PatKind::Tuple(pats) => {
            // Navigate to the sub-pattern at outer_idx and modify its type.
            let sub_pat_id = pats[outer_idx];
            let sub_pat = package.pats.get(sub_pat_id).expect("pat not found").clone();
            let new_ty = remove_ty_at_nested_path(package, &sub_pat.ty, inner_path);
            let sub_pat_mut = package.pats.get_mut(sub_pat_id).expect("pat not found");
            sub_pat_mut.ty = new_ty.clone();

            // Update the outer tuple's type to reflect the changed sub-parameter.
            let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
            if let Ty::Tuple(ref mut tys) = input_pat_mut.ty {
                tys[outer_idx] = new_ty;
            }
        }
        PatKind::Bind(_) => {
            // Single param that is a tuple type — modify the type directly.
            let new_ty = remove_ty_at_nested_path(package, &input_pat.ty, inner_path);
            let input_pat_mut = package.pats.get_mut(decl.input).expect("pat not found");
            input_pat_mut.ty = new_ty;
        }
        PatKind::Discard => {}
    }

    // Rewrite destructuring patterns in the body that bind param_var's tuple.
    if !inner_path.is_empty() {
        if let CallableImpl::Spec(spec_impl) = &decl.implementation {
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
            for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
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
        qsc_fir::fir::StmtKind::Item(_) => {}
    }
}

#[allow(clippy::too_many_lines)]
/// Recursively copies an expression and its transitive references into the
/// extraction target.
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
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
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
