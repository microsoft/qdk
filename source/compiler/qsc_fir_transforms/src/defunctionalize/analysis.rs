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
    BinOp, Block, BlockId, CallableImpl, CallableKind, Expr, ExprId, ExprKind, Field, FieldAssign,
    FieldPath, ItemId, ItemKind, Lit, LocalVarId, Mutability, Package, PackageId, PackageLookup,
    PackageStore, Pat, PatId, PatKind, Res, SpecImpl, Stmt, StmtId, StmtKind, StoreExprId,
    StoreItemId, StringComponent, UnOp,
};
use qsc_fir::ty::Ty;
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

/// Combined local variable state for the analysis phase.
///
/// `callable` holds flow-sensitive reaching-definitions for callable-typed
/// locals (both mutable and immutable). `exprs` holds raw `ExprId` bindings
/// for all immutable locals, supporting struct field resolution and type
/// look-ups. `condition_substitutions` maps each higher-order-function
/// parameter local to the caller-scope argument expression bound at the call
/// site, so an `if` guard that reads a forwarded parameter can be folded to a
/// literal or remapped to its caller-scope value when reconstructing branch
/// dispatch.
#[derive(Default)]
pub(super) struct LocalState {
    callable: FxHashMap<LocalVarId, CalleeLattice>,
    exprs: FxHashMap<LocalVarId, ExprId>,
    condition_substitutions: FxHashMap<LocalVarId, ExprId>,
    /// Types of the enclosing callable's capturable variable bindings
    /// (parameters and immutable `let` bindings), keyed by `LocalVarId`.
    /// `LocalVarId`s are scoped per callable and collide freely across
    /// callables in the same package, so a captured variable's type must be
    /// resolved against this per-callable map rather than a package-wide
    /// pattern scan. This map serves only closure-capture type resolution.
    /// Mutable locals may still exist and are tracked for flow in `callable`;
    /// they are simply never recorded here because a closure can never capture
    /// one.
    closure_capturable_var_types: FxHashMap<LocalVarId, Ty>,
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
            // An intrinsic callable has no body for the pass to rewrite, so its
            // callable parameters can never be invoked in a way that could be
            // specialized. Treating one as a higher-order function and dropping
            // the parameter would corrupt intrinsics that consume the argument
            // as data rather than invoking it (for example `Length`, whose
            // element type can monomorphize to a callable array): the parameter
            // would be removed from the signature while call sites still pass
            // the argument. Skip intrinsics so their callable arguments survive
            // unchanged.
            if matches!(decl.implementation, CallableImpl::Intrinsic) {
                continue;
            }
            let params = extract_arrow_params(store, pkg, store_id, decl.input);
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
    callable_id: StoreItemId,
    input_pat_id: qsc_fir::fir::PatId,
) -> Vec<CallableParam> {
    let pat = pkg.get_pat(input_pat_id);
    let mut params = Vec::new();
    let hof_input_is_tuple = matches!(pat.kind, PatKind::Tuple(_));

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
                        hof_input_is_tuple,
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
                hof_input_is_tuple,
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
    callable_id: StoreItemId,
    param_pat_id: PatId,
    param_var: LocalVarId,
    top_level_param: usize,
    hof_input_is_tuple: bool,
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
            context.hof_input_is_tuple,
        )),
        Ty::Tuple(items) => {
            for (index, item_ty) in items.iter().enumerate() {
                field_path.push(index);
                extract_arrow_params_from_ty(context, item_ty, field_path, params);
                field_path.pop();
            }
        }
        Ty::Array(item_ty) if matches!(item_ty.as_ref(), Ty::Arrow(_)) => {
            params.push(CallableParam::new(
                context.callable_id,
                context.param_pat_id,
                context.top_level_param,
                field_path.clone(),
                context.param_var,
                param_ty.clone(),
                context.hof_input_is_tuple,
            ));
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
    unresolved_direct_call_sites: &'a mut Vec<StoreExprId>,
    /// Spans of lambda bodies discarded by the identity-closure peephole,
    /// keyed by the collapsed init-expr node, stamped onto surviving direct
    /// calls so circuit instructions point at the original lambda body.
    collapsed_spans: &'a FxHashMap<ExprId, Span>,
    /// Whether already-direct concrete calls in the body being walked should be
    /// recorded. `true` for the entry package; `false` for foreign bodies,
    /// where only closure, local, or field-projection callees are recorded.
    /// Recording ordinary foreign item calls would re-introduce the standard
    /// library's entire call graph as spurious direct call sites.
    record_direct_calls: bool,
}

/// Walks the bodies of all reachable callables across every reachable package
/// and collects call sites where a HOF is invoked with a concrete callable
/// argument. Entry-package bodies additionally record already-direct concrete
/// call sites; foreign bodies (e.g. generic HOFs relocated into their owning
/// package by monomorphization) record only HOF call sites and closure, local,
/// or field-projection callees that require defunctionalization.
fn collect_call_sites(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    hof_params: &FxHashMap<StoreItemId, Vec<CallableParam>>,
    collapsed_spans: &FxHashMap<ExprId, Span>,
) -> (
    Vec<CallSite>,
    Vec<DirectCallSite>,
    Vec<StoreExprId>,
    LatticeStates,
) {
    let package = store.get(package_id);
    let mut call_sites = Vec::new();
    let mut direct_call_sites = Vec::new();
    let mut unresolved_direct_call_sites = Vec::new();
    let mut lattice_states: LatticeStates = FxHashMap::default();

    for &store_id in reachable {
        let body_pkg_id = store_id.package;
        let body_pkg = store.get(body_pkg_id);
        let item = body_pkg.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            // Foreign bodies record only HOF call sites and closure callees;
            // the entry package records every already-direct concrete call.
            let record_direct_calls = body_pkg_id == package_id;
            // Record call sites inline against the running state produced by the
            // ordered flow walk, so each call resolves against its own program
            // point rather than the callable's final whole-body state.
            let mut recorder = CallRecorder {
                hof_params,
                call_sites: &mut call_sites,
                direct_call_sites: &mut direct_call_sites,
                unresolved_direct_call_sites: &mut unresolved_direct_call_sites,
                collapsed_spans,
                record_direct_calls,
            };
            let locals = build_callable_flow_state(
                body_pkg,
                store,
                &decl.implementation,
                decl.input,
                body_pkg_id,
                Some(&mut recorder),
            );

            // Capture non-Bottom lattice entries for the entry package only,
            // keyed by LocalItemId. Foreign bodies are not snapshotted to avoid
            // cross-package key collisions in this diagnostic-only map.
            if body_pkg_id == package_id {
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
    }

    if let Some(entry_expr_id) = package.entry {
        let mut locals = LocalState {
            callable: FxHashMap::default(),
            exprs: FxHashMap::default(),
            condition_substitutions: FxHashMap::default(),
            closure_capturable_var_types: FxHashMap::default(),
        };
        let mut recorder = CallRecorder {
            hof_params,
            call_sites: &mut call_sites,
            direct_call_sites: &mut direct_call_sites,
            unresolved_direct_call_sites: &mut unresolved_direct_call_sites,
            collapsed_spans,
            record_direct_calls: true,
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
    unresolved_direct_call_sites: &mut Vec<StoreExprId>,
    package_id: PackageId,
    collapsed_spans: &FxHashMap<ExprId, Span>,
    record_direct_calls: bool,
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
        record_hof_call_sites(
            store,
            pkg,
            expr_id,
            *args_expr_id,
            locals,
            hof_store_id,
            hof_functor,
            hof_callable_params,
            call_sites,
            package_id,
        );

        return;
    }

    // Reaching here means the call is a plain direct call, not a HOF call site
    // (the HOF branch above already returned). Decide whether to record it.
    //
    // `record_direct_calls` is `false` for *foreign* bodies — callables owned
    // by a package other than the entry package. Such bodies are walked only to
    // discover closures they thread into a HOF; their own already-direct calls
    // are deliberately skipped, since recording every one would drag the entire
    // standard-library call graph in as spurious direct call sites (see
    // `CallRecorder::record_direct_calls`).
    //
    // The one exception is the empty-capture `Closure([], target)` callee that
    // specialization materializes in place when a no-capture closure is threaded
    // into a HOF specialized where it sits. The direct-call rewrite must lower
    // that closure into a direct item call, or the `PostDefunc` invariant breaks
    // and the convergence metric never reaches zero.
    //
    // So, in a foreign body, retain only closure, local, and projected callees
    // after peeling functor wrappers like `Adjoint`/`Controlled`. Ordinary item
    // calls remain skipped.
    //
    // Example (`LibApply` lives in a library, i.e. a foreign package; the
    // no-capture closure `x => H(x)` is threaded into it):
    //
    //     // library package
    //     operation LibApply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
    //     operation LibCaller(q : Qubit) : Unit { LibApply(x => H(x), q); }
    //
    // Specializing `LibApply` for `x => H(x)` clones its body into the library
    // package and turns the forwarded `op(q)` into a `Closure([], target)(q)`
    // callee. Walking that foreign clone, this is the single direct call kept:
    // the rewrite lowers it to the item call `H(q)`. Any other call in a foreign
    // body — an internal helper call, or an `op(q)` whose callee is still an
    // arrow-typed parameter — is skipped.
    if !record_direct_calls {
        let (base_id, _) = peel_body_functors(pkg, *callee_expr_id);
        if !matches!(
            pkg.get_expr(base_id).kind,
            ExprKind::Closure(_, _) | ExprKind::Var(Res::Local(_), _) | ExprKind::Field(_, _)
        ) {
            return;
        }
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

/// Records a [`CallSite`] for every arrow parameter of a resolved HOF callee.
///
/// For each callable parameter of the HOF, the argument at the parameter's
/// input path is resolved to its reaching-definitions lattice: a single
/// concrete callable yields one unconditional call site, a `Multi` lattice
/// yields one conditioned call site per candidate (the branch-split set), and a
/// dynamic or bottom lattice yields a single dynamic call site so the pass
/// surfaces an honest diagnostic later.
#[allow(clippy::too_many_arguments)]
fn record_hof_call_sites(
    store: &PackageStore,
    pkg: &Package,
    expr_id: ExprId,
    args_expr_id: ExprId,
    locals: &LocalState,
    hof_store_id: StoreItemId,
    hof_functor: FunctorApp,
    hof_callable_params: &[CallableParam],
    call_sites: &mut Vec<CallSite>,
    package_id: PackageId,
) {
    let uses_tuple_input = hof_uses_tuple_input_pattern(store, hof_store_id);
    for cp in hof_callable_params {
        let input_path = super::build_param_input_path(uses_tuple_input, cp, hof_functor);
        let resolved_arg_id = extract_arg_at_path(pkg, args_expr_id, &input_path);
        let allow_scoped_capture_exprs = matches!(
            pkg.get_expr(resolved_arg_id).kind,
            ExprKind::Block(_) | ExprKind::If(_, _, _)
        );
        let resolved = resolve_callee_at_path(
            pkg,
            store,
            locals,
            args_expr_id,
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
                    call_pkg_id: package_id,
                    hof_item_id: ItemId {
                        package: hof_store_id.package,
                        item: hof_store_id.item,
                    },
                    top_level_param: cp.top_level_param,
                    field_path: cp.field_path.clone(),
                    hof_input_is_tuple: cp.hof_input_is_tuple,
                    callable_arg: cc,
                    arg_expr_id: resolved_arg_id,
                    condition: vec![],
                });
            }
            CalleeLattice::Multi(candidates) => {
                for (cc, cond) in candidates {
                    call_sites.push(CallSite {
                        call_expr_id: expr_id,
                        call_pkg_id: package_id,
                        hof_item_id: ItemId {
                            package: hof_store_id.package,
                            item: hof_store_id.item,
                        },
                        top_level_param: cp.top_level_param,
                        field_path: cp.field_path.clone(),
                        hof_input_is_tuple: cp.hof_input_is_tuple,
                        callable_arg: cc,
                        arg_expr_id: resolved_arg_id,
                        condition: cond,
                    });
                }
            }
            CalleeLattice::Dynamic | CalleeLattice::Bottom => {
                call_sites.push(CallSite {
                    call_expr_id: expr_id,
                    call_pkg_id: package_id,
                    hof_item_id: ItemId {
                        package: hof_store_id.package,
                        item: hof_store_id.item,
                    },
                    top_level_param: cp.top_level_param,
                    field_path: cp.field_path.clone(),
                    hof_input_is_tuple: cp.hof_input_is_tuple,
                    callable_arg: ConcreteCallable::Dynamic,
                    arg_expr_id: resolved_arg_id,
                    condition: vec![],
                });
            }
        }
    }
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
    unresolved_direct_call_sites: &mut Vec<StoreExprId>,
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
                call_pkg_id: package_id,
                callable,
                condition: vec![],
                def_span,
            });
        }
        CalleeLattice::Multi(candidates) => {
            for (callable, condition) in candidates {
                direct_call_sites.push(DirectCallSite {
                    call_expr_id: expr_id,
                    call_pkg_id: package_id,
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
                unresolved_direct_call_sites.push((package_id, expr_id).into());
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
        ItemKind::Ty(..) => false,
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
        // Defensive `.get()` mirrors `resolve_callee_at_path`, which walks the
        // same path over the same argument expression: an out-of-range index
        // falls back to the whole argument rather than panicking, keeping the
        // two path walkers in lockstep instead of one crashing where the other
        // degrades.
        match elements.get(path[0]) {
            Some(&element_id) if path.len() == 1 => element_id,
            Some(&element_id) => extract_arg_at_path(pkg, element_id, &path[1..]),
            None => args_expr_id,
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
        if matches!(pkg.get_expr(args_expr_id).ty, Ty::Array(_))
            && let Some(candidates) = resolve_indexed_callable_candidates(
                pkg,
                store,
                locals,
                args_expr_id,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            )
        {
            return CalleeLattice::Multi(
                candidates
                    .into_iter()
                    .map(|callable| (callable, vec![]))
                    .collect(),
            );
        }
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
    if let Some(field_value_id) =
        resolve_struct_field(pkg, store, locals, args_expr_id, &field_path, 0)
    {
        if matches!(pkg.get_expr(field_value_id).ty, Ty::Array(_))
            && let Some(candidates) = resolve_indexed_callable_candidates(
                pkg,
                store,
                locals,
                field_value_id,
                depth + 1,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            )
        {
            return CalleeLattice::Multi(
                candidates
                    .into_iter()
                    .map(|callable| (callable, vec![]))
                    .collect(),
            );
        }

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

/// Resolves a callee expression to its reaching-definitions lattice of concrete
/// callables by peeling functor wrappers, following single-assignment immutable
/// locals, resolving if-value-expressions, recognising closures and global item
/// references, and tracing same-package callable returns — up to a recursion
/// depth limit.
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
                    if functor == FunctorApp::default()
                        && matches!(
                            store.get(item_id.package).get_item(item_id.item).kind,
                            ItemKind::Callable(_)
                        ) =>
                {
                    resolve_callable_return(
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
                store,
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
        // `resolve_callee_projection` deliberately does not fold, because
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
                closure_capturable_var_types: locals.closure_capturable_var_types.clone(),
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
                resolve_struct_field(pkg, store, locals, *inner_expr_id, path, depth + 1)
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
///
/// Where [`resolve_callee`] resolves an expression that *is* a callable,
/// this function resolves a callable that is *inside* an expression at a
/// tuple/struct field path — e.g. the `.op` field of a UDT, or element `[1]`
/// of a tuple. The distinction matters because the aggregate itself is not
/// callable; only a specific field within it is.
///
/// # Why this exists separately from `resolve_callee`
///
/// A `Field(inner, path)` expression first attempts direct struct-field
/// resolution via [`resolve_struct_field`] (which finds the initializer when
/// the aggregate is a literal construction). When that fast path fails —
/// because the aggregate flows through a local, a block tail, an `if`
/// branch, or a same-package callable return — this function recursively
/// *projects* into the intermediate expression kinds to locate the callable
/// at the requested path. It handles tuples, locals, blocks, `if`/`else`,
/// calls (both callable-returning functions and UDT constructors), struct
/// literals, and nested field accesses.
///
/// # Key semantic difference: `if` branches are never literal-folded
///
/// Unlike `resolve_callee`'s `If` arm (which folds a constant condition to
/// select a single branch), this function always joins both branches into a
/// `CalleeLattice::Multi`. Folding would leave the unselected branch's
/// closure target unregistered for specialization, and
/// `cleanup_consumed_closures` would be unable to neutralize the surviving
/// `ExprKind::Closure` node, breaking fixpoint convergence. The resulting
/// `Multi` is materialized as a constant-conditioned dispatch by the rewrite
/// phase's `branch_split_direct_call_rewrite`.
///
/// # Callers
///
/// - [`resolve_callee`] — when a `Field(inner, Path)` has no direct struct
///   resolution.
/// - [`resolve_callable_return`] — to trace a callable through the return
///   value of a same-package function along an `output_path`.
/// - [`bind_callable_pat_projections`] — to resolve arrow-typed sub-bindings
///   in destructuring patterns by indexing into the initializer along a
///   field path.
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
                closure_capturable_var_types: locals.closure_capturable_var_types.clone(),
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
            // deliberately do not literal-fold `cond` here. When projecting a
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
                    if functor == FunctorApp::default() =>
                {
                    let target_item = store.get(item_id.package).get_item(item_id.item);
                    match &target_item.kind {
                        ItemKind::Callable(_) => resolve_callable_return(
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
                        // A callable obtained by projecting a UDT field is
                        // only resolved when the UDT lives in the package being
                        // analyzed. For a cross-package UDT-projected callable
                        // we deliberately fall through to `Dynamic` below: the
                        // foreign UDT's field types are not walked here, so no
                        // concrete target can be proven and specializing it
                        // would be unsound. This conservative asymmetry vs the
                        // now-cross-package callable path above is intentional.
                        ItemKind::Ty(_, _) if item_id.package == package_id => {
                            resolve_callee_projection(
                                pkg,
                                store,
                                locals,
                                *args_expr_id,
                                path,
                                depth + 1,
                                allow_scoped_capture_exprs,
                                scoped_capture_vars,
                                package_id,
                            )
                        }
                        ItemKind::Ty(..) => CalleeLattice::Dynamic,
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

/// Reports whether following `path` into the (possibly nested tuple) type `ty`
/// lands on an arrow type.
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

/// Resolves the callable value returned by a (possibly cross-package) callable
/// invoked at a call site by treating the target body as a straight-line
/// function, binding its parameters to the call's argument expressions and
/// tracing the result back to a concrete callable.
///
/// The callee's body is read from its owning package (`item_id.package`), while
/// the call arguments and caller lattice come from the caller's package
/// (`pkg` / `package_id`). The returned closure's capture expressions therefore
/// remain caller-package nodes, which is what the call site rewrite consumes.
#[allow(clippy::too_many_arguments)]
fn resolve_callable_return(
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
    let callee_pkg = store.get(item_id.package);
    let callee_pkg_id = item_id.package;
    let item = callee_pkg.get_item(item_id.item);
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
        closure_capturable_var_types: collect_binding_types_from_pat(callee_pkg, body_input),
    };
    seed_param_bindings_from_call(
        pkg,
        callee_pkg,
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
    analyze_block_flow(
        callee_pkg,
        store,
        body_block_id,
        &mut state,
        callee_pkg_id,
        None,
    );

    let block = callee_pkg.get_block(body_block_id);
    let Some(&stmt_id) = block.stmts.last() else {
        return CalleeLattice::Dynamic;
    };
    let stmt = callee_pkg.get_stmt(stmt_id);
    let return_expr_id = match &stmt.kind {
        StmtKind::Expr(return_expr_id) => *return_expr_id,
        StmtKind::Semi(expr_id)
            if matches!(callee_pkg.get_expr(*expr_id).kind, ExprKind::Return(_)) =>
        {
            let ExprKind::Return(inner_expr_id) = callee_pkg.get_expr(*expr_id).kind else {
                unreachable!("guarded above")
            };
            inner_expr_id
        }
        _ => return CalleeLattice::Dynamic,
    };

    let result = materialize_capture_exprs_from_state(
        callee_pkg,
        &state,
        &param_substitutions,
        resolve_callee_projection(
            callee_pkg,
            store,
            &state,
            return_expr_id,
            output_path,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            callee_pkg_id,
        ),
    );

    if callee_pkg_id == package_id {
        return result;
    }

    // A callable returned from a foreign body is consumed at the caller's call
    // site. A `Global` callee carries its own package, so it threads correctly
    // across packages. A `Closure`, however, is keyed only by a package-local
    // target id, so a closure produced in a foreign body cannot be threaded at
    // the caller's call site. Downgrade any such cross-package closure to
    // `Dynamic` (a clean diagnostic) rather than emitting a dangling target.
    downgrade_closures_to_dynamic(result)
}

/// Maps any `Closure` entries in a lattice element to `Dynamic`, leaving
/// `Global` entries intact. Used to drop cross-package closures that cannot be
/// threaded at a foreign caller's call site.
fn downgrade_closures_to_dynamic(lattice: CalleeLattice) -> CalleeLattice {
    let is_closure = |cc: &ConcreteCallable| matches!(cc, ConcreteCallable::Closure { .. });
    match lattice {
        CalleeLattice::Single(cc) if is_closure(&cc) => CalleeLattice::Dynamic,
        CalleeLattice::Multi(entries) if entries.iter().any(|(cc, _)| is_closure(cc)) => {
            CalleeLattice::Dynamic
        }
        other => other,
    }
}

/// Resolves a branch-guard variable to a constant boolean, if analysis recorded
/// a substitution that fixes its value.
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

/// Follows recorded substitutions and local definitions to resolve an
/// expression to a constant boolean, up to a recursion depth limit.
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

/// Rewrites a guard expression to its recorded substitution so a guard captured
/// in one scope is expressed with values available at the dispatch site.
/// Returns the original id when no substitution applies.
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
                    // A resolved capture whose terminal is a producer-scope
                    // compound literal (struct/tuple/array constructor) still
                    // references the producing function's parameters through its
                    // inner `Var(Res::Local(_))` leaves. Record the caller-scope
                    // substitution for each such leaf so rewrite can deep-clone
                    // the literal and rebind it entirely to caller-scope values,
                    // instead of splicing unbound producer-scope locals into the
                    // caller.
                    if is_compound_capture_literal(pkg, expr) {
                        let substitutions = collect_compound_capture_substitutions(
                            pkg,
                            state,
                            param_substitutions,
                            expr,
                        );
                        // Rebuilding the captured literal in the caller is only
                        // safe when every producer leaf resolves to a
                        // caller-scope value. If any producer
                        // `Var(Res::Local)` leaf is left unresolved — a producer
                        // non-parameter local, or a leaf inside a kind we cannot
                        // safely remap such as a block, closure, assignment, or
                        // a non-pure operation call — it would be copied verbatim
                        // into the caller and break the `PostDefunc`
                        // local-variable consistency invariant.
                        //
                        // When that happens, decline the whole closure to a
                        // dynamic call site. `ConcreteCallable::Dynamic` is the
                        // "cannot specialize" signal, so the original dynamic
                        // dispatch is kept and a recoverable `DynamicCallable`
                        // diagnostic is emitted instead of panicking. On the
                        // base profile that diagnostic is a hard error, which is
                        // preferable to generating incorrect code.
                        if compound_literal_has_residual_leak(pkg, &substitutions, expr) {
                            return ConcreteCallable::Dynamic;
                        }
                        capture.caller_substitutions = substitutions;
                    }
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

/// Reports whether an expression is a compound-literal capture terminal: a
/// struct, tuple, or array constructor whose sub-exprs may reference the
/// producing function's parameters. Such a terminal cannot be spliced into the
/// caller verbatim; its inner producer-parameter leaves must be remapped to
/// caller-scope values first (see [`collect_compound_capture_substitutions`]).
fn is_compound_capture_literal(pkg: &Package, expr_id: ExprId) -> bool {
    matches!(
        pkg.get_expr(expr_id).kind,
        ExprKind::Struct(..)
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::ArrayLit(_)
            | ExprKind::ArrayRepeat(..)
    )
}

/// Collects the caller-scope substitutions needed to reconstruct a
/// producer-scope compound-literal capture in the caller.
///
/// Recurses through the safe, referentially-transparent, value-producing
/// expression kinds (compound containers — struct/tuple/array constructors —
/// plus pure `function` calls, binary/unary operators, field and index
/// accessors, index/field updates, and ranges) and, for each inner
/// `Var(Res::Local(var))` leaf, resolves `var` to its caller-scope argument
/// expression via [`resolve_capture_to_caller`]. Records `(var, caller_expr)`
/// whenever the leaf resolves to a distinct caller-scope expression,
/// de-duplicating by producer-parameter `LocalVarId`. Leaves that do not
/// resolve to a distinct caller-scope expression are left untouched. Kinds
/// outside the safe set are not recursed, so any producer leaf reachable only
/// through them is left for [`compound_literal_has_residual_leak`] to detect.
fn collect_compound_capture_substitutions(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    expr_id: ExprId,
) -> Vec<(LocalVarId, ExprId)> {
    let mut substitutions = Vec::new();
    collect_compound_capture_substitutions_into(
        pkg,
        state,
        param_substitutions,
        expr_id,
        &mut substitutions,
    );
    substitutions
}

/// Recursive worker for [`collect_compound_capture_substitutions`] that walks
/// `expr_id` and appends resolved `(var, caller_expr)` pairs into
/// `substitutions`.
///
/// Descends only through the safe, referentially-transparent, value-producing
/// expression kinds (compound containers — struct/tuple/array constructors —
/// plus pure `function` calls, binary/unary operators, field and index
/// accessors, index/field updates, and ranges). For each inner
/// `Var(Res::Local(var))` leaf it resolves `var` to its caller-scope argument
/// expression via [`resolve_capture_to_caller`] and records
/// `(var, caller_expr)` when the leaf resolves to a distinct caller-scope
/// expression not already recorded for that producer-parameter `LocalVarId`.
/// A `Call` is recursed only when its callee is a pure `function`
/// (via [`call_callee_is_pure_function`]); an `operation` callee is left in
/// place because relocating or duplicating it into caller-scope argument
/// construction would be unsound. Any other kind terminates the descent, so a
/// producer leaf reachable only through it is left for
/// [`compound_literal_has_residual_leak`] to detect.
#[allow(clippy::too_many_lines)]
fn collect_compound_capture_substitutions_into(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    expr_id: ExprId,
    substitutions: &mut Vec<(LocalVarId, ExprId)>,
) {
    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(var), _) => {
            if let Some(caller_expr) =
                resolve_capture_to_caller(pkg, state, param_substitutions, *var)
                && caller_expr != expr_id
                && !substitutions.iter().any(|(existing, _)| existing == var)
            {
                substitutions.push((*var, caller_expr));
            }
        }
        ExprKind::Tuple(elements) | ExprKind::Array(elements) | ExprKind::ArrayLit(elements) => {
            for &elem in elements {
                collect_compound_capture_substitutions_into(
                    pkg,
                    state,
                    param_substitutions,
                    elem,
                    substitutions,
                );
            }
        }
        ExprKind::ArrayRepeat(value, size) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *value,
                substitutions,
            );
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *size,
                substitutions,
            );
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy_id) = copy {
                collect_compound_capture_substitutions_into(
                    pkg,
                    state,
                    param_substitutions,
                    *copy_id,
                    substitutions,
                );
            }
            for field in fields {
                collect_compound_capture_substitutions_into(
                    pkg,
                    state,
                    param_substitutions,
                    field.value,
                    substitutions,
                );
            }
        }
        // A `Call` is only referentially transparent when its callee is a pure
        // `function`; an `operation` callee may carry observable side effects
        // and ordering, so relocating/duplicating the call into caller-scope
        // arg construction is unsound. Leave a non-pure call for the residual
        // leak guard to decline.
        ExprKind::Call(callee, arg) if call_callee_is_pure_function(pkg, *callee) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *callee,
                substitutions,
            );
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *arg,
                substitutions,
            );
        }
        ExprKind::BinOp(_, lhs, rhs) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *lhs,
                substitutions,
            );
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *rhs,
                substitutions,
            );
        }
        ExprKind::UnOp(_, operand) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *operand,
                substitutions,
            );
        }
        ExprKind::Field(base, _) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *base,
                substitutions,
            );
        }
        ExprKind::Index(base, index) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *base,
                substitutions,
            );
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *index,
                substitutions,
            );
        }
        ExprKind::UpdateIndex(container, index, value) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *container,
                substitutions,
            );
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *index,
                substitutions,
            );
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *value,
                substitutions,
            );
        }
        ExprKind::UpdateField(record, _, value) => {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *record,
                substitutions,
            );
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *value,
                substitutions,
            );
        }
        ExprKind::Range(start, step, end) => {
            for &part in [start, step, end].into_iter().flatten() {
                collect_compound_capture_substitutions_into(
                    pkg,
                    state,
                    param_substitutions,
                    part,
                    substitutions,
                );
            }
        }
        ExprKind::Parallel(limit, body) => {
            if let Some(limit_id) = limit {
                collect_compound_capture_substitutions_into(
                    pkg,
                    state,
                    param_substitutions,
                    *limit_id,
                    substitutions,
                );
            }
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *body,
                substitutions,
            );
        }
        ExprKind::Assign(..)
        | ExprKind::AssignOp(..)
        | ExprKind::AssignField(..)
        | ExprKind::AssignIndex(..)
        | ExprKind::Block(..)
        | ExprKind::Call(..)
        | ExprKind::Closure(..)
        | ExprKind::Fail(..)
        | ExprKind::Hole
        | ExprKind::If(..)
        | ExprKind::Lit(..)
        | ExprKind::Return(..)
        | ExprKind::String(..)
        | ExprKind::Var(..)
        | ExprKind::While(..) => {}
    }
}

/// Reports whether a `Call`'s callee resolves to a pure `function`.
///
/// A Q# `function` is guaranteed side-effect free (it cannot call operations,
/// allocate qubits, or measure) and its arrow type cannot bear functors, so it
/// is referentially transparent and its call may be relocated or duplicated
/// into caller-scope argument construction without changing observable
/// behavior. An `operation` may have observable side effects and ordering, so
/// its call must not be relocated. The callee's arrow-type `kind` is the
/// discriminator and is available directly at the call site for item, local,
/// and closure callees alike.
fn call_callee_is_pure_function(pkg: &Package, callee: ExprId) -> bool {
    matches!(
        &pkg.get_expr(callee).ty,
        Ty::Arrow(arrow) if arrow.kind == CallableKind::Function
    )
}

/// Reports whether rebuilding a captured compound literal in the caller would
/// leave an unresolved producer local behind.
///
/// This mirrors [`collect_compound_capture_substitutions`] and the deep-clone
/// in rewrite: it recurses the same safe, referentially-transparent kinds,
/// including the same rule that only pure `function` calls may be entered, so a
/// leaf that collect already recorded a substitution for is not flagged. A
/// `Var(Res::Local(var))` leaf reached directly is a leak when `var` has no
/// recorded substitution. Any leaf inside a kind the clone keeps verbatim — a
/// block, closure, assignment, control-flow expression, or non-pure operation
/// call — counts as a leak whenever it references a producer local.
///
/// The set of recursed kinds must match collect and clone exactly. Recursing
/// fewer kinds would wrongly decline captures that can in fact be rebuilt;
/// recursing more would accept a residue that cannot be represented in caller
/// scope.
fn compound_literal_has_residual_leak(
    pkg: &Package,
    substitutions: &[(LocalVarId, ExprId)],
    expr_id: ExprId,
) -> bool {
    match &pkg.get_expr(expr_id).kind {
        ExprKind::Var(Res::Local(var), _) => !substitutions.iter().any(|(k, _)| k == var),
        ExprKind::Tuple(elems) | ExprKind::Array(elems) | ExprKind::ArrayLit(elems) => elems
            .iter()
            .any(|&elem| compound_literal_has_residual_leak(pkg, substitutions, elem)),
        ExprKind::ArrayRepeat(value, size) => {
            compound_literal_has_residual_leak(pkg, substitutions, *value)
                || compound_literal_has_residual_leak(pkg, substitutions, *size)
        }
        ExprKind::Struct(_, copy, fields) => {
            copy.is_some_and(|copy| compound_literal_has_residual_leak(pkg, substitutions, copy))
                || fields.iter().any(|field| {
                    compound_literal_has_residual_leak(pkg, substitutions, field.value)
                })
        }
        ExprKind::Call(callee, arg) if call_callee_is_pure_function(pkg, *callee) => {
            compound_literal_has_residual_leak(pkg, substitutions, *callee)
                || compound_literal_has_residual_leak(pkg, substitutions, *arg)
        }
        // Non-pure calls cannot be relocated while rebuilding a captured
        // compound literal in the caller, even when the call has no producer
        // locals to leak. Decline the closure instead of duplicating or moving
        // operation effects.
        ExprKind::Call(..) => true,
        ExprKind::BinOp(_, lhs, rhs) => {
            compound_literal_has_residual_leak(pkg, substitutions, *lhs)
                || compound_literal_has_residual_leak(pkg, substitutions, *rhs)
        }
        ExprKind::UnOp(_, operand) => {
            compound_literal_has_residual_leak(pkg, substitutions, *operand)
        }
        ExprKind::Field(base, _) => compound_literal_has_residual_leak(pkg, substitutions, *base),
        ExprKind::Index(base, index) => {
            compound_literal_has_residual_leak(pkg, substitutions, *base)
                || compound_literal_has_residual_leak(pkg, substitutions, *index)
        }
        ExprKind::UpdateIndex(container, index, value) => {
            compound_literal_has_residual_leak(pkg, substitutions, *container)
                || compound_literal_has_residual_leak(pkg, substitutions, *index)
                || compound_literal_has_residual_leak(pkg, substitutions, *value)
        }
        ExprKind::UpdateField(record, _, value) => {
            compound_literal_has_residual_leak(pkg, substitutions, *record)
                || compound_literal_has_residual_leak(pkg, substitutions, *value)
        }
        ExprKind::Range(start, step, end) => [start, step, end]
            .into_iter()
            .flatten()
            .any(|&part| compound_literal_has_residual_leak(pkg, substitutions, part)),
        ExprKind::Parallel(limit, body) => {
            limit.is_some_and(|limit| compound_literal_has_residual_leak(pkg, substitutions, limit))
                || compound_literal_has_residual_leak(pkg, substitutions, *body)
        }
        // A non-pure `Call` (operation callee) and every other un-remappable
        // kind is kept verbatim by the clone, so it leaks if it references any
        // producer local.
        ExprKind::Assign(..)
        | ExprKind::AssignOp(..)
        | ExprKind::AssignField(..)
        | ExprKind::AssignIndex(..)
        | ExprKind::Block(..)
        | ExprKind::Closure(..)
        | ExprKind::Fail(..)
        | ExprKind::Hole
        | ExprKind::If(..)
        | ExprKind::Lit(..)
        | ExprKind::Return(..)
        | ExprKind::String(..)
        | ExprKind::Var(..)
        | ExprKind::While(..) => expr_references_local(pkg, expr_id),
    }
}

/// Reports whether `expr_id` transitively references any `Var(Res::Local(_))`.
///
/// A sound deep scan over every expression kind (via the FIR `Visitor`), used
/// by [`compound_literal_has_residual_leak`] on the verbatim-cloned leaves so
/// no producer local can slip past the decline guard.
fn expr_references_local(pkg: &Package, expr_id: ExprId) -> bool {
    let mut finder = LocalVarFinder {
        package: pkg,
        found: false,
    };
    finder.visit_expr(expr_id);
    finder.found
}

/// FIR `Visitor` that short-circuits on the first `Var(Res::Local(_))` reached
/// from a starting expression.
struct LocalVarFinder<'a> {
    package: &'a Package,
    found: bool,
}

impl<'a> Visitor<'a> for LocalVarFinder<'a> {
    fn visit_expr(&mut self, expr: ExprId) {
        if self.found {
            return;
        }
        if let ExprKind::Var(Res::Local(_), _) = &self.package.get_expr(expr).kind {
            self.found = true;
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

/// Seeds the callable-flow lattice for a HOF with the concrete callables
/// bound to its arrow parameters at a specific call site, enabling
/// reaching-def analysis to track parameter-forwarding chains.
#[allow(clippy::too_many_arguments)]
fn seed_param_bindings_from_call(
    caller_package: &Package,
    hof_package: &Package,
    store: &PackageStore,
    caller_locals: &LocalState,
    state: &mut LocalState,
    pat_id: PatId,
    arg_expr_id: ExprId,
    caller_package_id: PackageId,
) {
    let pat = hof_package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            state.exprs.insert(ident.id, arg_expr_id);
            state.condition_substitutions.insert(ident.id, arg_expr_id);
            if matches!(pat.ty, Ty::Arrow(_)) {
                let lattice = resolve_callee(
                    caller_package,
                    store,
                    caller_locals,
                    arg_expr_id,
                    0,
                    true,
                    &FxHashSet::default(),
                    caller_package_id,
                );
                state.callable.insert(ident.id, lattice);
            }
        }
        PatKind::Tuple(sub_pats) => {
            let arg_expr = caller_package.get_expr(arg_expr_id);
            if let ExprKind::Tuple(arg_elems) = &arg_expr.kind
                && sub_pats.len() == arg_elems.len()
            {
                for (&sub_pat_id, &arg_elem_id) in sub_pats.iter().zip(arg_elems.iter()) {
                    seed_param_bindings_from_call(
                        caller_package,
                        hof_package,
                        store,
                        caller_locals,
                        state,
                        sub_pat_id,
                        arg_elem_id,
                        caller_package_id,
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
    store: &PackageStore,
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
                    store,
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
        ExprKind::Call(callee_id, args_id) if is_type_constructor(pkg, store, *callee_id) => {
            resolve_struct_field(pkg, store, locals, *args_id, path, depth + 1)
        }
        ExprKind::Var(Res::Local(var), _) => {
            let &init_id = locals.exprs.get(var)?;
            resolve_struct_field(pkg, store, locals, init_id, path, depth + 1)
        }
        ExprKind::Field(nested_inner_id, Field::Path(nested_path)) => {
            // Two-level field access: resolve the outer field to get the inner
            // struct expression, then resolve the target field within that.
            let intermediate_id =
                resolve_struct_field(pkg, store, locals, *nested_inner_id, nested_path, depth + 1)?;
            resolve_struct_field(pkg, store, locals, intermediate_id, path, depth + 1)
        }
        _ => None,
    }
}

fn is_type_constructor(pkg: &Package, store: &PackageStore, expr_id: ExprId) -> bool {
    let ExprKind::Var(Res::Item(item_id), _) = pkg.get_expr(expr_id).kind else {
        return false;
    };
    matches!(
        store.get(item_id.package).get_item(item_id.item).kind,
        ItemKind::Ty(..)
    )
}

/// Resolves a single `Index(array, index)` expression to the concrete
/// callable at the indexed position when both the array and index are
/// statically known.
fn resolve_indexed_array_element(
    pkg: &Package,
    store: &PackageStore,
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
    resolve_array_element_at_index(pkg, store, locals, array_expr_id, index, depth + 1)
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
    let element_expr_ids = resolve_array_elements(pkg, store, locals, array_expr_id, depth + 1)?;
    let mut candidates = Vec::new();

    for elem_expr_id in element_expr_ids {
        let elem_allow_scoped_capture_exprs = allow_scoped_capture_exprs
            || matches!(
                pkg.get_expr(elem_expr_id).kind,
                ExprKind::Block(_) | ExprKind::If(_, _, _)
            );
        let resolved = resolve_callee(
            pkg,
            store,
            locals,
            elem_expr_id,
            depth + 1,
            elem_allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        );

        match resolved {
            CalleeLattice::Single(callable) => {
                candidates.push(callable);
            }
            CalleeLattice::Multi(entries) => {
                for (callable, condition) in entries {
                    if !condition.is_empty() {
                        return None;
                    }
                    candidates.push(callable);
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
    store: &PackageStore,
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
        ExprKind::Var(Res::Local(var), _) => locals.exprs.get(var).and_then(|&init_expr_id| {
            resolve_array_elements(pkg, store, locals, init_expr_id, depth + 1)
        }),
        ExprKind::Block(block_id) => {
            let block = pkg.get_block(*block_id);
            let stmt_id = *block.stmts.last()?;
            let stmt = pkg.get_stmt(stmt_id);
            let tail_expr_id = match &stmt.kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                _ => return None,
            };
            resolve_array_elements(pkg, store, locals, tail_expr_id, depth + 1)
        }
        ExprKind::Return(inner_expr_id) => {
            resolve_array_elements(pkg, store, locals, *inner_expr_id, depth + 1)
        }
        ExprKind::Field(inner_expr_id, Field::Path(path)) => {
            let field_value_id =
                resolve_struct_field(pkg, store, locals, *inner_expr_id, path, depth + 1)?;
            resolve_array_elements(pkg, store, locals, field_value_id, depth + 1)
        }
        _ => None,
    }
}

/// Resolves the element at a specific static index within an array-literal
/// expression (after [`resolve_array_elements`] has resolved each slot).
fn resolve_array_element_at_index(
    pkg: &Package,
    store: &PackageStore,
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
            resolve_array_element_at_index(pkg, store, locals, init_expr_id, index, depth + 1)
        }),
        ExprKind::Block(block_id) => {
            let block = pkg.get_block(*block_id);
            let stmt_id = *block.stmts.last()?;
            let stmt = pkg.get_stmt(stmt_id);
            let tail_expr_id = match &stmt.kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                _ => return None,
            };
            resolve_array_element_at_index(pkg, store, locals, tail_expr_id, index, depth + 1)
        }
        ExprKind::Return(inner_expr_id) => {
            resolve_array_element_at_index(pkg, store, locals, *inner_expr_id, index, depth + 1)
        }
        ExprKind::Field(inner_expr_id, Field::Path(path)) => {
            let field_value_id =
                resolve_struct_field(pkg, store, locals, *inner_expr_id, path, depth + 1)?;
            resolve_array_element_at_index(pkg, store, locals, field_value_id, index, depth + 1)
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
            let expr = resolve_known_callable_capture_expr(pkg, locals, var)
                .or_else(|| resolve_scoped_capture_expr(pkg, locals, var, scoped_capture_vars));
            Some(CapturedVar {
                var,
                ty,
                expr,
                caller_substitutions: Vec::new(),
            })
        })
        .collect()
}

/// Returns the initializer expression bound to `var` when it resolves to a
/// statically-known callable value (see [`is_known_callable_capture_expr`]),
/// used to recognize a capture that can be baked into a closure target.
fn resolve_known_callable_capture_expr(
    pkg: &Package,
    locals: &LocalState,
    var: LocalVarId,
) -> Option<ExprId> {
    let expr_id = *locals.exprs.get(&var)?;
    is_known_callable_capture_expr(pkg, locals, expr_id, 0).then_some(expr_id)
}

/// Tests whether an expression is a statically-known callable value: a
/// non-generic item reference, a capture-free closure, or a local that forwards
/// (through `LocalState`) to one of those.
///
/// Recursion is bounded by `MAX_RESOLVE_DEPTH` and guards against a local that
/// refers back to itself, so a forwarding chain cannot loop.
fn is_known_callable_capture_expr(
    pkg: &Package,
    locals: &LocalState,
    expr_id: ExprId,
    depth: usize,
) -> bool {
    if depth > MAX_RESOLVE_DEPTH {
        return false;
    }
    let (base_id, _) = peel_body_functors(pkg, expr_id);
    match &pkg.get_expr(base_id).kind {
        ExprKind::Var(Res::Item(_), generic_args) => generic_args.is_empty(),
        ExprKind::Closure(captures, _) => captures.is_empty(),
        ExprKind::Var(Res::Local(next), _) => locals.exprs.get(next).is_some_and(|&next_expr| {
            next_expr != expr_id
                && is_known_callable_capture_expr(pkg, locals, next_expr, depth + 1)
        }),
        _ => false,
    }
}

/// Resolves a capture expression by walking the enclosing block scope and
/// its visible local bindings, used when a direct `LocalState.exprs` lookup
/// cannot see the binding.
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

/// Finds the type of a local variable.
///
/// Resolution order: the immutable-locals initialiser map (`exprs`), then the
/// per-callable variable-type map (`var_types`, covering parameters and
/// immutable `let` bindings), then a package-wide pattern scan as a last
/// resort. The scoped lookups are preferred because `LocalVarId`s collide
/// across callables, so the global scan can return an unrelated binding.
fn find_local_var_type(pkg: &Package, locals: &LocalState, var: LocalVarId) -> Option<Ty> {
    if let Some(&init_expr_id) = locals.exprs.get(&var) {
        Some(pkg.get_expr(init_expr_id).ty.clone())
    } else if let Some(ty) = locals.closure_capturable_var_types.get(&var) {
        // Enclosing-callable parameter or immutable `let` binding. Resolve
        // against the per-callable variable map; `LocalVarId`s collide across
        // callables, so a package-wide pattern scan would return an unrelated
        // binding.
        Some(ty.clone())
    } else {
        // The variable may come from an outer scope not tracked above. Scan
        // all patterns as a last resort. This is unreliable when `LocalVarId`s
        // collide across callables, so the scoped lookups above are preferred.
        find_var_type_in_pats(pkg, var)
    }
}

/// Collects the types of a callable's parameter bindings into a per-callable
/// map keyed by `LocalVarId`, walking the body specialization input pattern
/// (falling back to the declaration input) and any functored specializations.
fn collect_callable_param_types(
    pkg: &Package,
    callable_impl: &CallableImpl,
    fallback_input: qsc_fir::fir::PatId,
) -> FxHashMap<LocalVarId, Ty> {
    let mut map = FxHashMap::default();
    match callable_impl {
        CallableImpl::Intrinsic => {
            collect_binding_types_from_pat_into(pkg, fallback_input, &mut map);
        }
        CallableImpl::Spec(spec_impl) => {
            collect_binding_types_from_pat_into(
                pkg,
                spec_impl.body.input.unwrap_or(fallback_input),
                &mut map,
            );
            for spec in functored_specs(spec_impl) {
                collect_binding_types_from_pat_into(
                    pkg,
                    spec.input.unwrap_or(fallback_input),
                    &mut map,
                );
            }
        }
        CallableImpl::SimulatableIntrinsic(spec_decl) => {
            collect_binding_types_from_pat_into(
                pkg,
                spec_decl.input.unwrap_or(fallback_input),
                &mut map,
            );
        }
    }
    map
}

/// Returns a fresh per-callable variable-type map built from a single input
/// pattern.
fn collect_binding_types_from_pat(
    pkg: &Package,
    pat_id: qsc_fir::fir::PatId,
) -> FxHashMap<LocalVarId, Ty> {
    let mut map = FxHashMap::default();
    collect_binding_types_from_pat_into(pkg, pat_id, &mut map);
    map
}

/// Recursively records `LocalVarId` => `Ty` for every binding in a pattern.
fn collect_binding_types_from_pat_into(
    pkg: &Package,
    pat_id: qsc_fir::fir::PatId,
    map: &mut FxHashMap<LocalVarId, Ty>,
) {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            map.insert(ident.id, pat.ty.clone());
        }
        PatKind::Tuple(sub_pats) => {
            for &sub_pat_id in sub_pats {
                collect_binding_types_from_pat_into(pkg, sub_pat_id, map);
            }
        }
        PatKind::Discard => {}
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
    input_pat: qsc_fir::fir::PatId,
    package_id: PackageId,
    recorder: Option<&mut CallRecorder>,
) -> LocalState {
    let mut state = LocalState {
        callable: FxHashMap::default(),
        exprs: FxHashMap::default(),
        condition_substitutions: FxHashMap::default(),
        closure_capturable_var_types: collect_callable_param_types(pkg, callable_impl, input_pat),
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
            // Record binding types so captured locals resolve against this
            // per-callable map instead of a collision-prone package-wide scan.
            // Only immutable bindings need recording: the frontend forbids
            // closures from capturing mutable variables so a mutable binding can never
            // appear as a capture whose type needs resolving here.
            collect_binding_types_from_pat_into(
                pkg,
                *pat_id,
                &mut state.closure_capturable_var_types,
            );
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

/// Walks a binding pattern and records, in the analysis state, the reaching
/// callables for each arrow-typed sub-binding by indexing into the initializer
/// along the accumulated field `path`.
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
    let Some(array_elem_ids) = resolve_array_elements(pkg, store, state, array_expr_id, 0) else {
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
    // Any write to a local invalidates conditional callables whose dispatch
    // guard reads that local. Rewrite re-evaluates guards at the apply site, so
    // reassigning a guard variable after a conditional callable's guarded value
    // was formed would make the apply-site read observe the new value and
    // dispatch to the wrong branch. Degrade those callables to `Dynamic` before
    // processing the write so the stale dispatch is rejected with a clear
    // diagnostic rather than silently miscompiled.
    if let Some(written) = assignment_written_local(pkg, expr) {
        invalidate_guard_dependents(pkg, state, written);
    }
    match &expr.kind {
        ExprKind::Assign(lhs_id, rhs_id) => {
            // Recurse into the RHS first (in evaluation order) so any nested
            // call or `set` is recorded against the running state, then apply
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
            // callable entries to their pre-loop values, but keep new entries
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
        // conditionally. Mirror the If-arm fork/join: recurse the LHS (always
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
            // `or`: RHS runs when the condition is false. Swap branches so the
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
        // then the container last (mirroring `rebuild_expr`'s
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
        ExprKind::Parallel(limit, expr) => {
            if let Some(l) = limit {
                analyze_expr_flow(pkg, store, *l, state, package_id, recorder.as_deref_mut());
            }
            analyze_expr_flow(
                pkg,
                store,
                *expr,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
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
            rec.record_direct_calls,
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

/// Resolves the base local of an assignment left-hand side, descending through
/// field and index projections (`x::field = ...`, `arr[i] = ...`) to the
/// underlying `Var(Local)`. Returns `None` when the target is not rooted in a
/// local.
fn assign_lhs_base_local(pkg: &Package, lhs_id: ExprId) -> Option<LocalVarId> {
    match &pkg.get_expr(lhs_id).kind {
        ExprKind::Var(Res::Local(var), _) => Some(*var),
        ExprKind::Field(base, _) | ExprKind::Index(base, _) => assign_lhs_base_local(pkg, *base),
        _ => None,
    }
}

/// Reports whether `expr` transitively reads the local `var`.
fn expr_reads_local(pkg: &Package, expr_id: ExprId, var: LocalVarId) -> bool {
    let mut found = false;
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_id, expr| {
        if let ExprKind::Var(Res::Local(v), _) = &expr.kind
            && *v == var
        {
            found = true;
        }
    });
    found
}

/// Degrades to `Dynamic` any conditional callable in `state` whose dispatch
/// guard reads `written`, invoked when `written` is reassigned during the
/// forward flow.
///
/// Rewrite reconstructs a conditional callable's dispatch by re-evaluating its
/// guards at the *apply* site, not the *binding* site. Once a guard variable is
/// reassigned after the callable's guarded value was formed, the guard read at
/// the apply site would observe the new value and select the wrong branch (see
/// the `reaching_def_conditional_callable_reassigned_guard_dynamic` regression).
/// Marking such callables `Dynamic` surfaces a clear "could not be resolved
/// statically" diagnostic instead of emitting incorrect dispatch. Guards formed
/// *after* this write are unaffected, so a normalization accumulator assigned
/// before the branch decision (e.g. `cond_normalize`'s `__cond`) stays
/// resolvable.
fn invalidate_guard_dependents(pkg: &Package, state: &mut LocalState, written: LocalVarId) {
    for lattice in state.callable.values_mut() {
        if let CalleeLattice::Multi(entries) = lattice {
            let depends = entries.iter().any(|(_, guards)| {
                guards
                    .iter()
                    .any(|&guard| expr_reads_local(pkg, guard, written))
            });
            if depends {
                *lattice = CalleeLattice::Dynamic;
            }
        }
    }
}

/// Resolves the base local written by an assignment expression, descending
/// through field and index projections (`x::field = ...`, `arr[i] = ...`) to
/// the underlying `Var(Local)`. Returns `None` when the expression is not an
/// assignment rooted in a local.
fn assignment_written_local(pkg: &Package, expr: &Expr) -> Option<LocalVarId> {
    let lhs_id = match &expr.kind {
        ExprKind::Assign(lhs, _)
        | ExprKind::AssignOp(_, lhs, _)
        | ExprKind::AssignField(lhs, _, _)
        | ExprKind::AssignIndex(lhs, _, _) => *lhs,
        _ => return None,
    };
    assign_lhs_base_local(pkg, lhs_id)
}

/// Extracts bindings from a pattern. For `Bind(ident)` patterns, records
/// `ident.id => init_expr_id`. For `Tuple` patterns, we cannot easily
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
