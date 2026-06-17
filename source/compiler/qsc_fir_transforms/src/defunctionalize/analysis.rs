// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Analysis phase of the defunctionalization pass.
//!
//! Discovers callable-typed parameters in higher-order functions, collects
//! call sites where those HOFs are invoked with concrete callable arguments,
//! and resolves each argument to a [`ConcreteCallable`].
//!
//! # Responsibilities
//!
//! - Discover arrow-typed callable parameters on reachable declarations
//!   (via [`find_callable_params`] / [`extract_arrow_params_from_ty`]).
//! - Collect direct and HOF call sites (via [`collect_call_sites`] /
//!   [`inspect_call_expr`] / [`inspect_direct_call_expr`]).
//! - Resolve callee expressions to concrete callables using flow-sensitive
//!   reaching definitions, closure captures, functor applications, indexed
//!   array elements, struct field accesses, and same-package callable
//!   returns (via [`resolve_callee`] and its helpers).
//! - Build per-callable lattice states that expose reaching-definition
//!   information back to the specialization and rewrite phases (via
//!   [`build_callable_flow_state`] / [`analyze_spec_flow`]).
//!
//! The defunctionalization pre-pass runs before this phase and owns callable
//! local promotion plus identity-closure peephole rewrites.

use super::types::{
    AnalysisResult, CallSite, CallableParam, CalleeLattice, CapturedVar, ConcreteCallable,
    DirectCallSite, LatticeStates, compose_functors, peel_body_functors,
};
use crate::fir_builder::functored_specs;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::fir::{
    BinOp, BlockId, CallableImpl, ExprId, ExprKind, Field, FieldAssign, FieldPath, ItemId,
    ItemKind, Lit, LocalVarId, Mutability, Package, PackageId, PackageLookup, PackageStore, PatId,
    PatKind, Res, SpecImpl, StmtKind, StoreItemId, StringComponent, UnOp,
};
use qsc_fir::ty::Ty;
use rustc_hash::{FxHashMap, FxHashSet};

/// Combined local variable state for the analysis phase.
///
/// `callable` holds flow-sensitive reaching-definitions for callable-typed
/// locals (both mutable and immutable). `exprs` holds raw `ExprId` bindings
/// for all immutable locals, supporting struct field resolution and type
/// look-ups.
#[derive(Default)]
pub(super) struct LocalState {
    callable: FxHashMap<LocalVarId, CalleeLattice>,
    exprs: FxHashMap<LocalVarId, ExprId>,
    condition_substitutions: FxHashMap<LocalVarId, ExprId>,
}

/// Maximum recursion depth when resolving callee expressions to prevent
/// infinite loops from unexpected circular references.
const MAX_RESOLVE_DEPTH: usize = 32;

/// Runs the analysis phase: finds callable parameters and collects call sites.
pub(super) fn analyze(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    collapsed_spans: &FxHashMap<ExprId, Span>,
) -> AnalysisResult {
    let hof_params = find_callable_params(store, reachable);
    let (call_sites, direct_call_sites, unresolved_direct_call_sites, lattice_states) =
        collect_call_sites(store, package_id, reachable, &hof_params, collapsed_spans);
    AnalysisResult {
        callable_params: hof_params.into_values().flatten().collect(),
        call_sites,
        direct_call_sites,
        unresolved_direct_call_sites,
        lattice_states,
    }
}

/// Scans all reachable callables (including cross-package ones like the
/// standard library) and returns a map from each HOF's `StoreItemId` to the
/// list of its arrow-typed parameters.
fn find_callable_params(
    store: &PackageStore,
    reachable: &FxHashSet<StoreItemId>,
) -> FxHashMap<StoreItemId, Vec<CallableParam>> {
    let mut result: FxHashMap<StoreItemId, Vec<CallableParam>> = FxHashMap::default();

    for &store_id in reachable {
        let pkg = store.get(store_id.package);
        let item = pkg.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let params = extract_arrow_params(store, pkg, store_id.item, decl.input);
            if !params.is_empty() {
                result.insert(store_id, params);
            }
        }
    }

    result
}

/// Extracts arrow-typed parameters from a callable's input pattern.
fn extract_arrow_params(
    store: &PackageStore,
    pkg: &Package,
    callable_id: qsc_fir::fir::LocalItemId,
    input_pat_id: qsc_fir::fir::PatId,
) -> Vec<CallableParam> {
    let pat = pkg.get_pat(input_pat_id);
    let mut params = Vec::new();

    match &pat.kind {
        PatKind::Tuple(sub_pats) => {
            for (index, &sub_pat_id) in sub_pats.iter().enumerate() {
                let sub_pat = pkg.get_pat(sub_pat_id);
                if let PatKind::Bind(ident) = &sub_pat.kind {
                    let mut field_path = Vec::new();
                    let context = ArrowParamExtraction {
                        store,
                        callable_id,
                        param_pat_id: sub_pat_id,
                        param_var: ident.id,
                        top_level_param: index,
                    };
                    extract_arrow_params_from_ty(
                        &context,
                        &sub_pat.ty,
                        &mut field_path,
                        &mut params,
                    );
                }
            }
        }
        PatKind::Bind(ident) => {
            let mut field_path = Vec::new();
            let context = ArrowParamExtraction {
                store,
                callable_id,
                param_pat_id: input_pat_id,
                param_var: ident.id,
                top_level_param: 0,
            };
            extract_arrow_params_from_ty(&context, &pat.ty, &mut field_path, &mut params);
        }
        PatKind::Discard => {}
    }

    params
}

/// Carries the invariant metadata needed while extracting callable parameters.
struct ArrowParamExtraction<'a> {
    store: &'a PackageStore,
    callable_id: qsc_fir::fir::LocalItemId,
    param_pat_id: PatId,
    param_var: LocalVarId,
    top_level_param: usize,
}

/// Recursively descends into the structural layers of a callable parameter
/// type and records every `Ty::Arrow` leaf as a `CallableParam`.
///
/// UDTs are expanded to their pure type so callable fields inside nested
/// newtypes are treated the same way as tuple fields.
fn extract_arrow_params_from_ty(
    context: &ArrowParamExtraction<'_>,
    param_ty: &Ty,
    field_path: &mut Vec<usize>,
    params: &mut Vec<CallableParam>,
) {
    match param_ty {
        Ty::Arrow(_) => params.push(CallableParam::new(
            context.callable_id,
            context.param_pat_id,
            context.top_level_param,
            field_path.clone(),
            context.param_var,
            param_ty.clone(),
        )),
        Ty::Tuple(items) => {
            for (index, item_ty) in items.iter().enumerate() {
                field_path.push(index);
                extract_arrow_params_from_ty(context, item_ty, field_path, params);
                field_path.pop();
            }
        }
        Ty::Udt(Res::Item(item_id)) => {
            let package = context.store.get(item_id.package);
            let item = package.get_item(item_id.item);
            let ItemKind::Ty(_, udt) = &item.kind else {
                return;
            };
            extract_arrow_params_from_ty(context, &udt.get_pure_ty(), field_path, params);
        }
        _ => {}
    }
}

/// Mutable context threaded through the ordered flow walk so each call site is
/// recorded against the running [`LocalState`] as of its evaluation point.
struct CallRecorder<'a> {
    hof_params: &'a FxHashMap<StoreItemId, Vec<CallableParam>>,
    call_sites: &'a mut Vec<CallSite>,
    direct_call_sites: &'a mut Vec<DirectCallSite>,
    /// Call expressions whose direct `Var(Res::Local)` callee resolved to
    /// `Dynamic`, recorded so the driver can emit a `DynamicCallable`
    /// diagnostic instead of only `FixpointNotReached`.
    unresolved_direct_call_sites: &'a mut Vec<ExprId>,
    /// Spans of lambda bodies discarded by the identity-closure peephole,
    /// keyed by the collapsed init-expr node, stamped onto surviving direct
    /// calls so circuit instructions point at the original lambda body.
    collapsed_spans: &'a FxHashMap<ExprId, Span>,
}

/// Walks the bodies of all reachable callables in the target package and
/// collects call sites where a HOF is invoked with a concrete callable
/// argument.
fn collect_call_sites(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    hof_params: &FxHashMap<StoreItemId, Vec<CallableParam>>,
    collapsed_spans: &FxHashMap<ExprId, Span>,
) -> (
    Vec<CallSite>,
    Vec<DirectCallSite>,
    Vec<ExprId>,
    LatticeStates,
) {
    let package = store.get(package_id);
    let mut call_sites = Vec::new();
    let mut direct_call_sites = Vec::new();
    let mut unresolved_direct_call_sites = Vec::new();
    let mut lattice_states: LatticeStates = FxHashMap::default();

    for &store_id in reachable {
        if store_id.package != package_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            // Record call sites inline against the running state produced by the
            // ordered flow walk, so each call resolves against its own program
            // point rather than the callable's final whole-body state.
            let mut recorder = CallRecorder {
                hof_params,
                call_sites: &mut call_sites,
                direct_call_sites: &mut direct_call_sites,
                unresolved_direct_call_sites: &mut unresolved_direct_call_sites,
                collapsed_spans,
            };
            let locals = build_callable_flow_state(
                package,
                store,
                &decl.implementation,
                package_id,
                Some(&mut recorder),
            );

            // Capture non-Bottom lattice entries, sorted by LocalVarId.
            let mut entries: Vec<(LocalVarId, CalleeLattice)> = locals
                .callable
                .iter()
                .filter(|(_, lat)| !matches!(lat, CalleeLattice::Bottom))
                .map(|(var, lat)| (*var, lat.clone()))
                .collect();
            entries.sort_by_key(|(var, _)| *var);
            if !entries.is_empty() {
                lattice_states.insert(store_id.item, entries);
            }
        }
    }

    if let Some(entry_expr_id) = package.entry {
        let mut locals = LocalState {
            callable: FxHashMap::default(),
            exprs: FxHashMap::default(),
            condition_substitutions: FxHashMap::default(),
        };
        let mut recorder = CallRecorder {
            hof_params,
            call_sites: &mut call_sites,
            direct_call_sites: &mut direct_call_sites,
            unresolved_direct_call_sites: &mut unresolved_direct_call_sites,
            collapsed_spans,
        };
        analyze_expr_flow(
            package,
            store,
            entry_expr_id,
            &mut locals,
            package_id,
            Some(&mut recorder),
        );
    }

    (
        call_sites,
        direct_call_sites,
        unresolved_direct_call_sites,
        lattice_states,
    )
}

/// Inspects a single expression for HOF call-site patterns.
#[allow(clippy::too_many_arguments)]
fn inspect_call_expr(
    store: &PackageStore,
    pkg: &Package,
    expr_id: ExprId,
    expr: &qsc_fir::fir::Expr,
    hof_params: &FxHashMap<StoreItemId, Vec<CallableParam>>,
    locals: &LocalState,
    call_sites: &mut Vec<CallSite>,
    direct_call_sites: &mut Vec<DirectCallSite>,
    unresolved_direct_call_sites: &mut Vec<ExprId>,
    package_id: PackageId,
    collapsed_spans: &FxHashMap<ExprId, Span>,
) {
    let ExprKind::Call(callee_expr_id, args_expr_id) = &expr.kind else {
        return;
    };

    if expr_contains_hole(pkg, *args_expr_id) {
        return;
    }

    if let Some((hof_store_id, hof_functor, hof_callable_params)) =
        resolve_hof_callee(pkg, *callee_expr_id, hof_params)
    {
        let uses_tuple_input = hof_uses_tuple_input_pattern(store, hof_store_id);
        for cp in hof_callable_params {
            let input_path = super::build_param_input_path(uses_tuple_input, cp, hof_functor);
            let resolved_arg_id = extract_arg_at_path(pkg, *args_expr_id, &input_path);
            let allow_scoped_capture_exprs = matches!(
                pkg.get_expr(resolved_arg_id).kind,
                ExprKind::Block(_) | ExprKind::If(_, _, _)
            );
            let resolved = resolve_callee_at_path(
                pkg,
                store,
                locals,
                *args_expr_id,
                &input_path,
                0,
                allow_scoped_capture_exprs,
                &FxHashSet::default(),
                package_id,
            );
            match resolved {
                CalleeLattice::Single(cc) => {
                    call_sites.push(CallSite {
                        call_expr_id: expr_id,
                        hof_item_id: ItemId {
                            package: hof_store_id.package,
                            item: hof_store_id.item,
                        },
                        callable_arg: cc,
                        arg_expr_id: resolved_arg_id,
                        condition: vec![],
                    });
                }
                CalleeLattice::Multi(candidates) => {
                    for (cc, cond) in candidates {
                        call_sites.push(CallSite {
                            call_expr_id: expr_id,
                            hof_item_id: ItemId {
                                package: hof_store_id.package,
                                item: hof_store_id.item,
                            },
                            callable_arg: cc,
                            arg_expr_id: resolved_arg_id,
                            condition: cond,
                        });
                    }
                }
                CalleeLattice::Dynamic | CalleeLattice::Bottom => {
                    call_sites.push(CallSite {
                        call_expr_id: expr_id,
                        hof_item_id: ItemId {
                            package: hof_store_id.package,
                            item: hof_store_id.item,
                        },
                        callable_arg: ConcreteCallable::Dynamic,
                        arg_expr_id: resolved_arg_id,
                        condition: vec![],
                    });
                }
            }
        }

        return;
    }

    inspect_direct_call_expr(
        store,
        pkg,
        expr_id,
        *callee_expr_id,
        locals,
        hof_params,
        direct_call_sites,
        unresolved_direct_call_sites,
        package_id,
        collapsed_spans,
    );
}

/// Returns `true` when an expression subtree contains an `ExprKind::Hole`
/// placeholder, which marks partial applications that the pass does not
/// yet specialize.
fn expr_contains_hole(pkg: &Package, expr_id: ExprId) -> bool {
    let mut contains_hole = false;
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_expr_id, expr| {
        if matches!(expr.kind, ExprKind::Hole) {
            contains_hole = true;
        }
    });
    contains_hole
}

/// Inspects a direct `Call(callee, args)` expression whose callee resolves
/// to a concrete callable value (global, closure, or functor-applied
/// callable) and, when resolution succeeds, records a [`DirectCallSite`].
#[allow(clippy::too_many_arguments)]
fn inspect_direct_call_expr(
    store: &PackageStore,
    pkg: &Package,
    expr_id: ExprId,
    callee_expr_id: ExprId,
    locals: &LocalState,
    hof_params: &FxHashMap<StoreItemId, Vec<CallableParam>>,
    direct_call_sites: &mut Vec<DirectCallSite>,
    unresolved_direct_call_sites: &mut Vec<ExprId>,
    package_id: PackageId,
    collapsed_spans: &FxHashMap<ExprId, Span>,
) {
    let callee_expr = pkg.get_expr(callee_expr_id);
    if matches!(callee_expr.kind, ExprKind::Var(Res::Item(_), _)) {
        return;
    }

    let callee_local_var = if let ExprKind::Var(Res::Local(var), _) = callee_expr.kind {
        Some(var)
    } else {
        None
    };

    let (resolved, def_span) = if let ExprKind::Var(Res::Local(var), _) = callee_expr.kind {
        if let Some(&init_expr_id) = locals.exprs.get(&var) {
            (
                resolve_callee(
                    pkg,
                    store,
                    locals,
                    init_expr_id,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                ),
                collapsed_spans.get(&init_expr_id).copied(),
            )
        } else {
            (
                resolve_callee(
                    pkg,
                    store,
                    locals,
                    callee_expr_id,
                    0,
                    false,
                    &FxHashSet::default(),
                    package_id,
                ),
                None,
            )
        }
    } else {
        let allow_scoped_capture_exprs = matches!(
            callee_expr.kind,
            ExprKind::Block(_) | ExprKind::If(_, _, _) | ExprKind::UnOp(_, _)
        );
        (
            resolve_callee(
                pkg,
                store,
                locals,
                callee_expr_id,
                0,
                allow_scoped_capture_exprs,
                &FxHashSet::default(),
                package_id,
            ),
            None,
        )
    };

    match resolved {
        CalleeLattice::Single(callable) => {
            direct_call_sites.push(DirectCallSite {
                call_expr_id: expr_id,
                callable,
                condition: vec![],
                def_span,
            });
        }
        CalleeLattice::Multi(candidates) => {
            for (callable, condition) in candidates {
                direct_call_sites.push(DirectCallSite {
                    call_expr_id: expr_id,
                    callable,
                    condition,
                    def_span,
                });
            }
        }
        CalleeLattice::Dynamic => {
            // A call whose callee is itself a HOF arrow-typed parameter (e.g.
            // `op(q)` in an un-specialized HOF body) is `Dynamic` only until
            // specialization substitutes the concrete callable. The HOF path
            // never diagnoses these forwarding calls, so neither do we.
            let callee_is_hof_param = callee_local_var.is_some_and(|var| {
                hof_params
                    .values()
                    .flatten()
                    .any(|param| param.param_var == var)
            });
            if !callee_is_hof_param {
                // An over-defined callee the pass cannot lower to direct
                // dispatch. Record the site so the driver emits an actionable
                // `DynamicCallable` (cleared per-pass by the driver's `retain`,
                // so only the converged state surfaces).
                unresolved_direct_call_sites.push(expr_id);
            }
        }
        // `Bottom`: the callee has not yet been observed reaching this point
        // (an intermediate fixpoint iteration). Emitting here would be
        // spurious, so it is a no-op.
        CalleeLattice::Bottom => {}
    }
}

/// Given a callee expression, peel functor layers and check whether the base
/// refers to a callable in the `hof_params` map. Returns the `StoreItemId` of
/// the HOF and a reference to its callable-typed parameters.
fn resolve_hof_callee<'a>(
    pkg: &Package,
    callee_expr_id: ExprId,
    hof_params: &'a FxHashMap<StoreItemId, Vec<CallableParam>>,
) -> Option<(StoreItemId, FunctorApp, &'a Vec<CallableParam>)> {
    let (base_id, functor) = peel_body_functors(pkg, callee_expr_id);
    let base_expr = pkg.get_expr(base_id);
    if let ExprKind::Var(Res::Item(item_id), _) = &base_expr.kind {
        let store_id = StoreItemId {
            package: item_id.package,
            item: item_id.item,
        };
        hof_params
            .get(&store_id)
            .map(|params| (store_id, functor, params))
    } else {
        None
    }
}

/// Returns `true` when the HOF's input pattern is a single tuple pattern
/// bound to one name. Used to gate tuple-field locator bookkeeping for HOFs
/// whose arrow parameter is nested inside a single tuple binding.
fn hof_uses_tuple_input_pattern(store: &PackageStore, hof_store_id: StoreItemId) -> bool {
    let hof_pkg = store.get(hof_store_id.package);
    let hof_item = hof_pkg.get_item(hof_store_id.item);
    match &hof_item.kind {
        ItemKind::Callable(decl) => matches!(hof_pkg.get_pat(decl.input).kind, PatKind::Tuple(_)),
        _ => false,
    }
}

/// Extracts the argument expression at the given relative field path from an
/// already-selected outer call argument.
fn extract_arg_at_path(pkg: &Package, args_expr_id: ExprId, path: &[usize]) -> ExprId {
    if path.is_empty() {
        return args_expr_id;
    }
    let args_expr = pkg.get_expr(args_expr_id);
    if let ExprKind::Tuple(elements) = &args_expr.kind {
        if path.len() == 1 {
            elements[path[0]]
        } else {
            extract_arg_at_path(pkg, elements[path[0]], &path[1..])
        }
    } else {
        // Single-parameter callable: the args expression IS the argument.
        args_expr_id
    }
}

/// Resolves a callable argument selected by `path`, following local UDT/tuple
/// initializers when the selected value is nested inside a single argument.
#[allow(clippy::too_many_arguments)]
fn resolve_callee_at_path(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    args_expr_id: ExprId,
    path: &[usize],
    depth: usize,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> CalleeLattice {
    if depth > MAX_RESOLVE_DEPTH {
        return CalleeLattice::Dynamic;
    }

    if path.is_empty() {
        return resolve_callee(
            pkg,
            store,
            locals,
            args_expr_id,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        );
    }

    let args_expr = pkg.get_expr(args_expr_id);
    if let ExprKind::Tuple(elements) = &args_expr.kind
        && let Some(&element_id) = elements.get(path[0])
    {
        return resolve_callee_at_path(
            pkg,
            store,
            locals,
            element_id,
            &path[1..],
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        );
    }

    let field_path = FieldPath {
        indices: path.to_vec(),
    };
    if let Some(field_value_id) = resolve_struct_field(pkg, locals, args_expr_id, &field_path, 0) {
        return resolve_callee(
            pkg,
            store,
            locals,
            field_value_id,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        );
    }

    resolve_callee(
        pkg,
        store,
        locals,
        args_expr_id,
        depth + 1,
        allow_scoped_capture_exprs,
        scoped_capture_vars,
        package_id,
    )
}

/// Resolves an expression to a [`CalleeLattice`] by peeling functor
/// applications, following single-assignment immutable locals, resolving
/// if-value-expressions, and recognising closures and global item references.
#[allow(
    clippy::only_used_in_recursion,
    clippy::too_many_lines,
    clippy::too_many_arguments
)]
fn resolve_callee(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    expr_id: ExprId,
    depth: usize,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> CalleeLattice {
    if depth > MAX_RESOLVE_DEPTH {
        return CalleeLattice::Dynamic;
    }

    let (base_id, outer_functor) = peel_body_functors(pkg, expr_id);
    let base_expr = pkg.get_expr(base_id);

    let base_resolved = match &base_expr.kind {
        ExprKind::Var(Res::Item(item_id), _) => CalleeLattice::Single(ConcreteCallable::Global {
            item_id: *item_id,
            functor: FunctorApp::default(),
        }),
        ExprKind::Closure(captured_vars, target) => {
            let Some(captures) = resolve_captures(pkg, locals, captured_vars, scoped_capture_vars)
            else {
                return CalleeLattice::Dynamic;
            };
            CalleeLattice::Single(ConcreteCallable::Closure {
                target: *target,
                captures,
                functor: FunctorApp::default(),
            })
        }
        ExprKind::Var(Res::Local(var), _) => {
            // Check flow-sensitive callable lattice first.
            if let Some(lattice) = locals.callable.get(var) {
                lattice.clone()
            } else if let Some(&init_expr_id) = locals.exprs.get(var) {
                // Fallback to immutable ExprId bindings (struct fields, etc.).
                resolve_callee(
                    pkg,
                    store,
                    locals,
                    init_expr_id,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            } else {
                CalleeLattice::Dynamic
            }
        }
        ExprKind::Return(inner_expr_id) => resolve_callee(
            pkg,
            store,
            locals,
            *inner_expr_id,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        ),
        ExprKind::Call(callee_expr_id, args_expr_id) => {
            let callee_lattice = resolve_callee(
                pkg,
                store,
                locals,
                *callee_expr_id,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            );

            match callee_lattice {
                CalleeLattice::Single(ConcreteCallable::Global { item_id, functor })
                    if item_id.package == package_id && functor == FunctorApp::default() =>
                {
                    resolve_same_package_callable_return(
                        pkg,
                        store,
                        locals,
                        item_id,
                        *args_expr_id,
                        &[],
                        depth + 1,
                        allow_scoped_capture_exprs,
                        scoped_capture_vars,
                        package_id,
                    )
                }
                _ => CalleeLattice::Dynamic,
            }
        }
        ExprKind::Index(array_expr_id, index_expr_id) => {
            if let Some(elem_expr_id) = resolve_indexed_array_element(
                pkg,
                locals,
                *array_expr_id,
                *index_expr_id,
                depth + 1,
            ) {
                resolve_callee(
                    pkg,
                    store,
                    locals,
                    elem_expr_id,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            } else if let Some(candidates) = resolve_indexed_callable_candidates(
                pkg,
                store,
                locals,
                *array_expr_id,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            ) {
                CalleeLattice::Multi(
                    candidates
                        .into_iter()
                        .map(|callable| (callable, vec![]))
                        .collect(),
                )
            } else {
                CalleeLattice::Dynamic
            }
        }
        // For a bare callable result, literal-folding `cond` is safe: the
        // selected branch yields a single concrete callable and the
        // unselected branch contributes no further targets that need
        // specialization. The sibling projection arm in
        // `resolve_callee_projection` deliberately does NOT fold, because
        // when the callable is projected out of an aggregate (e.g. a UDT
        // ctor whose args carry closure candidates in both branches),
        // dropping the unselected branch would leave its closure target
        // unregistered for specialization and its `ExprKind::Closure` node
        // could not be neutralized during cleanup, breaking convergence.
        ExprKind::If(cond, body, otherwise) => {
            if let Some(condition_value) = resolve_condition_literal(pkg, locals, *cond, 0) {
                let selected_expr_id = if condition_value {
                    Some(*body)
                } else {
                    *otherwise
                };
                if let Some(selected_expr_id) = selected_expr_id {
                    resolve_callee(
                        pkg,
                        store,
                        locals,
                        selected_expr_id,
                        depth + 1,
                        allow_scoped_capture_exprs,
                        scoped_capture_vars,
                        package_id,
                    )
                } else {
                    CalleeLattice::Dynamic
                }
            } else {
                let true_res = resolve_callee(
                    pkg,
                    store,
                    locals,
                    *body,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                );
                let false_res = if let Some(else_id) = otherwise {
                    resolve_callee(
                        pkg,
                        store,
                        locals,
                        *else_id,
                        depth + 1,
                        allow_scoped_capture_exprs,
                        scoped_capture_vars,
                        package_id,
                    )
                } else {
                    CalleeLattice::Dynamic
                };
                true_res.join_with_condition(false_res, remap_condition_expr(pkg, locals, *cond))
            }
        }
        ExprKind::Block(block_id) => {
            let block = pkg.get_block(*block_id);
            let mut block_state = LocalState {
                callable: locals.callable.clone(),
                exprs: locals.exprs.clone(),
                condition_substitutions: locals.condition_substitutions.clone(),
            };
            analyze_block_flow(pkg, store, *block_id, &mut block_state, package_id, None);
            let block_scoped_vars = if allow_scoped_capture_exprs {
                let mut vars = scoped_capture_vars.clone();
                collect_block_local_bindings(pkg, *block_id, &mut vars);
                vars
            } else {
                scoped_capture_vars.clone()
            };
            if let Some(&last_stmt_id) = block.stmts.last() {
                let stmt = pkg.get_stmt(last_stmt_id);
                match &stmt.kind {
                    StmtKind::Expr(e) | StmtKind::Semi(e) => resolve_callee(
                        pkg,
                        store,
                        &block_state,
                        *e,
                        depth + 1,
                        allow_scoped_capture_exprs,
                        &block_scoped_vars,
                        package_id,
                    ),
                    _ => CalleeLattice::Dynamic,
                }
            } else {
                CalleeLattice::Dynamic
            }
        }
        ExprKind::Field(inner_expr_id, Field::Path(path)) => {
            if let Some(field_value_id) =
                resolve_struct_field(pkg, locals, *inner_expr_id, path, depth + 1)
            {
                resolve_callee(
                    pkg,
                    store,
                    locals,
                    field_value_id,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            } else {
                resolve_callee_projection(
                    pkg,
                    store,
                    locals,
                    *inner_expr_id,
                    &path.indices,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            }
        }
        _ => CalleeLattice::Dynamic,
    };

    // Compose the outer functor with the base's functor.
    apply_outer_functor_lattice(base_resolved, outer_functor)
}

/// Resolves a callable nested at `path` inside an aggregate expression.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn resolve_callee_projection(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    expr_id: ExprId,
    path: &[usize],
    depth: usize,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> CalleeLattice {
    if depth > MAX_RESOLVE_DEPTH {
        return CalleeLattice::Dynamic;
    }

    if path.is_empty() {
        return resolve_callee(
            pkg,
            store,
            locals,
            expr_id,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        );
    }

    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Tuple(elements) => {
            let Some((&field_index, rest)) = path.split_first() else {
                return CalleeLattice::Dynamic;
            };
            let Some(&field_expr_id) = elements.get(field_index) else {
                return CalleeLattice::Dynamic;
            };
            resolve_callee_projection(
                pkg,
                store,
                locals,
                field_expr_id,
                rest,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            )
        }
        ExprKind::Var(Res::Local(var), _) => {
            if let Some(&init_expr_id) = locals.exprs.get(var) {
                resolve_callee_projection(
                    pkg,
                    store,
                    locals,
                    init_expr_id,
                    path,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            } else {
                CalleeLattice::Dynamic
            }
        }
        ExprKind::Return(inner_expr_id) | ExprKind::UnOp(UnOp::Unwrap, inner_expr_id) => {
            resolve_callee_projection(
                pkg,
                store,
                locals,
                *inner_expr_id,
                path,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            )
        }
        ExprKind::Block(block_id) => {
            let block = pkg.get_block(*block_id);
            let mut block_state = LocalState {
                callable: locals.callable.clone(),
                exprs: locals.exprs.clone(),
                condition_substitutions: locals.condition_substitutions.clone(),
            };
            analyze_block_flow(pkg, store, *block_id, &mut block_state, package_id, None);
            let block_scoped_vars = if allow_scoped_capture_exprs {
                let mut vars = scoped_capture_vars.clone();
                collect_block_local_bindings(pkg, *block_id, &mut vars);
                vars
            } else {
                scoped_capture_vars.clone()
            };
            if let Some(&last_stmt_id) = block.stmts.last() {
                let stmt = pkg.get_stmt(last_stmt_id);
                match &stmt.kind {
                    StmtKind::Expr(e) | StmtKind::Semi(e) => resolve_callee_projection(
                        pkg,
                        store,
                        &block_state,
                        *e,
                        path,
                        depth + 1,
                        allow_scoped_capture_exprs,
                        &block_scoped_vars,
                        package_id,
                    ),
                    _ => CalleeLattice::Dynamic,
                }
            } else {
                CalleeLattice::Dynamic
            }
        }
        ExprKind::If(cond, body, otherwise) => {
            // Unlike `resolve_callee`'s If arm at the bare-callable site, we
            // deliberately do NOT literal-fold `cond` here. When projecting a
            // callable out of an aggregate returned from a same-package
            // callable (e.g. a UDT ctor `Call` whose args carry two closure
            // candidates), short-circuiting to one branch would leave the
            // other branch's closure target unregistered for specialization;
            // `cleanup_consumed_closures` would then be unable to neutralize
            // the surviving `ExprKind::Closure` node and convergence would
            // fail. The join below produces a `CalleeLattice::Multi`
            // that `branch_split_direct_call_rewrite` materializes as a
            // constant-conditioned dispatch in the caller.
            let true_res = resolve_callee_projection(
                pkg,
                store,
                locals,
                *body,
                path,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            );
            let false_res = if let Some(else_id) = otherwise {
                resolve_callee_projection(
                    pkg,
                    store,
                    locals,
                    *else_id,
                    path,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            } else {
                CalleeLattice::Dynamic
            };
            true_res.join_with_condition(false_res, remap_condition_expr(pkg, locals, *cond))
        }
        ExprKind::Call(callee_expr_id, args_expr_id) => {
            let callee_lattice = resolve_callee(
                pkg,
                store,
                locals,
                *callee_expr_id,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            );

            match callee_lattice {
                CalleeLattice::Single(ConcreteCallable::Global { item_id, functor })
                    if item_id.package == package_id && functor == FunctorApp::default() =>
                {
                    let target_item = pkg.get_item(item_id.item);
                    match &target_item.kind {
                        ItemKind::Callable(_) => resolve_same_package_callable_return(
                            pkg,
                            store,
                            locals,
                            item_id,
                            *args_expr_id,
                            path,
                            depth + 1,
                            allow_scoped_capture_exprs,
                            scoped_capture_vars,
                            package_id,
                        ),
                        ItemKind::Ty(_, _) => resolve_callee_projection(
                            pkg,
                            store,
                            locals,
                            *args_expr_id,
                            path,
                            depth + 1,
                            allow_scoped_capture_exprs,
                            scoped_capture_vars,
                            package_id,
                        ),
                        _ => CalleeLattice::Dynamic,
                    }
                }
                _ => CalleeLattice::Dynamic,
            }
        }
        ExprKind::Struct(_, _, fields) => {
            let Some((&field_index, rest)) = path.split_first() else {
                return CalleeLattice::Dynamic;
            };
            let mut found: Option<ExprId> = None;
            for fa in fields {
                if let Field::Path(fa_path) = &fa.field
                    && fa_path.indices.first() == Some(&field_index)
                {
                    found = Some(fa.value);
                    break;
                }
            }
            if let Some(field_expr_id) = found {
                resolve_callee_projection(
                    pkg,
                    store,
                    locals,
                    field_expr_id,
                    rest,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            } else {
                CalleeLattice::Dynamic
            }
        }
        ExprKind::Field(inner_expr_id, Field::Path(field_path)) => {
            let mut composed: Vec<usize> = field_path.indices.clone();
            composed.extend_from_slice(path);
            resolve_callee_projection(
                pkg,
                store,
                locals,
                *inner_expr_id,
                &composed,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            )
        }
        _ => CalleeLattice::Dynamic,
    }
}

fn output_path_resolves_to_arrow(store: &PackageStore, ty: &Ty, path: &[usize]) -> bool {
    match ty {
        Ty::Arrow(_) => path.is_empty(),
        Ty::Tuple(items) => {
            let Some((&field_index, rest)) = path.split_first() else {
                return false;
            };
            items
                .get(field_index)
                .is_some_and(|item_ty| output_path_resolves_to_arrow(store, item_ty, rest))
        }
        Ty::Udt(Res::Item(item_id)) => {
            let package = store.get(item_id.package);
            let item = package.get_item(item_id.item);
            let ItemKind::Ty(_, udt) = &item.kind else {
                return false;
            };
            output_path_resolves_to_arrow(store, &udt.get_pure_ty(), path)
        }
        _ => false,
    }
}

/// Attempts to resolve a callable-returning call whose target lives in the
/// same package by treating the target body as a straight-line function,
/// binding its parameters to the call's argument expressions and tracing
/// the result back to a concrete callable.
#[allow(clippy::too_many_arguments)]
fn resolve_same_package_callable_return(
    pkg: &Package,
    store: &PackageStore,
    caller_locals: &LocalState,
    item_id: ItemId,
    args_expr_id: ExprId,
    output_path: &[usize],
    depth: usize,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> CalleeLattice {
    let item = pkg.get_item(item_id.item);
    let ItemKind::Callable(decl) = &item.kind else {
        return CalleeLattice::Dynamic;
    };

    if !output_path_resolves_to_arrow(store, &decl.output, output_path) {
        return CalleeLattice::Dynamic;
    }

    let (body_block_id, body_input) = match &decl.implementation {
        CallableImpl::Spec(spec_impl) => (
            spec_impl.body.block,
            spec_impl.body.input.unwrap_or(decl.input),
        ),
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            (spec_decl.block, spec_decl.input.unwrap_or(decl.input))
        }
        CallableImpl::Intrinsic => return CalleeLattice::Dynamic,
    };

    let mut state = LocalState {
        callable: FxHashMap::default(),
        exprs: FxHashMap::default(),
        condition_substitutions: FxHashMap::default(),
    };
    seed_param_bindings_from_call(
        pkg,
        store,
        caller_locals,
        &mut state,
        body_input,
        args_expr_id,
        package_id,
    );
    // Snapshot the parameter -> caller-argument expression map immediately
    // after seeding and before the body is analyzed. The body's own local
    // bindings can collide with caller-scope `LocalVarId`s, which would make
    // a transitive walk over the merged `state.exprs` ambiguous. This clean
    // snapshot lets capture resolution stop at a producing-function parameter
    // and substitute the caller-scope argument bound to it.
    let param_substitutions: FxHashMap<LocalVarId, ExprId> = state.exprs.clone();
    analyze_block_flow(pkg, store, body_block_id, &mut state, package_id, None);

    let block = pkg.get_block(body_block_id);
    let Some(&stmt_id) = block.stmts.last() else {
        return CalleeLattice::Dynamic;
    };
    let stmt = pkg.get_stmt(stmt_id);
    let return_expr_id = match &stmt.kind {
        StmtKind::Expr(return_expr_id) => *return_expr_id,
        StmtKind::Semi(expr_id) if matches!(pkg.get_expr(*expr_id).kind, ExprKind::Return(_)) => {
            let ExprKind::Return(inner_expr_id) = pkg.get_expr(*expr_id).kind else {
                unreachable!("guarded above")
            };
            inner_expr_id
        }
        _ => return CalleeLattice::Dynamic,
    };

    materialize_capture_exprs_from_state(
        pkg,
        &state,
        &param_substitutions,
        resolve_callee_projection(
            pkg,
            store,
            &state,
            return_expr_id,
            output_path,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        ),
    )
}

fn resolve_condition_literal(
    pkg: &Package,
    locals: &LocalState,
    expr_id: ExprId,
    depth: usize,
) -> Option<bool> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(var), _) => {
            locals
                .condition_substitutions
                .get(var)
                .and_then(|&expr_id| {
                    resolve_condition_substitution_literal(pkg, locals, expr_id, depth + 1)
                })
        }
        _ => None,
    }
}

fn resolve_condition_substitution_literal(
    pkg: &Package,
    locals: &LocalState,
    expr_id: ExprId,
    depth: usize,
) -> Option<bool> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Lit(Lit::Bool(value)) => Some(*value),
        ExprKind::Var(Res::Local(var), _) => locals
            .condition_substitutions
            .get(var)
            .or_else(|| locals.exprs.get(var))
            .and_then(|&expr_id| {
                resolve_condition_substitution_literal(pkg, locals, expr_id, depth + 1)
            }),
        _ => None,
    }
}

fn remap_condition_expr(pkg: &Package, locals: &LocalState, expr_id: ExprId) -> ExprId {
    let expr = pkg.get_expr(expr_id);
    if let ExprKind::Var(Res::Local(var), _) = &expr.kind
        && let Some(&replacement_expr_id) = locals.condition_substitutions.get(var)
    {
        replacement_expr_id
    } else {
        expr_id
    }
}

/// Materializes `CapturedVar::expr` fields for each capture appearing in a
/// `CalleeLattice` by resolving the capture's defining expression in the
/// callee's analyzed `LocalState`, substituting producing-function parameters
/// with the caller-scope argument expressions in `param_substitutions`, so
/// rewrite can re-emit the captures as caller-scope arguments.
fn materialize_capture_exprs_from_state(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    resolved: CalleeLattice,
) -> CalleeLattice {
    match resolved {
        CalleeLattice::Single(concrete) => CalleeLattice::Single(
            materialize_capture_exprs_in_callable(pkg, state, param_substitutions, concrete),
        ),
        CalleeLattice::Multi(entries) => CalleeLattice::Multi(
            entries
                .into_iter()
                .map(|(concrete, condition)| {
                    (
                        materialize_capture_exprs_in_callable(
                            pkg,
                            state,
                            param_substitutions,
                            concrete,
                        ),
                        condition,
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

/// Resolves each closure capture to a caller-scope expression. For a
/// partial-application closure returned across a function boundary, the
/// capture references a producing-function parameter; this walks the callee
/// `state` from the capture var, stops at the first producing-function
/// parameter, and substitutes the caller-scope argument bound to it.
fn materialize_capture_exprs_in_callable(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    concrete: ConcreteCallable,
) -> ConcreteCallable {
    match concrete {
        ConcreteCallable::Closure {
            target,
            mut captures,
            functor,
        } => {
            for capture in &mut captures {
                if let Some(expr) =
                    resolve_capture_to_caller(pkg, state, param_substitutions, capture.var)
                {
                    capture.expr = Some(expr);
                }
            }

            ConcreteCallable::Closure {
                target,
                captures,
                functor,
            }
        }
        other => other,
    }
}

/// Resolves a closure capture variable to a caller-scope expression.
///
/// Walks `Var(Local)` indirection through the callee's analyzed `state`
/// starting from `var`. When the walk reaches a producing-function parameter
/// it returns the caller-scope argument expression bound to that parameter at
/// the call site. Checking `param_substitutions` first is essential: the
/// callee body's local bindings can collide with caller-scope `LocalVarId`s,
/// so following the merged `state.exprs` past a parameter would misinterpret
/// a caller-scope id as a callee-scope one. Returns the terminal expression
/// when the walk ends at a non-variable, or `None` when nothing is resolvable.
fn resolve_capture_to_caller(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    var: LocalVarId,
) -> Option<ExprId> {
    let mut current = var;
    for _ in 0..MAX_RESOLVE_DEPTH {
        if let Some(&arg_expr_id) = param_substitutions.get(&current) {
            return Some(arg_expr_id);
        }
        let &expr_id = state.exprs.get(&current)?;
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Var(Res::Local(next), _) = &expr.kind
            && *next != current
        {
            current = *next;
            continue;
        }
        return Some(expr_id);
    }
    None
}

/// Seeds the callable-flow lattice for a HOF with the concrete callables
/// bound to its arrow parameters at a specific call site, enabling
/// reaching-def analysis to track parameter-forwarding chains.
fn seed_param_bindings_from_call(
    pkg: &Package,
    store: &PackageStore,
    caller_locals: &LocalState,
    state: &mut LocalState,
    pat_id: PatId,
    arg_expr_id: ExprId,
    package_id: PackageId,
) {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            state.exprs.insert(ident.id, arg_expr_id);
            state.condition_substitutions.insert(ident.id, arg_expr_id);
            if matches!(pat.ty, Ty::Arrow(_)) {
                let lattice = resolve_callee(
                    pkg,
                    store,
                    caller_locals,
                    arg_expr_id,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                );
                state.callable.insert(ident.id, lattice);
            }
        }
        PatKind::Tuple(sub_pats) => {
            let arg_expr = pkg.get_expr(arg_expr_id);
            if let ExprKind::Tuple(arg_elems) = &arg_expr.kind
                && sub_pats.len() == arg_elems.len()
            {
                for (&sub_pat_id, &arg_elem_id) in sub_pats.iter().zip(arg_elems.iter()) {
                    seed_param_bindings_from_call(
                        pkg,
                        store,
                        caller_locals,
                        state,
                        sub_pat_id,
                        arg_elem_id,
                        package_id,
                    );
                }
            }
        }
        PatKind::Discard => {}
    }
}

/// Applies an outer functor application to a resolved callable.
fn apply_outer_functor_cc(resolved: ConcreteCallable, outer: FunctorApp) -> ConcreteCallable {
    match resolved {
        ConcreteCallable::Global { item_id, functor } => ConcreteCallable::Global {
            item_id,
            functor: compose_functors(&outer, &functor),
        },
        ConcreteCallable::Closure {
            target,
            captures,
            functor,
        } => ConcreteCallable::Closure {
            target,
            captures,
            functor: compose_functors(&outer, &functor),
        },
        ConcreteCallable::Dynamic => ConcreteCallable::Dynamic,
    }
}

/// Applies an outer functor application to all entries in a lattice element.
fn apply_outer_functor_lattice(resolved: CalleeLattice, outer: FunctorApp) -> CalleeLattice {
    if outer == FunctorApp::default() {
        return resolved;
    }
    match resolved {
        CalleeLattice::Single(cc) => CalleeLattice::Single(apply_outer_functor_cc(cc, outer)),
        CalleeLattice::Multi(entries) => CalleeLattice::Multi(
            entries
                .into_iter()
                .map(|(cc, cond)| (apply_outer_functor_cc(cc, outer), cond))
                .collect(),
        ),
        other => other,
    }
}

/// Resolves a field access expression to the initialiser `ExprId` of that
/// field within a struct construction. Traces through immutable locals and
/// nested field accesses to locate the struct construction site.
fn resolve_struct_field(
    pkg: &Package,
    locals: &LocalState,
    inner_expr_id: ExprId,
    path: &FieldPath,
    depth: usize,
) -> Option<ExprId> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }
    let inner_expr = pkg.get_expr(inner_expr_id);
    match &inner_expr.kind {
        ExprKind::Tuple(elements) => {
            let (&field_index, rest) = path.indices.split_first()?;
            let &field_expr_id = elements.get(field_index)?;
            if rest.is_empty() {
                Some(field_expr_id)
            } else {
                resolve_struct_field(
                    pkg,
                    locals,
                    field_expr_id,
                    &FieldPath {
                        indices: rest.to_vec(),
                    },
                    depth + 1,
                )
            }
        }
        ExprKind::Struct(_, _, fields) => extract_field_value(fields, path),
        ExprKind::Call(_, args_id) => resolve_struct_field(pkg, locals, *args_id, path, depth + 1),
        ExprKind::Var(Res::Local(var), _) => {
            let &init_id = locals.exprs.get(var)?;
            resolve_struct_field(pkg, locals, init_id, path, depth + 1)
        }
        ExprKind::Field(nested_inner_id, Field::Path(nested_path)) => {
            // Two-level field access: resolve the outer field to get the inner
            // struct expression, then resolve the target field within that.
            let intermediate_id =
                resolve_struct_field(pkg, locals, *nested_inner_id, nested_path, depth + 1)?;
            resolve_struct_field(pkg, locals, intermediate_id, path, depth + 1)
        }
        _ => None,
    }
}

/// Resolves a single `Index(array, index)` expression to the concrete
/// callable at the indexed position when both the array and index are
/// statically known.
fn resolve_indexed_array_element(
    pkg: &Package,
    locals: &LocalState,
    array_expr_id: ExprId,
    index_expr_id: ExprId,
    depth: usize,
) -> Option<ExprId> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let index = usize::try_from(resolve_static_int_expr(
        pkg,
        locals,
        index_expr_id,
        depth + 1,
    )?)
    .ok()?;
    resolve_array_element_at_index(pkg, locals, array_expr_id, index, depth + 1)
}

/// Resolves an `Index(array, index)` where the array is known but the
/// index may vary, returning a `CalleeLattice` of all statically possible
/// callables keyed against each index value.
#[allow(clippy::too_many_arguments)]
fn resolve_indexed_callable_candidates(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    array_expr_id: ExprId,
    depth: usize,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> Option<Vec<ConcreteCallable>> {
    let element_expr_ids = resolve_array_elements(pkg, locals, array_expr_id, depth + 1)?;
    let mut candidates = Vec::new();

    for elem_expr_id in element_expr_ids {
        let resolved = resolve_callee(
            pkg,
            store,
            locals,
            elem_expr_id,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        );

        match resolved {
            CalleeLattice::Single(callable) => {
                if !candidates.contains(&callable) {
                    candidates.push(callable);
                }
            }
            CalleeLattice::Multi(entries) => {
                for (callable, condition) in entries {
                    if !condition.is_empty() {
                        return None;
                    }
                    if !candidates.contains(&callable) {
                        candidates.push(callable);
                    }
                }
            }
            CalleeLattice::Bottom | CalleeLattice::Dynamic => return None,
        }

        if candidates.len() > super::types::MULTI_CAP {
            return None;
        }
    }

    (!candidates.is_empty()).then_some(candidates)
}

/// Resolves an array-literal expression to the concrete callables stored in
/// each element slot, yielding `None` when any element is not statically
/// known.
fn resolve_array_elements(
    pkg: &Package,
    locals: &LocalState,
    expr_id: ExprId,
    depth: usize,
) -> Option<Vec<ExprId>> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Array(elements) | ExprKind::ArrayLit(elements) | ExprKind::Tuple(elements) => {
            Some(elements.clone())
        }
        ExprKind::Var(Res::Local(var), _) => locals
            .exprs
            .get(var)
            .and_then(|&init_expr_id| resolve_array_elements(pkg, locals, init_expr_id, depth + 1)),
        ExprKind::Block(block_id) => {
            let block = pkg.get_block(*block_id);
            let stmt_id = *block.stmts.last()?;
            let stmt = pkg.get_stmt(stmt_id);
            let tail_expr_id = match &stmt.kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                _ => return None,
            };
            resolve_array_elements(pkg, locals, tail_expr_id, depth + 1)
        }
        ExprKind::Return(inner_expr_id) => {
            resolve_array_elements(pkg, locals, *inner_expr_id, depth + 1)
        }
        ExprKind::Field(inner_expr_id, Field::Path(path)) => {
            let field_value_id =
                resolve_struct_field(pkg, locals, *inner_expr_id, path, depth + 1)?;
            resolve_array_elements(pkg, locals, field_value_id, depth + 1)
        }
        _ => None,
    }
}

/// Resolves the element at a specific static index within an array-literal
/// expression (after [`resolve_array_elements`] has resolved each slot).
fn resolve_array_element_at_index(
    pkg: &Package,
    locals: &LocalState,
    expr_id: ExprId,
    index: usize,
    depth: usize,
) -> Option<ExprId> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Array(elements) | ExprKind::ArrayLit(elements) | ExprKind::Tuple(elements) => {
            elements.get(index).copied()
        }
        ExprKind::Var(Res::Local(var), _) => locals.exprs.get(var).and_then(|&init_expr_id| {
            resolve_array_element_at_index(pkg, locals, init_expr_id, index, depth + 1)
        }),
        ExprKind::Block(block_id) => {
            let block = pkg.get_block(*block_id);
            let stmt_id = *block.stmts.last()?;
            let stmt = pkg.get_stmt(stmt_id);
            let tail_expr_id = match &stmt.kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                _ => return None,
            };
            resolve_array_element_at_index(pkg, locals, tail_expr_id, index, depth + 1)
        }
        ExprKind::Return(inner_expr_id) => {
            resolve_array_element_at_index(pkg, locals, *inner_expr_id, index, depth + 1)
        }
        ExprKind::Field(inner_expr_id, Field::Path(path)) => {
            let field_value_id =
                resolve_struct_field(pkg, locals, *inner_expr_id, path, depth + 1)?;
            resolve_array_element_at_index(pkg, locals, field_value_id, index, depth + 1)
        }
        _ => None,
    }
}

/// Attempts to reduce an expression to a compile-time integer value so that
/// indexed lookups can locate their source element statically.
fn resolve_static_int_expr(
    pkg: &Package,
    locals: &LocalState,
    expr_id: ExprId,
    depth: usize,
) -> Option<i64> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Lit(Lit::Int(value)) => Some(*value),
        ExprKind::Var(Res::Local(var), _) => locals.exprs.get(var).and_then(|&init_expr_id| {
            resolve_static_int_expr(pkg, locals, init_expr_id, depth + 1)
        }),
        ExprKind::Block(block_id) => {
            let block = pkg.get_block(*block_id);
            let stmt_id = *block.stmts.last()?;
            let stmt = pkg.get_stmt(stmt_id);
            let tail_expr_id = match &stmt.kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                _ => return None,
            };
            resolve_static_int_expr(pkg, locals, tail_expr_id, depth + 1)
        }
        ExprKind::Return(inner_expr_id) => {
            resolve_static_int_expr(pkg, locals, *inner_expr_id, depth + 1)
        }
        ExprKind::UnOp(UnOp::Neg, inner_expr_id) => {
            resolve_static_int_expr(pkg, locals, *inner_expr_id, depth + 1).map(std::ops::Neg::neg)
        }
        _ => None,
    }
}

/// Extracts the value `ExprId` for a field from a struct construction's field
/// assignments by matching on the first index of the access path.
fn extract_field_value(fields: &[FieldAssign], path: &FieldPath) -> Option<ExprId> {
    let target_index = path.indices.first()?;
    for fa in fields {
        if let Field::Path(fa_path) = &fa.field
            && fa_path.indices.first() == Some(target_index)
        {
            return Some(fa.value);
        }
    }
    None
}

/// Resolves the types of captured variables in a closure expression.
pub(super) fn resolve_captures(
    pkg: &Package,
    locals: &LocalState,
    captured_vars: &[LocalVarId],
    scoped_capture_vars: &FxHashSet<LocalVarId>,
) -> Option<Vec<CapturedVar>> {
    captured_vars
        .iter()
        .map(|&var| {
            let ty = find_local_var_type(pkg, locals, var)?;
            let expr = resolve_scoped_capture_expr(pkg, locals, var, scoped_capture_vars);
            Some(CapturedVar { var, ty, expr })
        })
        .collect()
}

/// Resolves a capture expression by walking the enclosing block scope and
/// its visible local bindings, used when the straightforward
/// [`resolve_capture_expr_from_state`] lookup cannot see the binding.
fn resolve_scoped_capture_expr(
    pkg: &Package,
    locals: &LocalState,
    var: LocalVarId,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
) -> Option<ExprId> {
    if !scoped_capture_vars.contains(&var) {
        return None;
    }

    let mut current = var;
    for _ in 0..MAX_RESOLVE_DEPTH {
        let &expr_id = locals.exprs.get(&current)?;
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Var(Res::Local(next_var), _) = &expr.kind
            && *next_var != current
            && scoped_capture_vars.contains(next_var)
        {
            current = *next_var;
            continue;
        }

        return Some(expr_id);
    }

    None
}

/// Collects all local variables bound within a block (recursively through
/// statements and nested blocks) into `bound`, used to scope capture
/// resolution.
fn collect_block_local_bindings(
    pkg: &Package,
    block_id: BlockId,
    bound: &mut FxHashSet<LocalVarId>,
) {
    let block = pkg.get_block(block_id);
    for stmt_id in &block.stmts {
        let stmt = pkg.get_stmt(*stmt_id);
        if let StmtKind::Local(_, pat_id, _) = stmt.kind {
            collect_pat_local_bindings(pkg, pat_id, bound);
        }
    }
}

/// Collects every local-variable binding introduced by a pattern into
/// `bound`, recursing into tuple patterns.
fn collect_pat_local_bindings(pkg: &Package, pat_id: PatId, bound: &mut FxHashSet<LocalVarId>) {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            bound.insert(ident.id);
        }
        PatKind::Discard => {}
        PatKind::Tuple(pats) => {
            for &sub_pat_id in pats {
                collect_pat_local_bindings(pkg, sub_pat_id, bound);
            }
        }
    }
}

/// Finds the type of a local variable by looking up its initialiser expression.
/// Falls back to a full pattern scan when the variable is not in the
/// immutable-locals map (e.g. function parameters or outer-scope bindings).
fn find_local_var_type(pkg: &Package, locals: &LocalState, var: LocalVarId) -> Option<Ty> {
    if let Some(&init_expr_id) = locals.exprs.get(&var) {
        Some(pkg.get_expr(init_expr_id).ty.clone())
    } else {
        // The variable may be a function parameter or from an outer scope not
        // tracked in the immutable-locals map. Scan all patterns as a fallback.
        find_var_type_in_pats(pkg, var)
    }
}

/// Scans all patterns in a package to find the type of a given `LocalVarId`.
///
/// Returns `None` if no binding pattern is found. Valid FIR gives every
/// `LocalVarId` a corresponding binding pattern, but returning `None` lets
/// callers degrade analysis for malformed or partially transformed input
/// instead of panicking.
fn find_var_type_in_pats(pkg: &Package, var: LocalVarId) -> Option<Ty> {
    for pat in pkg.pats.values() {
        if let PatKind::Bind(ident) = &pat.kind
            && ident.id == var
        {
            return Some(pat.ty.clone());
        }
    }
    None
}

/// Builds flow-sensitive local variable state by performing a single forward
/// pass over the callable's body.
///
/// For callable-typed locals, the analysis tracks reaching definitions through
/// `set` assignments, forks state at `if`/`else` branches, and conservatively
/// marks mutable callable vars assigned inside `while` loops as `Dynamic`.
///
/// For all immutable locals, the raw `ExprId` binding is also recorded for
/// struct field resolution and type look-ups.
fn build_callable_flow_state(
    pkg: &Package,
    store: &PackageStore,
    callable_impl: &CallableImpl,
    package_id: PackageId,
    recorder: Option<&mut CallRecorder>,
) -> LocalState {
    let mut state = LocalState {
        callable: FxHashMap::default(),
        exprs: FxHashMap::default(),
        condition_substitutions: FxHashMap::default(),
    };
    match callable_impl {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            analyze_spec_flow(pkg, store, spec_impl, &mut state, package_id, recorder);
        }
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            analyze_block_flow(
                pkg,
                store,
                spec_decl.block,
                &mut state,
                package_id,
                recorder,
            );
        }
    }
    state
}

/// Runs callable-flow analysis over a single `SpecImpl`, merging the
/// resulting per-variable lattice with the caller-provided accumulator.
fn analyze_spec_flow(
    pkg: &Package,
    store: &PackageStore,
    spec_impl: &SpecImpl,
    state: &mut LocalState,
    package_id: PackageId,
    mut recorder: Option<&mut CallRecorder>,
) {
    analyze_block_flow(
        pkg,
        store,
        spec_impl.body.block,
        state,
        package_id,
        recorder.as_deref_mut(),
    );
    for spec in functored_specs(spec_impl) {
        analyze_block_flow(
            pkg,
            store,
            spec.block,
            state,
            package_id,
            recorder.as_deref_mut(),
        );
    }
}

/// Walks a block's statements, propagating callable-flow lattice updates
/// top-down so conditional joins preserve per-branch condition tags.
fn analyze_block_flow(
    pkg: &Package,
    store: &PackageStore,
    block_id: BlockId,
    state: &mut LocalState,
    package_id: PackageId,
    mut recorder: Option<&mut CallRecorder>,
) {
    let block = pkg.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = pkg.get_stmt(stmt_id);
        analyze_stmt_flow(
            pkg,
            store,
            &stmt.kind,
            state,
            package_id,
            recorder.as_deref_mut(),
        );
    }
}

/// Updates the callable-flow lattice for a single statement (local
/// bindings, assignments, and expression statements) before recursing into
/// nested blocks.
fn analyze_stmt_flow(
    pkg: &Package,
    store: &PackageStore,
    kind: &StmtKind,
    state: &mut LocalState,
    package_id: PackageId,
    recorder: Option<&mut CallRecorder>,
) {
    match kind {
        StmtKind::Local(Mutability::Immutable, pat_id, init_expr_id) => {
            // Record ExprId bindings for all immutable locals.
            collect_bindings_from_pat(pkg, *pat_id, *init_expr_id, &mut state.exprs);
            // For callable-typed bindings, resolve and store in lattice.
            bind_callable_pat(pkg, store, state, *pat_id, *init_expr_id, package_id);
            analyze_expr_flow(pkg, store, *init_expr_id, state, package_id, recorder);
        }
        StmtKind::Local(Mutability::Mutable, pat_id, init_expr_id) => {
            bind_callable_pat(pkg, store, state, *pat_id, *init_expr_id, package_id);
            analyze_expr_flow(pkg, store, *init_expr_id, state, package_id, recorder);
        }
        StmtKind::Expr(e) | StmtKind::Semi(e) => {
            analyze_expr_flow(pkg, store, *e, state, package_id, recorder);
        }
        StmtKind::Item(_) => {}
    }
}

/// Binds callable-typed variables from a pattern to their resolved
/// `CalleeLattice` values.
fn bind_callable_pat(
    pkg: &Package,
    store: &PackageStore,
    state: &mut LocalState,
    pat_id: qsc_fir::fir::PatId,
    init_expr_id: ExprId,
    package_id: PackageId,
) {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            if matches!(pat.ty, Ty::Arrow(_)) {
                let lattice = resolve_callee(
                    pkg,
                    store,
                    state,
                    init_expr_id,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                );
                state.callable.insert(ident.id, lattice);
            }
        }
        PatKind::Tuple(sub_pats) => {
            let init_expr = pkg.get_expr(init_expr_id);
            if let ExprKind::Tuple(init_elems) = &init_expr.kind
                && sub_pats.len() == init_elems.len()
            {
                for (&sub_pat_id, &elem_expr_id) in sub_pats.iter().zip(init_elems.iter()) {
                    bind_callable_pat(pkg, store, state, sub_pat_id, elem_expr_id, package_id);
                }
            } else {
                // Non-tuple init (e.g., ExprKind::Index from for-loop desugaring).
                // Resolve the init through variable indirection first.
                let resolved_init_id = resolve_through_vars(pkg, state, init_expr_id);
                let resolved_init = pkg.get_expr(resolved_init_id);

                if let ExprKind::Tuple(init_elems) = &resolved_init.kind
                    && sub_pats.len() == init_elems.len()
                {
                    // Resolved to a literal tuple — recurse element-wise.
                    for (&sub_pat_id, &elem_expr_id) in sub_pats.iter().zip(init_elems.iter()) {
                        bind_callable_pat(pkg, store, state, sub_pat_id, elem_expr_id, package_id);
                    }
                } else if let ExprKind::Index(array_expr_id, _) = &resolved_init.kind {
                    // Dynamic array index: resolve all array elements and extract
                    // per-field callables for each arrow-typed sub-pattern.
                    bind_callable_pats_from_indexed_array(
                        pkg,
                        store,
                        state,
                        sub_pats,
                        *array_expr_id,
                        package_id,
                    );
                } else {
                    let mut path = Vec::new();
                    bind_callable_pat_projections(
                        pkg,
                        store,
                        state,
                        pat_id,
                        init_expr_id,
                        &mut path,
                        package_id,
                    );
                }
            }
        }
        PatKind::Discard => {}
    }
}

fn bind_callable_pat_projections(
    pkg: &Package,
    store: &PackageStore,
    state: &mut LocalState,
    pat_id: PatId,
    init_expr_id: ExprId,
    path: &mut Vec<usize>,
    package_id: PackageId,
) {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            if matches!(pat.ty, Ty::Arrow(_)) {
                let lattice = resolve_callee_projection(
                    pkg,
                    store,
                    state,
                    init_expr_id,
                    path,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                );
                if !matches!(lattice, CalleeLattice::Bottom | CalleeLattice::Dynamic) {
                    state.callable.insert(ident.id, lattice);
                }
            }
        }
        PatKind::Tuple(sub_pats) => {
            for (index, &sub_pat_id) in sub_pats.iter().enumerate() {
                path.push(index);
                bind_callable_pat_projections(
                    pkg,
                    store,
                    state,
                    sub_pat_id,
                    init_expr_id,
                    path,
                    package_id,
                );
                path.pop();
            }
        }
        PatKind::Discard => {}
    }
}

/// Follows `ExprKind::Var(Res::Local(var))` through `state.exprs` to find
/// the underlying expression, stopping when no further indirection exists.
fn resolve_through_vars(pkg: &Package, state: &LocalState, expr_id: ExprId) -> ExprId {
    let expr = pkg.get_expr(expr_id);
    if let ExprKind::Var(Res::Local(var), _) = &expr.kind
        && let Some(&init_id) = state.exprs.get(var)
    {
        return resolve_through_vars(pkg, state, init_id);
    }
    expr_id
}

/// Binds callable-typed sub-patterns from a tuple pattern where the init
/// expression is `array[dynamic_index]`. Resolves all array elements,
/// extracts the field at each sub-pattern position, and joins the resolved
/// callables into a `CalleeLattice`.
fn bind_callable_pats_from_indexed_array(
    pkg: &Package,
    store: &PackageStore,
    state: &mut LocalState,
    sub_pats: &[PatId],
    array_expr_id: ExprId,
    package_id: PackageId,
) {
    // Resolve the array to its element ExprIds.
    let Some(array_elem_ids) = resolve_array_elements(pkg, state, array_expr_id, 0) else {
        return; // Cannot resolve array — leave sub-patterns unbound (conservative).
    };

    for (field_idx, &sub_pat_id) in sub_pats.iter().enumerate() {
        let sub_pat = pkg.get_pat(sub_pat_id);
        let PatKind::Bind(ident) = &sub_pat.kind else {
            continue; // Skip Discard and nested Tuple for now.
        };
        if !matches!(sub_pat.ty, Ty::Arrow(_)) {
            continue; // Only bind arrow-typed locals.
        }

        // Collect the callable at field_idx from each array element tuple.
        let mut lattice = CalleeLattice::Bottom;
        for &elem_expr_id in &array_elem_ids {
            let elem_expr = pkg.get_expr(elem_expr_id);
            if let ExprKind::Tuple(fields) = &elem_expr.kind
                && let Some(&field_expr_id) = fields.get(field_idx)
            {
                let field_lattice = resolve_callee(
                    pkg,
                    store,
                    state,
                    field_expr_id,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                );
                lattice = lattice.join(field_lattice);
            }
        }

        if !matches!(lattice, CalleeLattice::Bottom) {
            state.callable.insert(ident.id, lattice);
        }
    }
}

/// Walks an expression for control-flow structures that affect reaching
/// definitions: assignments, blocks, conditionals, and loops.
#[allow(clippy::too_many_lines)]
fn analyze_expr_flow(
    pkg: &Package,
    store: &PackageStore,
    expr_id: ExprId,
    state: &mut LocalState,
    package_id: PackageId,
    mut recorder: Option<&mut CallRecorder>,
) {
    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Assign(lhs_id, rhs_id) => {
            // Recurse into the RHS first (in evaluation order) so any nested
            // call or `set` is recorded against the running state, THEN apply
            // this assignment's own lattice update.
            analyze_expr_flow(
                pkg,
                store,
                *rhs_id,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            let lhs = pkg.get_expr(*lhs_id);
            if let ExprKind::Var(Res::Local(var), _) = &lhs.kind
                && state.callable.contains_key(var)
            {
                let lattice = resolve_callee(
                    pkg,
                    store,
                    state,
                    *rhs_id,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                );
                state.callable.insert(*var, lattice);
            }
        }
        ExprKind::Block(block_id) => {
            analyze_block_flow(
                pkg,
                store,
                *block_id,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
        }
        ExprKind::If(cond, body, otherwise) => {
            analyze_expr_flow(
                pkg,
                store,
                *cond,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            // Fork: save callable state before branches.
            let pre_if = state.callable.clone();
            analyze_expr_flow(
                pkg,
                store,
                *body,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            let true_state = state.callable.clone();
            // Restore pre-if state and analyze false branch.
            state.callable = pre_if;
            if let Some(else_expr) = otherwise {
                analyze_expr_flow(
                    pkg,
                    store,
                    *else_expr,
                    state,
                    package_id,
                    recorder.as_deref_mut(),
                );
            }
            // Join: merge true and false branch states per variable, tagging
            // entries with the condition for branch splitting. Route through
            // `remap_condition_expr` (matching the immutable path) so a
            // HOF-parameter-substituted boolean survives cleanup; a no-op for
            // ordinary runtime conditions.
            let false_state = std::mem::take(&mut state.callable);
            let remapped_cond = remap_condition_expr(pkg, state, *cond);
            state.callable =
                join_callable_states_with_condition(&true_state, &false_state, remapped_cond);
        }
        ExprKind::While(cond, block_id) => {
            analyze_expr_flow(
                pkg,
                store,
                *cond,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            // Conservative: mark all mutable callable vars assigned inside
            // the loop body as Dynamic.
            let assigned = collect_assigned_vars_in_block(pkg, *block_id);
            for var in &assigned {
                if state.callable.contains_key(var) {
                    state.callable.insert(*var, CalleeLattice::Dynamic);
                }
            }
            // Analyze the body for nested let bindings. Restore pre-existing
            // callable entries to their pre-loop values, but keep NEW entries
            // added by loop-body analysis (loop-local immutable bindings).
            let pre_loop_callable = state.callable.clone();
            analyze_block_flow(
                pkg,
                store,
                *block_id,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            for (var, lattice) in pre_loop_callable {
                state.callable.insert(var, lattice);
            }
        }
        // Operand-position variants: recurse into every nested expression in
        // evaluation order (mirroring `walk_utils::walk_children`) so that a
        // `set` hidden in an operand block updates `state.callable` before any
        // later statement or call is analyzed.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for &e in exprs {
                analyze_expr_flow(pkg, store, e, state, package_id, recorder.as_deref_mut());
            }
        }
        // Short-circuit logical operators (`and`/`or`, including the compound
        // `and=`/`or=` forms): the RHS executes only when the LHS does not
        // short-circuit, so a `set` hidden in the RHS must be applied
        // CONDITIONALLY. Mirror the If-arm fork/join: recurse the LHS (always
        // evaluated), fork the lattice, recurse the RHS on the running state,
        // then join the after-RHS and pre-RHS states tagged with the LHS
        // condition so branch-split dispatch can reconstruct the runtime choice.
        ExprKind::BinOp(BinOp::AndL, cond, rhs) | ExprKind::AssignOp(BinOp::AndL, cond, rhs) => {
            analyze_expr_flow(
                pkg,
                store,
                *cond,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            let pre_rhs = state.callable.clone();
            analyze_expr_flow(pkg, store, *rhs, state, package_id, recorder.as_deref_mut());
            let after_rhs = std::mem::take(&mut state.callable);
            // `and`: RHS runs when the condition is true.
            let remapped_cond = remap_condition_expr(pkg, state, *cond);
            state.callable =
                join_callable_states_with_condition(&after_rhs, &pre_rhs, remapped_cond);
        }
        ExprKind::BinOp(BinOp::OrL, cond, rhs) | ExprKind::AssignOp(BinOp::OrL, cond, rhs) => {
            analyze_expr_flow(
                pkg,
                store,
                *cond,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            let pre_rhs = state.callable.clone();
            analyze_expr_flow(pkg, store, *rhs, state, package_id, recorder.as_deref_mut());
            let after_rhs = std::mem::take(&mut state.callable);
            // `or`: RHS runs when the condition is FALSE. Swap branches so the
            // reused condition `ExprId` dispatches as `if cond { orig } else { rhs }`.
            let remapped_cond = remap_condition_expr(pkg, state, *cond);
            state.callable =
                join_callable_states_with_condition(&pre_rhs, &after_rhs, remapped_cond);
        }
        // Replace-then-record variants: runtime evaluates the replace operand
        // before the record/container operand (mirroring `rebuild_expr`'s
        // `AssignField`/`UpdateField`).
        ExprKind::AssignField(record, _, replace) | ExprKind::UpdateField(record, _, replace) => {
            analyze_expr_flow(
                pkg,
                store,
                *replace,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            analyze_expr_flow(
                pkg,
                store,
                *record,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
        }
        // Indexed assignment variants: runtime evaluates index, then replace,
        // then the container LAST (mirroring `rebuild_expr`'s
        // `AssignIndex`/`UpdateIndex`). The container is a store target; it is
        // recursed last for nested call discovery without mutating the lattice
        // before the index/replace operands.
        ExprKind::AssignIndex(container, index, replace)
        | ExprKind::UpdateIndex(container, index, replace) => {
            analyze_expr_flow(
                pkg,
                store,
                *index,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            analyze_expr_flow(
                pkg,
                store,
                *replace,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            analyze_expr_flow(
                pkg,
                store,
                *container,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b) => {
            analyze_expr_flow(pkg, store, *a, state, package_id, recorder.as_deref_mut());
            analyze_expr_flow(pkg, store, *b, state, package_id, recorder.as_deref_mut());
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            analyze_expr_flow(pkg, store, *e, state, package_id, recorder.as_deref_mut());
        }
        ExprKind::Range(start, step, end) => {
            for e in [start, step, end].into_iter().flatten() {
                analyze_expr_flow(pkg, store, *e, state, package_id, recorder.as_deref_mut());
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                analyze_expr_flow(pkg, store, *c, state, package_id, recorder.as_deref_mut());
            }
            for fa in fields {
                analyze_expr_flow(
                    pkg,
                    store,
                    fa.value,
                    state,
                    package_id,
                    recorder.as_deref_mut(),
                );
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let StringComponent::Expr(e) = component {
                    analyze_expr_flow(pkg, store, *e, state, package_id, recorder.as_deref_mut());
                }
            }
        }
        // Leaves: no nested expressions to analyze.
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }

    // Post-order: record this expression against the running state. This is a
    // no-op for non-`Call` expressions, so visiting every node exactly once
    // resolves each call site against the state as of its evaluation point
    // (operands, including any `set`, are visited before the call node).
    if let Some(rec) = recorder {
        inspect_call_expr(
            store,
            pkg,
            expr_id,
            expr,
            rec.hof_params,
            state,
            rec.call_sites,
            rec.direct_call_sites,
            rec.unresolved_direct_call_sites,
            package_id,
            rec.collapsed_spans,
        );
    }
}

/// Joins two callable-state maps by performing per-variable lattice join
/// with an associated condition from an if/else branch.
fn join_callable_states_with_condition(
    true_state: &FxHashMap<LocalVarId, CalleeLattice>,
    false_state: &FxHashMap<LocalVarId, CalleeLattice>,
    condition: ExprId,
) -> FxHashMap<LocalVarId, CalleeLattice> {
    let mut result = FxHashMap::default();
    let all_vars: FxHashSet<LocalVarId> = true_state
        .keys()
        .chain(false_state.keys())
        .copied()
        .collect();
    for var in all_vars {
        let a_val = true_state
            .get(&var)
            .cloned()
            .unwrap_or(CalleeLattice::Bottom);
        let b_val = false_state
            .get(&var)
            .cloned()
            .unwrap_or(CalleeLattice::Bottom);
        result.insert(var, a_val.join_with_condition(b_val, condition));
    }
    result
}

/// Collects all `LocalVarId`s that are targets of `Assign` expressions
/// within a block (recursively including nested blocks and control flow).
fn collect_assigned_vars_in_block(pkg: &Package, block_id: BlockId) -> Vec<LocalVarId> {
    let mut vars = Vec::new();
    collect_assigned_vars_block(pkg, block_id, &mut vars);
    vars
}

/// Collects every `LocalVarId` assigned within a block (mutable update or
/// `Assign`), accumulating into `vars` so branch joins can invalidate
/// stale lattice entries.
fn collect_assigned_vars_block(pkg: &Package, block_id: BlockId, vars: &mut Vec<LocalVarId>) {
    let block = pkg.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = pkg.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                collect_assigned_vars_expr(pkg, *e, vars);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Collects every `LocalVarId` assigned within an expression subtree,
/// recursing through every nested expression via the exhaustive
/// [`crate::walk_utils::for_each_expr`] walker so that `set` statements
/// hidden in operand-position blocks are observed.
fn collect_assigned_vars_expr(pkg: &Package, expr_id: ExprId, vars: &mut Vec<LocalVarId>) {
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_id, expr| {
        if let ExprKind::Assign(lhs_id, _) = &expr.kind {
            let lhs = pkg.get_expr(*lhs_id);
            if let ExprKind::Var(Res::Local(var), _) = &lhs.kind {
                vars.push(*var);
            }
        }
    });
}

/// Extracts bindings from a pattern. For `Bind(ident)` patterns, records
/// `ident.id → init_expr_id`. For `Tuple` patterns, we cannot easily
/// split the init expression, so we skip those.
fn collect_bindings_from_pat(
    pkg: &Package,
    pat_id: qsc_fir::fir::PatId,
    init_expr_id: ExprId,
    map: &mut FxHashMap<LocalVarId, ExprId>,
) {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            map.insert(ident.id, init_expr_id);
        }
        PatKind::Tuple(sub_pats) => {
            // If the init is also a tuple expression, match element-wise.
            let init_expr = pkg.get_expr(init_expr_id);
            if let ExprKind::Tuple(init_elems) = &init_expr.kind
                && sub_pats.len() == init_elems.len()
            {
                for (&sub_pat_id, &elem_expr_id) in sub_pats.iter().zip(init_elems.iter()) {
                    collect_bindings_from_pat(pkg, sub_pat_id, elem_expr_id, map);
                }
            }
        }
        PatKind::Discard => {}
    }
}
