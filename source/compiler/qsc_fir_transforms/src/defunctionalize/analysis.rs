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
//!   array elements, struct field accesses, and callable returns. Cross-package
//!   return tracing retains global identities but declines foreign closures
//!   (via [`resolve_callee`] and its helpers).
//! - Build per-callable lattice snapshots for diagnostics and tests (via
//!   [`build_callable_flow_state`] / [`analyze_spec_flow`]).
//!
//! Conditional normalization owns guard snapshots, including when the pre-pass
//! invokes it for fresh selections. The pre-pass normalizes captures, exposes
//! initializer prefixes and aggregate aliases, and promotes callable locals.
//! The driver normalizes direct callee control flow before collecting these facts.

use super::captures::{callee_has_function_kind, capture_expr_children};
use super::rewrite::{ConsumptionSite, EvaluationDisposition, consumed_callable_expr_disposition};
use super::types::{
    AnalysisResult, CallSite, CallableParam, CalleeLattice, CaptureScope, CaptureSubstitution,
    CapturedVar, ConcreteCallable, DirectCallSite, LatticeStates, ScopedLocal, UdtMetadata,
    compose_functors, peel_body_functors,
};
use crate::fir_builder::functored_specs;
use crate::walk_utils::{
    collect_total_foreign_callables, expr_is_safe_to_discard,
    expr_is_safe_to_discard_with_total_foreign,
};
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::fir::{
    BinOp, Block, BlockId, CallableImpl, Expr, ExprId, ExprKind, Field, FieldAssign, FieldPath,
    Global, ItemId, ItemKind, Lit, LocalItemId, LocalVarId, Mutability, Package, PackageId,
    PackageLookup, PackageStore, Pat, PatId, PatKind, Res, SpecImpl, Stmt, StmtId, StmtKind,
    StoreExprId, StoreItemId, StringComponent, UnOp,
};
use qsc_fir::ty::Ty;
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// Combined local variable state for the analysis phase.
///
/// `callable` holds flow-sensitive reaching-definitions for callable-typed
/// locals (both mutable and immutable), plus `Dynamic` tombstones for immutable
/// aggregates whose callable provenance can no longer be replayed. `exprs` holds
/// raw `ExprId` bindings for immutable locals, supporting struct field resolution
/// and type look-ups.
/// `condition_substitutions` maps each same-package callable-return producer parameter to
/// the caller-scope argument expression bound at the call site, so an `if` guard
/// that reads a forwarded parameter can be folded to a literal or remapped to
/// its caller-scope value when reconstructing branch dispatch.
/// Those expressions are interpreted in `caller`, never through producer-local
/// bindings: local IDs are reused independently by each callable.
#[derive(Clone, Default)]
pub(super) struct LocalState {
    owner: CaptureScope,
    clone_items: Rc<FxHashSet<StoreItemId>>,
    udt_metadata: Rc<UdtMetadata>,
    callable: FxHashMap<LocalVarId, CalleeLattice>,
    /// Possible sources of replayed callable facts, including projections and
    /// assignments with no immutable initializer. Retained across branch joins.
    callable_sources: FxHashMap<LocalVarId, FxHashSet<ExprId>>,
    /// Cached local-read occurrences with an existing callable lattice fact,
    /// including functor wrappers, observed before later operand evaluation.
    evaluated_callables: FxHashMap<ExprId, CalleeLattice>,
    /// Whether evaluated guards, indices and replayed callable reads still have
    /// their original operands, including before their destination is bound.
    evaluated_selections: FxHashMap<ExprId, bool>,
    exprs: FxHashMap<LocalVarId, ExprId>,
    condition_substitutions: FxHashMap<LocalVarId, ExprId>,
    caller: Option<Rc<LocalState>>,
    /// Producer bodies currently being traced, including through local bindings.
    active_returns: Rc<FxHashSet<StoreItemId>>,
    /// Bindings visible at the current program point. Unlike `exprs`, this is
    /// restored when analysis leaves a lexical block.
    visible_bindings: FxHashSet<LocalVarId>,
    /// Mutable bindings visible at the current program point.
    mutable_bindings: FxHashSet<LocalVarId>,
    /// Types of the enclosing callable's capturable variable bindings
    /// (parameters and immutable `let` bindings), keyed by `LocalVarId`.
    /// `LocalVarId`s are scoped per callable and collide freely across
    /// callables in the same package, so a captured variable's type must be
    /// resolved against this per-callable map rather than a package-wide
    /// pattern scan. This map also identifies stable bindings when deciding
    /// whether a capture expression can be replayed at a later call site.
    /// Mutable locals may still exist and are tracked for flow in `callable`;
    /// they are simply never recorded here because a closure can never capture
    /// one.
    closure_capturable_var_types: FxHashMap<LocalVarId, Ty>,
}

/// Bounds expression/alias traversal and nested producer-return analysis.
const MAX_RESOLVE_DEPTH: usize = 32;

fn local_initializer(locals: &LocalState, var: LocalVarId) -> Option<(ExprId, &LocalState)> {
    if matches!(locals.callable.get(&var), Some(CalleeLattice::Dynamic)) {
        return None;
    }
    let (expr, owner) = if let Some(&arg) = locals.condition_substitutions.get(&var) {
        (arg, locals.caller.as_deref()?)
    } else {
        (*locals.exprs.get(&var)?, locals)
    };
    selection_can_be_replayed(owner, expr).then_some((expr, owner))
}

fn normalize_index(length: usize, index: i64) -> Option<usize> {
    if index >= 0 {
        usize::try_from(index).ok().filter(|&index| index < length)
    } else {
        let from_end = usize::try_from(index.unsigned_abs()).ok()?;
        length.checked_sub(from_end)
    }
}

/// Runs the analysis phase: finds callable parameters and collects call sites.
///
/// `preserved_direct_lambda_calls` carries the prior iteration's occurrence-local
/// operands, which survive a closure callee having been rewritten to its lifted
/// lambda item.
///
/// `total_foreign` names the callables outside `package_id` that are known
/// side-effect free and total. It reaches the argument-position disposition
/// check in [`record_hof_call_sites`], where it keeps a pure cross-package
/// factory from being mistaken for an observable producer.
pub(super) fn analyze(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    specialized_items: &FxHashSet<StoreItemId>,
    collapsed_spans: &FxHashMap<(PackageId, ExprId), Span>,
    preserved_direct_lambda_calls: &[DirectCallSite],
    total_foreign: &FxHashSet<ItemId>,
) -> AnalysisResult {
    let hof_params = find_callable_params(store, reachable);
    let udt_metadata = UdtMetadata::new(store);
    let CollectedCallSites {
        call_sites,
        direct_call_sites,
        unresolved_direct_call_sites,
        lattice_states,
    } = collect_call_sites(
        store,
        package_id,
        reachable,
        specialized_items,
        &hof_params,
        collapsed_spans,
        preserved_direct_lambda_calls,
        total_foreign,
        Rc::new(udt_metadata.clone()),
    );
    AnalysisResult {
        udt_metadata,
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
/// type and records arrow leaves and arrays of arrows as `CallableParam`s.
///
/// UDTs are expanded to their pure type so callable fields inside nested
/// newtypes are treated the same way as tuple fields.
///
/// Unguarded UDT recursion; terminates only because the frontend rejects cyclic UDTs.
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
    /// Calls with unresolved callees or inadmissible captures, recorded so the
    /// driver can emit a call-site `DynamicCallable` diagnostic.
    unresolved_direct_call_sites: &'a mut Vec<StoreExprId>,
    /// Spans of lambda bodies discarded by the identity-closure peephole,
    /// keyed by package and collapsed init-expr node, stamped onto surviving direct
    /// calls so circuit instructions point at the original lambda body.
    collapsed_spans: &'a FxHashMap<(PackageId, ExprId), Span>,
    /// Occurrence-local operands retained from the prior fixpoint iteration
    /// after rewrite replaced a closure callee with its lifted lambda item.
    preserved_direct_lambda_calls: &'a [DirectCallSite],
    /// Whether direct-call analysis accepts all callee expression shapes.
    /// Foreign bodies admit only closure, local, field-projection, and
    /// previously rewritten lifted-lambda callees. Ordinary literal item calls
    /// need no direct-call record in either case.
    record_direct_calls: bool,
    /// Callables outside the package being rewritten that are known
    /// side-effect free and total, used by the argument-position disposition
    /// check in [`record_hof_call_sites`].
    total_foreign: &'a FxHashSet<ItemId>,
}

struct CollectedCallSites {
    call_sites: Vec<CallSite>,
    direct_call_sites: Vec<DirectCallSite>,
    unresolved_direct_call_sites: Vec<StoreExprId>,
    lattice_states: LatticeStates,
}

/// Walks the bodies of all reachable callables across every reachable package
/// and collects call sites where a HOF is invoked with a concrete callable
/// argument. Non-HOF callee expressions are also analyzed for direct rewriting.
/// Foreign bodies restrict that analysis to closure, local, field-projection,
/// and previously rewritten lifted-lambda callees.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn collect_call_sites(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    specialized_items: &FxHashSet<StoreItemId>,
    hof_params: &FxHashMap<StoreItemId, Vec<CallableParam>>,
    collapsed_spans: &FxHashMap<(PackageId, ExprId), Span>,
    preserved_direct_lambda_calls: &[DirectCallSite],
    total_foreign: &FxHashSet<ItemId>,
    udt_metadata: Rc<UdtMetadata>,
) -> CollectedCallSites {
    let package = store.get(package_id);
    let mut call_sites = Vec::new();
    let mut direct_call_sites = Vec::new();
    let mut unresolved_direct_call_sites = Vec::new();
    let mut lattice_states: LatticeStates = FxHashMap::default();
    let clone_items = Rc::new(specialized_items.clone());

    for &store_id in reachable {
        let body_pkg_id = store_id.package;
        let body_pkg = store.get(body_pkg_id);
        let item = body_pkg.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            // Foreign bodies restrict callee shapes; entry-package bodies also
            // analyze computed callees such as blocks and conditionals.
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
                preserved_direct_lambda_calls,
                record_direct_calls,
                total_foreign,
            };
            let locals = build_callable_flow_state(
                body_pkg,
                store,
                &decl.implementation,
                decl.input,
                if specialized_items.contains(&store_id) {
                    CaptureScope::CloneScope(store_id.item)
                } else {
                    CaptureScope::Callable(store_id.item)
                },
                Rc::clone(&clone_items),
                Rc::clone(&udt_metadata),
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
            owner: CaptureScope::Entry,
            clone_items,
            udt_metadata,
            callable: FxHashMap::default(),
            callable_sources: FxHashMap::default(),
            evaluated_callables: FxHashMap::default(),
            evaluated_selections: FxHashMap::default(),
            exprs: FxHashMap::default(),
            condition_substitutions: FxHashMap::default(),
            caller: None,
            active_returns: Rc::default(),
            visible_bindings: FxHashSet::default(),
            mutable_bindings: FxHashSet::default(),
            closure_capturable_var_types: FxHashMap::default(),
        };
        let mut recorder = CallRecorder {
            hof_params,
            call_sites: &mut call_sites,
            direct_call_sites: &mut direct_call_sites,
            unresolved_direct_call_sites: &mut unresolved_direct_call_sites,
            collapsed_spans,
            preserved_direct_lambda_calls,
            record_direct_calls: true,
            total_foreign,
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

    CollectedCallSites {
        call_sites,
        direct_call_sites,
        unresolved_direct_call_sites,
        lattice_states,
    }
}

/// Returns whether every capture operand a rewrite would splice for `callable`
/// has an admissible owner and stable local bindings at the call site.
///
/// A capture that carries its own initializer expression is materialized from
/// that expression, so only a bare capture variable has to be in scope. The
/// value of a closure can be known interprocedurally, through a parameter of
/// the enclosing callable, while its captured locals stay behind in the caller;
/// splicing them here would emit references no scope binds.
///
/// This checks scope and binding stability, not expression purity or the
/// preservation of evaluation timing.
fn closure_captures_can_be_replayed(
    pkg: &Package,
    callable: &ConcreteCallable,
    locals: &LocalState,
) -> bool {
    let ConcreteCallable::Closure { captures, .. } = callable else {
        return true;
    };
    captures_can_be_replayed(pkg, captures, locals)
}

fn captures_can_be_replayed(pkg: &Package, captures: &[CapturedVar], locals: &LocalState) -> bool {
    if captures.is_empty() {
        return true;
    }
    // A factory snapshots its operands when called. Replaying a mutable caller
    // local at the eventual invocation could observe a different value. Leave
    // those closures intact for downstream evaluation rather than reconstructing
    // their environments from reads that are merely in scope, not stable.
    let stable_bindings: FxHashSet<_> = locals
        .visible_bindings
        .iter()
        .filter(|var| locals.closure_capturable_var_types.contains_key(var))
        .copied()
        .collect();
    captures.iter().all(|capture| {
        if capture.local.scope != locals.owner {
            return false;
        }
        capture.expr.map_or_else(
            || stable_bindings.contains(&capture.local.var),
            |expr| {
                capture_expr_is_in_scope(pkg, expr, &stable_bindings, &capture.caller_substitutions)
            },
        )
    })
}

fn capture_expr_is_in_scope(
    pkg: &Package,
    expr_id: ExprId,
    visible_bindings: &FxHashSet<LocalVarId>,
    substitutions: &[CaptureSubstitution],
) -> bool {
    if substitutions.iter().any(|substitution| {
        !capture_expr_is_in_scope(
            pkg,
            substitution.expr,
            visible_bindings,
            &substitution.substitutions,
        )
    }) {
        return false;
    }
    let mut bound = visible_bindings.clone();
    bound.extend(substitutions.iter().map(|substitution| substitution.local));
    expr_is_in_scope(pkg, expr_id, &bound)
}

fn expr_is_in_scope(pkg: &Package, expr_id: ExprId, bound: &FxHashSet<LocalVarId>) -> bool {
    let mut checker = CaptureExprScopeChecker {
        package: pkg,
        bound: bound.clone(),
        valid: true,
    };
    checker.visit_expr(expr_id);
    checker.valid
}

struct CaptureExprScopeChecker<'a> {
    package: &'a Package,
    bound: FxHashSet<LocalVarId>,
    valid: bool,
}

impl<'a> Visitor<'a> for CaptureExprScopeChecker<'a> {
    fn visit_block(&mut self, id: BlockId) {
        let outer_bound = self.bound.clone();
        visit::walk_block(self, id);
        self.bound = outer_bound;
    }

    fn visit_stmt(&mut self, id: StmtId) {
        if let StmtKind::Local(_, pat, expr) = self.package.get_stmt(id).kind {
            self.visit_expr(expr);
            collect_pat_local_bindings(self.package, pat, &mut self.bound);
        } else {
            visit::walk_stmt(self, id);
        }
    }

    fn visit_expr(&mut self, id: ExprId) {
        if !self.valid {
            return;
        }
        match &self.package.get_expr(id).kind {
            ExprKind::Var(Res::Local(var), _) if !self.bound.contains(var) => {
                self.valid = false;
                return;
            }
            ExprKind::Closure(captures, _)
                if captures.iter().any(|var| !self.bound.contains(var)) =>
            {
                self.valid = false;
                return;
            }
            _ => {}
        }
        visit::walk_expr(self, id);
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
    collapsed_spans: &FxHashMap<(PackageId, ExprId), Span>,
    preserved_direct_lambda_calls: &[DirectCallSite],
    record_direct_calls: bool,
    total_foreign: &FxHashSet<ItemId>,
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
            total_foreign,
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
    // Closure, local, and field-projection callees still need analysis there.
    // Previously rewritten lifted-lambda calls are retained too, because their
    // occurrence-local capture operands must survive subsequent iterations.
    // Peel functor wrappers before applying this shape filter.
    if !record_direct_calls {
        let (base_id, _) = peel_body_functors(pkg, *callee_expr_id);
        if !matches!(
            pkg.get_expr(base_id).kind,
            ExprKind::Closure(_, _) | ExprKind::Var(Res::Local(_), _) | ExprKind::Field(_, _)
        ) && !is_preserved_direct_lifted_lambda_call(
            store,
            pkg,
            expr_id,
            package_id,
            base_id,
            preserved_direct_lambda_calls,
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
        preserved_direct_lambda_calls,
    );
}

/// Returns whether `item_id` names a lifted lambda callable.
///
/// An interpreter line that failed validation leaves its callees resolving to
/// items the store never received, so a missing item answers `false` instead of
/// panicking on lookup.
fn is_lifted_lambda_item(store: &PackageStore, item_id: ItemId) -> bool {
    matches!(
        store.get(item_id.package).get_global(item_id.item),
        Some(Global::Callable(decl)) if decl.name.name.starts_with(".lambda")
    )
}

/// Returns whether a previously rewritten closure call is now a literal lifted
/// lambda item. Foreign bodies otherwise skip direct item calls, but this
/// occurrence needs its retained operands reattached.
fn is_preserved_direct_lifted_lambda_call(
    store: &PackageStore,
    pkg: &Package,
    call_expr_id: ExprId,
    package_id: PackageId,
    callee_expr_id: ExprId,
    preserved_direct_lambda_calls: &[DirectCallSite],
) -> bool {
    let ExprKind::Var(Res::Item(item_id), _) = pkg.get_expr(callee_expr_id).kind else {
        return false;
    };
    is_lifted_lambda_item(store, item_id)
        && preserved_direct_lambda_calls.iter().any(|site| {
            if site.call_expr_id != call_expr_id || site.call_pkg_id != package_id {
                return false;
            }
            match &site.callable {
                ConcreteCallable::Closure { target, .. } => *target == item_id.item,
                ConcreteCallable::Global {
                    item_id: prior_item_id,
                    ..
                } => *prior_item_id == item_id,
                ConcreteCallable::Dynamic => false,
            }
        })
}

/// Whether discarding or simplifying a callable selection can erase its guards.
///
/// Replay is a value-reconstruction strategy, not an effect proof: lattice
/// joins may collapse identical alternatives and omit their conditions. Require
/// every condition to be discardable regardless of the resolved target count.
/// Binding cleanup and argument admission must use the same rule.
pub(super) fn callable_selection_guards_are_discardable(
    pkg: &Package,
    package_id: PackageId,
    expr_id: ExprId,
    total_foreign: &FxHashSet<ItemId>,
) -> bool {
    let mut discardable = true;
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_, expr| {
        if let ExprKind::If(condition, _, _) = expr.kind {
            discardable &= expr_is_safe_to_discard_with_total_foreign(
                pkg,
                package_id,
                condition,
                total_foreign,
            );
        }
    });
    discardable
}

/// Records a [`CallSite`] for every arrow parameter of a resolved HOF callee.
///
/// For each callable parameter of the HOF, the argument at the parameter's
/// input path is resolved to its reaching-definitions lattice: a single
/// concrete callable yields one unconditional call site, a `Multi` lattice
/// yields one conditioned call site per candidate (the branch-split set), and a
/// dynamic or bottom lattice yields a dynamic call site for downstream
/// capability analysis and partial evaluation.
///
/// Specializing a call site removes callable-valued fields while the shared
/// argument builder reconstructs the surviving input and appends captures.
/// A valid input layout alone does not authorize deleting observable evaluation. The
/// disposition decision in [`super::rewrite::consumed_callable_expr_disposition`]
/// is therefore applied here, once, before the call site is accepted: an
/// argument classified [`EvaluationDisposition::Retained`] is declined to
/// `ConcreteCallable::Dynamic`, the pass's established "cannot specialize"
/// signal, which keeps the original dynamic dispatch for downstream resolution
/// instead of deleting observable evaluation.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
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
    total_foreign: &FxHashSet<ItemId>,
) {
    let uses_tuple_input = hof_uses_tuple_input_pattern(store, hof_store_id);
    for cp in hof_callable_params {
        let input_path = super::build_param_input_path(uses_tuple_input, cp, hof_functor);
        let resolved_arg_id = extract_arg_at_path(pkg, store, args_expr_id, &input_path);
        let allow_scoped_capture_exprs = matches!(
            pkg.get_expr(resolved_arg_id).kind,
            ExprKind::Block(_) | ExprKind::If(_, _, _)
        );
        let resolved = if consumed_callable_expr_disposition(
            pkg,
            package_id,
            resolved_arg_id,
            ConsumptionSite::Argument,
            total_foreign,
        ) == EvaluationDisposition::Retained
            || !callable_selection_guards_are_discardable(
                pkg,
                package_id,
                resolved_arg_id,
                total_foreign,
            ) {
            // Rewriting would delete this expression outright, and its
            // evaluation is observable with nothing to reproduce it. Decline.
            CalleeLattice::Dynamic
        } else if let Some(evaluated) = evaluated_callable(locals, resolved_arg_id) {
            evaluated
        } else {
            resolve_callee_at_path(
                pkg,
                store,
                locals,
                args_expr_id,
                &input_path,
                0,
                allow_scoped_capture_exprs,
                &FxHashSet::default(),
                package_id,
            )
        };
        let mut record_dynamic_call_site = || {
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
        };
        match without_stale_guards(locals, resolved) {
            CalleeLattice::Single(cc) if closure_captures_can_be_replayed(pkg, &cc, locals) => {
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
                if candidates
                    .iter()
                    .any(|(cc, _)| !closure_captures_can_be_replayed(pkg, cc, locals))
                {
                    record_dynamic_call_site();
                } else {
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
            }
            CalleeLattice::Dynamic | CalleeLattice::Bottom | CalleeLattice::Single(_) => {
                record_dynamic_call_site();
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
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
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
    collapsed_spans: &FxHashMap<(PackageId, ExprId), Span>,
    preserved_direct_lambda_calls: &[DirectCallSite],
) {
    let callee_expr = pkg.get_expr(callee_expr_id);
    if let ExprKind::Var(Res::Item(item_id), _) = callee_expr.kind {
        record_preserved_direct_lifted_lambda_calls(
            store,
            expr_id,
            package_id,
            item_id,
            preserved_direct_lambda_calls,
            direct_call_sites,
        );
        return;
    }

    let (callee_base_id, _) = peel_body_functors(pkg, callee_expr_id);
    let callee_local_var =
        if let ExprKind::Var(Res::Local(var), _) = pkg.get_expr(callee_base_id).kind {
            Some(var)
        } else {
            None
        };

    let (resolved, def_span) = if let Some(evaluated) = evaluated_callable(locals, callee_expr_id) {
        let def_span = callee_local_var
            .and_then(|var| locals.exprs.get(&var))
            .and_then(|initializer| collapsed_spans.get(&(package_id, *initializer)))
            .copied();
        (evaluated, def_span)
    } else if let ExprKind::Var(Res::Local(var), _) = callee_expr.kind {
        if let Some(&init_expr_id) = locals.exprs.get(&var) {
            (
                locals.callable.get(&var).cloned().unwrap_or_else(|| {
                    resolve_callee(
                        pkg,
                        store,
                        locals,
                        init_expr_id,
                        0,
                        true,
                        &FxHashSet::default(),
                        package_id,
                    )
                }),
                collapsed_spans.get(&(package_id, init_expr_id)).copied(),
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

    match without_stale_guards(locals, resolved) {
        CalleeLattice::Single(callable) => {
            let Some(captures) = resolve_direct_call_captures(
                pkg,
                store,
                locals,
                callee_expr_id,
                &callable,
                package_id,
            ) else {
                unresolved_direct_call_sites.push((package_id, expr_id).into());
                return;
            };
            direct_call_sites.push(DirectCallSite {
                call_expr_id: expr_id,
                call_pkg_id: package_id,
                callable,
                captures,
                condition: vec![],
                def_span,
            });
        }
        CalleeLattice::Multi(candidates) => {
            let mut resolved_candidates = Vec::with_capacity(candidates.len());
            for (callable, condition) in candidates {
                let Some(captures) = resolve_direct_call_captures(
                    pkg,
                    store,
                    locals,
                    callee_expr_id,
                    &callable,
                    package_id,
                ) else {
                    unresolved_direct_call_sites.push((package_id, expr_id).into());
                    return;
                };
                resolved_candidates.push((callable, captures, condition));
            }
            for (callable, captures, condition) in resolved_candidates {
                direct_call_sites.push(DirectCallSite {
                    call_expr_id: expr_id,
                    call_pkg_id: package_id,
                    callable,
                    captures,
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
            let owner = match locals.owner {
                CaptureScope::Callable(item) | CaptureScope::CloneScope(item) => {
                    Some(StoreItemId::from((package_id, item)))
                }
                CaptureScope::Entry => None,
            };
            let callee_is_hof_param =
                owner
                    .and_then(|owner| hof_params.get(&owner))
                    .is_some_and(|params| {
                        callee_local_var
                            .is_some_and(|var| params.iter().any(|param| param.param_var == var))
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

fn resolve_direct_call_captures(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    callee_expr_id: ExprId,
    callable: &ConcreteCallable,
    package_id: PackageId,
) -> Option<Vec<CapturedVar>> {
    if !closure_captures_can_be_replayed(pkg, callable, locals) {
        return None;
    }
    let captures = resolve_direct_lifted_lambda_captures(
        pkg,
        store,
        locals,
        callee_expr_id,
        callable,
        package_id,
    )?;
    captures_can_be_replayed(pkg, &captures, locals).then_some(captures)
}

/// Recovers a lifted lambda's partial-application operands before
/// rewrite destroys the local factory-result occurrence.
pub(super) fn resolve_direct_lifted_lambda_captures(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    callee_expr_id: ExprId,
    callable: &ConcreteCallable,
    package_id: PackageId,
) -> Option<Vec<CapturedVar>> {
    let ConcreteCallable::Global { item_id, .. } = callable else {
        return Some(Vec::new());
    };
    if !is_lifted_lambda_item(store, *item_id) {
        return Some(Vec::new());
    }

    resolve_lifted_lambda_captures_from_expr(
        pkg,
        store,
        locals,
        callee_expr_id,
        item_id.item,
        package_id,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_lifted_lambda_captures_from_expr(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    expr_id: ExprId,
    target: LocalItemId,
    package_id: PackageId,
    depth: usize,
) -> Option<Vec<CapturedVar>> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    match pkg.get_expr(expr_id).kind {
        ExprKind::Var(Res::Local(var), _) => locals.exprs.get(&var).and_then(|init_expr_id| {
            resolve_lifted_lambda_captures_from_expr(
                pkg,
                store,
                locals,
                *init_expr_id,
                target,
                package_id,
                depth + 1,
            )
        }),
        ExprKind::Call(..) => match resolve_callee(
            pkg,
            store,
            locals,
            expr_id,
            0,
            true,
            &FxHashSet::default(),
            package_id,
        ) {
            CalleeLattice::Single(ConcreteCallable::Closure {
                target: closure_target,
                captures,
                ..
            }) if closure_target == target => Some(captures),
            _ => None,
        },
        ExprKind::Return(inner) | ExprKind::UnOp(_, inner) => {
            resolve_lifted_lambda_captures_from_expr(
                pkg,
                store,
                locals,
                inner,
                target,
                package_id,
                depth + 1,
            )
        }
        _ => None,
    }
}

/// Rehydrates operands from the previous iteration after rewrite replaced a
/// closure callee occurrence with its lifted lambda item.
fn record_preserved_direct_lifted_lambda_calls(
    store: &PackageStore,
    call_expr_id: ExprId,
    package_id: PackageId,
    item_id: ItemId,
    preserved_direct_lambda_calls: &[DirectCallSite],
    direct_call_sites: &mut Vec<DirectCallSite>,
) {
    if !is_lifted_lambda_item(store, item_id) {
        return;
    }

    for prior_site in preserved_direct_lambda_calls {
        let (target, captures, functor) = match &prior_site.callable {
            ConcreteCallable::Closure {
                target,
                captures,
                functor,
            } => (*target, captures, *functor),
            ConcreteCallable::Global {
                item_id: prior_item_id,
                functor,
            } if *prior_item_id == item_id => (prior_item_id.item, &prior_site.captures, *functor),
            ConcreteCallable::Global { .. } | ConcreteCallable::Dynamic => continue,
        };
        if prior_site.call_expr_id == call_expr_id
            && prior_site.call_pkg_id == package_id
            && (target == item_id.item
                || matches!(
                    &prior_site.callable,
                    ConcreteCallable::Global { item_id: prior_item_id, .. } if *prior_item_id == item_id
                ))
        {
            direct_call_sites.push(DirectCallSite {
                call_expr_id,
                call_pkg_id: package_id,
                callable: ConcreteCallable::Global { item_id, functor },
                captures: captures.clone(),
                condition: prior_site.condition.clone(),
                def_span: prior_site.def_span,
            });
        }
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

/// Returns `true` when the HOF input is a `PatKind::Tuple`, rather than a single
/// binding whose type happens to be a tuple. This determines whether argument
/// paths include a top-level parameter slot before the nested field path.
fn hof_uses_tuple_input_pattern(store: &PackageStore, hof_store_id: StoreItemId) -> bool {
    let hof_pkg = store.get(hof_store_id.package);
    let hof_item = hof_pkg.get_item(hof_store_id.item);
    match &hof_item.kind {
        ItemKind::Callable(decl) => matches!(hof_pkg.get_pat(decl.input).kind, PatKind::Tuple(_)),
        ItemKind::Ty(..) => false,
    }
}

/// Extracts the argument expression at the given relative field path from an
/// already-selected outer call argument. Explicit struct fields and type
/// constructors expose the consumed callable independently of retained sibling
/// operands, whose evaluation is preserved by argument reconstruction.
fn extract_arg_at_path(
    pkg: &Package,
    store: &PackageStore,
    args_expr_id: ExprId,
    path: &[usize],
) -> ExprId {
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
            Some(&element_id) => extract_arg_at_path(pkg, store, element_id, &path[1..]),
            None => args_expr_id,
        }
    } else if let ExprKind::Struct(_, _, fields) = &args_expr.kind {
        fields
            .iter()
            .find_map(|field| {
                matches!(&field.field, Field::Path(field_path) if field_path.indices == [path[0]])
                    .then_some(field.value)
            })
            .map_or(args_expr_id, |field| {
                extract_arg_at_path(pkg, store, field, &path[1..])
            })
    } else if let ExprKind::Call(callee, args) = args_expr.kind
        && is_type_constructor(pkg, store, callee)
    {
        extract_arg_at_path(pkg, store, args, path)
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
            && let Some(candidates) = resolve_array_callable_candidates(
                pkg,
                store,
                locals,
                args_expr_id,
                &[],
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
    if let Some((field_value_id, field_locals)) =
        resolve_struct_field(pkg, store, locals, args_expr_id, &field_path, 0)
    {
        if matches!(pkg.get_expr(field_value_id).ty, Ty::Array(_))
            && let Some(candidates) = resolve_array_callable_candidates(
                pkg,
                store,
                field_locals,
                field_value_id,
                &[],
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
            field_locals,
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
/// locals and flow-sensitive callable bindings, resolving if-value-expressions,
/// recognizing closures and global item references, and tracing callable returns
/// up to a recursion depth limit.
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

    // Composite expressions must use the values of their evaluated children,
    // not resolve those reads again in the state after later operands.
    if let Some(evaluated) = evaluated_callable(locals, expr_id) {
        return evaluated;
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
            } else if let Some((init_expr_id, init_locals)) = local_initializer(locals, *var) {
                // Fallback to immutable ExprId bindings (struct fields, etc.).
                resolve_callee(
                    pkg,
                    store,
                    init_locals,
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
                        expr_id,
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
            if let Some((elem_expr_id, elem_locals)) = resolve_indexed_array_element(
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
                    elem_locals,
                    elem_expr_id,
                    depth + 1,
                    allow_scoped_capture_exprs,
                    scoped_capture_vars,
                    package_id,
                )
            } else if let Some(candidates) = resolve_array_callable_candidates(
                pkg,
                store,
                locals,
                *array_expr_id,
                &[],
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
            let Some((tail, block_state, block_scoped_vars)) = analyze_block_result(
                pkg,
                store,
                locals,
                *block_id,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            ) else {
                return CalleeLattice::Dynamic;
            };
            resolve_callee(
                pkg,
                store,
                &block_state,
                tail,
                depth + 1,
                allow_scoped_capture_exprs,
                &block_scoped_vars,
                package_id,
            )
        }
        ExprKind::Field(inner_expr_id, Field::Path(path)) => {
            if let Some((field_value_id, field_locals)) =
                resolve_struct_field(pkg, store, locals, *inner_expr_id, path, depth + 1)
            {
                resolve_callee(
                    pkg,
                    store,
                    field_locals,
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
    without_stale_guards(
        locals,
        apply_outer_functor_lattice(base_resolved, outer_functor),
    )
}

/// Replays a block without recording calls, preserving occurrence snapshots and
/// the caller environment. Its tail must be resolved against this returned state.
fn analyze_block_result(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    block_id: BlockId,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> Option<(ExprId, LocalState, FxHashSet<LocalVarId>)> {
    let mut state = locals.clone();
    analyze_block_flow(pkg, store, block_id, &mut state, package_id, None);
    let mut scoped_vars = scoped_capture_vars.clone();
    if allow_scoped_capture_exprs {
        collect_block_local_bindings(pkg, block_id, &mut scoped_vars);
    }
    let stmt = pkg.get_stmt(*pkg.get_block(block_id).stmts.last()?);
    match stmt.kind {
        StmtKind::Expr(tail) | StmtKind::Semi(tail) => Some((tail, state, scoped_vars)),
        _ => None,
    }
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
/// Unlike `resolve_callee`'s `If` arm (which folds a parameter guard when its
/// caller argument resolves to a boolean literal), this function joins both
/// branches without folding.
/// Identical alternatives can stay `Single`; distinct alternatives become
/// guarded `Multi` entries, or `Dynamic` when unsupported. Folding would leave the unselected branch's
/// closure target unregistered for specialization, and
/// `cleanup_consumed_closures` would be unable to neutralize the surviving
/// `ExprKind::Closure` node, breaking fixpoint convergence. When replay is
/// admissible, rewrite materializes guarded dispatch; the guards are constant
/// only when parameter substitution supplies constant expressions.
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
    if depth > MAX_RESOLVE_DEPTH || !selection_can_be_replayed(locals, expr_id) {
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
        ExprKind::Index(array, _) => resolve_array_callable_candidates(
            pkg,
            store,
            locals,
            *array,
            path,
            depth + 1,
            allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        )
        .map_or(CalleeLattice::Dynamic, |candidates| {
            CalleeLattice::Multi(
                candidates
                    .into_iter()
                    .map(|callable| (callable, vec![]))
                    .collect(),
            )
        }),
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
            if let Some((init_expr_id, init_locals)) = local_initializer(locals, *var) {
                resolve_callee_projection(
                    pkg,
                    store,
                    init_locals,
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
            let Some((tail, block_state, block_scoped_vars)) = analyze_block_result(
                pkg,
                store,
                locals,
                *block_id,
                allow_scoped_capture_exprs,
                scoped_capture_vars,
                package_id,
            ) else {
                return CalleeLattice::Dynamic;
            };
            resolve_callee_projection(
                pkg,
                store,
                &block_state,
                tail,
                path,
                depth + 1,
                allow_scoped_capture_exprs,
                &block_scoped_vars,
                package_id,
            )
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
            // fail. The join retains both targets for guarded dispatch when
            // replay is admissible.
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
            without_stale_guards(
                locals,
                true_res.join_with_condition(false_res, remap_condition_expr(pkg, locals, *cond)),
            )
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
                            expr_id,
                            *args_expr_id,
                            path,
                            depth + 1,
                            allow_scoped_capture_exprs,
                            scoped_capture_vars,
                            package_id,
                        ),
                        // This projection fallback handles only local UDT
                        // constructors. The resolve_struct_field fast path can
                        // also expose fields of literal foreign constructors.
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
///
/// Unguarded UDT recursion; terminates only because the frontend rejects cyclic UDTs.
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
/// invoked at a call site by analyzing its body and tracing its final expression
/// or explicit return to a concrete callable.
///
/// The callee's body is read from its owning package (`item_id.package`), while
/// the call arguments and caller lattice come from the caller's package
/// (`pkg` / `package_id`). Same-package calls can substitute caller expressions
/// for parameters. Across packages, raw expression IDs are not seeded into the
/// callee's state, and returned closures are declined because their targets and
/// capture expressions are package-local; global callable identities can cross
/// that boundary.
///
/// # The effectful-producer decline
///
/// Reconstructing a returned closure's captures or a guarded result's selection
/// can duplicate evaluation if the producing call must remain for its effects
/// or possible failure. Guarded direct dispatch can also omit producer effects
/// that do not appear in its guards. `call_expr_id` identifies that call;
/// [`expr_is_safe_to_discard`] proves when reconstruction is admissible.
///
/// Otherwise the result remains `CalleeLattice::Dynamic` for downstream
/// analysis. Cleanup separately protects reachable producers; that liveness
/// protection does not make replaying their capture expressions safe. Functions
/// as well as operations can require retention, for example because they log
/// or fail.
///
/// The discard proof sees through the named total intrinsics
/// ([`collect_total_foreign_callables`]) but not through arbitrary foreign
/// bodies, so a producing call that reaches a cross-package non-intrinsic
/// callable is declined rather than proven. That is conservative in the safe
/// direction, and a cross-package producer cannot yield a threadable closure
/// anyway (see [`downgrade_closures_to_dynamic`]).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn resolve_callable_return(
    pkg: &Package,
    store: &PackageStore,
    caller_locals: &LocalState,
    item_id: ItemId,
    call_expr_id: ExprId,
    args_expr_id: ExprId,
    output_path: &[usize],
    depth: usize,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> CalleeLattice {
    let producer = StoreItemId::from((item_id.package, item_id.item));
    if caller_locals.active_returns.contains(&producer)
        || caller_locals.active_returns.len() >= MAX_RESOLVE_DEPTH
    {
        return CalleeLattice::Dynamic;
    }
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
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {
            return CalleeLattice::Dynamic;
        }
    };

    let mut state = LocalState {
        owner: if caller_locals
            .clone_items
            .contains(&StoreItemId::from((item_id.package, item_id.item)))
        {
            CaptureScope::CloneScope(item_id.item)
        } else {
            CaptureScope::Callable(item_id.item)
        },
        clone_items: Rc::clone(&caller_locals.clone_items),
        udt_metadata: Rc::clone(&caller_locals.udt_metadata),
        callable: FxHashMap::default(),
        callable_sources: FxHashMap::default(),
        evaluated_callables: FxHashMap::default(),
        evaluated_selections: FxHashMap::default(),
        exprs: FxHashMap::default(),
        condition_substitutions: FxHashMap::default(),
        caller: (callee_pkg_id == package_id).then(|| Rc::new(caller_locals.clone())),
        active_returns: {
            let mut active = caller_locals.active_returns.as_ref().clone();
            active.insert(producer);
            Rc::new(active)
        },
        visible_bindings: {
            let mut bindings = FxHashSet::default();
            collect_pat_local_bindings(callee_pkg, body_input, &mut bindings);
            bindings
        },
        mutable_bindings: FxHashSet::default(),
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
        callee_pkg_id,
    );
    // Capture reconstruction stops at the parameter boundary, just as ordinary
    // value resolution switches to the caller's environment there.
    let param_substitutions = state.condition_substitutions.clone();
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
        caller_locals.owner,
        &caller_locals.mutable_bindings,
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

    // Array alternatives need the producer's selected index. A caller can
    // replay a guarded decision tree, but not several unconditional candidates.
    if matches!(&result, CalleeLattice::Multi(entries)
        if entries.iter().filter(|(_, guards)| guards.is_empty()).count() > 1)
    {
        return CalleeLattice::Dynamic;
    }

    if callee_pkg_id == package_id {
        return decline_effectful_producer_replay(pkg, store, package_id, call_expr_id, result);
    }

    // A callable returned from a foreign body is consumed at the caller's call
    // site. A single `Global` carries its package, but dispatch guards and
    // indexed alternatives still belong to the producer's expression graph.
    // Neither those discriminators nor a closure's local target/environment
    // can cross packages without relocation.
    if matches!(result, CalleeLattice::Multi(_)) {
        return CalleeLattice::Dynamic;
    }
    downgrade_closures_to_dynamic(result)
}

/// Declines capture or guarded-dispatch reconstruction for an observable producer.
///
/// See the "effectful-producer decline" section on [`resolve_callable_return`]
/// for why this is the deciding question. An unguarded `Global` names an item
/// independently of the producer, so its direct rewrite can retain the original
/// evaluation without reconstructing captures or replaying a branch decision.
/// A `Multi` still requires this proof even when every target is global.
///
/// The total-intrinsic set is collected here rather than threaded in from the
/// pass driver because every function between `analyze` and this point would
/// otherwise gain a parameter it does not use. The collection walks item
/// headers only — [`crate::walk_utils::extend_with_discardable_foreign_callables`], which walks
/// foreign bodies, is deliberately not run — and it runs only for a lattice
/// that requires capture or guarded-dispatch reconstruction.
fn decline_effectful_producer_replay(
    pkg: &Package,
    store: &PackageStore,
    package_id: PackageId,
    call_expr_id: ExprId,
    resolved: CalleeLattice,
) -> CalleeLattice {
    if !lattice_has_closure(&resolved) && !matches!(resolved, CalleeLattice::Multi(_)) {
        return resolved;
    }
    if expr_is_safe_to_discard(pkg, package_id, call_expr_id) {
        return resolved;
    }
    let total_foreign = collect_total_foreign_callables(store);
    if expr_is_safe_to_discard_with_total_foreign(pkg, package_id, call_expr_id, &total_foreign) {
        return resolved;
    }
    CalleeLattice::Dynamic
}

/// Reports whether a lattice element carries at least one closure candidate.
fn lattice_has_closure(lattice: &CalleeLattice) -> bool {
    let is_closure = |cc: &ConcreteCallable| matches!(cc, ConcreteCallable::Closure { .. });
    match lattice {
        CalleeLattice::Single(cc) => is_closure(cc),
        CalleeLattice::Multi(entries) => entries.iter().any(|(cc, _)| is_closure(cc)),
        CalleeLattice::Dynamic | CalleeLattice::Bottom => false,
    }
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
                    resolve_condition_substitution_literal(
                        pkg,
                        locals.caller.as_deref()?,
                        expr_id,
                        depth + 1,
                    )
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
        ExprKind::Var(Res::Local(var), _) => {
            local_initializer(locals, *var).and_then(|(expr_id, init_locals)| {
                resolve_condition_substitution_literal(pkg, init_locals, expr_id, depth + 1)
            })
        }
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

fn selection_can_be_replayed(locals: &LocalState, expr: ExprId) -> bool {
    locals.evaluated_selections.get(&expr) != Some(&false)
}

fn evaluated_callable(locals: &LocalState, expr: ExprId) -> Option<CalleeLattice> {
    if !selection_can_be_replayed(locals, expr) {
        return Some(CalleeLattice::Dynamic);
    }
    locals
        .evaluated_callables
        .get(&expr)
        .map(|value| without_stale_guards(locals, value.clone()))
}

fn without_stale_guards(locals: &LocalState, resolved: CalleeLattice) -> CalleeLattice {
    if matches!(&resolved, CalleeLattice::Multi(entries) if entries.iter().any(|(_, guards)| {
        guards.iter().any(|&guard| !selection_can_be_replayed(locals, guard))
    })) {
        CalleeLattice::Dynamic
    } else {
        resolved
    }
}

/// Materializes `CapturedVar::expr` fields for each capture appearing in a
/// `CalleeLattice` by resolving the capture's defining expression in the
/// callee's analyzed `LocalState`, substituting producing-function parameters
/// with the caller-scope argument expressions in `param_substitutions`, so
/// rewrite can re-emit the captures as caller-scope arguments.
/// An unresolved capture makes the entire lattice dynamic. Keeping an unknown
/// inside `Single` or `Multi` would let specialization discard that choice and
/// incorrectly retain only a known target from another branch.
fn materialize_capture_exprs_from_state(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    caller_owner: CaptureScope,
    caller_mutable_bindings: &FxHashSet<LocalVarId>,
    resolved: CalleeLattice,
) -> CalleeLattice {
    let materialize = |concrete| {
        materialize_capture_exprs_in_callable(
            pkg,
            state,
            param_substitutions,
            caller_owner,
            caller_mutable_bindings,
            concrete,
        )
    };
    match resolved {
        CalleeLattice::Single(concrete) => CalleeLattice::from_concrete(materialize(concrete)),
        CalleeLattice::Multi(entries) => {
            let mut materialized = Vec::with_capacity(entries.len());
            for (concrete, condition) in entries {
                // Joins already substitute producer guards into caller scope.
                // Reapplying the producer map would reinterpret caller local IDs.
                if condition
                    .iter()
                    .any(|&guard| expr_contains_local_reference(pkg, guard))
                {
                    return CalleeLattice::Dynamic;
                }
                let concrete = materialize(concrete);
                if matches!(concrete, ConcreteCallable::Dynamic) {
                    return CalleeLattice::Dynamic;
                }
                materialized.push((concrete, condition));
            }
            CalleeLattice::Multi(materialized)
        }
        other => other,
    }
}

fn expr_contains_local_reference(pkg: &Package, expr_id: ExprId) -> bool {
    let mut contains_local = false;
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_expr_id, expr| {
        if matches!(expr.kind, ExprKind::Var(Res::Local(_), _))
            || matches!(&expr.kind, ExprKind::Closure(captures, _) if !captures.is_empty())
        {
            contains_local = true;
        }
    });
    contains_local
}

fn expr_reads_mutable_local(
    pkg: &Package,
    expr_id: ExprId,
    substitutions: &[CaptureSubstitution],
    caller_mutable_bindings: &FxHashSet<LocalVarId>,
) -> bool {
    caller_mutable_bindings
        .iter()
        .any(|&local| capture_expr_reads_local(pkg, expr_id, substitutions, local))
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
    caller_owner: CaptureScope,
    caller_mutable_bindings: &FxHashSet<LocalVarId>,
    concrete: ConcreteCallable,
) -> ConcreteCallable {
    match concrete {
        ConcreteCallable::Closure {
            target,
            mut captures,
            functor,
        } => {
            for capture in &mut captures {
                if capture.local.scope != state.owner {
                    continue;
                }
                if let Some(expr) = capture.expr {
                    if !rebind_capture_expression(
                        pkg,
                        state,
                        param_substitutions,
                        expr,
                        &mut capture.caller_substitutions,
                    ) {
                        return ConcreteCallable::Dynamic;
                    }
                } else {
                    let Some(resolved) = resolve_capture_to_caller(
                        pkg,
                        state,
                        param_substitutions,
                        capture.local.var,
                    ) else {
                        continue;
                    };
                    match resolved {
                        ResolvedCaptureExpr::Caller(expr) => {
                            if capture_expr_contains_operation_call(pkg, expr) {
                                return ConcreteCallable::Dynamic;
                            }
                            capture.expr = Some(expr);
                        }
                        ResolvedCaptureExpr::Producer(expr) => {
                            if !rebind_capture_expression(
                                pkg,
                                state,
                                param_substitutions,
                                expr,
                                &mut capture.caller_substitutions,
                            ) {
                                return ConcreteCallable::Dynamic;
                            }
                            capture.expr = Some(expr);
                        }
                    }
                }
                if capture.expr.is_some_and(|expr| {
                    expr_reads_mutable_local(
                        pkg,
                        expr,
                        &capture.caller_substitutions,
                        caller_mutable_bindings,
                    )
                }) {
                    return ConcreteCallable::Dynamic;
                }
                capture.local.scope = caller_owner;
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

/// Extends only the current caller's leaves. The original expression and earlier
/// substitutions still belong to their own scopes, even if local IDs coincide.
fn rebind_capture_expression(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    expr: ExprId,
    substitutions: &mut Vec<CaptureSubstitution>,
) -> bool {
    if substitutions.is_empty() {
        let Some(rebound) =
            collect_compound_capture_substitutions(pkg, state, param_substitutions, expr)
        else {
            return false;
        };
        *substitutions = rebound;
        true
    } else {
        substitutions.iter_mut().all(|substitution| {
            rebind_capture_expression(
                pkg,
                state,
                param_substitutions,
                substitution.expr,
                &mut substitution.substitutions,
            )
        })
    }
}

enum ResolvedCaptureExpr {
    /// A parameter's argument is already in caller scope; do not resolve its
    /// numeric local IDs through the producer's bindings.
    Caller(ExprId),
    /// An initializer still owned by the producer needs substitution first.
    Producer(ExprId),
}

fn capture_expr_contains_operation_call(pkg: &Package, expr: ExprId) -> bool {
    let mut found = false;
    crate::walk_utils::for_each_expr(pkg, expr, &mut |_, expr| {
        if let ExprKind::Call(callee, _) = expr.kind
            && !callee_has_function_kind(pkg, callee)
        {
            found = true;
        }
    });
    found
}

/// Resolves a closure capture variable to an expression, retaining whether that
/// expression belongs to the caller or still needs producer-local substitution.
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
) -> Option<ResolvedCaptureExpr> {
    let mut current = var;
    for _ in 0..MAX_RESOLVE_DEPTH {
        if let Some(&arg_expr_id) = param_substitutions.get(&current) {
            return Some(ResolvedCaptureExpr::Caller(arg_expr_id));
        }
        let &expr_id = state.exprs.get(&current)?;
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Var(Res::Local(next), _) = &expr.kind
            && *next != current
        {
            current = *next;
            continue;
        }
        return Some(ResolvedCaptureExpr::Producer(expr_id));
    }
    None
}

/// Collects the caller-scope substitutions needed to reconstruct a
/// producer-scope compound-literal capture in the caller.
///
/// Uses reconstruction's [`capture_expr_children`] contract. Every producer
/// local must resolve in its own environment; unsupported forms with locals
/// and operation calls are declined, not left for a later residual scan.
/// A failure discards all substitutions collected for this expression.
fn collect_compound_capture_substitutions(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    expr_id: ExprId,
) -> Option<Vec<CaptureSubstitution>> {
    let mut substitutions = Vec::new();
    collect_compound_capture_substitutions_into(
        pkg,
        state,
        param_substitutions,
        expr_id,
        &mut substitutions,
    )?;
    Some(substitutions)
}

/// Recursive worker for [`collect_compound_capture_substitutions`] that walks
/// `expr_id` and appends scoped replacement expressions to `substitutions`.
///
/// Success proves scope and reconstructibility, not that function evaluation
/// is unobservable. The producer-discard proof remains a separate prerequisite.
fn collect_compound_capture_substitutions_into(
    pkg: &Package,
    state: &LocalState,
    param_substitutions: &FxHashMap<LocalVarId, ExprId>,
    expr_id: ExprId,
    substitutions: &mut Vec<CaptureSubstitution>,
) -> Option<()> {
    let expr = pkg.get_expr(expr_id);
    if let ExprKind::Var(Res::Local(var), _) = expr.kind {
        if substitutions
            .iter()
            .any(|substitution| substitution.local == var)
        {
            return Some(());
        }
        let (replacement, nested) =
            match resolve_capture_to_caller(pkg, state, param_substitutions, var)? {
                ResolvedCaptureExpr::Caller(replacement) => {
                    if capture_expr_contains_operation_call(pkg, replacement) {
                        return None;
                    }
                    (replacement, Vec::new())
                }
                ResolvedCaptureExpr::Producer(replacement) if replacement != expr_id => (
                    replacement,
                    collect_compound_capture_substitutions(
                        pkg,
                        state,
                        param_substitutions,
                        replacement,
                    )?,
                ),
                ResolvedCaptureExpr::Producer(_) => return None,
            };
        substitutions.push(CaptureSubstitution {
            local: var,
            expr: replacement,
            substitutions: nested,
        });
        return Some(());
    }
    if let ExprKind::Call(callee, _) = expr.kind
        && !callee_has_function_kind(pkg, callee)
    {
        return None;
    }
    let mut kind = expr.kind.clone();
    if let Some(children) = capture_expr_children(&mut kind) {
        for child in children {
            collect_compound_capture_substitutions_into(
                pkg,
                state,
                param_substitutions,
                *child,
                substitutions,
            )?;
        }
        Some(())
    } else {
        (!expr_contains_local_reference(pkg, expr_id)
            && !capture_expr_contains_operation_call(pkg, expr_id))
        .then_some(())
    }
}

/// Seeds a callable-return producer's parameter bindings at a specific call.
/// Same-package parameters receive caller-expression substitutions, and arrow
/// parameters also receive callable lattices. Across packages, only a single
/// package-qualified global callable is admitted; raw expressions stay unseeded.
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
    hof_package_id: PackageId,
) {
    let pat = hof_package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            // These maps are package-local expression graphs. Foreign operands
            // remain runtime parameters until specialization clones the body
            // into their package; their raw IDs must never enter this graph.
            if caller_package_id == hof_package_id {
                state.condition_substitutions.insert(ident.id, arg_expr_id);
            }
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
                let lattice = if caller_package_id == hof_package_id
                    || matches!(
                        lattice,
                        CalleeLattice::Single(ConcreteCallable::Global { .. })
                    ) {
                    lattice
                } else {
                    // Closures and branch guards carry package-local IDs.
                    CalleeLattice::Dynamic
                };
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
                        hof_package_id,
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
fn resolve_struct_field<'a>(
    pkg: &Package,
    store: &PackageStore,
    locals: &'a LocalState,
    inner_expr_id: ExprId,
    path: &FieldPath,
    depth: usize,
) -> Option<(ExprId, &'a LocalState)> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }
    let inner_expr = pkg.get_expr(inner_expr_id);
    match &inner_expr.kind {
        ExprKind::Tuple(elements) => {
            let (&field_index, rest) = path.indices.split_first()?;
            let &field_expr_id = elements.get(field_index)?;
            if rest.is_empty() {
                Some((field_expr_id, locals))
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
        ExprKind::Struct(_, _, fields) => {
            extract_field_value(fields, path).map(|expr| (expr, locals))
        }
        ExprKind::Call(callee_id, args_id) if is_type_constructor(pkg, store, *callee_id) => {
            resolve_struct_field(pkg, store, locals, *args_id, path, depth + 1)
        }
        ExprKind::Var(Res::Local(var), _) => {
            let (init_id, init_locals) = local_initializer(locals, *var)?;
            resolve_struct_field(pkg, store, init_locals, init_id, path, depth + 1)
        }
        ExprKind::Field(nested_inner_id, Field::Path(nested_path)) => {
            // Two-level field access: resolve the outer field to get the inner
            // struct expression, then resolve the target field within that.
            let (intermediate_id, intermediate_locals) =
                resolve_struct_field(pkg, store, locals, *nested_inner_id, nested_path, depth + 1)?;
            resolve_struct_field(
                pkg,
                store,
                intermediate_locals,
                intermediate_id,
                path,
                depth + 1,
            )
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
fn resolve_indexed_array_element<'a>(
    pkg: &Package,
    store: &PackageStore,
    locals: &'a LocalState,
    array_expr_id: ExprId,
    index_expr_id: ExprId,
    depth: usize,
) -> Option<(ExprId, &'a LocalState)> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let index = resolve_static_int_expr(pkg, locals, index_expr_id, depth + 1)?;
    let (elements, element_locals) =
        resolve_array_elements(pkg, store, locals, array_expr_id, depth + 1)?;
    let index = normalize_index(elements.len(), index)?;
    Some((elements[index], element_locals))
}

/// Resolves one callable at `path` in every array element, retaining physical
/// positions, including duplicates. Ambiguous or unresolved elements, an empty
/// array, or exceeding `MULTI_CAP` cause this helper to return `None`.
#[allow(clippy::too_many_arguments)]
fn resolve_array_callable_candidates(
    pkg: &Package,
    store: &PackageStore,
    locals: &LocalState,
    array_expr_id: ExprId,
    path: &[usize],
    depth: usize,
    allow_scoped_capture_exprs: bool,
    scoped_capture_vars: &FxHashSet<LocalVarId>,
    package_id: PackageId,
) -> Option<Vec<ConcreteCallable>> {
    let (element_expr_ids, element_locals) =
        resolve_array_elements(pkg, store, locals, array_expr_id, depth + 1)?;
    let mut candidates = Vec::new();

    for elem_expr_id in element_expr_ids {
        let elem_allow_scoped_capture_exprs = allow_scoped_capture_exprs
            || matches!(
                pkg.get_expr(elem_expr_id).kind,
                ExprKind::Block(_) | ExprKind::If(_, _, _)
            );
        let resolved = resolve_callee_projection(
            pkg,
            store,
            element_locals,
            elem_expr_id,
            path,
            depth + 1,
            elem_allow_scoped_capture_exprs,
            scoped_capture_vars,
            package_id,
        );

        let CalleeLattice::Single(callable) = resolved else {
            return None;
        };
        if matches!(callable, ConcreteCallable::Dynamic) {
            return None;
        }
        candidates.push(callable);

        if candidates.len() > super::types::MULTI_CAP {
            return None;
        }
    }

    (!candidates.is_empty()).then_some(candidates)
}

/// Finds the element expression IDs of a literal array or tuple through
/// supported local, block, return, and struct-field wrappers. This does not
/// resolve the callable values of the elements.
fn resolve_array_elements<'a>(
    pkg: &Package,
    store: &PackageStore,
    locals: &'a LocalState,
    expr_id: ExprId,
    depth: usize,
) -> Option<(Vec<ExprId>, &'a LocalState)> {
    if depth > MAX_RESOLVE_DEPTH {
        return None;
    }

    let expr = pkg.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Array(elements) | ExprKind::ArrayLit(elements) | ExprKind::Tuple(elements) => {
            Some((elements.clone(), locals))
        }
        ExprKind::Var(Res::Local(var), _) => {
            let (init_expr_id, init_locals) = local_initializer(locals, *var)?;
            resolve_array_elements(pkg, store, init_locals, init_expr_id, depth + 1)
        }
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
            let (field_value_id, field_locals) =
                resolve_struct_field(pkg, store, locals, *inner_expr_id, path, depth + 1)?;
            resolve_array_elements(pkg, store, field_locals, field_value_id, depth + 1)
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
        ExprKind::Var(Res::Local(var), _) => {
            let (init_expr_id, init_locals) = local_initializer(locals, *var)?;
            resolve_static_int_expr(pkg, init_locals, init_expr_id, depth + 1)
        }
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
            resolve_static_int_expr(pkg, locals, *inner_expr_id, depth + 1).map(i64::wrapping_neg)
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
            let static_callable = match (&ty, locals.callable.get(&var)) {
                (
                    Ty::Arrow(arrow),
                    Some(CalleeLattice::Single(ConcreteCallable::Global { item_id, functor })),
                ) if !locals.udt_metadata.contains_arrow(&arrow.input) => {
                    Some((*item_id, *functor))
                }
                _ => None,
            };
            Some(CapturedVar {
                local: ScopedLocal::new(var, locals.owner),
                ty,
                static_callable,
                expr,
                caller_substitutions: Vec::new(),
            })
        })
        .collect()
}

/// Returns the initializer expression bound to `var` when it resolves to a
/// statically-known callable value (see [`is_known_callable_capture_expr`]),
/// retained as an expression operand. Embedding eligibility is recorded
/// separately in `CapturedVar::static_callable`.
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

/// Follows initializer bindings within `scoped_capture_vars`, stopping at the
/// first expression that no longer aliases a local in that set. The defining
/// bindings must already be present in `LocalState.exprs`.
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

/// Collects locals declared directly in a block, recursing through their
/// patterns but not into nested blocks, to scope capture resolution.
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
/// per-callable `closure_capturable_var_types` map (covering parameters and
/// immutable `let` bindings). Missing scoped evidence returns `None` because
/// `LocalVarId`s collide across callables.
fn find_local_var_type(pkg: &Package, locals: &LocalState, var: LocalVarId) -> Option<Ty> {
    if let Some(&init_expr_id) = locals.exprs.get(&var) {
        Some(pkg.get_expr(init_expr_id).ty.clone())
    } else {
        // Enclosing-callable parameter or immutable `let` binding. Resolve
        // against the per-callable variable map; `LocalVarId`s collide across
        // callables, so a package-wide pattern scan would return an unrelated
        // binding.
        locals.closure_capturable_var_types.get(&var).cloned()
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
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {
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

/// Builds flow-sensitive local variable state by performing a single forward
/// pass over the callable's body.
///
/// For callable-typed locals, the analysis tracks reaching definitions through
/// `set` assignments, forks state at `if`/`else` branches, and conservatively
/// marks mutable callable vars assigned inside `while` loops as `Dynamic`.
///
/// For all immutable locals, the raw `ExprId` binding is also recorded for
/// struct field resolution and type look-ups.
#[allow(clippy::too_many_arguments)]
fn build_callable_flow_state(
    pkg: &Package,
    store: &PackageStore,
    callable_impl: &CallableImpl,
    input_pat: qsc_fir::fir::PatId,
    owner: CaptureScope,
    clone_items: Rc<FxHashSet<StoreItemId>>,
    udt_metadata: Rc<UdtMetadata>,
    package_id: PackageId,
    recorder: Option<&mut CallRecorder>,
) -> LocalState {
    let mut state = LocalState {
        owner,
        clone_items,
        udt_metadata,
        callable: FxHashMap::default(),
        callable_sources: FxHashMap::default(),
        evaluated_callables: FxHashMap::default(),
        evaluated_selections: FxHashMap::default(),
        exprs: FxHashMap::default(),
        condition_substitutions: FxHashMap::default(),
        caller: None,
        active_returns: Rc::default(),
        visible_bindings: FxHashSet::default(),
        mutable_bindings: FxHashSet::default(),
        closure_capturable_var_types: collect_callable_param_types(pkg, callable_impl, input_pat),
    };
    match callable_impl {
        CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {}
        CallableImpl::Spec(spec_impl) => {
            analyze_spec_flow(
                pkg, store, spec_impl, input_pat, &mut state, package_id, recorder,
            );
        }
    }
    state
}

/// Analyzes the body and functored specializations in order using the supplied
/// flow state, resetting visible input bindings for each specialization.
fn analyze_spec_flow(
    pkg: &Package,
    store: &PackageStore,
    spec_impl: &SpecImpl,
    input_pat: PatId,
    state: &mut LocalState,
    package_id: PackageId,
    mut recorder: Option<&mut CallRecorder>,
) {
    set_visible_spec_input_bindings(
        pkg,
        input_pat,
        spec_impl.body.input,
        &mut state.visible_bindings,
    );
    analyze_block_flow(
        pkg,
        store,
        spec_impl.body.block,
        state,
        package_id,
        recorder.as_deref_mut(),
    );
    for spec in functored_specs(spec_impl) {
        set_visible_spec_input_bindings(pkg, input_pat, spec.input, &mut state.visible_bindings);
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

fn set_visible_spec_input_bindings(
    pkg: &Package,
    callable_input: PatId,
    spec_input: Option<PatId>,
    visible_bindings: &mut FxHashSet<LocalVarId>,
) {
    visible_bindings.clear();
    collect_pat_local_bindings(pkg, callable_input, visible_bindings);
    if let Some(spec_input) = spec_input
        && spec_input != callable_input
    {
        collect_pat_local_bindings(pkg, spec_input, visible_bindings);
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
    let outer_visible_bindings = state.visible_bindings.clone();
    let outer_mutable_bindings = state.mutable_bindings.clone();
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
    state.visible_bindings = outer_visible_bindings;
    state.mutable_bindings = outer_mutable_bindings;
}

/// Analyzes a statement in evaluation order. Initializers are walked before
/// their bindings and callable facts are published.
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
            analyze_expr_flow(pkg, store, *init_expr_id, state, package_id, recorder);
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
            collect_pat_local_bindings(pkg, *pat_id, &mut state.visible_bindings);
        }
        StmtKind::Local(Mutability::Mutable, pat_id, init_expr_id) => {
            analyze_expr_flow(pkg, store, *init_expr_id, state, package_id, recorder);
            bind_callable_pat(pkg, store, state, *pat_id, *init_expr_id, package_id);
            collect_pat_local_bindings(pkg, *pat_id, &mut state.visible_bindings);
            collect_pat_local_bindings(pkg, *pat_id, &mut state.mutable_bindings);
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
                bind_callable_value(state, ident.id, init_expr_id, lattice);
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
                let Some((resolved_init_id, source_state)) =
                    resolve_through_vars(pkg, state, init_expr_id)
                else {
                    return;
                };
                let source_state = source_state.clone();
                let mut path = Vec::new();
                bind_callable_pat_projections(
                    pkg,
                    store,
                    state,
                    &source_state,
                    pat_id,
                    resolved_init_id,
                    &mut path,
                    package_id,
                );
            }
        }
        PatKind::Discard => {}
    }
}

/// Walks a binding pattern and records, in the analysis state, the reaching
/// callables for each arrow-typed sub-binding by indexing into the initializer
/// along the accumulated field `path`.
#[allow(clippy::too_many_arguments)]
fn bind_callable_pat_projections(
    pkg: &Package,
    store: &PackageStore,
    state: &mut LocalState,
    source_state: &LocalState,
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
                    source_state,
                    init_expr_id,
                    path,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                );
                if !matches!(lattice, CalleeLattice::Bottom | CalleeLattice::Dynamic)
                    || state.callable.contains_key(&ident.id)
                {
                    bind_callable_value(state, ident.id, init_expr_id, lattice);
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
                    source_state,
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

/// Follows local initializers and parameter substitutions with their owning
/// states. Stops at unresolved or invalidated locals, or at the depth bound.
fn resolve_through_vars<'a>(
    pkg: &Package,
    mut state: &'a LocalState,
    mut expr_id: ExprId,
) -> Option<(ExprId, &'a LocalState)> {
    for _ in 0..MAX_RESOLVE_DEPTH {
        if let ExprKind::Var(Res::Local(var), _) = pkg.get_expr(expr_id).kind
            && let Some((init_id, init_state)) = local_initializer(state, var)
        {
            expr_id = init_id;
            state = init_state;
        } else {
            return Some((expr_id, state));
        }
    }
    None
}

fn bind_callable_value(
    state: &mut LocalState,
    var: LocalVarId,
    source: ExprId,
    value: CalleeLattice,
) {
    state
        .callable_sources
        .entry(var)
        .or_default()
        .insert(source);
    state.callable.insert(var, value);
}

fn resolve_assignment_lhs(
    pkg: &Package,
    store: &PackageStore,
    state: &LocalState,
    lhs_id: ExprId,
    rhs_id: ExprId,
    path: &mut Vec<usize>,
    package_id: PackageId,
) -> Vec<(LocalVarId, Option<CalleeLattice>)> {
    let lhs = pkg.get_expr(lhs_id);
    match &lhs.kind {
        ExprKind::Tuple(elements) => {
            let mut values = Vec::new();
            for (index, &element) in elements.iter().enumerate() {
                path.push(index);
                values.extend(resolve_assignment_lhs(
                    pkg, store, state, element, rhs_id, path, package_id,
                ));
                path.pop();
            }
            values
        }
        ExprKind::Var(Res::Local(var), _) => {
            let callable = matches!(lhs.ty, Ty::Arrow(_)).then(|| {
                resolve_callee_projection(
                    pkg,
                    store,
                    state,
                    rhs_id,
                    path,
                    0,
                    true,
                    &FxHashSet::default(),
                    package_id,
                )
            });
            vec![(*var, callable)]
        }
        _ => assign_lhs_base_local(pkg, lhs_id)
            .map(|var| vec![(var, None)])
            .unwrap_or_default(),
    }
}

fn analyze_assignment_values(
    pkg: &Package,
    store: &PackageStore,
    state: &mut LocalState,
    lhs_id: ExprId,
    rhs_id: ExprId,
    package_id: PackageId,
    mut recorder: Option<&mut CallRecorder>,
) -> Vec<(LocalVarId, Option<CalleeLattice>)> {
    if let (ExprKind::Tuple(targets), ExprKind::Tuple(values)) =
        (&pkg.get_expr(lhs_id).kind, &pkg.get_expr(rhs_id).kind)
    {
        assert_eq!(
            targets.len(),
            values.len(),
            "assignment tuple arity must match"
        );
        let mut assignments = Vec::new();
        for (&target, &value) in targets.iter().zip(values) {
            assignments.extend(analyze_assignment_values(
                pkg,
                store,
                state,
                target,
                value,
                package_id,
                recorder.as_deref_mut(),
            ));
        }
        assignments
    } else {
        analyze_expr_flow(pkg, store, rhs_id, state, package_id, recorder);
        resolve_assignment_lhs(
            pkg,
            store,
            state,
            lhs_id,
            rhs_id,
            &mut Vec::new(),
            package_id,
        )
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
            let values = analyze_assignment_values(
                pkg,
                store,
                state,
                *lhs_id,
                *rhs_id,
                package_id,
                recorder.as_deref_mut(),
            );
            for (written, _) in &values {
                invalidate_replayed_expression_dependents(pkg, state, *written);
            }
            for (written, callable) in values {
                if let Some(callable) = callable {
                    let callable = without_stale_guards(state, callable);
                    bind_callable_value(state, written, *rhs_id, callable);
                }
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
            state.evaluated_selections.entry(*cond).or_insert(true);
            // Fork callable facts; retain occurrence and invalidation evidence
            // conservatively across both branch walks.
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
            let mut written = collect_written_vars_in_block(pkg, *block_id);
            collect_written_vars_expr(pkg, *cond, &mut written);
            for &var in &written {
                invalidate_replayed_expression_dependents(pkg, state, var);
                if state.callable.contains_key(&var) {
                    state.callable.insert(var, CalleeLattice::Dynamic);
                }
            }

            analyze_expr_flow(
                pkg,
                store,
                *cond,
                state,
                package_id,
                recorder.as_deref_mut(),
            );

            // Analyze the body for nested let bindings. Restore pre-existing
            // callable entries to their pre-loop values, but keep new entries
            // added by loop-body analysis (loop-local immutable bindings).
            let loop_summary = state.callable.clone();
            analyze_block_flow(
                pkg,
                store,
                *block_id,
                state,
                package_id,
                recorder.as_deref_mut(),
            );
            for (var, lattice) in loop_summary {
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
            state.evaluated_selections.entry(*cond).or_insert(true);
            let written = assignment_written_local(pkg, expr);
            let guard_dependents = written
                .map(|written| guard_dependent_callables(pkg, state, written))
                .unwrap_or_default();
            let pre_rhs = state.callable.clone();
            analyze_expr_flow(pkg, store, *rhs, state, package_id, recorder.as_deref_mut());
            let after_rhs = std::mem::take(&mut state.callable);
            // `and`: RHS runs when the condition is true.
            let remapped_cond = remap_condition_expr(pkg, state, *cond);
            state.callable =
                join_callable_states_with_condition(&after_rhs, &pre_rhs, remapped_cond);
            if let Some(written) = written {
                invalidate_callable_value_dependents(pkg, state, written);
                invalidate_named_callables(state, &guard_dependents);
            }
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
            state.evaluated_selections.entry(*cond).or_insert(true);
            let written = assignment_written_local(pkg, expr);
            let guard_dependents = written
                .map(|written| guard_dependent_callables(pkg, state, written))
                .unwrap_or_default();
            let pre_rhs = state.callable.clone();
            analyze_expr_flow(pkg, store, *rhs, state, package_id, recorder.as_deref_mut());
            let after_rhs = std::mem::take(&mut state.callable);
            // `or`: RHS runs when the condition is false. Swap branches so the
            // reused condition `ExprId` dispatches as `if cond { orig } else { rhs }`.
            let remapped_cond = remap_condition_expr(pkg, state, *cond);
            state.callable =
                join_callable_states_with_condition(&pre_rhs, &after_rhs, remapped_cond);
            if let Some(written) = written {
                invalidate_callable_value_dependents(pkg, state, written);
                invalidate_named_callables(state, &guard_dependents);
            }
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
            if matches!(expr.kind, ExprKind::AssignField(..))
                && let Some(written) = assignment_written_local(pkg, expr)
            {
                invalidate_replayed_expression_dependents(pkg, state, written);
            }
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
            if matches!(expr.kind, ExprKind::AssignIndex(..))
                && let Some(written) = assignment_written_local(pkg, expr)
            {
                invalidate_replayed_expression_dependents(pkg, state, written);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b) => {
            analyze_expr_flow(pkg, store, *a, state, package_id, recorder.as_deref_mut());
            analyze_expr_flow(pkg, store, *b, state, package_id, recorder.as_deref_mut());
        }
        ExprKind::AssignOp(_, lhs, rhs) => {
            analyze_expr_flow(pkg, store, *lhs, state, package_id, recorder.as_deref_mut());
            analyze_expr_flow(pkg, store, *rhs, state, package_id, recorder.as_deref_mut());
            if let Some(written) = assignment_written_local(pkg, expr) {
                invalidate_replayed_expression_dependents(pkg, state, written);
            }
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

    // Producer walks need the same operand snapshots as call-site recording.
    // Reanalyzing an already evaluated block must not overwrite its old reads.
    // Do not eagerly resolve producers here.
    let (base_id, functor) = peel_body_functors(pkg, expr_id);
    if matches!(expr.kind, ExprKind::Index(..)) {
        state.evaluated_selections.entry(expr_id).or_insert(true);
    }
    if let ExprKind::Var(Res::Local(local), _) = pkg.get_expr(base_id).kind
        && let Some(evaluated) = state.callable.get(&local)
    {
        if !matches!(
            evaluated,
            CalleeLattice::Single(ConcreteCallable::Global { .. })
        ) {
            state.evaluated_selections.entry(expr_id).or_insert(true);
        }
        state
            .evaluated_callables
            .entry(expr_id)
            .or_insert_with(|| apply_outer_functor_lattice(evaluated.clone(), functor));
    }

    // Call operands have finished, but their callable values were observed
    // before any writes in later operands.
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
            rec.preserved_direct_lambda_calls,
            rec.record_direct_calls,
            rec.total_foreign,
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
fn collect_written_vars_in_block(pkg: &Package, block_id: BlockId) -> Vec<LocalVarId> {
    let mut vars = Vec::new();
    collect_written_vars_block(pkg, block_id, &mut vars);
    vars
}

/// Collects every `LocalVarId` assigned within a block (mutable update or
/// `Assign`), accumulating into `vars` so branch joins can invalidate
/// stale lattice entries.
fn collect_written_vars_block(pkg: &Package, block_id: BlockId, vars: &mut Vec<LocalVarId>) {
    let block = pkg.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = pkg.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                collect_written_vars_expr(pkg, *e, vars);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Collects every `LocalVarId` assigned within an expression subtree,
/// recursing through every nested expression via the exhaustive
/// [`crate::walk_utils::for_each_expr`] walker so that `set` statements
/// hidden in operand-position blocks are observed.
fn collect_written_vars_expr(pkg: &Package, expr_id: ExprId, vars: &mut Vec<LocalVarId>) {
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_id, expr| {
        vars.extend(crate::walk_utils::assignment_written_locals(pkg, expr));
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

/// Cached global callable reads are stored values, not replayed local reads.
fn initializer_replays_local(
    pkg: &Package,
    state: &LocalState,
    expr_id: ExprId,
    var: LocalVarId,
) -> bool {
    let mut found = false;
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |id, expr| {
        if matches!(expr.kind, ExprKind::Var(Res::Local(local), _) if local == var)
            && !matches!(
                state.evaluated_callables.get(&id),
                Some(CalleeLattice::Single(ConcreteCallable::Global { .. }))
            )
        {
            found = true;
        }
    });
    found
}

/// Degrades callable entries to `Dynamic` when their replayed guard,
/// initializer, or capture depends on `written`. Called after the assignment's
/// operands have been analyzed, when the store takes effect.
///
/// Rewrite reconstructs a conditional callable's dispatch by re-evaluating its
/// guards at the *apply* site, not the *binding* site. Once a guard variable is
/// reassigned after the callable's guarded value was formed, the guard read at
/// the apply site would observe the new value and select the wrong branch (see
/// the `reaching_def_conditional_callable_reassigned_guard_dynamic` regression).
/// Marking such callables `Dynamic` leaves their dispatch to downstream analysis
/// instead of reconstructing it from a changed value. Guards formed
/// *after* this write are unaffected, so a normalization accumulator assigned
/// before the branch decision (e.g. `cond_normalize`'s `__cond`) stays
/// resolvable.
fn invalidate_replayed_expression_dependents(
    pkg: &Package,
    state: &mut LocalState,
    written: LocalVarId,
) {
    let guard_dependents = guard_dependent_callables(pkg, state, written);
    invalidate_callable_value_dependents(pkg, state, written);
    invalidate_named_callables(state, &guard_dependents);
}

fn invalidate_callable_value_dependents(
    pkg: &Package,
    state: &mut LocalState,
    written: LocalVarId,
) {
    // An alias keeps the same replayed selector/capture provenance as its
    // source. Invalidating only the first binding would let later aliases
    // reconstruct that value using a selector that has since changed.
    let mut dependents = FxHashSet::from_iter([written]);
    loop {
        let before = dependents.len();
        for (var, expr) in state.exprs.iter().map(|(&var, &expr)| (var, expr)).chain(
            state
                .callable_sources
                .iter()
                .flat_map(|(&var, sources)| sources.iter().map(move |&expr| (var, expr))),
        ) {
            // Data-only immutable bindings are stored snapshots, not replayed
            // callable selectors. Their readers must retain those saved values.
            if state.udt_metadata.contains_arrow(&pkg.get_expr(expr).ty)
                && dependents
                    .iter()
                    .any(|&local| initializer_replays_local(pkg, state, expr, local))
            {
                dependents.insert(var);
            }
        }
        for (&var, lattice) in &state.callable {
            let capture_depends = dependents.iter().any(|&local| match lattice {
                CalleeLattice::Single(callable) => {
                    concrete_callable_reads_local(pkg, callable, local)
                }
                CalleeLattice::Multi(entries) => entries
                    .iter()
                    .any(|(callable, _)| concrete_callable_reads_local(pkg, callable, local)),
                CalleeLattice::Bottom | CalleeLattice::Dynamic => false,
            });
            if capture_depends {
                dependents.insert(var);
            }
        }
        if dependents.len() == before {
            break;
        }
    }
    let invalidated: Vec<_> = state
        .evaluated_selections
        .keys()
        .copied()
        .filter(|&expr| {
            dependents
                .iter()
                .any(|&local| initializer_replays_local(pkg, state, expr, local))
        })
        .collect();
    for expr in invalidated {
        state.evaluated_selections.insert(expr, false);
    }
    // Aggregate bindings have no concrete lattice entry, but their initializer
    // walkers must still honor invalidation instead of reconstructing stale fields.
    for &var in &dependents {
        if state.exprs.contains_key(&var) {
            state.callable.insert(var, CalleeLattice::Dynamic);
        }
    }
    invalidate_named_callables(state, &dependents);
}

fn guard_dependent_callables(
    pkg: &Package,
    state: &LocalState,
    written: LocalVarId,
) -> FxHashSet<LocalVarId> {
    state
        .callable
        .iter()
        .filter_map(|(callable_var, lattice)| {
            let CalleeLattice::Multi(entries) = lattice else {
                return None;
            };
            entries
                .iter()
                .any(|(_, guards)| {
                    guards
                        .iter()
                        .any(|&guard| expr_reads_local(pkg, guard, written))
                })
                .then_some(*callable_var)
        })
        .collect()
}

fn invalidate_named_callables(state: &mut LocalState, callables: &FxHashSet<LocalVarId>) {
    for callable in callables {
        if let Some(lattice) = state.callable.get_mut(callable) {
            *lattice = CalleeLattice::Dynamic;
        }
    }
}

fn concrete_callable_reads_local(
    pkg: &Package,
    callable: &ConcreteCallable,
    local: LocalVarId,
) -> bool {
    let ConcreteCallable::Closure { captures, .. } = callable else {
        return false;
    };
    captures.iter().any(|capture| {
        capture.expr.map_or_else(
            || capture.local.var == local,
            |expr| capture_expr_reads_local(pkg, expr, &capture.caller_substitutions, local),
        )
    })
}

/// Reads in substituted expressions belong to their own environments. A
/// replaced producer local must not be mistaken for a same-numbered caller local.
fn capture_expr_reads_local(
    pkg: &Package,
    expr: ExprId,
    substitutions: &[CaptureSubstitution],
    local: LocalVarId,
) -> bool {
    substitutions.iter().any(|substitution| {
        capture_expr_reads_local(pkg, substitution.expr, &substitution.substitutions, local)
    }) || (!substitutions
        .iter()
        .any(|substitution| substitution.local == local)
        && expr_reads_local(pkg, expr, local))
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

/// Records initializer expressions for bound locals. Tuple patterns are
/// traversed only when the initializer is a tuple literal of matching arity.
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
