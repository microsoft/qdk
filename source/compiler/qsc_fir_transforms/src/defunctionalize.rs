// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Defunctionalization pass.
//!
//! Eliminates all callable-valued expressions (arrow-typed locals, closures,
//! functor-applied callable values) in entry-reachable code through a
//! dispatch-free specialization approach. Unlike classical defunctionalization
//! (which introduces a tagged union and an `apply` function), this
//! implementation directly specializes each higher-order function (HOF) call
//! site where a concrete callable argument is known at compile time.
//! Single-bound tuple parameters whose type contains callable values are
//! supported via a split locator model that tracks the top-level parameter
//! slot separately from the nested tuple field path.
//!
//! Establishes [`crate::invariants::InvariantLevel::PostDefunc`]: no
//! `ExprKind::Closure` remains in reachable code, no arrow-typed callable
//! parameters remain in reachable declarations, and all indirect dispatch
//! is rewritten to direct dispatch.
//!
//! Each iteration of the fixpoint loop consists of three phases:
//!
//! - **Analysis** — discovers callable-typed parameters in HOFs, collects
//!   call sites where those HOFs are invoked with concrete callable arguments,
//!   and runs an identity-closure peephole optimization that replaces
//!   `(args) => f(args)` wrappers with direct references to `f`.
//! - **Specialization** — clones each HOF for each concrete argument
//!   combination, replacing the callable parameter reference with a direct
//!   call to the concrete callee. A deduplication map keyed by [`types::SpecKey`]
//!   ensures identical specializations are created only once.
//! - **Rewrite** — redirects original call sites to invoke the specialized
//!   clones, removes the callable argument from the argument tuple, and
//!   threads closure captures as extra arguments.
//!
//! These phases iterate until no reachable closures or arrow-typed parameters
//! remain in the target package. The iteration limit is dynamically scaled on
//! the first pass based on the number of discovered callable values
//! (`remaining_count.clamp(5, 20)`), preventing unnecessary iterations for
//! simple programs while allowing complex HOF nesting patterns to resolve. If
//! convergence is not reached within the limit, an error is reported.
//!
//! In the future, this pass could be extended to support tagged-union-style
//! defunctionalization for cases where specialization does not converge,
//! but the current approach is required for QIR generation because the QIR
//! specification requires direct calls to known callees.
//!
//! # Input patterns
//!
//! - `operation Apply(op : Qubit => Unit, q : Qubit) { op(q); }` — an arrow
//!   parameter consumed by a HOF.
//! - `Apply(H, q)` — a call site binding the arrow parameter to a concrete
//!   global callable.
//! - `Apply(q => Y(q), q)` — a call site binding the arrow parameter to a
//!   lambda.
//!
//! # Rewrites
//!
//! ```text
//! // Before
//! operation Apply(op : Qubit => Unit, q : Qubit) { op(q); }
//! Apply(q => Y(q), target);
//!
//! // After (closure identity peephole collapses the lambda to `Y`)
//! operation Apply_specialized_Y(q : Qubit) { Y(q); }
//! Apply_specialized_Y(target);
//! ```
//!
//! # Notes
//!
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`; the
//!   [`crate::exec_graph_rebuild`] pass repairs them at the end of the
//!   pipeline.

mod analysis;
mod prepass;
mod rewrite;
mod specialize;
pub mod types;

pub use types::Error;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::reachability::collect_reachable_from_entry;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    ExprId, ExprKind, ItemKind, LocalItemId, Package, PackageId, PackageLookup, PackageStore, Res,
};
use qsc_fir::ty::Ty;
use rustc_hash::FxHashSet;
use types::{
    CallSite, CallableParam, ConcreteCallable, ConcreteCallableKey, SpecKey, peel_body_functors,
};

/// Maximum number of analysis → specialize → rewrite iterations before
/// reporting a convergence failure.
///
/// The value of 5 is the floor: after the first iteration the limit is
/// recomputed as
/// `max(callable_params.len(), remaining_count).clamp(MAX_ITERATIONS, 20)`,
/// giving one iteration of margin beyond the deepest observed HOF chain
/// (4 levels in the chemistry library's Trotter simulation pipeline) and
/// an upper bound of 20 iterations for pathological programs.
const MAX_ITERATIONS: usize = 5;

/// Defunctionalizes all callable-valued expressions in the entry-reachable
/// portion of a package.
///
/// After this pass:
/// - No `ExprKind::Closure` nodes remain in reachable code.
/// - No arrow-typed parameters remain in reachable callable declarations.
/// - All indirect callable dispatch is replaced with direct dispatch calls.
///
/// Returns a vector of errors encountered during defunctionalization.
/// An empty vector indicates success.
pub fn defunctionalize(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> Vec<Error> {
    let package = store.get(package_id);
    if package.entry.is_none() {
        return vec![];
    }

    let mut errors: Vec<Error> = Vec::new();
    let mut warnings: Vec<Error> = Vec::new();
    let mut max_iterations = MAX_ITERATIONS;
    let mut iteration_count = 0;
    let mut specialized_closure_targets: FxHashSet<LocalItemId> = FxHashSet::default();
    let mut specialized_items: FxHashSet<LocalItemId> = FxHashSet::default();

    // Capture initial callable-value count for before/after progress tracking
    // (mirrors LLVM DevirtSCCRepeatedPass: detect when an iteration fails to
    // reduce the remaining work set).
    let (_, mut prev_remaining_count, _) = remaining_callable_value_info(store, package_id);
    let mut _stuck = false;

    while iteration_count < max_iterations {
        iteration_count += 1;

        // Clear DynamicCallable errors from prior iterations. These are
        // re-discovered each pass, and transient ones (e.g. parameter
        // forwarding like `Inner(op, q)` inside a HOF that hasn't been
        // specialized yet) disappear once the outer HOF is specialized.
        errors.retain(|e| !matches!(e, Error::DynamicCallable(_)));

        let reachable = collect_reachable_from_entry(store, package_id);

        // Simplify defunctionalization analysis by eliminating callable
        // indirection patterns and exposing direct call sites.
        prepass::run(store, package_id);

        let analysis = analysis::analyze(store, package_id, &reachable);

        let (spec_map, mut spec_errors) = if analysis.call_sites.is_empty() {
            (Default::default(), Vec::new())
        } else {
            specialize::specialize(store, package_id, &analysis, assigner)
        };
        // Separate warnings from errors so the `retain` at the top of each
        // iteration does not discard them.
        warnings.extend(
            spec_errors
                .iter()
                .filter(|e| matches!(e, Error::ExcessiveSpecializations(..)))
                .cloned(),
        );
        spec_errors.retain(|e| !matches!(e, Error::ExcessiveSpecializations(..)));
        errors.append(&mut spec_errors);

        // Rewrite call sites and run dead callable-local cleanup even on
        // iterations where no new specializations were discovered.
        let package = store.get_mut(package_id);
        rewrite::rewrite(package, package_id, &analysis, &spec_map, assigner);

        // Collect closure targets that were specialized in this iteration and
        // replace consumed closure expressions with Unit. A closure is
        // "consumed" when its target callable has had a specialization created
        // for it, meaning the HOF call that used this closure has been
        // rewritten to a direct call.
        for cs in &analysis.call_sites {
            let spec_key = build_spec_key(cs);
            if spec_map.contains_key(&spec_key)
                && let ConcreteCallable::Closure { target, .. } = &cs.callable_arg
            {
                specialized_closure_targets.insert(*target);
            }
        }
        specialized_items.extend(spec_map.values().copied());
        cleanup_consumed_closures(package, &specialized_closure_targets, &specialized_items);

        // Check convergence
        let (has_remaining, remaining_count, _) = remaining_callable_value_info(store, package_id);

        // Before/after progress check: remaining callable expressions must
        // decrease or new call sites must have been discovered. Without
        // progress the loop cannot converge and should exit early.
        let made_progress =
            remaining_count < prev_remaining_count || !analysis.call_sites.is_empty();
        prev_remaining_count = remaining_count;

        // On the first iteration, compute a dynamic iteration limit based on
        // the number of remaining callable values discovered. This scales with
        // program complexity while capping runaway iteration.
        if iteration_count == 1 {
            max_iterations = analysis
                .callable_params
                .len()
                .max(remaining_count)
                .clamp(MAX_ITERATIONS, 20);
        }

        if !has_remaining {
            break;
        }

        if !made_progress {
            // Stuck: remaining callable expressions unchanged and no new call
            // sites were discovered. The post-loop check will emit
            // FixpointNotReached.
            _stuck = true;
            break;
        }
    }

    let (has_remaining, remaining_count, span) = remaining_callable_value_info(store, package_id);
    if has_remaining && errors.is_empty() {
        errors.push(Error::FixpointNotReached(
            iteration_count,
            remaining_count,
            span,
        ));
    }

    // Merge accumulated warnings into the returned error list.
    errors.extend(warnings);

    errors
}

/// Replaces all remaining closure expressions whose target callable was
/// consumed by specialization with Unit values, clearing references so
/// subsequent iterations do not count them as work remaining.
///
/// A closure is "consumed" when its target callable has been specialized —
/// meaning the HOF call site that passed this closure as an argument has been
/// rewritten to a direct call to the specialized version. The closure node
/// in the producer function body is now dead: no analysis will discover new
/// call sites for it, but `remaining_callable_value_info` would still count
/// it as work remaining, causing false convergence failure.
///
/// Only closures that are NOT direct children of a `Call` argument subtree
/// are eligible for cleanup. Closures that are still live as arguments to a
/// call expression (e.g., in a multi-param HOF where only one param has been
/// specialized so far) must survive to the next iteration.
///
/// # Before
/// ```text
/// Closure([captures], consumed_target) : Arrow
/// ```
/// # After
/// ```text
/// Tuple([]) : Unit   // closure replaced with unit
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` to `Tuple(Vec::new())` and `Expr.ty` to `Unit`
///   for consumed closure expressions outside call-argument subtrees.
fn cleanup_consumed_closures(
    package: &mut Package,
    specialized_targets: &FxHashSet<LocalItemId>,
    skip_items: &FxHashSet<LocalItemId>,
) -> usize {
    if specialized_targets.is_empty() {
        return 0;
    }

    // First pass: collect the ExprIds of all call argument subtrees.
    // Closures inside these subtrees are still live as HOF arguments.
    let mut call_arg_exprs: FxHashSet<ExprId> = FxHashSet::default();
    let item_ids: Vec<_> = package.items.iter().map(|(id, _)| id).collect();
    for item_id in &item_ids {
        if skip_items.contains(item_id) {
            continue;
        }
        let item = package.get_item(*item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_expr_id, expr| {
                    if let ExprKind::Call(_, args_id) = &expr.kind {
                        collect_all_expr_ids(package, *args_id, &mut call_arg_exprs);
                    }
                },
            );
        }
    }
    if let Some(entry_id) = package.entry {
        crate::walk_utils::for_each_expr(package, entry_id, &mut |_expr_id, expr| {
            if let ExprKind::Call(_, args_id) = &expr.kind {
                collect_all_expr_ids(package, *args_id, &mut call_arg_exprs);
            }
        });
    }

    // Second pass: collect consumed closures that are NOT in call argument
    // positions.
    let mut to_replace: Vec<ExprId> = Vec::new();
    for item_id in &item_ids {
        if skip_items.contains(item_id) {
            continue;
        }
        let item = package.get_item(*item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, expr| {
                    if let ExprKind::Closure(_, target) = &expr.kind
                        && specialized_targets.contains(target)
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
                && specialized_targets.contains(target)
                && !call_arg_exprs.contains(&expr_id)
            {
                to_replace.push(expr_id);
            }
        });
    }

    let count = to_replace.len();
    for expr_id in to_replace {
        let expr = package.exprs.get_mut(expr_id).expect("expr must exist");
        expr.kind = ExprKind::Tuple(Vec::new());
        expr.ty = Ty::UNIT;
    }

    count
}

/// Recursively collects all `ExprId`s reachable from an expression node.
fn collect_all_expr_ids(package: &Package, expr_id: ExprId, ids: &mut FxHashSet<ExprId>) {
    crate::walk_utils::for_each_expr(package, expr_id, &mut |child_id, _| {
        ids.insert(child_id);
    });
}

/// Checks whether any reachable target-package callable value still requires
/// defunctionalization work.
///
/// Returns `(has_remaining, count, first_span)` in a single reachability scan.
fn remaining_callable_value_info(
    store: &PackageStore,
    package_id: PackageId,
) -> (bool, usize, Span) {
    let reachable = collect_reachable_from_entry(store, package_id);
    let package = store.get(package_id);
    let mut count = 0;
    let mut first_span = Span::default();

    let mut record_remaining = |span: Span| {
        if count == 0 {
            first_span = span;
        }
        count += 1;
    };

    for store_id in &reachable {
        if store_id.package != package_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let input_pat = package.get_pat(decl.input);
            if ty_contains_arrow_through_udts(store, &input_pat.ty) {
                record_remaining(input_pat.span);
            }

            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_expr_id, expr| {
                    if matches!(expr.kind, ExprKind::Closure(_, _)) {
                        record_remaining(expr.span);
                    }
                    // Count indirect calls through arrow-typed local variables.
                    // After defunc iteration 1 specializes HOFs and removes callable
                    // parameters, conditional callable bindings like
                    //   let u = if power >= 0 { op } else { Adjoint op };
                    //   u(target);
                    // leave arrow-typed locals with indirect Call expressions.
                    // The existing branch-split infrastructure resolves these in
                    // a subsequent iteration, but only if the convergence check
                    // reports them as remaining.
                    if let ExprKind::Call(callee_id, _) = &expr.kind {
                        let (base_id, _) = peel_body_functors(package, *callee_id);
                        let base_expr = package.get_expr(base_id);
                        if matches!(base_expr.kind, ExprKind::Var(Res::Local(_), _))
                            && ty_contains_arrow(&base_expr.ty)
                        {
                            record_remaining(base_expr.span);
                        }
                    }
                },
            );
        }
    }

    if let Some(entry_id) = package.entry {
        crate::walk_utils::for_each_expr(package, entry_id, &mut |_expr_id, expr| {
            if matches!(expr.kind, ExprKind::Closure(_, _)) {
                record_remaining(expr.span);
            }
            // Same indirect-call check as callable body walker.
            if let ExprKind::Call(callee_id, _) = &expr.kind {
                let (base_id, _) = peel_body_functors(package, *callee_id);
                let base_expr = package.get_expr(base_id);
                if matches!(base_expr.kind, ExprKind::Var(Res::Local(_), _))
                    && ty_contains_arrow(&base_expr.ty)
                {
                    record_remaining(base_expr.span);
                }
            }
        });
    }

    (count > 0, count, first_span)
}

/// Checks whether a type contains an arrow type anywhere within its structure.
///
/// This intentionally does NOT recurse into `Ty::Udt` or `Ty::Array`:
///
/// - **`Ty::Udt`**: Defunc runs before UDT erasure, so UDT wrappers are still
///   opaque here. Callable values inside UDTs are handled at the *expression*
///   level by the analysis phase (`extract_arrow_params_from_ty` also ignores
///   `Ty::Udt`, but `build_callable_flow_state` tracks field-extraction
///   expressions like `config.Op` to resolve concrete callable values). After
///   defunc, callable values are either specialized or rejected as
///   `DynamicCallable`. Post-UDT-erasure passes (SROA, `arg_promote`) may expose
///   bare `Ty::Arrow` parameters, but partial eval handles them correctly
///   because it dispatches on *values* (`Value::Global` / `Value::Closure`),
///   not on the `Ty::Arrow` type annotation.
///
/// - **`Ty::Array`**: Array-of-callable parameters (`(Qubit => Unit)[]`) are
///   dynamically indexed, so defunc cannot specialize them. Ignoring
///   `Ty::Array` is consistent with defunc's capabilities.
///
/// A separate copy of this function in `codegen.rs` does handle `Ty::Array`
/// for codegen routing; unifying the two is unnecessary because their
/// contexts differ.
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

/// Builds the deduplication key for a call site's specialization.
pub(crate) fn build_spec_key(call_site: &CallSite) -> SpecKey {
    let concrete_key = match &call_site.callable_arg {
        ConcreteCallable::Global { item_id, functor } => ConcreteCallableKey::Global {
            item_id: *item_id,
            functor: *functor,
        },
        ConcreteCallable::Closure {
            target, functor, ..
        } => ConcreteCallableKey::Closure {
            target: *target,
            functor: *functor,
        },
        ConcreteCallable::Dynamic => {
            // Dynamic callables are filtered out before reaching here, but
            // provide a deterministic key regardless.
            ConcreteCallableKey::Global {
                item_id: call_site.hof_item_id,
                functor: FunctorApp::default(),
            }
        }
    };
    SpecKey {
        hof_id: call_site.hof_item_id.item,
        concrete_args: vec![concrete_key],
    }
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
