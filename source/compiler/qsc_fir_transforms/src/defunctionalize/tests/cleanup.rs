// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Unit tests for [`cleanup_consumed_closures`], exercising each filter step
//! that decides whether a consumed closure expression is replaced.
//!
//! Each test compiles Q# to monomorphized FIR (closures still present, before
//! defunctionalization), then calls `cleanup_consumed_closures` directly with
//! crafted `specialized_targets` / `skip_items` inputs to drive one branch of
//! the filter and asserts the resulting expression kinds. This mirrors the
//! source-compilation strategy used by the other defunctionalization tests and
//! avoids any QIR generation.
//!
//! Both replacements keep the closure's arrow type, so nothing type-unsafe is
//! left behind: a capture-free closure becomes a reference to its own target
//! callable, while a capturing closure — whose target takes the captures as
//! leading parameters — becomes a reference to a fail-bodied stand-in
//! synthesized for its signature.

use super::*;
use crate::defunctionalize::{
    ClosureStandInCache, ConsumedClosures, ConsumedClosuresInPackage, cleanup_consumed_closures,
    cleanup_consumed_closures_per_package, remaining_callable_value_info,
};
use crate::package_assigners::PackageAssigners;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    CallableImpl, ExprId, ExprKind, LocalItemId, Package, PackageLookup, Res, StmtKind, StoreItemId,
};
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

/// Runs `cleanup_consumed_closures` with a fresh assigner and stand-in cache,
/// returning how many closures it replaced.
///
/// Cleanup no longer reports a count itself — every eligible closure now gets a
/// well-typed replacement, so there is nothing left for it to hand back — and
/// the tests below still need one, so it is recovered by differencing the
/// closure census across the call.
fn run_cleanup(
    fir_store: &mut fir::PackageStore,
    fir_pkg_id: fir::PackageId,
    consumed: &ConsumedClosuresInPackage,
    reachable_item_ids: &[LocalItemId],
) -> usize {
    let before = all_closures(fir_store.get(fir_pkg_id), reachable_item_ids).len();
    let mut assigner = Assigner::from_package(fir_store.get(fir_pkg_id));
    cleanup_consumed_closures(
        fir_store.get_mut(fir_pkg_id),
        &mut assigner,
        fir_pkg_id,
        consumed,
        reachable_item_ids,
        &mut ClosureStandInCache::default(),
    );
    before - all_closures(fir_store.get(fir_pkg_id), reachable_item_ids).len()
}

/// Builds the per-package consumed-closure projection that
/// `cleanup_consumed_closures` and the remaining-work count both consult.
fn consumed(
    targets: FxHashSet<LocalItemId>,
    skipped: FxHashSet<LocalItemId>,
) -> ConsumedClosuresInPackage {
    ConsumedClosuresInPackage { targets, skipped }
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

/// True when the expression is a reference to `target` that kept the closure's
/// arrow type, which is the well-typed replacement a capture-free closure gets.
fn is_arrow_typed_ref_to(package: &Package, expr_id: ExprId, target: LocalItemId) -> bool {
    let expr = package.get_expr(expr_id);
    let names_target = matches!(
        &expr.kind,
        ExprKind::Var(Res::Item(item_id), args) if item_id.item == target && args.is_empty()
    );
    names_target && matches!(&expr.ty, Ty::Arrow(_))
}

/// The item a replaced closure now references, asserting the node is an
/// arrow-typed `Var(Res::Item(_))` in the same package.
fn referenced_item(package: &Package, expr_id: ExprId) -> LocalItemId {
    let expr = package.get_expr(expr_id);
    assert!(
        matches!(&expr.ty, Ty::Arrow(_)),
        "replacement must keep the closure's arrow type, found {:?}",
        expr.ty
    );
    let ExprKind::Var(Res::Item(item_id), args) = &expr.kind else {
        panic!(
            "replacement must be an item reference, found {:?}",
            expr.kind
        );
    };
    assert!(args.is_empty(), "replacement must carry no generic args");
    item_id.item
}

/// True when `item_id` names a synthesized stand-in: a callable whose body is a
/// single `fail`, which is what makes it well-typed for any output type while
/// being impossible to invoke silently.
fn is_fail_bodied_stand_in(package: &Package, item_id: LocalItemId) -> bool {
    let ItemKind::Callable(decl) = &package.get_item(item_id).kind else {
        return false;
    };
    if !decl.name.name.starts_with("__defunc_consumed_closure_") {
        return false;
    }
    let CallableImpl::Spec(spec) = &decl.implementation else {
        return false;
    };
    let block = package.get_block(spec.body.block);
    let [stmt_id] = block.stmts[..] else {
        return false;
    };
    let (StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id)) = package.get_stmt(stmt_id).kind else {
        return false;
    };
    matches!(package.get_expr(expr_id).kind, ExprKind::Fail(_))
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

/// The same shape with a *capturing* closure. A capture-free closure can name
/// its own target callable, but a capturing closure cannot, because that target
/// takes the captures as leading parameters. This fixture is therefore the one
/// that drives the synthesized stand-in branch.
const CAPTURING_LET_BOUND_SOURCE: &str = r#"
    operation Main() : Unit {
        let angle = 1.0;
        let f = x => Rx(angle, x);
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

/// The aggregate-slot shape that has no well-typed replacement without a
/// synthesized stand-in: a *capturing* closure as the argument of a UDT
/// constructor. The constructor's argument slot keeps the field's arrow type
/// over whatever replaces the closure, and no invariant walks that slot, so a
/// `Unit` there would be silent invalid FIR.
///
/// This is the shape `Std.TableLookup.MakeAndChain` presents, which is why it
/// is exercised directly here as well as end to end.
const CAPTURING_UDT_CTOR_SOURCE: &str = r#"
    newtype W = (Qubit => Unit);
    operation Main() : Unit {
        let angle = 1.0;
        let w = W(x => Rx(angle, x));
        use q = Qubit();
        (w!)(q);
    }
    "#;

/// Two capturing closures of the same signature in the same package, used to
/// pin that the stand-in cache issues one synthesized item for both rather than
/// one per slot.
const TWO_CAPTURING_CLOSURES_SOURCE: &str = r#"
    operation Main() : Unit {
        let angle = 1.0;
        let f = x => Rx(angle, x);
        let g = x => Ry(angle, x);
        use q = Qubit();
        f(q);
        g(q);
    }
    "#;

/// Filter step: the `specialized_targets.is_empty()` early return. With no
/// consumed targets, the function returns 0 and leaves every closure intact.
#[test]
fn empty_specialized_targets_returns_zero_and_preserves_closure() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(LET_BOUND_SOURCE);
    let (closure_expr, _target) = single_closure(fir_store.get(fir_pkg_id), &reachable_item_ids);

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(FxHashSet::default(), FxHashSet::default()),
        &reachable_item_ids,
    );

    assert_eq!(
        replaced, 0,
        "no targets specialized, nothing should be cleaned"
    );
    assert!(
        is_closure(fir_store.get(fir_pkg_id), closure_expr),
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

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, FxHashSet::default()),
        &reachable_item_ids,
    );

    assert_eq!(replaced, 0, "closure target is not in specialized set");
    assert!(
        is_closure(fir_store.get(fir_pkg_id), closure_expr),
        "closure must be preserved when its target is not specialized"
    );
}

/// Filter step: the positive cleanup path. A consumed capture-free closure that
/// is not a live call argument is replaced with a reference to its own target
/// callable, which carries the same arrow type, so nothing type-unsafe is left
/// behind.
#[test]
fn consumed_closure_outside_call_arg_is_cleaned() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(LET_BOUND_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, FxHashSet::default()),
        &reachable_item_ids,
    );

    assert_eq!(replaced, 1, "the let-bound closure should be replaced");
    assert!(
        is_arrow_typed_ref_to(fir_store.get(fir_pkg_id), closure_expr, target),
        "cleaned closure must become an arrow-typed reference to its target"
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

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, skip_items),
        &reachable_item_ids,
    );

    assert_eq!(replaced, 0, "closure in a skipped item must not be cleaned");
    assert!(
        is_closure(fir_store.get(fir_pkg_id), closure_expr),
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

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, FxHashSet::default()),
        &reachable_item_ids,
    );

    assert_eq!(
        replaced, 0,
        "a live call-argument closure must not be cleaned"
    );
    assert!(
        is_closure(fir_store.get(fir_pkg_id), closure_expr),
        "closure passed as a live HOF argument must be preserved"
    );
}

/// Filter step: the `is_udt_ctor_call` exception. A closure inside a UDT
/// constructor call-argument subtree is a structural wrapper, not a live HOF
/// argument, so it remains eligible for cleanup.
///
/// This is the aggregate-slot position: the constructor's argument tuple keeps
/// the field's arrow type over whatever replaces the closure, and no invariant
/// walks it. The replacement must therefore be arrow-typed, which is what the
/// target reference gives.
#[test]
fn udt_ctor_wrapped_closure_is_cleaned() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(UDT_CTOR_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, FxHashSet::default()),
        &reachable_item_ids,
    );

    assert_eq!(replaced, 1, "the UDT-wrapped closure should be replaced");
    assert!(
        is_arrow_typed_ref_to(fir_store.get(fir_pkg_id), closure_expr, target),
        "the constructor argument slot must keep its arrow type"
    );
}

/// The capturing counterpart of the test above, and the shape that closes the
/// last blanking hole. `Std.TableLookup.MakeAndChain` presents exactly this:
/// a capturing closure as a UDT-constructor argument, in a body that stays
/// entry-reachable.
///
/// The closure captures, so it cannot name its own target — that target takes
/// the captures as leading parameters. Cleanup must therefore synthesize a
/// stand-in with the closure's own signature and leave the slot arrow-typed,
/// not rewrite the node to `Unit`.
#[test]
fn capturing_closure_in_aggregate_slot_gets_fail_bodied_stand_in() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(CAPTURING_UDT_CTOR_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let (closure_expr, target) = single_closure(package, &reachable_item_ids);
    let closure_ty = package.get_expr(closure_expr).ty.clone();

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, FxHashSet::default()),
        &reachable_item_ids,
    );
    assert_eq!(replaced, 1, "the capturing closure should be replaced");

    let package = fir_store.get(fir_pkg_id);
    assert!(
        !is_unit_tuple(package, closure_expr),
        "the aggregate slot must not be left holding a Unit"
    );
    assert_eq!(
        package.get_expr(closure_expr).ty,
        closure_ty,
        "the replacement must keep the closure's own arrow type"
    );

    let stand_in = referenced_item(package, closure_expr);
    assert_ne!(
        stand_in, target,
        "a capturing closure cannot name its target, whose leading parameters are the captures"
    );
    assert!(
        is_fail_bodied_stand_in(package, stand_in),
        "the replacement must reference a synthesized fail-bodied stand-in"
    );

    let Ty::Arrow(arrow) = &closure_ty else {
        panic!("a closure expression always carries an arrow type")
    };
    let ItemKind::Callable(decl) = &package.get_item(stand_in).kind else {
        panic!("the stand-in must be a callable item")
    };
    assert_eq!(decl.kind, arrow.kind, "stand-in callable kind must match");
    assert_eq!(
        package.get_pat(decl.input).ty,
        *arrow.input,
        "stand-in input type must match the closure's arrow input"
    );
    assert_eq!(
        decl.output, *arrow.output,
        "stand-in output type must match the closure's arrow output"
    );
}

/// The same stand-in replacement in local-initializer position. This is where
/// blanking used to leave a `Unit`-valued initializer under an arrow-typed
/// `Local` in a still-reachable item, which no invariant walked.
#[test]
fn capturing_closure_in_local_initializer_gets_stand_in() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(CAPTURING_LET_BOUND_SOURCE);
    let (closure_expr, target) = single_closure(fir_store.get(fir_pkg_id), &reachable_item_ids);

    let mut specialized_targets = FxHashSet::default();
    specialized_targets.insert(target);

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, FxHashSet::default()),
        &reachable_item_ids,
    );
    assert_eq!(replaced, 1, "the let-bound closure should be replaced");

    let package = fir_store.get(fir_pkg_id);
    assert!(
        !is_unit_tuple(package, closure_expr),
        "the local initializer must not be left holding a Unit"
    );
    assert!(
        is_fail_bodied_stand_in(package, referenced_item(package, closure_expr)),
        "the replacement must reference a synthesized fail-bodied stand-in"
    );
}

/// One stand-in serves every slot of the same signature. Both closures here are
/// `Qubit => Unit` with one captured `Double`, so a per-slot synthesis would
/// leave two items behind.
#[test]
fn same_signature_capturing_closures_share_one_stand_in() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(TWO_CAPTURING_CLOSURES_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let closures = all_closures(package, &reachable_item_ids);
    assert_eq!(closures.len(), 2, "the fixture should present two closures");

    let specialized_targets: FxHashSet<LocalItemId> =
        closures.iter().map(|(_, target)| *target).collect();

    let replaced = run_cleanup(
        &mut fir_store,
        fir_pkg_id,
        &consumed(specialized_targets, FxHashSet::default()),
        &reachable_item_ids,
    );
    assert_eq!(replaced, 2, "both closures should be replaced");

    let package = fir_store.get(fir_pkg_id);
    let first = referenced_item(package, closures[0].0);
    let second = referenced_item(package, closures[1].0);
    assert!(
        is_fail_bodied_stand_in(package, first),
        "both replacements must reference a synthesized stand-in"
    );
    assert_eq!(
        first, second,
        "closures of the same signature must share one synthesized stand-in"
    );
}

/// A program covering every branch of the consumed-closure predicate at once:
/// a `let`-bound closure that is eligible, a closure that is still a live
/// higher-order call argument, a closure inside an item that is skipped, and a
/// closure wrapped by a UDT constructor, which is eligible despite sitting in a
/// call-argument subtree.
const MIXED_DISPOSITION_SOURCE: &str = r#"
    newtype W = (Qubit => Unit);
    operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
        op(q);
    }
    operation InSkippedItem(q : Qubit) : Unit {
        let g = x => Y(x);
        g(q);
    }
    operation Main() : Unit {
        use q = Qubit();
        let f = x => H(x);
        f(q);
        ApplyOp(x => Z(x), q);
        let w = W(x => S(x));
        (w!)(q);
        InSkippedItem(q);
    }
    "#;

/// Closure cleanup and remaining-work counting must classify exactly the same
/// closures as consumed. They share the two set conditions but each computes
/// the call-argument condition over its own walk, so this pins that the two
/// walks agree end to end rather than only on the shared half.
///
/// The count taken before cleanup with the consumed side set must equal the
/// count taken after cleanup with no side set at all: whatever the side set
/// excluded is exactly what cleanup replaced. A disagreement in either
/// direction shows up here — a closure counted but replaced, or excluded from
/// the count but left standing.
#[test]
fn remaining_count_and_cleanup_agree_on_consumed_closures() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(MIXED_DISPOSITION_SOURCE);
    let package = fir_store.get(fir_pkg_id);
    let closures = all_closures(package, &reachable_item_ids);
    assert_eq!(
        closures.len(),
        4,
        "the fixture should present all four predicate branches"
    );

    // Every closure target is consumed, so only the call-argument and skip
    // conditions decide the outcome.
    let targets: FxHashSet<StoreItemId> = closures
        .iter()
        .map(|(_, target)| StoreItemId::from((fir_pkg_id, *target)))
        .collect();
    let skipped_item = find_callable(package, &reachable_item_ids, "InSkippedItem");
    let mut specialized_items = FxHashSet::default();
    specialized_items.insert(StoreItemId::from((fir_pkg_id, skipped_item)));

    let consumed = ConsumedClosures::new(&fir_store, &targets, &specialized_items);
    let reachable = collect_reachable_from_entry(&fir_store, fir_pkg_id);

    let (_, baseline, _, _) =
        remaining_callable_value_info(&fir_store, fir_pkg_id, &ConsumedClosures::default());
    let (_, excluded_by_side_set, _, _) =
        remaining_callable_value_info(&fir_store, fir_pkg_id, &consumed);
    assert!(
        excluded_by_side_set < baseline,
        "the side set must exclude something, otherwise the agreement is vacuous"
    );

    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    cleanup_consumed_closures_per_package(
        &mut fir_store,
        fir_pkg_id,
        &reachable,
        &consumed,
        &mut assigners,
        &mut ClosureStandInCache::default(),
    );

    let (_, after_cleanup, _, _) =
        remaining_callable_value_info(&fir_store, fir_pkg_id, &ConsumedClosures::default());
    assert_eq!(
        excluded_by_side_set,
        after_cleanup,
        "counting and cleanup disagree: the side set excluded {} closures but cleanup removed {}",
        baseline - excluded_by_side_set,
        baseline - after_cleanup
    );
}
