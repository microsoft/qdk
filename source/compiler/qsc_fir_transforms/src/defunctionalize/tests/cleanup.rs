// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Closure cleanup tests using source-compiled, monomorphized FIR and explicit
//! consumed-target and skipped-owner sets. Assertions cover live-value preservation,
//! package isolation, replacement signatures, and agreement with convergence counting.
//!
//! An eligible closure references its target only when capture-free and its full
//! signature matches; otherwise cleanup uses a typed fail-bodied stand-in.

use super::*;
use crate::defunctionalize::{
    ClosureStandInCache, ConsumedClosures, ConsumedClosuresInPackage, cleanup_consumed_closures,
    cleanup_consumed_closures_per_package, remaining_callable_value_info,
};
use crate::package_assigners::PackageAssigners;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    CallableImpl, ExprId, ExprKind, LocalItemId, Package, PackageLookup, PatKind, Res, StmtKind,
    StoreItemId,
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
/// Counts closures before and after cleanup; production convergence instead
/// uses the separate remaining-work analysis.
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

fn setup_with_dead_callee_reads(
    source: &str,
    reads: &[&str],
) -> (fir::PackageStore, fir::PackageId, Vec<LocalItemId>) {
    let (mut store, package_id, reachable) = setup(source);
    let closures = all_closures(store.get(package_id), &reachable);
    assert!(!closures.is_empty());
    let targets = closures.iter().map(|(_, target)| *target).collect();
    assert_eq!(
        run_cleanup(
            &mut store,
            package_id,
            &consumed(targets, FxHashSet::default()),
            &reachable,
        ),
        0,
        "live callee dependencies must survive even when their targets are consumed"
    );
    assert_eq!(all_closures(store.get(package_id), &reachable), closures);

    let mut dead_source = source.to_string();
    for read in reads {
        assert_eq!(dead_source.matches(read).count(), 1);
        dead_source = dead_source.replace(read, "");
    }
    let result = setup(&dead_source);
    assert_eq!(
        all_closures(result.0.get(result.1), &result.2).len(),
        closures.len()
    );
    result
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

/// True when the expression is an arrow-typed reference to `target` in this package.
fn is_arrow_typed_ref_to(package: &Package, expr_id: ExprId, target: LocalItemId) -> bool {
    let expr = package.get_expr(expr_id);
    let names_target = matches!(
        &expr.kind,
        ExprKind::Var(Res::Item(item_id), args)
            if item_id.package == package.id && item_id.item == target && args.is_empty()
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
    assert_eq!(
        item_id.package, package.id,
        "replacement must reference an item in the same package"
    );
    assert!(args.is_empty(), "replacement must carry no generic args");
    item_id.item
}

/// Whether `item_id` names a cleanup stand-in with a single `fail` body.
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

/// The same shape with a capturing closure. Its lifted target takes captures
/// as leading parameters, so cleanup must synthesize a stand-in.
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

/// No reads can protect this binding, so tests isolate the cleanup filters.
const UNUSED_LET_BOUND_SOURCE: &str = r#"
    operation Main() : Unit {
        let offset = 17;
        let f = x -> x+offset;
    }
    "#;

/// A capturing closure inside a UDT constructor. Cleanup must preserve the
/// field's arrow type even when the wrapper remains but its callable is unused.
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

/// With no consumed targets, cleanup leaves every closure intact and the test
/// helper reports zero replacements.
#[test]
fn cleanup_without_consumed_targets_preserves_closures() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(UNUSED_LET_BOUND_SOURCE);
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

#[test]
fn callable_argument_aliases_preserve_their_closure_initializers() {
    let source = r#"
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        operation Main() : Int {
            let offset = 17;
            let f = x -> x+offset;
            let firstAlias = f;
            let secondAlias = firstAlias;
            Apply(secondAlias, 3)
        }
    "#;
    let (mut store, package_id, reachable) = setup(source);
    let (closure, target) = single_closure(store.get(package_id), &reachable);
    let replaced = run_cleanup(
        &mut store,
        package_id,
        &consumed(FxHashSet::from_iter([target]), FxHashSet::default()),
        &reachable,
    );
    assert_eq!(replaced, 0, "a live aliased argument is not consumed");
    assert!(is_closure(store.get(package_id), closure));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn callable_argument_assignments_preserve_all_reaching_closures() {
    let source = r#"
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        operation Main() : Int {
            let offset = 17;
            mutable f = x -> x+offset;
            set f = x -> x-offset;
            Apply(f, 3)
        }
    "#;
    let (mut store, package_id, reachable) = setup(source);
    let closures = all_closures(store.get(package_id), &reachable);
    assert_eq!(closures.len(), 2);
    let targets = closures.iter().map(|(_, target)| *target).collect();
    let replaced = run_cleanup(
        &mut store,
        package_id,
        &consumed(targets, FxHashSet::default()),
        &reachable,
    );
    assert_eq!(
        replaced, 0,
        "both definitions of a live argument must survive"
    );
    for (closure, _) in closures {
        assert!(is_closure(store.get(package_id), closure));
    }
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn tuple_bound_argument_preserves_transitively_captured_closure() {
    let source = r#"
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        operation Main() : Int {
            let offset = 17;
            let f = x -> x+offset;
            let (_, g) = (0, x -> 2*f(x));
            Apply(g, 3)
        }
    "#;
    let (mut store, package_id, reachable) = setup(source);
    let closures = all_closures(store.get(package_id), &reachable);
    assert_eq!(closures.len(), 2);
    let targets = closures.iter().map(|(_, target)| *target).collect();
    let replaced = run_cleanup(
        &mut store,
        package_id,
        &consumed(targets, FxHashSet::default()),
        &reachable,
    );
    assert_eq!(
        replaced, 0,
        "the argument and its captured callable remain live"
    );
    for (closure, _) in closures {
        assert!(is_closure(store.get(package_id), closure));
    }
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn cyclic_callable_alias_assignments_preserve_live_closures_and_terminate() {
    let source = r#"
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        operation Main() : Int {
            let offset = 17;
            mutable first = x -> x+offset;
            mutable second = first;
            set first = second;
            set second = first;
            Apply(second, 3)
        }
    "#;
    let (mut store, package_id, reachable) = setup(source);
    let (closure, target) = single_closure(store.get(package_id), &reachable);
    let replaced = run_cleanup(
        &mut store,
        package_id,
        &consumed(FxHashSet::from_iter([target]), FxHashSet::default()),
        &reachable,
    );
    assert_eq!(replaced, 0);
    assert!(is_closure(store.get(package_id), closure));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn unresolved_direct_callee_preserves_its_closure_initializer() {
    let source = r#"
        operation Main() : Int {
            let offset = 17;
            let f = x -> x+offset;
            f(3)
        }
    "#;
    let (mut store, package_id, reachable) = setup(source);
    let (closure, target) = single_closure(store.get(package_id), &reachable);
    let replaced = run_cleanup(
        &mut store,
        package_id,
        &consumed(FxHashSet::from_iter([target]), FxHashSet::default()),
        &reachable,
    );
    assert_eq!(replaced, 0, "the callee still reads its closure value");
    assert!(is_closure(store.get(package_id), closure));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn live_let_bound_callables_are_preserved_with_and_without_udt_wrapping() {
    for source in [LET_BOUND_SOURCE, UDT_CTOR_SOURCE] {
        let (mut store, package_id, reachable) = setup(source);
        let (closure, target) = single_closure(store.get(package_id), &reachable);
        let replaced = run_cleanup(
            &mut store,
            package_id,
            &consumed(FxHashSet::from_iter([target]), FxHashSet::default()),
            &reachable,
        );
        assert_eq!(
            replaced, 0,
            "a surviving callable usage needs its initializer"
        );
        assert!(is_closure(store.get(package_id), closure));
    }
}

/// Filter step: the `specialized_targets.contains(target)` membership test.
/// When the set holds an unrelated callable id, the closure's target does not
/// match, so the closure is preserved.
#[test]
fn non_matching_target_preserves_closure() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(UNUSED_LET_BOUND_SOURCE);
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

/// A consumed capturing closure with no live reads gets an arrow-typed stand-in.
#[test]
fn unused_consumed_capturing_closure_gets_typed_stand_in() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(UNUSED_LET_BOUND_SOURCE);
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

    assert_eq!(replaced, 1, "the let-bound closure should be replaced");
    let package = fir_store.get(fir_pkg_id);
    let replacement = referenced_item(package, closure_expr);
    assert_ne!(
        replacement, target,
        "capturing closures cannot name their lifted target"
    );
    assert!(is_arrow_typed_ref_to(package, closure_expr, replacement));
    assert!(is_fail_bodied_stand_in(package, replacement));
    assert_eq!(package.get_expr(closure_expr).ty, closure_ty);
}

/// Capture-free replacements must match the referenced callable's actual signature,
/// not merely preserve the arrow annotation on the expression. Check lifted inputs
/// both with their extra tuple layer and after that layer has been removed.
#[test]
fn capture_free_replacements_match_target_signatures() {
    for lambda in ["() -> 1", "x -> x + 1", "(x, y) -> x + y + 1"] {
        for normalized_input in [false, true] {
            let source = format!("operation Main() : Unit {{ let unused = {lambda}; }}");
            let (mut store, package_id, reachable) = setup(&source);
            let (closure, target) = single_closure(store.get(package_id), &reachable);
            let package = store.get_mut(package_id);
            let ExprKind::Closure(captures, _) = &package.get_expr(closure).kind else {
                panic!("expected closure");
            };
            assert!(captures.is_empty(), "{lambda}");
            let closure_ty = package.get_expr(closure).ty.clone();
            if normalized_input {
                let ItemKind::Callable(decl) = &package.get_item(target).kind else {
                    panic!("closure target must be a callable");
                };
                let PatKind::Tuple(inputs) = &package.get_pat(decl.input).kind else {
                    panic!("lifted input must be a tuple");
                };
                let [input] = inputs[..] else {
                    panic!("capture-free target must have one input");
                };
                let ItemKind::Callable(decl) =
                    &mut package.items.get_mut(target).expect("target exists").kind
                else {
                    unreachable!("target was checked above");
                };
                decl.input = input;
            }
            assert_eq!(
                run_cleanup(
                    &mut store,
                    package_id,
                    &consumed(FxHashSet::from_iter([target]), FxHashSet::default()),
                    &reachable,
                ),
                1,
                "{lambda}"
            );
            let package = store.get(package_id);
            let replacement = referenced_item(package, closure);
            assert_eq!(replacement == target, normalized_input, "{lambda}");
            if !normalized_input {
                assert!(is_fail_bodied_stand_in(package, replacement), "{lambda}");
            }
            assert_eq!(package.get_expr(closure).ty, closure_ty, "{lambda}");
            let Ty::Arrow(arrow) = closure_ty else {
                panic!("closure must have an arrow type");
            };
            let ItemKind::Callable(decl) = &package.get_item(replacement).kind else {
                panic!("replacement must be a callable");
            };
            assert_eq!(decl.kind, arrow.kind, "{lambda}");
            assert_eq!(package.get_pat(decl.input).ty, *arrow.input, "{lambda}");
            assert_eq!(decl.output, *arrow.output, "{lambda}");
            assert_eq!(
                qsc_fir::ty::FunctorSet::Value(decl.functors),
                arrow.functors,
                "{lambda}"
            );
        }
    }
}

/// Filter step: the skipped-owner guard. Even when the
/// closure's target is specialized, a closure inside a skipped (freshly
/// specialized) item is left untouched.
#[test]
fn closure_in_skipped_item_is_preserved() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(UNUSED_LET_BOUND_SOURCE);
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
/// argument, so it remains eligible for cleanup when the wrapper is unused.
///
/// The aggregate field keeps its arrow type through a fail-bodied stand-in.
#[test]
fn unused_udt_wrapped_closure_gets_typed_stand_in() {
    let source = r#"
        newtype W = (Int -> Int);
        operation Main() : Unit {
            let offset = 17;
            let w = W(x -> x+offset);
        }
    "#;
    let (mut fir_store, fir_pkg_id, reachable_item_ids) = setup(source);
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

    assert_eq!(replaced, 1, "the UDT-wrapped closure should be replaced");
    let package = fir_store.get(fir_pkg_id);
    let replacement = referenced_item(package, closure_expr);
    assert_ne!(replacement, target);
    assert!(is_arrow_typed_ref_to(package, closure_expr, replacement));
    assert!(is_fail_bodied_stand_in(package, replacement));
    assert_eq!(package.get_expr(closure_expr).ty, closure_ty);
}

/// A capturing closure's target has extra parameters. Its aggregate-slot
/// replacement must instead name a fail-bodied callable with the closure's
/// kind, input, and output types.
#[test]
fn capturing_closure_in_aggregate_slot_gets_fail_bodied_stand_in() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) =
        setup_with_dead_callee_reads(CAPTURING_UDT_CTOR_SOURCE, &["(w!)(q);"]);
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

/// Cleanup must preserve an unused local initializer's arrow type, not replace
/// its closure with Unit.
#[test]
fn capturing_closure_in_local_initializer_gets_stand_in() {
    let (mut fir_store, fir_pkg_id, reachable_item_ids) =
        setup_with_dead_callee_reads(CAPTURING_LET_BOUND_SOURCE, &["f(q);"]);
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
    let (mut fir_store, fir_pkg_id, reachable_item_ids) =
        setup_with_dead_callee_reads(TWO_CAPTURING_CLOSURES_SOURCE, &["f(q);", "g(q);"]);
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

/// The library and user package deliberately reuse both the owner and target
/// `LocalItemId`s. Cross-package consumption and skipped-owner sets must distinguish
/// them by `PackageId` before cleanup projects either set to local IDs.
/// Both sources export their owner to keep the item layouts aligned; the ID
/// assertions make the collision requirement explicit rather than incidental.
#[test]
fn cleanup_keeps_colliding_local_item_ids_isolated_between_packages() {
    let (store, user_package) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        r#"
        namespace Lib {
            operation Owner() : Unit {
                let offset = 1;
                let unused = x -> x + offset;
            }
            export Owner;
        }
        "#,
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                let offset = 2;
                let unused = x -> x + offset;
                Lib.Owner();
            }
            export Main;
        }
        "#,
        crate::PipelineStage::Mono,
    );
    let library_owner = crate::test_utils::find_library_callable(&store, user_package, "Owner");
    let user_owner = StoreItemId::from((
        user_package,
        crate::test_utils::callable_id_by_name(store.get(user_package), "Main"),
    ));
    let owners = [library_owner, user_owner];
    let closures = owners.map(|owner| single_closure(store.get(owner.package), &[owner.item]));
    let targets: [StoreItemId; 2] =
        std::array::from_fn(|index| StoreItemId::from((owners[index].package, closures[index].1)));
    assert_ne!(library_owner.package, user_owner.package);
    assert_eq!(
        library_owner.item, user_owner.item,
        "owner IDs must collide"
    );
    assert_eq!(targets[0].item, targets[1].item, "target IDs must collide");
    let reachable = collect_reachable_from_entry(&store, user_package);
    assert!(owners.iter().all(|owner| reachable.contains(owner)));

    // Expected replacements are ordered [library, user]. Consuming both also
    // checks that the shared cache creates a stand-in in each owning package.
    for (label, consumed_targets, skipped, replaced) in [
        (
            "consume library only",
            vec![targets[0]],
            vec![],
            [true, false],
        ),
        ("consume user only", vec![targets[1]], vec![], [false, true]),
        (
            "skip library owner",
            targets.to_vec(),
            vec![owners[0]],
            [false, true],
        ),
        (
            "skip user owner",
            targets.to_vec(),
            vec![owners[1]],
            [true, false],
        ),
        ("consume both", targets.to_vec(), vec![], [true, true]),
    ] {
        let mut store = store.clone();
        let consumed = ConsumedClosures {
            targets: consumed_targets.into_iter().collect(),
            skipped: skipped.into_iter().collect(),
        };
        let mut assigners = PackageAssigners::new(&store, user_package);
        cleanup_consumed_closures_per_package(
            &mut store,
            user_package,
            &reachable,
            &consumed,
            &mut assigners,
            &mut ClosureStandInCache::default(),
        );
        for (index, owner) in owners.iter().enumerate() {
            let package = store.get(owner.package);
            let (closure, target) = closures[index];
            if replaced[index] {
                let replacement = referenced_item(package, closure);
                assert_ne!(replacement, target, "{label}: {owner:?}");
                assert!(
                    is_fail_bodied_stand_in(package, replacement),
                    "{label}: {owner:?}"
                );
            } else {
                assert!(
                    matches!(&package.get_expr(closure).kind,
                        ExprKind::Closure(captures, item) if *item == target && captures.len() == 1),
                    "{label}: {owner:?} must retain its original closure"
                );
            }
        }
    }
}

/// Covers four cleanup dispositions with consumed targets:
/// a `let`-bound closure that is eligible, a closure that is still a live
/// higher-order call argument, a closure inside an item that is skipped, and a
/// closure wrapped by a UDT constructor, which remains live through a callee read.
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
        ApplyOp(x => Z(x), q);
        let w = W(x => S(x));
        (w!)(q);
        InSkippedItem(q);
    }
    "#;

/// The consumed-aware count before cleanup must equal the ordinary count after
/// cleanup. Exactly one dead closure is excluded; live arguments, callees, and
/// skipped owners must remain counted and preserved.
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
    assert_eq!(
        baseline - excluded_by_side_set,
        1,
        "only the dead local closure is consumed"
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
