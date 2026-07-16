// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Entry-rooted call graph walker.
//!
//! [`collect_reachable_from_entry`] starts from a package's entry expression
//! and transitively discovers every callable item reachable through the FIR
//! call graph, including cross-package references.
//!
//! The algorithm is a worklist-based breadth-first walk. Starting from the
//! entry expression, it follows every `Res::Item` reference encountered in
//! expression trees, adding newly discovered
//! callables to the worklist until a fixed point is reached.
//!
//! [`collect_reachable_with_seeds`] extends this by accepting additional seed
//! items as extra roots alongside the entry expression. The production pipeline
//! validates explicit pinned items before using this generic seeded walker.
//!
//! [`collect_reachable_package_closure`] computes the cross-package
//! reachability closure needed by UDT erasure to determine which packages
//! require type-item removal.

#[cfg(test)]
mod tests;

use qsc_fir::fir::{CallableImpl, ExprKind, ItemKind, PackageId, PackageStore, Res, StoreItemId};
use rustc_hash::FxHashSet;

/// Returns the set of all callable items transitively reachable from the entry
/// expression of the given package.
///
/// Cross-package references are followed, so the result may contain items from
/// library packages. Intrinsic callables are included as reachable (they have
/// no body to walk but are still referenced).
///
/// # Scoping contract
///
/// - **Missing items are silently skipped.** Interpreter entry expressions
///   can carry runtime-unbound item references that survive a rejected
///   callable definition. When the worklist encounters a `StoreItemId` that
///   no longer exists in its package's item table, the walker drops it and
///   continues; later evaluation reports the diagnostic instead of failing
///   here.
/// - **Closures resolve in the current package only.**
///   [`ExprKind::Closure(_, local_item_id)`](ExprKind::Closure) carries a
///   bare [`LocalItemId`](qsc_fir::fir::LocalItemId); the walker pairs it
///   with the *containing* package id rather than any source package id. As
///   a result closures cannot point outside the package in which they
///   appear, and the walker treats them accordingly.
///
/// # Panics
///
/// Panics if the package has no entry expression.
#[must_use]
pub fn collect_reachable_from_entry(
    store: &PackageStore,
    package_id: PackageId,
) -> FxHashSet<StoreItemId> {
    let package = store.get(package_id);
    let entry_expr_id = package
        .entry
        .expect("package must have an entry expression");

    let mut visited = FxHashSet::default();
    let mut worklist: Vec<StoreItemId> = Vec::new();

    walk_expr(store, package_id, entry_expr_id, &mut worklist);

    while let Some(item_id) = worklist.pop() {
        if visited.contains(&item_id) {
            continue;
        }
        let item_pkg = store.get(item_id.package);
        let Some(item) = item_pkg.items.get(item_id.item) else {
            // Interpreter entry expressions can carry runtime-unbound item references
            // after a rejected callable definition. Leave those for later evaluation
            // diagnostics instead of panicking during reachability discovery.
            continue;
        };
        visited.insert(item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            walk_callable_impl(store, item_id.package, &decl.implementation, &mut worklist);
        }
    }

    visited
}

/// Returns the set of all callable items transitively reachable from the
/// entry expression **and** from the additional `seeds`.
///
/// Seeds are added to the worklist alongside the items discovered from the
/// entry expression, so their transitive dependencies are also included in
/// the output set.
///
/// Missing seed and transitive item IDs are silently skipped when their package
/// exists, matching [`collect_reachable_from_entry`]. Pipeline callers that use
/// explicit pinned items validate those pins before calling this generic walker.
///
/// # Panics
///
/// Panics if the package has no entry expression.
#[must_use]
pub fn collect_reachable_with_seeds(
    store: &PackageStore,
    package_id: PackageId,
    seeds: &[StoreItemId],
) -> FxHashSet<StoreItemId> {
    let package = store.get(package_id);
    let entry_expr_id = package
        .entry
        .expect("package must have an entry expression");

    let mut visited = FxHashSet::default();
    let mut worklist: Vec<StoreItemId> = seeds.to_vec();

    walk_expr(store, package_id, entry_expr_id, &mut worklist);

    while let Some(item_id) = worklist.pop() {
        if visited.contains(&item_id) {
            continue;
        }
        let item_pkg = store.get(item_id.package);
        let Some(item) = item_pkg.items.get(item_id.item) else {
            continue;
        };
        visited.insert(item_id);
        if let ItemKind::Callable(decl) = &item.kind {
            walk_callable_impl(store, item_id.package, &decl.implementation, &mut worklist);
        }
    }

    visited
}

/// Returns the package closure induced by an entry-reachable callable set.
///
/// The returned set always includes the root package, even when the entry
/// expression reaches no other callables.
#[must_use]
pub fn collect_reachable_package_closure<'a>(
    package_id: PackageId,
    reachable: impl IntoIterator<Item = &'a StoreItemId>,
) -> FxHashSet<PackageId> {
    let mut packages = FxHashSet::default();
    packages.insert(package_id);
    packages.extend(reachable.into_iter().map(|item_id| item_id.package));
    packages
}

/// Walks the bodies of a callable implementation, enqueueing every referenced
/// item onto `worklist`. Closures enqueue `(pkg_id, local_item_id)` because
/// `ExprKind::Closure` always resolves within the containing package.
fn walk_callable_impl(
    store: &PackageStore,
    pkg_id: PackageId,
    callable_impl: &CallableImpl,
    worklist: &mut Vec<StoreItemId>,
) {
    if matches!(callable_impl, CallableImpl::SimulatableIntrinsic(_)) {
        // A `@SimulatableIntrinsic` callable carries a simulation override body that
        // is meaningful only to the simulator. For QIR code generation the callable
        // behaves like an intrinsic, so reachability treats it as bodyless and does
        // **not** descend its simulation body. As a result, items referenced *only*
        // from a simulation body are not kept reachable and are pruned by item DCE,
        // rather than being monomorphized, type-erased, and processed as if they were
        // part of the generated program. The simulation body itself is left intact in
        // the FIR; partial evaluation still classically evaluates it through the
        // separate `qsc_eval` interpreter path when a call is purely classical.
        return;
    }
    let pkg = store.get(pkg_id);
    crate::walk_utils::for_each_expr_in_callable_impl(pkg, callable_impl, &mut |_eid, expr| {
        match &expr.kind {
            ExprKind::Var(Res::Item(item_id), _) => {
                worklist.push(StoreItemId::from((item_id.package, item_id.item)));
            }
            ExprKind::Closure(_, local_item_id) => {
                worklist.push(StoreItemId::from((pkg_id, *local_item_id)));
            }
            _ => {}
        }
    });
}

/// Walks the expression subtree rooted at `expr_id`, enqueueing every
/// referenced item onto `worklist`. Mirrors the closure scoping rule in
/// [`walk_callable_impl`].
fn walk_expr(
    store: &PackageStore,
    pkg_id: PackageId,
    expr_id: qsc_fir::fir::ExprId,
    worklist: &mut Vec<StoreItemId>,
) {
    let pkg = store.get(pkg_id);
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_eid, expr| match &expr.kind {
        ExprKind::Var(Res::Item(item_id), _) => {
            worklist.push(StoreItemId::from((item_id.package, item_id.item)));
        }
        ExprKind::Closure(_, local_item_id) => {
            worklist.push(StoreItemId::from((pkg_id, *local_item_id)));
        }
        _ => {}
    });
}
