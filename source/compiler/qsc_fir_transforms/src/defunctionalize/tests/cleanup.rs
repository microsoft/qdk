// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Unit tests for [`cleanup_consumed_closures`], exercising each filter step
//! that decides whether a consumed closure expression is replaced with `Unit`.
//!
//! Each test compiles Q# to monomorphized FIR (closures still present, before
//! defunctionalization), then calls `cleanup_consumed_closures` directly with
//! crafted `specialized_targets` / `skip_items` inputs to drive one branch of
//! the filter and asserts the returned replacement count and the resulting
//! expression kinds. This mirrors the source-compilation strategy used by the
//! other defunctionalization tests and avoids any QIR generation.

use super::*;
use crate::defunctionalize::cleanup_consumed_closures;
use qsc_fir::fir::{ExprId, ExprKind, LocalItemId, Package, PackageLookup};
use qsc_fir::ty::Ty;
use rustc_hash::FxHashSet;

/// Compiles `source` to monomorphized FIR and returns the store, the user
/// package id, and the reachable local callable ids (the scope passed to
/// `cleanup_consumed_closures`).
fn setup(source: &str) -> (fir::PackageStore, fir::PackageId, Vec<LocalItemId>) {
    let (fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let reachable = collect_reachable_from_entry(&fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    let reachable_item_ids: Vec<LocalItemId> =
        reachable_local_callables(package, fir_pkg_id, &reachable)
            .map(|(id, _)| id)
            .collect();
    (fir_store, fir_pkg_id, reachable_item_ids)
}

/// Collects every `(closure expr id, target callable id)` pair reachable from
/// the entry-reachable callables and the entry expression.
fn all_closures(
    package: &Package,
    reachable_item_ids: &[LocalItemId],
) -> Vec<(ExprId, LocalItemId)> {
    let mut found: Vec<(ExprId, LocalItemId)> = Vec::new();
    for &item_id in reachable_item_ids {
        if let ItemKind::Callable(decl) = &package.get_item(item_id).kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, expr| {
                    if let ExprKind::Closure(_, target) = &expr.kind {
                        found.push((expr_id, *target));
                    }
                },
            );
        }
    }
    if let Some(entry_id) = package.entry {
        crate::walk_utils::for_each_expr(package, entry_id, &mut |expr_id, expr| {
            if let ExprKind::Closure(_, target) = &expr.kind {
                found.push((expr_id, *target));
            }
        });
    }
    found
}

/// Returns the single closure expr id and its target callable id, asserting
/// that exactly one closure is present in the reachable scope.
fn single_closure(package: &Package, reachable_item_ids: &[LocalItemId]) -> (ExprId, LocalItemId) {
    let closures = all_closures(package, reachable_item_ids);
    assert_eq!(
        closures.len(),
        1,
        "expected exactly one closure in the reachable scope, found {}",
        closures.len()
    );
    closures[0]
}

/// Finds the reachable callable item with the given display name.
fn find_callable(package: &Package, reachable_item_ids: &[LocalItemId], name: &str) -> LocalItemId {
    for &item_id in reachable_item_ids {
        if let ItemKind::Callable(decl) = &package.get_item(item_id).kind
            && decl.name.name.as_ref() == name
        {
            return item_id;
        }
    }
    panic!("callable {name} not found in reachable scope");
}

/// True when the expression has been rewritten to the empty-tuple `Unit` value.
fn is_unit_tuple(package: &Package, expr_id: ExprId) -> bool {
    let expr = package.get_expr(expr_id);
    let kind_is_empty_tuple = matches!(&expr.kind, ExprKind::Tuple(elems) if elems.is_empty());
    let ty_is_unit = matches!(&expr.ty, Ty::Tuple(elems) if elems.is_empty());
    kind_is_empty_tuple && ty_is_unit
}

/// True when the expression is still a closure.
fn is_closure(package: &Package, expr_id: ExprId) -> bool {
    matches!(package.get_expr(expr_id).kind, ExprKind::Closure(_, _))
}

/// A closure passed directly as an argument to an ordinary (non-UDT) HOF call,
/// which after monomorphization is `ApplyOp_Empty_(closure, q)`.
const CALL_ARG_SOURCE: &str = r#"
    operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
        op(q);
    }
    operation Main() : Unit {
        use q = Qubit();
        ApplyOp(x => H(x), q);
    }
    "#;

/// A closure bound to a `let` local (`let f = closure; f(q)`), so the closure
/// expression is not nested inside any call-argument subtree.
const LET_BOUND_SOURCE: &str = r#"
    operation Main() : Unit {
        let f = x => H(x);
        use q = Qubit();
        f(q);
    }
    "#;

/// A closure passed to a `newtype` UDT constructor (`let w = W(closure)`), so
/// the closure sits inside a UDT-constructor call-argument subtree.
const UDT_CTOR_SOURCE: &str = r#"
    newtype W = (Qubit => Unit);
    operation Main() : Unit {
        let w = W(x => H(x));
        use q = Qubit();
        (w!)(q);
    }
    "#;

/// Filter step: the `specialized_targets.is_empty()` early return. With no
/// consumed targets, the function returns 0 and leaves every closure intact.
#[test]
fn empty_specialized_targets_returns_zero_and_preserves_closure() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(LET_BOUND_SOURCE);
    let (closure_expr, _target) = single_closure(fir_store.get(fir_pkg_id), &reachable_item_ids);

    let package = fir_store.get_mut(fir_pkg_id);
    let replaced = cleanup_consumed_closures(
        package,
        fir_pkg_id,
        &FxHashSet::default(),
        &FxHashSet::default(),
        &reachable_item_ids,
    );

    assert_eq!(
        replaced, 0,
        "no targets specialized, nothing should be cleaned"
    );
    assert!(
        is_closure(package, closure_expr),
        "closure must be preserved when no targets are specialized"
    );
}

/// Filter step: the `specialized_targets.contains(target)` membership test.
/// When the set holds an unrelated callable id, the closure's target does not
/// match, so the closure is preserved.
#[test]
fn non_matching_target_preserves_closure() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(LET_BOUND_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);
    // Use `Main` as a real-but-unrelated id that is not the closure target.
    let unrelated = find_callable(package, &reachable_item_ids, "Main");
    assert_ne!(
        unrelated, target,
        "Main must differ from the closure target"
    );

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(unrelated);

    let package = fir_store.get_mut(fir_pkg_id);
    let replaced = cleanup_consumed_closures(
        package,
        fir_pkg_id,
        &specialized_targets,
        &FxHashSet::default(),
        &reachable_item_ids,
    );

    assert_eq!(replaced, 0, "closure target is not in specialized set");
    assert!(
        is_closure(package, closure_expr),
        "closure must be preserved when its target is not specialized"
    );
}

/// Filter step: the positive cleanup path. A consumed closure that is not a
/// live call argument is replaced with the empty-tuple `Unit` value.
#[test]
fn consumed_closure_outside_call_arg_is_cleaned() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(LET_BOUND_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);

    let package = fir_store.get_mut(fir_pkg_id);
    let replaced = cleanup_consumed_closures(
        package,
        fir_pkg_id,
        &specialized_targets,
        &FxHashSet::default(),
        &reachable_item_ids,
    );

    assert_eq!(
        replaced, 1,
        "the consumed let-bound closure should be cleaned"
    );
    assert!(
        is_unit_tuple(package, closure_expr),
        "cleaned closure must become an empty-tuple Unit value"
    );
}

/// Filter step: the `skip_items.contains(item_id)` guard. Even when the
/// closure's target is specialized, a closure inside a skipped (freshly
/// specialized) item is left untouched.
#[test]
fn closure_in_skipped_item_is_preserved() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(LET_BOUND_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);
    // The closure lives in `Main`'s body; skipping `Main` must suppress cleanup.
    let main_id = find_callable(package, &reachable_item_ids, "Main");

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);
    let mut skip_items = FxHashSet::default();
    skip_items.insert(main_id);

    let package = fir_store.get_mut(fir_pkg_id);
    let replaced = cleanup_consumed_closures(
        package,
        fir_pkg_id,
        &specialized_targets,
        &skip_items,
        &reachable_item_ids,
    );

    assert_eq!(replaced, 0, "closure in a skipped item must not be cleaned");
    assert!(
        is_closure(package, closure_expr),
        "closure must be preserved when its enclosing item is skipped"
    );
}

/// Filter step: the `!call_arg_exprs.contains(expr_id)` guard. A consumed
/// closure that is still a live argument of an ordinary HOF call must survive
/// so a later fixpoint iteration can specialize on it.
#[test]
fn live_call_arg_closure_is_preserved() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(CALL_ARG_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);

    let package = fir_store.get_mut(fir_pkg_id);
    let replaced = cleanup_consumed_closures(
        package,
        fir_pkg_id,
        &specialized_targets,
        &FxHashSet::default(),
        &reachable_item_ids,
    );

    assert_eq!(
        replaced, 0,
        "a live call-argument closure must not be cleaned"
    );
    assert!(
        is_closure(package, closure_expr),
        "closure passed as a live HOF argument must be preserved"
    );
}

/// Filter step: the `is_udt_ctor_call` exception. A closure inside a UDT
/// constructor call-argument subtree is a structural wrapper, not a live HOF
/// argument, so it remains eligible for cleanup.
#[test]
fn udt_ctor_wrapped_closure_is_cleaned() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(UDT_CTOR_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);

    let package = fir_store.get_mut(fir_pkg_id);
    let replaced = cleanup_consumed_closures(
        package,
        fir_pkg_id,
        &specialized_targets,
        &FxHashSet::default(),
        &reachable_item_ids,
    );

    assert_eq!(
        replaced, 1,
        "closure inside a UDT-constructor call must be cleaned"
    );
    assert!(
        is_unit_tuple(package, closure_expr),
        "cleaned UDT-wrapped closure must become an empty-tuple Unit value"
    );
}
