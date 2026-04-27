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
//!   in [`super::specialize::apply_target_input_at_control_path`]. The copy
//!   is retained so that specialize and rewrite can evolve their
//!   controlled-layer handling independently without forcing a shared
//!   abstraction boundary; update both copies in lockstep when
//!   controlled-layer semantics change.

use super::types::{
    AnalysisResult, CallSite, CallableParam, CapturedVar, ConcreteCallable, DirectCallSite,
    SpecKey, peel_body_functors,
};
use super::{build_spec_key, ty_contains_arrow};
use crate::EMPTY_EXEC_RANGE;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, Expr, ExprId, ExprKind, Functor, ItemId, ItemKind, Lit, LocalItemId, LocalVarId,
    Mutability, Package, PackageId, PackageLookup, PatId, PatKind, Res, StmtKind, UnOp,
};
use qsc_fir::ty::{Arrow, Prim, Ty};
use rustc_hash::{FxHashMap, FxHashSet};

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
#[allow(clippy::too_many_lines)]
pub(super) fn rewrite(
    package: &mut Package,
    package_id: PackageId,
    analysis: &AnalysisResult,
    spec_map: &FxHashMap<SpecKey, LocalItemId>,
    assigner: &mut Assigner,
) {
    let expr_owner_lookup = build_expr_owner_lookup(package);
    let mut rewritten_callable_arg_locals = FxHashSet::default();

    // Build a lookup from HOF LocalItemId → CallableParam.
    let param_lookup: FxHashMap<LocalItemId, &CallableParam> = {
        let mut map = FxHashMap::default();
        for p in &analysis.callable_params {
            map.entry(p.callable_id).or_insert(p);
        }
        map
    };

    // Group resolved call sites by call_expr_id so that multi-callee sites
    // (from branch-split analysis) are handled together.
    let mut grouped: FxHashMap<ExprId, Vec<(&CallSite, LocalItemId, &CallableParam)>> =
        FxHashMap::default();

    for call_site in &analysis.call_sites {
        // Skip dynamic callables — they have no specialization.
        if matches!(call_site.callable_arg, ConcreteCallable::Dynamic) {
            continue;
        }

        let spec_key = build_spec_key(call_site);
        let Some(&spec_local_id) = spec_map.get(&spec_key) else {
            continue;
        };

        let hof_local_id = call_site.hof_item_id.item;
        let Some(&param) = param_lookup.get(&hof_local_id) else {
            continue;
        };

        grouped
            .entry(call_site.call_expr_id)
            .or_default()
            .push((call_site, spec_local_id, param));
    }

    for (call_expr_id, entries) in &grouped {
        if entries.len() == 1 {
            let (call_site, spec_local_id, param) = entries[0];
            collect_rewritten_callable_arg_local(
                package,
                &expr_owner_lookup,
                call_site.call_expr_id,
                call_site.arg_expr_id,
                &mut rewritten_callable_arg_locals,
            );
            rewrite_one(
                package,
                package_id,
                call_site,
                param,
                spec_local_id,
                assigner,
            );
        } else {
            for (call_site, _, _) in entries {
                collect_rewritten_callable_arg_local(
                    package,
                    &expr_owner_lookup,
                    call_site.call_expr_id,
                    call_site.arg_expr_id,
                    &mut rewritten_callable_arg_locals,
                );
            }
            branch_split_rewrite(
                package,
                package_id,
                *call_expr_id,
                entries,
                &expr_owner_lookup,
                assigner,
            );
        }
    }

    let mut grouped_direct: FxHashMap<ExprId, Vec<&DirectCallSite>> = FxHashMap::default();
    for direct_call_site in &analysis.direct_call_sites {
        grouped_direct
            .entry(direct_call_site.call_expr_id)
            .or_default()
            .push(direct_call_site);
    }

    for entries in grouped_direct.values() {
        if entries.len() == 1 && entries[0].condition.is_none() {
            rewrite_direct_call(
                package,
                package_id,
                entries[0],
                &expr_owner_lookup,
                &mut rewritten_callable_arg_locals,
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
                &expr_owner_lookup,
                call_expr_id,
                callee_id,
                &mut rewritten_callable_arg_locals,
            );
            branch_split_direct_call_rewrite(
                package,
                package_id,
                call_expr_id,
                entries,
                &expr_owner_lookup,
                assigner,
            );
        }
    }

    prune_dead_callable_arg_locals(package, &rewritten_callable_arg_locals);
}

/// Rewrites a `DirectCallSite` whose callee was resolved to a specific
/// concrete callable into a direct invocation of that callable, pruning
/// the now-unused callee expression.
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

    let mut conditioned: Vec<(&DirectCallSite, ExprId)> = Vec::new();
    let mut default = None;
    for &entry in entries {
        if let Some(condition) = entry.condition {
            conditioned.push((entry, condition));
        } else if default.is_none() {
            default = Some(entry);
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
            .map(|(entry_idx, condition)| (entries[entry_idx], condition))
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

    let else_call_id = create_direct_branch_call(
        package,
        package_id,
        &orig_callee,
        &orig_args,
        span,
        &result_ty,
        default_entry,
        assigner,
    );

    let mut current_else = else_call_id;
    for (entry, cond_id) in conditioned.into_iter().rev() {
        let branch_call_id = create_direct_branch_call(
            package,
            package_id,
            &orig_callee,
            &orig_args,
            span,
            &result_ty,
            entry,
            assigner,
        );
        current_else = alloc_if_expr(
            package,
            span,
            &result_ty,
            cond_id,
            branch_call_id,
            current_else,
            assigner,
        );
    }

    let dispatch = package
        .exprs
        .get(current_else)
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

/// Synthesizes an index-dispatch `if`/`else` chain for a HOF call site that
/// resolves to multiple callables via branch-split analysis.
fn synthesize_callsite_index_dispatch(
    package: &mut Package,
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    call_expr_id: ExprId,
    entries: &[(&CallSite, LocalItemId, &CallableParam)],
    span: Span,
    assigner: &mut Assigner,
) -> Option<(Vec<(usize, ExprId)>, usize)> {
    let callables = entries
        .iter()
        .map(|(call_site, _, _)| call_site.callable_arg.clone())
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

    let mut conditioned = Vec::with_capacity(callables.len().saturating_sub(1));
    for (entry_idx, position) in entry_positions.into_iter().enumerate() {
        if entry_idx == default_idx {
            continue;
        }
        let condition = alloc_index_eq_expr(package, index_expr_id, position, span, assigner);
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
        _ => None,
    }
}

/// Allocates a `BinOp(Eq, index_expr, Int(index_value))` expression used as
/// the condition guard for index-dispatch branches.
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

    find_local_init_expr_in_callable_impl(package, &decl.implementation, local_var)
}

/// Recurses over a `CallableImpl` variant to locate a local variable's
/// initializer expression.
fn find_local_init_expr_in_callable_impl(
    package: &Package,
    callable_impl: &qsc_fir::fir::CallableImpl,
    local_var: LocalVarId,
) -> Option<ExprId> {
    match callable_impl {
        qsc_fir::fir::CallableImpl::Intrinsic => None,
        qsc_fir::fir::CallableImpl::SimulatableIntrinsic(spec_decl) => {
            find_local_init_expr_in_block(package, spec_decl.block, local_var)
        }
        qsc_fir::fir::CallableImpl::Spec(spec_impl) => {
            find_local_init_expr_in_block(package, spec_impl.body.block, local_var).or_else(|| {
                [
                    spec_impl.adj.as_ref(),
                    spec_impl.ctl.as_ref(),
                    spec_impl.ctl_adj.as_ref(),
                ]
                .into_iter()
                .flatten()
                .find_map(|spec| find_local_init_expr_in_block(package, spec.block, local_var))
            })
        }
    }
}

/// Walks a block's statements looking for the `Local` binding of the
/// requested local variable.
fn find_local_init_expr_in_block(
    package: &Package,
    block_id: qsc_fir::fir::BlockId,
    local_var: LocalVarId,
) -> Option<ExprId> {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        if let StmtKind::Local(_, pat_id, init_expr_id) = stmt.kind
            && pat_binds_local_var(package, pat_id, local_var)
        {
            return Some(init_expr_id);
        }

        let nested = match stmt.kind {
            StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
                find_local_init_expr_in_expr(package, expr_id, local_var)
            }
            StmtKind::Item(_) => None,
        };
        if nested.is_some() {
            return nested;
        }
    }

    None
}

/// Descends into nested expressions (blocks, conditionals, loops) while
/// searching for a local variable's initializer.
fn find_local_init_expr_in_expr(
    package: &Package,
    expr_id: ExprId,
    local_var: LocalVarId,
) -> Option<ExprId> {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => exprs
            .iter()
            .find_map(|&expr_id| find_local_init_expr_in_expr(package, expr_id, local_var)),
        ExprKind::ArrayRepeat(lhs, rhs)
        | ExprKind::Assign(lhs, rhs)
        | ExprKind::AssignOp(_, lhs, rhs)
        | ExprKind::BinOp(_, lhs, rhs)
        | ExprKind::Call(lhs, rhs)
        | ExprKind::Index(lhs, rhs)
        | ExprKind::AssignField(lhs, _, rhs)
        | ExprKind::UpdateField(lhs, _, rhs) => {
            find_local_init_expr_in_expr(package, *lhs, local_var)
                .or_else(|| find_local_init_expr_in_expr(package, *rhs, local_var))
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            find_local_init_expr_in_expr(package, *a, local_var)
                .or_else(|| find_local_init_expr_in_expr(package, *b, local_var))
                .or_else(|| find_local_init_expr_in_expr(package, *c, local_var))
        }
        ExprKind::Block(block_id) => find_local_init_expr_in_block(package, *block_id, local_var),
        ExprKind::Fail(inner)
        | ExprKind::Field(inner, _)
        | ExprKind::Return(inner)
        | ExprKind::UnOp(_, inner) => find_local_init_expr_in_expr(package, *inner, local_var),
        ExprKind::If(cond, body, otherwise) => {
            find_local_init_expr_in_expr(package, *cond, local_var)
                .or_else(|| find_local_init_expr_in_expr(package, *body, local_var))
                .or_else(|| {
                    otherwise.and_then(|expr_id| {
                        find_local_init_expr_in_expr(package, expr_id, local_var)
                    })
                })
        }
        ExprKind::Range(start, step, end) => start
            .and_then(|expr_id| find_local_init_expr_in_expr(package, expr_id, local_var))
            .or_else(|| {
                step.and_then(|expr_id| find_local_init_expr_in_expr(package, expr_id, local_var))
            })
            .or_else(|| {
                end.and_then(|expr_id| find_local_init_expr_in_expr(package, expr_id, local_var))
            }),
        ExprKind::String(components) => components.iter().find_map(|component| match component {
            qsc_fir::fir::StringComponent::Expr(expr_id) => {
                find_local_init_expr_in_expr(package, *expr_id, local_var)
            }
            qsc_fir::fir::StringComponent::Lit(_) => None,
        }),
        ExprKind::Struct(_, copy, fields) => copy
            .and_then(|expr_id| find_local_init_expr_in_expr(package, expr_id, local_var))
            .or_else(|| {
                fields
                    .iter()
                    .find_map(|field| find_local_init_expr_in_expr(package, field.value, local_var))
            }),
        ExprKind::While(cond, block_id) => find_local_init_expr_in_expr(package, *cond, local_var)
            .or_else(|| find_local_init_expr_in_block(package, *block_id, local_var)),
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => None,
    }
}

/// Removes callable-typed argument locals whose only remaining uses were
/// rewritten into direct dispatch calls, leaving no arrow-typed residue.
fn prune_dead_callable_arg_locals(
    package: &mut Package,
    rewritten_callable_arg_locals: &FxHashSet<(LocalItemId, LocalVarId)>,
) {
    for &(callable_id, local_var) in rewritten_callable_arg_locals {
        if !local_var_is_used_in_callable(package, callable_id, local_var) {
            remove_dead_callable_local_from_callable(package, callable_id, local_var);
        }
    }

    prune_dead_top_level_callable_locals(package);
}

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

/// Removes a specific dead callable local from the given callable's body
/// by deleting its `Local` binding and any references that remain.
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
/// dispatch rewrites, scoped to the package-level entry expression.
fn prune_dead_top_level_callable_locals(package: &mut Package) {
    let callable_items: Vec<(LocalItemId, qsc_fir::fir::CallableImpl)> = package
        .items
        .iter()
        .filter_map(|(item_id, item)| match &item.kind {
            ItemKind::Callable(decl) => Some((item_id, decl.implementation.clone())),
            _ => None,
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
/// rather than requiring multiple outer fixpoint iterations.
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
                    if ty_contains_arrow(&pat.ty) {
                        let mut bound_vars = Vec::new();
                        collect_bound_pat_vars(package, pat_id, &mut bound_vars);
                        !bound_vars.is_empty()
                            && bound_vars.iter().all(|var| {
                                let mut uses = Vec::new();
                                crate::walk_utils::collect_uses_in_block(
                                    package, block_id, *var, &mut uses,
                                );
                                uses.is_empty()
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
/// `Local` binding and any remaining references.
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
            && ty_contains_arrow(&package.get_pat(pat_id).ty)
            && pat_binds_local_var(package, pat_id, local_var)
        {
            // Only remove when ALL bound variables in the pattern are
            // unused; a tuple pattern may bind siblings that are still live.
            let mut bound_vars = Vec::new();
            collect_bound_pat_vars(package, pat_id, &mut bound_vars);
            bound_vars.iter().all(|&var| {
                let mut uses = Vec::new();
                crate::walk_utils::collect_uses_in_block(package, block_id, var, &mut uses);
                uses.is_empty()
            })
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

/// Inspects a single statement for dead callable-local bindings and
/// deletes them when safe.
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
/// bindings introduced by direct-call rewrites.
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

/// Removes a specific dead callable local scoped to a single statement.
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
/// subtree.
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

fn pat_binds_local_var(package: &Package, pat_id: PatId, local_var: LocalVarId) -> bool {
    let mut bound_vars = Vec::new();
    collect_bound_pat_vars(package, pat_id, &mut bound_vars);
    bound_vars
        .into_iter()
        .any(|bound_var| bound_var == local_var)
}

/// For a local variable bound inside a tuple pattern (e.g.,
/// `let (_, callee, _) = tuple_expr`), returns the field position
/// path (e.g., `[1]` for position 1).
fn find_var_tuple_field_path_in_callable(
    package: &Package,
    callable_id: LocalItemId,
    local_var: LocalVarId,
) -> Option<Vec<usize>> {
    let item = package.items.get(callable_id)?;
    let ItemKind::Callable(decl) = &item.kind else {
        return None;
    };
    match &decl.implementation {
        qsc_fir::fir::CallableImpl::Intrinsic => None,
        qsc_fir::fir::CallableImpl::SimulatableIntrinsic(spec_decl) => {
            find_var_tuple_field_path_in_block(package, spec_decl.block, local_var)
        }
        qsc_fir::fir::CallableImpl::Spec(spec_impl) => find_var_tuple_field_path_in_block(
            package,
            spec_impl.body.block,
            local_var,
        )
        .or_else(|| {
            [
                spec_impl.adj.as_ref(),
                spec_impl.ctl.as_ref(),
                spec_impl.ctl_adj.as_ref(),
            ]
            .into_iter()
            .flatten()
            .find_map(|spec| find_var_tuple_field_path_in_block(package, spec.block, local_var))
        }),
    }
}

/// Walks a block's statements looking for a `PatKind::Tuple` binding that
/// contains the requested local variable.
fn find_var_tuple_field_path_in_block(
    package: &Package,
    block_id: qsc_fir::fir::BlockId,
    local_var: LocalVarId,
) -> Option<Vec<usize>> {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        if let StmtKind::Local(_, pat_id, _) = stmt.kind
            && let Some(path) = find_var_field_path_in_pat(package, pat_id, local_var)
            && !path.is_empty()
        {
            return Some(path);
        }
        // Also descend into nested blocks and control flow
        let nested = match stmt.kind {
            StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
                find_var_tuple_field_path_in_expr(package, expr_id, local_var)
            }
            StmtKind::Item(_) => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

/// Descends into nested expressions (blocks, conditionals, loops) to find
/// the tuple field path of a local variable binding.
fn find_var_tuple_field_path_in_expr(
    package: &Package,
    expr_id: ExprId,
    local_var: LocalVarId,
) -> Option<Vec<usize>> {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Block(block_id) | ExprKind::While(_, block_id) => {
            find_var_tuple_field_path_in_block(package, *block_id, local_var)
        }
        ExprKind::If(_, body, otherwise) => {
            find_var_tuple_field_path_in_expr(package, *body, local_var).or_else(|| {
                otherwise.and_then(|e| find_var_tuple_field_path_in_expr(package, e, local_var))
            })
        }
        _ => None,
    }
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
/// [`super::specialize::apply_target_input_at_control_path`]; keep the two
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
/// that take a single tuple parameter are named with a leading `"<lambda>"`
/// prefix. Do not rename lambda items without updating this predicate.
fn direct_lambda_packaged_input(package: &Package, item_id: LocalItemId) -> Option<Ty> {
    let ItemKind::Callable(decl) = &package.get_item(item_id).kind else {
        return None;
    };

    let input_ty = package.get_pat(decl.input).ty.clone();
    if decl.name.name.as_ref().starts_with("<lambda>")
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
fn rewrite_one(
    package: &mut Package,
    package_id: PackageId,
    call_site: &CallSite,
    param: &CallableParam,
    spec_local_id: LocalItemId,
    assigner: &mut Assigner,
) {
    let call_expr = package.get_expr(call_site.call_expr_id).clone();

    let ExprKind::Call(callee_id, args_id) = call_expr.kind else {
        return;
    };

    // Replace callee with the specialized callable reference
    let spec_item_id = ItemId {
        package: package_id,
        item: spec_local_id,
    };

    // Build the new callee type: remove the callable param from the arrow input.
    let input_path = callable_param_input_path(package, callee_id, param);
    let new_callee_ty =
        build_specialized_callee_ty(package, callee_id, &input_path, &call_site.callable_arg);
    rewrite_specialized_callee(package, callee_id, spec_item_id, new_callee_ty, assigner);

    // Remove the callable argument from the args tuple
    // Insert closure captures as extra arguments
    let captures = match &call_site.callable_arg {
        ConcreteCallable::Closure { captures, .. } => {
            resolve_rewrite_captures(package, call_site.arg_expr_id, captures)
        }
        _ => Vec::new(),
    };
    rewrite_args(package, args_id, &input_path, &captures, assigner);
}

/// Removes the callable argument selected by `param` from the call arguments
/// and appends closure captures when needed.
fn rewrite_args(
    package: &mut Package,
    args_id: ExprId,
    input_path: &[usize],
    captures: &[CapturedVar],
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
        if input_path.len() == 1 {
            rewrite_args_remove_tuple_element(package, args_id, input_path[0], captures, assigner);
        } else {
            rewrite_args_nested_tuple_input(
                package,
                args_id,
                input_path[0],
                &input_path[1..],
                captures,
                assigner,
            );
        }
    } else if input_path.len() == 1 {
        rewrite_args_remove_tuple_element(package, args_id, input_path[0], captures, assigner);
    } else {
        rewrite_single_arg_nested(package, args_id, input_path, captures, assigner);
    }
}

/// Removes a top-level element from a tuple-structured args expression and
/// appends any closure captures.
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
            let new_ty = build_tuple_ty_without_path(&args_expr.ty, &[param_index], captures);

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
/// Captures are appended to the top-level args tuple.
fn rewrite_args_nested_tuple_input(
    package: &mut Package,
    args_id: ExprId,
    top_level_param: usize,
    field_path: &[usize],
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
        // Remove the nested element from the inner tuple.
        remove_element_at_path(package, inner_id, field_path);

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

/// Rewrites args when the callable is nested inside the single argument value.
fn rewrite_single_arg_nested(
    package: &mut Package,
    args_id: ExprId,
    field_path: &[usize],
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) {
    remove_element_at_path(package, args_id, field_path);
    if !captures.is_empty() {
        let span = package.get_expr(args_id).span;
        let capture_ids = allocate_capture_exprs(package, span, captures, assigner);
        let modified_expr = package.exprs.get(args_id).expect("expr not found").clone();
        let mut new_elements = match &modified_expr.kind {
            ExprKind::Tuple(elems) => elems.clone(),
            _ => vec![args_id],
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

/// Replaces the single direct callable argument with unit or a capture tuple.
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
    } else {
        let capture_ids = allocate_capture_exprs(package, args_expr.span, captures, assigner);
        let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();
        let args_mut = package.exprs.get_mut(args_id).expect("args expr not found");
        args_mut.kind = ExprKind::Tuple(capture_ids);
        args_mut.ty = Ty::Tuple(capture_tys);
    }
}

/// Removes the element at `path` from a tuple expression, modifying
/// the expression in place. Collapses single-element tuples.
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

/// Allocates new expressions for each captured variable and inserts them into
/// the package. Returns the `ExprId`s of the newly created expressions.
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
            ids.push(expr_id);
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

/// Builds the new callee type by removing the callable parameter from the
/// arrow's input type and appending capture types when applicable.
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

    let new_input = remove_ty_at_path(&arrow.input, input_path, captures);
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
fn remove_ty_at_path(ty: &Ty, path: &[usize], captures: &[CapturedVar]) -> Ty {
    let capture_tys: Vec<Ty> = captures.iter().map(|c| c.ty.clone()).collect();

    if path.is_empty() {
        return if capture_tys.is_empty() {
            Ty::UNIT
        } else {
            Ty::Tuple(capture_tys)
        };
    }

    if path.len() == 1 {
        if let Ty::Tuple(tys) = ty {
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
        if let Ty::Tuple(tys) = ty {
            let mut new_tys = tys.clone();
            // Remove nested element without captures at inner level.
            new_tys[path[0]] = remove_ty_at_path(&tys[path[0]], &path[1..], &[]);
            // Append captures at the top level.
            new_tys.extend(capture_tys);
            Ty::Tuple(new_tys)
        } else {
            // Single param that is a tuple type — remove from within.
            let modified = remove_ty_at_path(ty, &path[1..], &[]);
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
fn build_tuple_ty_without_path(ty: &Ty, param_path: &[usize], captures: &[CapturedVar]) -> Ty {
    remove_ty_at_path(ty, param_path, captures)
}

fn callable_uses_tuple_input_pattern(package: &Package, callable_id: LocalItemId) -> bool {
    let item = package.get_item(callable_id);
    match &item.kind {
        ItemKind::Callable(decl) => matches!(package.get_pat(decl.input).kind, PatKind::Tuple(_)),
        _ => false,
    }
}

fn callable_param_input_path(
    package: &Package,
    callee_id: ExprId,
    param: &CallableParam,
) -> Vec<usize> {
    let (_, outer_functor) = peel_body_functors(package, callee_id);
    let uses_tuple = callable_uses_tuple_input_pattern(package, param.callable_id);
    super::build_param_input_path(uses_tuple, param, outer_functor)
}

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

/// Rewrites a call site that has multiple callee candidates (from branch-split
/// analysis) into an if/elif/else dispatch chain where each branch calls the
/// appropriate specialization.
fn branch_split_rewrite(
    package: &mut Package,
    package_id: PackageId,
    call_expr_id: ExprId,
    entries: &[(&CallSite, LocalItemId, &CallableParam)],
    expr_owner_lookup: &FxHashMap<ExprId, LocalItemId>,
    assigner: &mut Assigner,
) {
    // Save original call info before any modifications.
    let orig_call = package.get_expr(call_expr_id).clone();
    let ExprKind::Call(orig_callee_id, orig_args_id) = orig_call.kind else {
        return;
    };
    let span = orig_call.span;
    let result_ty = orig_call.ty.clone();

    // Separate conditioned entries (if branches) from the default (else).
    let mut conditioned: Vec<((&CallSite, LocalItemId, &CallableParam), ExprId)> = Vec::new();
    let mut default: Option<(&CallSite, LocalItemId, &CallableParam)> = None;
    for &entry in entries {
        if let Some(condition) = entry.0.condition {
            conditioned.push((entry, condition));
        } else if default.is_none() {
            default = Some(entry);
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
            .map(|(entry_idx, condition)| (entries[entry_idx], condition))
            .collect();
        default = Some(entries[default_idx]);
    }

    // Must have a default for the else branch; steal last conditioned if needed.
    let default_entry = if let Some(d) = default {
        d
    } else {
        if conditioned.is_empty() {
            return;
        }
        conditioned.pop().expect("non-empty conditioned").0
    };

    if conditioned.is_empty() {
        // Single effective entry — use normal rewrite.
        rewrite_one(
            package,
            package_id,
            default_entry.0,
            default_entry.2,
            default_entry.1,
            assigner,
        );
        return;
    }

    // Clone original callee and args expressions before modifications.
    let orig_callee = package.get_expr(orig_callee_id).clone();
    let orig_args = package.get_expr(orig_args_id).clone();

    // Create the else (default) branch call.
    let else_call_id = create_branch_call(
        package,
        package_id,
        &orig_callee,
        &orig_args,
        span,
        &result_ty,
        default_entry.0,
        default_entry.2,
        default_entry.1,
        assigner,
    );

    // Build the if/elif chain from the bottom up.
    let mut current_else = else_call_id;
    for ((cs, spec_id, param), cond_id) in conditioned.into_iter().rev() {
        let branch_call_id = create_branch_call(
            package,
            package_id,
            &orig_callee,
            &orig_args,
            span,
            &result_ty,
            cs,
            param,
            spec_id,
            assigner,
        );
        current_else = alloc_if_expr(
            package,
            span,
            &result_ty,
            cond_id,
            branch_call_id,
            current_else,
            assigner,
        );
    }

    // Replace the original call expression with the dispatch chain.
    let dispatch = package
        .exprs
        .get(current_else)
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
#[allow(clippy::too_many_arguments)]
fn create_branch_call(
    package: &mut Package,
    package_id: PackageId,
    orig_callee: &Expr,
    orig_args: &Expr,
    span: Span,
    result_ty: &Ty,
    call_site: &CallSite,
    param: &CallableParam,
    spec_local_id: LocalItemId,
    assigner: &mut Assigner,
) -> ExprId {
    let spec_item_id = ItemId {
        package: package_id,
        item: spec_local_id,
    };

    // Specialised callee type.
    let input_path = callable_param_input_path(package, orig_callee.id, param);
    let new_callee_ty =
        build_specialized_callee_ty_from_expr(orig_callee, &input_path, &call_site.callable_arg);
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

/// Builds a `LocalVarId → ExprId` map from a block's statements, capturing
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
                    let new_ty = build_tuple_ty_without_path(&orig_args.ty, input_path, captures);
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
                    let new_ty = build_tuple_ty_without_path(&orig_args.ty, input_path, captures);
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
                build_tuple_ty_without_path(&orig_args.ty, input_path, captures),
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
                let new_ty = build_tuple_ty_without_path(&orig_args.ty, input_path, captures);
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
                build_tuple_ty_without_path(&orig_args.ty, input_path, captures),
            ),
        }
    } else {
        // Nested path: rebuild both the args type and expression with the
        // nested element removed.
        remove_element_at_path(package, orig_args.id, input_path);
        let new_ty = build_tuple_ty_without_path(&orig_args.ty, input_path, captures);
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

/// Allocates a fresh `Var` expression that references a specialized
/// callable item, returning its new `ExprId`.
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

/// Allocates a fresh callee expression that wraps an item reference with
/// the requested functor applications (`Adj` and/or `Ctl` layers).
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

/// Allocates a new `ExprKind::If` expression and inserts it into the package.
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

/// Builds the specialised callee type from a saved callee expression snapshot.
fn build_specialized_callee_ty_from_expr(
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
    let new_input = remove_ty_at_path(&arrow.input, input_path, captures);
    Some(Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(new_input),
        output: arrow.output.clone(),
        functors: arrow.functors,
    })))
}
