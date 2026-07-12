// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::package_assigners::PackageAssigners;

use super::*;
use expect_test::expect;
use indoc::indoc;

use super::super::build_spec_key;
use super::super::types::CallSite;
use qsc_fir::fir::{ExprId, ItemId, LocalItemId, PackageId};

/// Regression guard: two call sites that differ only in the package owning the
/// closure body (`call_pkg_id`) must produce distinct `SpecKey`s. The closure
/// dispatch key is package-qualified via `StoreItemId`, so closures sharing the
/// same package-local `LocalItemId` in different packages cannot collide onto a
/// single specialization.
///
/// This is asserted directly against `build_spec_key` rather than relying on a
/// natural id collision, so the guard is deterministic.
#[test]
fn closure_spec_key_distinguishes_packages() {
    // Build a call site whose only varying field is `call_pkg_id`; the closure
    // value (package-local target, no captures, no functors) and the HOF being
    // called are identical across both sites.
    let make_site = |call_pkg_id: PackageId| CallSite {
        call_expr_id: ExprId::from(0u32),
        call_pkg_id,
        hof_item_id: ItemId {
            package: PackageId::from(0usize),
            item: LocalItemId::from(7usize),
        },
        top_level_param: 0,
        field_path: vec![],
        hof_input_is_tuple: false,
        callable_arg: ConcreteCallable::Closure {
            target: LocalItemId::from(0usize),
            captures: vec![],
            functor: FunctorApp::default(),
        },
        arg_expr_id: ExprId::from(0u32),
        condition: vec![],
    };

    let key_pkg_1 = build_spec_key(&make_site(PackageId::from(1usize)));
    let key_pkg_2 = build_spec_key(&make_site(PackageId::from(2usize)));

    assert_ne!(
        key_pkg_1, key_pkg_2,
        "closure spec keys must be package-qualified to avoid cross-package collisions"
    );
}

/// Specializing a library higher-order callable whose body contains a nested
/// lambda into a different package relocates the lambda into that package with
/// a fresh id, so the resulting closure references a real item instead of
/// dangling. Exercises the shape used by public std operations like
/// `ApplyIfEqualL`.
#[test]
fn cross_package_hof_body_with_nested_lambda_clones_into_target() {
    // `LibApply`'s body builds a nested lambda whose lifted item lives in the
    // library package. Calling it with a concrete operation specializes the
    // library body into the entry package.
    let lib_source = r#"
        namespace TestLib {
            operation LibApply(op : Int => Unit, x : Int) : Unit {
                let g = y => op(x + y);
                g(1);
            }
            export LibApply;
        }
    "#;
    let user_source = r#"
        import TestLib.*;

        operation Noop(x : Int) : Unit {}
        @EntryPoint()
        operation Main() : Unit {
            LibApply(Noop, 5);
        }
    "#;

    let (mut fir_store, fir_pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    crate::monomorphize::monomorphize(&mut fir_store, fir_pkg_id, &mut assigners);

    // Snapshot entry-package callable ids so the relocated lambda can be
    // identified after specialization.
    let entry_items_before: std::collections::BTreeSet<LocalItemId> = fir_store
        .get(fir_pkg_id)
        .items
        .iter()
        .filter(|(_, item)| matches!(item.kind, ItemKind::Callable(_)))
        .map(|(id, _)| id)
        .collect();

    // Specialization succeeds because the closure target is extracted and
    // relocated rather than left dangling.
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors(
        "cross_package_hof_body_with_nested_lambda_clones_into_target",
        &errors,
    );

    // No closure may reference an item id that is absent from its own package.
    for (pkg_id, package) in &fir_store {
        for expr in package.exprs.values() {
            if let fir::ExprKind::Closure(_, target_item) = &expr.kind {
                assert!(
                    package.items.contains_key(*target_item),
                    "closure in package {pkg_id} references item {target_item} \
                     that does not exist in that package (dangling cross-package id)"
                );
            }
        }
    }

    // The specialized clone and its relocated lambda must land in the entry
    // package with fresh ids.
    let new_entry_items: Vec<LocalItemId> = fir_store
        .get(fir_pkg_id)
        .items
        .iter()
        .filter(|(id, item)| {
            matches!(item.kind, ItemKind::Callable(_)) && !entry_items_before.contains(id)
        })
        .map(|(id, _)| id)
        .collect();
    assert!(
        new_entry_items.len() >= 2,
        "expected the specialized HOF clone plus its relocated inner lambda to be \
         added to the entry package with remapped ids; new callables: {new_entry_items:?}"
    );
}

/// When a library callable's nested lambda captures the callable parameter,
/// specializing it into the entry package relocates the lambda with a fresh id
/// and the fixpoint loop resolves the relocated closure, leaving no first-class
/// closure in the reachable entry-package callables.
#[test]
fn cross_package_nested_lambda_relocated_with_remapped_id_and_defunctionalized() {
    // `LibApply`'s body builds a nested lambda that captures the callable
    // parameter `op`. Its lifted item lives in the library package and must be
    // relocated when `LibApply` specializes into the entry package.
    let lib_source = r#"
        namespace TestLib {
            operation LibApply(op : Int => Unit, x : Int) : Unit {
                let g = y => op(x + y);
                g(1);
            }
            export LibApply;
        }
    "#;
    let user_source = r#"
        import TestLib.*;

        operation Noop(x : Int) : Unit {}
        @EntryPoint()
        operation Main() : Unit {
            LibApply(Noop, 5);
        }
    "#;

    let (mut fir_store, fir_pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    crate::monomorphize::monomorphize(&mut fir_store, fir_pkg_id, &mut assigners);

    // The entry package starts with no lifted lambdas; the only one is defined
    // in the library package.
    let lambda_ids_before: std::collections::BTreeSet<LocalItemId> =
        lambda_item_ids(fir_store.get(fir_pkg_id));
    assert!(
        lambda_ids_before.is_empty(),
        "precondition: the entry package starts with no lifted lambda items"
    );

    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors(
        "cross_package_nested_lambda_relocated_with_remapped_id_and_defunctionalized",
        &errors,
    );

    // The library lambda is cloned into the entry package with a fresh id.
    let lambda_ids_after = lambda_item_ids(fir_store.get(fir_pkg_id));
    let relocated: Vec<LocalItemId> = lambda_ids_after
        .difference(&lambda_ids_before)
        .copied()
        .collect();
    assert!(
        !relocated.is_empty(),
        "the library lambda must be relocated into the entry package with a fresh \
         remapped id; lambda ids after specialization: {lambda_ids_after:?}"
    );

    // The relocated lambda's id is owned by the entry package.
    let entry_package = fir_store.get(fir_pkg_id);
    for &id in &relocated {
        assert!(
            entry_package.items.contains_key(id),
            "relocated lambda id {id} must exist in the entry package"
        );
    }

    // The fixpoint loop defunctionalizes the relocated closure, leaving no
    // first-class closure in any reachable entry-package callable.
    let reachable = crate::reachability::collect_reachable_from_entry(&fir_store, fir_pkg_id);
    for store_id in &reachable {
        if store_id.package != fir_pkg_id {
            continue;
        }
        let package = fir_store.get(store_id.package);
        let ItemKind::Callable(decl) = &package.get_item(store_id.item).kind else {
            continue;
        };
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |_id, expr| {
                assert!(
                    !matches!(expr.kind, fir::ExprKind::Closure(..)),
                    "reachable entry-package callable `{}` still contains a first-class \
                     closure after defunctionalization; the relocated closure was not \
                     defunctionalized",
                    decl.name.name
                );
            },
        );
    }
}

/// Collects the ids of lifted-lambda callables in `package`.
fn lambda_item_ids(package: &fir::Package) -> std::collections::BTreeSet<LocalItemId> {
    package
        .items
        .iter()
        .filter_map(|(id, item)| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref().starts_with(".lambda") => Some(id),
            _ => None,
        })
        .collect()
}

/// Asserts that no closure references an item id absent from its own package.
/// Every positive cross-package test must uphold this.
fn assert_no_dangling_cross_package_closures(store: &fir::PackageStore) {
    for (pkg_id, package) in store {
        for expr in package.exprs.values() {
            if let fir::ExprKind::Closure(_, target_item) = &expr.kind {
                assert!(
                    package.items.contains_key(*target_item),
                    "closure in package {pkg_id} references item {target_item} \
                     that does not exist in that package (dangling cross-package id)"
                );
            }
        }
    }
}

/// Collects the ids of every callable item in `package`.
fn callable_item_ids(package: &fir::Package) -> std::collections::BTreeSet<LocalItemId> {
    package
        .items
        .iter()
        .filter(|(_, item)| matches!(item.kind, ItemKind::Callable(_)))
        .map(|(id, _)| id)
        .collect()
}

/// A library higher-order callable that calls its callable parameter directly,
/// with no nested lambda, specializes into the entry package when invoked with
/// a concrete operation, and the whole pipeline runs to completion. This is the
/// simplest supported cross-package higher-order shape.
#[test]
fn cross_package_foreign_hof_without_nested_lambda_specializes_into_entry() {
    let lib_source = r#"
        namespace TestLib {
            operation LibApplyDirect(op : Int => Unit, x : Int) : Unit {
                op(x);
            }
            export LibApplyDirect;
        }
    "#;
    let user_source = r#"
        import TestLib.*;

        operation Noop(x : Int) : Unit {}
        @EntryPoint()
        operation Main() : Unit {
            LibApplyDirect(Noop, 5);
        }
    "#;

    let (mut fir_store, fir_pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    crate::monomorphize::monomorphize(&mut fir_store, fir_pkg_id, &mut assigners);

    let entry_items_before = callable_item_ids(fir_store.get(fir_pkg_id));

    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors(
        "cross_package_foreign_hof_without_nested_lambda_specializes_into_entry",
        &errors,
    );

    // The library callable is specialized into the entry package, adding a new
    // callable id.
    let new_entry_items: Vec<LocalItemId> = callable_item_ids(fir_store.get(fir_pkg_id))
        .difference(&entry_items_before)
        .copied()
        .collect();
    assert!(
        !new_entry_items.is_empty(),
        "the foreign HOF must be specialized into the entry package with a fresh id; \
         new callables: {new_entry_items:?}"
    );

    // A direct-call callable has no closures, so specialization must not add
    // any dangling closure ids.
    assert_no_dangling_cross_package_closures(&fir_store);

    // The whole pipeline runs to completion, confirming the specialized
    // callable is valid through codegen prep.
    let _ = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        crate::PipelineStage::Full,
    );
}

/// A higher-order callable that forwards its own callable parameter to another
/// higher-order call cannot be specialized: the forwarded parameter cannot be
/// resolved statically, so defunctionalize emits a `DynamicCallable`
/// diagnostic. This is a by-design limitation of the pass, not a cross-package
/// regression.
///
/// The test asserts the cross-package mutual recursion is rejected identically
/// to the structurally-equivalent same-package program, proving cross-package
/// transformation adds no new failure mode for recursive callables.
#[test]
fn cross_package_recursive_hof_forwarding_callable_rejected_like_same_package() {
    // The mutually-recursive callables live in a library package, seeded with a
    // concrete operation from the entry package.
    let lib_source = r#"
        namespace TestLib {
            operation LibPing(op : Int => Unit, n : Int) : Unit {
                if n > 0 {
                    op(n);
                    LibPong(op, n - 1);
                }
            }
            operation LibPong(op : Int => Unit, n : Int) : Unit {
                if n > 0 {
                    op(n);
                    LibPing(op, n - 1);
                }
            }
            export LibPing, LibPong;
        }
    "#;
    let user_source = r#"
        import TestLib.*;

        operation Noop(x : Int) : Unit {}
        @EntryPoint()
        operation Main() : Unit {
            LibPing(Noop, 3);
        }
    "#;

    let (mut fir_store, fir_pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    crate::monomorphize::monomorphize(&mut fir_store, fir_pkg_id, &mut assigners);
    let cross_package_errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);

    // The identical mutual recursion declared entirely in the entry package.
    let same_package_source = r#"
        operation LibPing(op : Int => Unit, n : Int) : Unit {
            if n > 0 {
                op(n);
                LibPong(op, n - 1);
            }
        }
        operation LibPong(op : Int => Unit, n : Int) : Unit {
            if n > 0 {
                op(n);
                LibPing(op, n - 1);
            }
        }
        operation Noop(x : Int) : Unit {}
        @EntryPoint()
        operation Main() : Unit {
            LibPing(Noop, 3);
        }
    "#;
    let (mut same_store, same_pkg_id) =
        crate::test_utils::compile_to_monomorphized_fir(same_package_source);
    let mut same_assigners = PackageAssigners::new(&same_store, same_pkg_id);
    let same_package_errors = defunctionalize(&mut same_store, same_pkg_id, &mut same_assigners);

    // Both reject the forwarded callable parameter.
    assert!(
        !cross_package_errors.is_empty(),
        "cross-package recursive HOF forwarding its callable parameter must be rejected"
    );
    for error in &cross_package_errors {
        assert_eq!(
            error.to_string(),
            "callable argument could not be resolved statically",
            "expected the documented DynamicCallable diagnostic"
        );
    }

    // The rejection matches the same-package program in count and message,
    // proving cross-package transformation adds no new failure mode.
    assert_eq!(
        cross_package_errors.len(),
        same_package_errors.len(),
        "cross-package and same-package recursive HOFs must yield the same number of \
         diagnostics; cross-package: {cross_package_errors:?}, same-package: \
         {same_package_errors:?}"
    );
    let cross_messages: Vec<String> = cross_package_errors
        .iter()
        .map(ToString::to_string)
        .collect();
    let same_messages: Vec<String> = same_package_errors
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        cross_messages, same_messages,
        "cross-package recursive HOF must be rejected identically to the same-package equivalent"
    );
}

/// A library function whose return type is a callable hands a function value
/// back across the package boundary for the entry package to invoke.
/// Defunctionalize and the rest of the pipeline must handle the function-typed
/// return without dangling closures or errors.
#[test]
fn cross_package_function_typed_return_flows_across_packages() {
    let lib_source = r#"
        namespace TestLib {
            operation LibTarget(x : Int) : Unit {}
            function LibSelectOp() : (Int => Unit) {
                return LibTarget;
            }
            export LibTarget, LibSelectOp;
        }
    "#;
    let user_source = r#"
        import TestLib.*;

        @EntryPoint()
        operation Main() : Unit {
            let op = LibSelectOp();
            op(5);
        }
    "#;

    let (mut fir_store, fir_pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    crate::monomorphize::monomorphize(&mut fir_store, fir_pkg_id, &mut assigners);

    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors(
        "cross_package_function_typed_return_flows_across_packages",
        &errors,
    );

    // A function-typed return must not leave any closure referencing a foreign
    // package-local id.
    assert_no_dangling_cross_package_closures(&fir_store);

    // The whole pipeline runs to completion.
    let _ = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        crate::PipelineStage::Full,
    );
}

#[test]
fn analysis_apply_operation_power_ca_consumer() {
    let source = r#"
        operation Consume(apply_power_of_u : (Int, Qubit[]) => Unit is Adj + Ctl, target : Qubit[]) : Unit {
            apply_power_of_u(1, target);
        }

        operation U(qs : Qubit[]) : Unit is Adj + Ctl {
            H(qs[0]);
        }

        operation Main() : Unit {
            use qs = Qubit[1];
            Consume(ApplyOperationPowerCA(_, U, _), qs);
        }
                "#;
    check_analysis_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            callable_params: 3
              param: callable_id=<item 4 in package 2>, path=[0], ty=((Qubit)[] => Unit is Adj + Ctl)
              param: callable_id=<item 1018 in package 1>, path=[1], ty=((Qubit)[] => Unit is Adj + Ctl)
              param: callable_id=<item 5 in package 2>, path=[0], ty=((Int, (Qubit)[]) => Unit is Adj + Ctl)
            call_sites: 5
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=Consume<AdjCtl>, arg=Closure(target=4, Body)
            direct_call_sites: 3
              site: callee=H:Adj, default
              site: callee=H:Ctl, default
              site: callee=H:CtlAdj, default
            lattice states:
              callable Main:
                2: Single(U:Body)"#]],
    );
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            BEFORE:
            operation Consume(apply_power_of_u : ((Int, Qubit[]) => Unit), target : Qubit[]) : Unit {
                apply_power_of_u(1, target);
            }
            operation U(qs : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    H(qs[0]);
                }
                adjoint ... {
                    Adjoint H(qs[0]);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, qs[0]);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, qs[0]);
                }
            }
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(1);
                Consume_AdjCtl_({
                    let arg : (Qubit[] => Unit is Adj + Ctl) = U;
                    / * closure item = 4 captures = [arg] * / _lambda_4
                }, qs);
                ReleaseQubitArray(qs);
            }
            operation _lambda_4(arg : (Qubit[] => Unit is Adj + Ctl), (hole : Int, hole : Qubit[])) : Unit is Adj + Ctl {
                body ... {
                    ApplyOperationPowerCA__Qubit_____AdjCtl_(hole, arg, hole)
                }
                adjoint ... {
                    Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl_(hole, arg, hole)
                }
                controlled (ctls, ...) {
                    Controlled ApplyOperationPowerCA__Qubit_____AdjCtl_(ctls, (hole, arg, hole))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl_(ctls, (hole, arg, hole))
                }
            }
            operation Consume_AdjCtl_(apply_power_of_u : ((Int, Qubit[]) => Unit is Adj + Ctl), target : Qubit[]) : Unit {
                apply_power_of_u(1, target);
            }
            // entry
            Main()

            AFTER:
            operation Consume(apply_power_of_u : ((Int, Qubit[]) => Unit), target : Qubit[]) : Unit {
                apply_power_of_u(1, target);
            }
            operation U(qs : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    H(qs[0]);
                }
                adjoint ... {
                    Adjoint H(qs[0]);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, qs[0]);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, qs[0]);
                }
            }
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(1);
                Consume_AdjCtl__closure_(qs);
                ReleaseQubitArray(qs);
            }
            operation _lambda_4(hole : Int, hole : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    ApplyOperationPowerCA__Qubit_____AdjCtl__U_(hole, hole)
                }
                adjoint ... {
                    Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl__U_(hole, hole)
                }
                controlled (ctls, ...) {
                    Controlled ApplyOperationPowerCA__Qubit_____AdjCtl__U_(ctls, (hole, hole))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl__U_(ctls, (hole, hole))
                }
            }
            operation Consume_AdjCtl_(apply_power_of_u : ((Int, Qubit[]) => Unit is Adj + Ctl), target : Qubit[]) : Unit {
                apply_power_of_u(1, target);
            }
            operation Consume_AdjCtl__closure_(target : Qubit[]) : Unit {
                _lambda_4(1, target);
            }
            operation ApplyOperationPowerCA__Qubit_____AdjCtl__U_(power : Int, target : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    {
                        let _range_id_48117 : Range = 1..AbsI(power);
                        mutable _index_id_48120 : Int = _range_id_48117::Start;
                        let _step_id_48125 : Int = _range_id_48117::Step;
                        let _end_id_48130 : Int = _range_id_48117::End;
                        while _step_id_48125 > 0 and _index_id_48120 <= _end_id_48130 or _step_id_48125 < 0 and _index_id_48120 >= _end_id_48130 {
                            let _ : Int = _index_id_48120;
                            if power >= 0 {
                                U(target)
                            } else {
                                Adjoint U(target)
                            };
                            _index_id_48120 += _step_id_48125;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _range : Range = 1..AbsI(power);
                        {
                            let _range_id_48160 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                            mutable _index_id_48163 : Int = _range_id_48160::Start;
                            let _step_id_48168 : Int = _range_id_48160::Step;
                            let _end_id_48173 : Int = _range_id_48160::End;
                            while _step_id_48168 > 0 and _index_id_48163 <= _end_id_48173 or _step_id_48168 < 0 and _index_id_48163 >= _end_id_48173 {
                                let _ : Int = _index_id_48163;
                                if power >= 0 {
                                    Adjoint U(target)
                                } else {
                                    U(target)
                                };
                                _index_id_48163 += _step_id_48168;
                            }

                        }

                    }

                }
                controlled (ctls, ...) {
                    {
                        let _range_id_48203 : Range = 1..AbsI(power);
                        mutable _index_id_48206 : Int = _range_id_48203::Start;
                        let _step_id_48211 : Int = _range_id_48203::Step;
                        let _end_id_48216 : Int = _range_id_48203::End;
                        while _step_id_48211 > 0 and _index_id_48206 <= _end_id_48216 or _step_id_48211 < 0 and _index_id_48206 >= _end_id_48216 {
                            let _ : Int = _index_id_48206;
                            if power >= 0 {
                                Controlled U(ctls, target)
                            } else {
                                Controlled Adjoint U(ctls, target)
                            };
                            _index_id_48206 += _step_id_48211;
                        }

                    }

                }
                controlled adjoint (ctls, ...) {
                    {
                        let _range : Range = 1..AbsI(power);
                        {
                            let _range_id_48246 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                            mutable _index_id_48249 : Int = _range_id_48246::Start;
                            let _step_id_48254 : Int = _range_id_48246::Step;
                            let _end_id_48259 : Int = _range_id_48246::End;
                            while _step_id_48254 > 0 and _index_id_48249 <= _end_id_48259 or _step_id_48254 < 0 and _index_id_48249 >= _end_id_48259 {
                                let _ : Int = _index_id_48249;
                                if power >= 0 {
                                    Controlled Adjoint U(ctls, target)
                                } else {
                                    Controlled U(ctls, target)
                                };
                                _index_id_48249 += _step_id_48254;
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_bernstein_vazirani_sample_shape() {
    let source = r#"
        import Std.Arrays.*;
        import Std.Convert.*;
        import Std.Diagnostics.*;
        import Std.Math.*;
        import Std.Measurement.*;

        operation Main() : Unit {
            let nQubits = 10;
            let integers = [127, 238, 512];
            for integer in integers {
                let parityOperation = EncodeIntegerAsParityOperation(integer);
                let _ = BernsteinVazirani(parityOperation, nQubits);
            }
        }

        operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
            use queryRegister = Qubit[n];
            use target = Qubit();
            X(target);
            within {
                ApplyToEachA(H, queryRegister);
            } apply {
                H(target);
                Uf(queryRegister, target);
            }
            let resultArray = MResetEachZ(queryRegister);
            Reset(target);
            resultArray
        }

        operation ApplyParityOperation(bitStringAsInt : Int, xRegister : Qubit[], yQubit : Qubit) : Unit {
            let requiredBits = BitSizeI(bitStringAsInt);
            let availableQubits = Length(xRegister);
            Fact(availableQubits >= requiredBits, "enough qubits");
            for index in IndexRange(xRegister) {
                if ((bitStringAsInt &&& 2^index) != 0) {
                    CNOT(xRegister[index], yQubit);
                }
            }
        }

        function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bitStringAsInt, _, _);
        }
                "#;
    check_analysis_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 1018 in package 1>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
              param: callable_id=<item 6 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)
            call_sites: 3
              site: hof=BernsteinVazirani<Empty>, arg=Closure(target=5, Body)
              site: hof=ApplyToEachA<Qubit, AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyToEachA<Qubit, AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                7: Single(Closure(5):Body)"#]],
    );
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let nQubits : Int = 10;
                let integers : Int[] = [127, 238, 512];
                {
                    let _array_id_207 : Int[] = integers;
                    let _len_id_210 : Int = 3;
                    mutable _index_id_214 : Int = 0;
                    while _index_id_214 < _len_id_210 {
                        let integer : Int = _array_id_207[_index_id_214];
                        let parityOperation : ((Qubit[], Qubit) => Unit) = EncodeIntegerAsParityOperation(integer);
                        let _ : Result[] = BernsteinVazirani_Empty_(parityOperation, nQubits);
                        _index_id_214 += 1;
                    }

                }

            }
            operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                {
                    {
                        ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                    }

                    let _apply_res : Unit = {
                        H(target);
                        Uf(queryRegister, target);
                    };
                    {
                        Adjoint ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                    }

                    _apply_res
                }

                let resultArray : Result[] = MResetEachZ(queryRegister);
                Reset(target);
                let _generated_ident_286 : Result[] = resultArray;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_286
            }
            operation ApplyParityOperation(bitStringAsInt : Int, xRegister : Qubit[], yQubit : Qubit) : Unit {
                let requiredBits : Int = BitSizeI(bitStringAsInt);
                let availableQubits : Int = Length(xRegister);
                Fact(availableQubits >= requiredBits, $"enough qubits");
                {
                    let _range_id_233 : Range = IndexRange_Qubit_(xRegister);
                    mutable _index_id_236 : Int = _range_id_233::Start;
                    let _step_id_241 : Int = _range_id_233::Step;
                    let _end_id_246 : Int = _range_id_233::End;
                    while _step_id_241 > 0 and _index_id_236 <= _end_id_246 or _step_id_241 < 0 and _index_id_236 >= _end_id_246 {
                        let index : Int = _index_id_236;
                        if bitStringAsInt &&& 2^index != 0 {
                            CNOT(xRegister[index], yQubit);
                        }

                        _index_id_236 += _step_id_241;
                    }

                }

            }
            function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = bitStringAsInt;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation _lambda_5(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation BernsteinVazirani_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                {
                    {
                        ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                    }

                    let _apply_res : Unit = {
                        H(target);
                        Uf(queryRegister, target);
                    };
                    {
                        Adjoint ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                    }

                    _apply_res
                }

                let resultArray : Result[] = MResetEachZ(queryRegister);
                Reset(target);
                let _generated_ident_286 : Result[] = resultArray;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_286
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let nQubits : Int = 10;
                let integers : Int[] = [127, 238, 512];
                {
                    let _array_id_207 : Int[] = integers;
                    let _len_id_210 : Int = 3;
                    mutable _index_id_214 : Int = 0;
                    while _index_id_214 < _len_id_210 {
                        let integer : Int = _array_id_207[_index_id_214];
                        let _ : Result[] = BernsteinVazirani_Empty__closure_(nQubits, integer);
                        _index_id_214 += 1;
                    }

                }

            }
            operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                {
                    {
                        ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                    }

                    let _apply_res : Unit = {
                        H(target);
                        Uf(queryRegister, target);
                    };
                    {
                        Adjoint ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                    }

                    _apply_res
                }

                let resultArray : Result[] = MResetEachZ(queryRegister);
                Reset(target);
                let _generated_ident_286 : Result[] = resultArray;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_286
            }
            operation ApplyParityOperation(bitStringAsInt : Int, xRegister : Qubit[], yQubit : Qubit) : Unit {
                let requiredBits : Int = BitSizeI(bitStringAsInt);
                let availableQubits : Int = Length(xRegister);
                Fact(availableQubits >= requiredBits, $"enough qubits");
                {
                    let _range_id_233 : Range = IndexRange_Qubit_(xRegister);
                    mutable _index_id_236 : Int = _range_id_233::Start;
                    let _step_id_241 : Int = _range_id_233::Step;
                    let _end_id_246 : Int = _range_id_233::End;
                    while _step_id_241 > 0 and _index_id_236 <= _end_id_246 or _step_id_241 < 0 and _index_id_236 >= _end_id_246 {
                        let index : Int = _index_id_236;
                        if bitStringAsInt &&& 2^index != 0 {
                            CNOT(xRegister[index], yQubit);
                        }

                        _index_id_236 += _step_id_241;
                    }

                }

            }
            function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = bitStringAsInt;
                    ()
                };
            }
            operation _lambda_5(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation BernsteinVazirani_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                {
                    {
                        ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                    }

                    let _apply_res : Unit = {
                        H(target);
                        Uf(queryRegister, target);
                    };
                    {
                        Adjoint ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                    }

                    _apply_res
                }

                let resultArray : Result[] = MResetEachZ(queryRegister);
                Reset(target);
                let _generated_ident_286 : Result[] = resultArray;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_286
            }
            operation BernsteinVazirani_Empty__closure_(n : Int, __capture_0 : Int) : Result[] {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                {
                    {
                        ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                    }

                    let _apply_res : Unit = {
                        H(target);
                        _lambda_5(__capture_0, (queryRegister, target));
                    };
                    {
                        Adjoint ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                    }

                    _apply_res
                }

                let resultArray : Result[] = MResetEachZ(queryRegister);
                Reset(target);
                let _generated_ident_286 : Result[] = resultArray;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_286
            }
            operation ApplyToEachA_Qubit__AdjCtl__H_(register : Qubit[]) : Unit is Adj {
                body ... {
                    {
                        let _array_id_46334 : Qubit[] = register;
                        let _len_id_46339 : Int = Length(_array_id_46334);
                        mutable _index_id_46343 : Int = 0;
                        while _index_id_46343 < _len_id_46339 {
                            let item : Qubit = _array_id_46334[_index_id_46343];
                            H(item);
                            _index_id_46343 += 1;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46362 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46365 : Int = _range_id_46362::Start;
                            let _step_id_46370 : Int = _range_id_46362::Step;
                            let _end_id_46375 : Int = _range_id_46362::End;
                            while _step_id_46370 > 0 and _index_id_46365 <= _end_id_46375 or _step_id_46370 < 0 and _index_id_46365 >= _end_id_46375 {
                                let _index : Int = _index_id_46365;
                                let item : Qubit = _array[_index];
                                Adjoint H(item);
                                _index_id_46365 += _step_id_46370;
                            }

                        }

                    }

                }
            }
            operation ApplyToEachA_Qubit__AdjCtl__H_(register : Qubit[]) : Unit is Adj {
                body ... {
                    {
                        let _array_id_46334 : Qubit[] = register;
                        let _len_id_46339 : Int = Length(_array_id_46334);
                        mutable _index_id_46343 : Int = 0;
                        while _index_id_46343 < _len_id_46339 {
                            let item : Qubit = _array_id_46334[_index_id_46343];
                            H(item);
                            _index_id_46343 += 1;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46362 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46365 : Int = _range_id_46362::Start;
                            let _step_id_46370 : Int = _range_id_46362::Step;
                            let _end_id_46375 : Int = _range_id_46362::End;
                            while _step_id_46370 > 0 and _index_id_46365 <= _end_id_46375 or _step_id_46370 < 0 and _index_id_46365 >= _end_id_46375 {
                                let _index : Int = _index_id_46365;
                                let item : Qubit = _array[_index];
                                Adjoint H(item);
                                _index_id_46365 += _step_id_46370;
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_deutsch_jozsa_sample_shape() {
    let source = r#"
        import Std.Diagnostics.*;
        import Std.Math.*;
        import Std.Measurement.*;

        operation Main() : Unit {
            let functionsToTest = [SimpleConstantBoolF, SimpleBalancedBoolF, ConstantBoolF, BalancedBoolF];
            for fn in functionsToTest {
                let _ = DeutschJozsa(fn, 5);
            }
        }

        operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
            use queryRegister = Qubit[n];
            use target = Qubit();
            X(target);
            H(target);
            within {
                for q in queryRegister {
                    H(q);
                }
            } apply {
                Uf(queryRegister, target);
            }
            mutable result = true;
            for q in queryRegister {
                if MResetZ(q) == One {
                    result = false;
                }
            }
            Reset(target);
            result
        }

        operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            X(target);
        }

        operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            CX(args[0], target);
        }

        operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            for i in 0..(2^Length(args)) - 1 {
                ApplyControlledOnInt(i, X, args, target);
            }
        }

        operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            for i in 0..2..(2^Length(args)) - 1 {
                ApplyControlledOnInt(i, X, args, target);
            }
        }
                "#;
    check_analysis_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 1018 in package 1>, path=[1], ty=(Qubit => Unit is Adj + Ctl)
              param: callable_id=<item 7 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)
            call_sites: 6
              site: hof=ApplyControlledOnInt<Qubit, AdjCtl>, arg=Global(X, Body)
              site: hof=ApplyControlledOnInt<Qubit, AdjCtl>, arg=Global(X, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(SimpleConstantBoolF, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(SimpleBalancedBoolF, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(ConstantBoolF, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(BalancedBoolF, Body)
            direct_call_sites: 1
              site: callee=H:Adj, default
            lattice states:
              callable Main:
                5: Multi([SimpleConstantBoolF:Body, SimpleBalancedBoolF:Body, ConstantBoolF:Body, BalancedBoolF:Body])"#]],
    );
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let functionsToTest : ((Qubit[], Qubit) => Unit)[] = [SimpleConstantBoolF, SimpleBalancedBoolF, ConstantBoolF, BalancedBoolF];
                {
                    let _array_id_244 : ((Qubit[], Qubit) => Unit)[] = functionsToTest;
                    let _len_id_247 : Int = 4;
                    mutable _index_id_251 : Int = 0;
                    while _index_id_251 < _len_id_247 {
                        let fn : ((Qubit[], Qubit) => Unit) = _array_id_244[_index_id_251];
                        let _ : Bool = DeutschJozsa_Empty_(fn, 5);
                        _index_id_251 += 1;
                    }

                }

            }
            operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        Uf(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                X(target);
            }
            operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                CX(args[0], target);
            }
            operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                {
                    let _range_id_369 : Range = 0..2^Length(args) - 1;
                    mutable _index_id_372 : Int = _range_id_369::Start;
                    let _step_id_377 : Int = _range_id_369::Step;
                    let _end_id_382 : Int = _range_id_369::End;
                    while _step_id_377 > 0 and _index_id_372 <= _end_id_382 or _step_id_377 < 0 and _index_id_372 >= _end_id_382 {
                        let i : Int = _index_id_372;
                        ApplyControlledOnInt_Qubit__AdjCtl_(i, X, args, target);
                        _index_id_372 += _step_id_377;
                    }

                }

            }
            operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                {
                    let _range_id_412 : Range = 0..2..2^Length(args) - 1;
                    mutable _index_id_415 : Int = _range_id_412::Start;
                    let _step_id_420 : Int = _range_id_412::Step;
                    let _end_id_425 : Int = _range_id_412::End;
                    while _step_id_420 > 0 and _index_id_415 <= _end_id_425 or _step_id_420 < 0 and _index_id_415 >= _end_id_425 {
                        let i : Int = _index_id_415;
                        ApplyControlledOnInt_Qubit__AdjCtl_(i, X, args, target);
                        _index_id_415 += _step_id_420;
                    }

                }

            }
            operation DeutschJozsa_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        Uf(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let functionsToTest : ((Qubit[], Qubit) => Unit)[] = [SimpleConstantBoolF, SimpleBalancedBoolF, ConstantBoolF, BalancedBoolF];
                {
                    let _array_id_244 : ((Qubit[], Qubit) => Unit)[] = functionsToTest;
                    let _len_id_247 : Int = 4;
                    mutable _index_id_251 : Int = 0;
                    while _index_id_251 < _len_id_247 {
                        let _ : Bool = if _index_id_251 == 0 {
                            DeutschJozsa_Empty__SimpleConstantBoolF_(5)
                        } else if _index_id_251 == 1 {
                            DeutschJozsa_Empty__SimpleBalancedBoolF_(5)
                        } else if _index_id_251 == 2 {
                            DeutschJozsa_Empty__ConstantBoolF_(5)
                        } else {
                            DeutschJozsa_Empty__BalancedBoolF_(5)
                        };
                        _index_id_251 += 1;
                    }

                }

            }
            operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        Uf(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                X(target);
            }
            operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                CX(args[0], target);
            }
            operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                {
                    let _range_id_369 : Range = 0..2^Length(args) - 1;
                    mutable _index_id_372 : Int = _range_id_369::Start;
                    let _step_id_377 : Int = _range_id_369::Step;
                    let _end_id_382 : Int = _range_id_369::End;
                    while _step_id_377 > 0 and _index_id_372 <= _end_id_382 or _step_id_377 < 0 and _index_id_372 >= _end_id_382 {
                        let i : Int = _index_id_372;
                        ApplyControlledOnInt_Qubit__AdjCtl__X_(i, args, target);
                        _index_id_372 += _step_id_377;
                    }

                }

            }
            operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                {
                    let _range_id_412 : Range = 0..2..2^Length(args) - 1;
                    mutable _index_id_415 : Int = _range_id_412::Start;
                    let _step_id_420 : Int = _range_id_412::Step;
                    let _end_id_425 : Int = _range_id_412::End;
                    while _step_id_420 > 0 and _index_id_415 <= _end_id_425 or _step_id_420 < 0 and _index_id_415 >= _end_id_425 {
                        let i : Int = _index_id_415;
                        ApplyControlledOnInt_Qubit__AdjCtl__X_(i, args, target);
                        _index_id_415 += _step_id_420;
                    }

                }

            }
            operation DeutschJozsa_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        Uf(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            operation ApplyControlledOnInt_Qubit__AdjCtl__X_(numberState : Int, controlRegister : Qubit[], target : Qubit) : Unit is Adj + Ctl {
                body ... {
                    {
                        {
                            ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        let _apply_res : Unit = {
                            Controlled X(controlRegister, target);
                        };
                        {
                            Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        _apply_res
                    }

                }
                adjoint ... {
                    {
                        {
                            ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        let _apply_res : Unit = {
                            Controlled Adjoint X(controlRegister, target);
                        };
                        {
                            Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        _apply_res
                    }

                }
                controlled (ctls, ...) {
                    {
                        {
                            ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        let _apply_res : Unit = {
                            Controlled Controlled X(ctls, (controlRegister, target));
                        };
                        {
                            Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        _apply_res
                    }

                }
                controlled adjoint (ctls, ...) {
                    {
                        {
                            ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        let _apply_res : Unit = {
                            Controlled Controlled Adjoint X(ctls, (controlRegister, target));
                        };
                        {
                            Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                        }

                        _apply_res
                    }

                }
            }
            operation DeutschJozsa_Empty__SimpleConstantBoolF_(n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        SimpleConstantBoolF(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            operation DeutschJozsa_Empty__SimpleBalancedBoolF_(n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        SimpleBalancedBoolF(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            operation DeutschJozsa_Empty__ConstantBoolF_(n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        ConstantBoolF(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            operation DeutschJozsa_Empty__BalancedBoolF_(n : Int) : Bool {
                let queryRegister : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                X(target);
                H(target);
                {
                    {
                        {
                            let _array_id_270 : Qubit[] = queryRegister;
                            let _len_id_275 : Int = Length(_array_id_270);
                            mutable _index_id_279 : Int = 0;
                            while _index_id_279 < _len_id_275 {
                                let q : Qubit = _array_id_270[_index_id_279];
                                H(q);
                                _index_id_279 += 1;
                            }

                        }

                    }

                    let _apply_res : Unit = {
                        BalancedBoolF(queryRegister, target);
                    };
                    {
                        {
                            let _array : Qubit[] = queryRegister;
                            {
                                let _range_id_298 : Range = Length(_array) - 1..-1..0;
                                mutable _index_id_301 : Int = _range_id_298::Start;
                                let _step_id_306 : Int = _range_id_298::Step;
                                let _end_id_311 : Int = _range_id_298::End;
                                while _step_id_306 > 0 and _index_id_301 <= _end_id_311 or _step_id_306 < 0 and _index_id_301 >= _end_id_311 {
                                    let _index : Int = _index_id_301;
                                    let q : Qubit = _array[_index];
                                    Adjoint H(q);
                                    _index_id_301 += _step_id_306;
                                }

                            }

                        }

                    }

                    _apply_res
                }

                mutable result : Bool = true;
                {
                    let _array_id_341 : Qubit[] = queryRegister;
                    let _len_id_346 : Int = Length(_array_id_341);
                    mutable _index_id_350 : Int = 0;
                    while _index_id_350 < _len_id_346 {
                        let q : Qubit = _array_id_341[_index_id_350];
                        if MResetZ(q) == One {
                            result = false;
                        }

                        _index_id_350 += 1;
                    }

                }

                Reset(target);
                let _generated_ident_465 : Bool = result;
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(queryRegister);
                _generated_ident_465
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn full_pipeline_handles_stdlib_apply_to_each() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(H, qs);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl_(H, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl__H_(qs);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEach_Qubit__AdjCtl__H_(register : Qubit[]) : Unit {
                {
                    let _array_id_46296 : Qubit[] = register;
                    let _len_id_46301 : Int = Length(_array_id_46296);
                    mutable _index_id_46305 : Int = 0;
                    while _index_id_46305 < _len_id_46301 {
                        let item : Qubit = _array_id_46296[_index_id_46305];
                        H(item);
                        _index_id_46305 += 1;
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn full_pipeline_handles_stdlib_apply_to_each_with_custom_intrinsic() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(SX, qs);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl_(SX, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl__SX_(qs);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEach_Qubit__AdjCtl__SX_(register : Qubit[]) : Unit {
                {
                    let _array_id_46296 : Qubit[] = register;
                    let _len_id_46301 : Int = Length(_array_id_46296);
                    mutable _index_id_46305 : Int = 0;
                    while _index_id_46305 < _len_id_46301 {
                        let item : Qubit = _array_id_46296[_index_id_46305];
                        SX(item);
                        _index_id_46305 += 1;
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn apply_to_each_body_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(H, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl_(H, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl__H_(qs);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEach_Qubit__AdjCtl__H_(register : Qubit[]) : Unit {
                {
                    let _array_id_46296 : Qubit[] = register;
                    let _len_id_46301 : Int = Length(_array_id_46296);
                    mutable _index_id_46305 : Int = 0;
                    while _index_id_46305 < _len_id_46301 {
                        let item : Qubit = _array_id_46296[_index_id_46305];
                        H(item);
                        _index_id_46305 += 1;
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn apply_to_each_a_adjoint_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEachA(S, qs);
            Adjoint ApplyToEachA(S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEachA_Qubit__AdjCtl_(S, qs);
                Adjoint ApplyToEachA_Qubit__AdjCtl_(S, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEachA_Qubit__AdjCtl__S_(qs);
                Adjoint ApplyToEachA_Qubit__AdjCtl__S_(qs);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEachA_Qubit__AdjCtl__S_(register : Qubit[]) : Unit is Adj {
                body ... {
                    {
                        let _array_id_46324 : Qubit[] = register;
                        let _len_id_46329 : Int = Length(_array_id_46324);
                        mutable _index_id_46333 : Int = 0;
                        while _index_id_46333 < _len_id_46329 {
                            let item : Qubit = _array_id_46324[_index_id_46333];
                            S(item);
                            _index_id_46333 += 1;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46352 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46355 : Int = _range_id_46352::Start;
                            let _step_id_46360 : Int = _range_id_46352::Step;
                            let _end_id_46365 : Int = _range_id_46352::End;
                            while _step_id_46360 > 0 and _index_id_46355 <= _end_id_46365 or _step_id_46360 < 0 and _index_id_46355 >= _end_id_46365 {
                                let _index : Int = _index_id_46355;
                                let item : Qubit = _array[_index];
                                Adjoint S(item);
                                _index_id_46355 += _step_id_46360;
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn apply_to_each_c_controlled_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use (ctl, qs) = (Qubit(), Qubit[3]);
            ApplyToEachC(X, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let _generated_ident_25 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_27 : Qubit[] = AllocateQubitArray(3);
                let (ctl : Qubit, qs : Qubit[]) = (_generated_ident_25, _generated_ident_27);
                ApplyToEachC_Qubit__AdjCtl_(X, qs);
                ReleaseQubitArray(_generated_ident_27);
                __quantum__rt__qubit_release(_generated_ident_25);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let _generated_ident_25 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_27 : Qubit[] = AllocateQubitArray(3);
                let (ctl : Qubit, qs : Qubit[]) = (_generated_ident_25, _generated_ident_27);
                ApplyToEachC_Qubit__AdjCtl__X_(qs);
                ReleaseQubitArray(_generated_ident_27);
                __quantum__rt__qubit_release(_generated_ident_25);
            }
            operation ApplyToEachC_Qubit__AdjCtl__X_(register : Qubit[]) : Unit is Ctl {
                body ... {
                    {
                        let _array_id_46395 : Qubit[] = register;
                        let _len_id_46400 : Int = Length(_array_id_46395);
                        mutable _index_id_46404 : Int = 0;
                        while _index_id_46404 < _len_id_46400 {
                            let item : Qubit = _array_id_46395[_index_id_46404];
                            X(item);
                            _index_id_46404 += 1;
                        }

                    }

                }
                controlled (ctls, ...) {
                    {
                        let _array_id_46423 : Qubit[] = register;
                        let _len_id_46428 : Int = Length(_array_id_46423);
                        mutable _index_id_46432 : Int = 0;
                        while _index_id_46432 < _len_id_46428 {
                            let item : Qubit = _array_id_46423[_index_id_46432];
                            Controlled X(ctls, item);
                            _index_id_46432 += 1;
                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn apply_to_each_ca_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEachCA(S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEachCA_Qubit__AdjCtl_(S, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEachCA_Qubit__AdjCtl__S_(qs);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEachCA_Qubit__AdjCtl__S_(register : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    {
                        let _array_id_46451 : Qubit[] = register;
                        let _len_id_46456 : Int = Length(_array_id_46451);
                        mutable _index_id_46460 : Int = 0;
                        while _index_id_46460 < _len_id_46456 {
                            let item : Qubit = _array_id_46451[_index_id_46460];
                            S(item);
                            _index_id_46460 += 1;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46479 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46482 : Int = _range_id_46479::Start;
                            let _step_id_46487 : Int = _range_id_46479::Step;
                            let _end_id_46492 : Int = _range_id_46479::End;
                            while _step_id_46487 > 0 and _index_id_46482 <= _end_id_46492 or _step_id_46487 < 0 and _index_id_46482 >= _end_id_46492 {
                                let _index : Int = _index_id_46482;
                                let item : Qubit = _array[_index];
                                Adjoint S(item);
                                _index_id_46482 += _step_id_46487;
                            }

                        }

                    }

                }
                controlled (ctls, ...) {
                    {
                        let _array_id_46522 : Qubit[] = register;
                        let _len_id_46527 : Int = Length(_array_id_46522);
                        mutable _index_id_46531 : Int = 0;
                        while _index_id_46531 < _len_id_46527 {
                            let item : Qubit = _array_id_46522[_index_id_46531];
                            Controlled S(ctls, item);
                            _index_id_46531 += 1;
                        }

                    }

                }
                controlled adjoint (ctls, ...) {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46550 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46553 : Int = _range_id_46550::Start;
                            let _step_id_46558 : Int = _range_id_46550::Step;
                            let _end_id_46563 : Int = _range_id_46550::End;
                            while _step_id_46558 > 0 and _index_id_46553 <= _end_id_46563 or _step_id_46558 < 0 and _index_id_46553 >= _end_id_46563 {
                                let _index : Int = _index_id_46553;
                                let item : Qubit = _array[_index];
                                Controlled Adjoint S(ctls, item);
                                _index_id_46553 += _step_id_46558;
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn cross_package_apply_to_each_closure_arg_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            let angle = 1.0;
            ApplyToEach(q => Rx(angle, q), qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                let angle : Double = 1.;
                ApplyToEach_Qubit__Empty_(/ * closure item = 2 captures = [angle] * / _lambda_2, qs);
                ReleaseQubitArray(qs);
            }
            operation _lambda_2(angle : Double, q : Qubit) : Unit {
                Rx(angle, q)
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                let angle : Double = 1.;
                ApplyToEach_Qubit__Empty__closure_(qs, angle);
                ReleaseQubitArray(qs);
            }
            operation _lambda_2(angle : Double, q : Qubit) : Unit {
                Rx(angle, q)
            }
            operation ApplyToEach_Qubit__Empty__closure_(register : Qubit[], __capture_0 : Double) : Unit {
                {
                    let _array_id_46296 : Qubit[] = register;
                    let _len_id_46301 : Int = Length(_array_id_46296);
                    mutable _index_id_46305 : Int = 0;
                    while _index_id_46305 < _len_id_46301 {
                        let item : Qubit = _array_id_46296[_index_id_46305];
                        _lambda_2(__capture_0, item);
                        _index_id_46305 += 1;
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn cross_package_apply_to_each_adjoint_arg_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(Adjoint S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl_(Adjoint S, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ApplyToEach_Qubit__AdjCtl__Adj_S_(qs);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEach_Qubit__AdjCtl__Adj_S_(register : Qubit[]) : Unit {
                {
                    let _array_id_46296 : Qubit[] = register;
                    let _len_id_46301 : Int = Length(_array_id_46296);
                    mutable _index_id_46305 : Int = 0;
                    while _index_id_46305 < _len_id_46301 {
                        let item : Qubit = _array_id_46296[_index_id_46305];
                        Adjoint S(item);
                        _index_id_46305 += 1;
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn adjoint_cross_package_apply_to_each_ca_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            Adjoint ApplyToEachCA(S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                Adjoint ApplyToEachCA_Qubit__AdjCtl_(S, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                Adjoint ApplyToEachCA_Qubit__AdjCtl__S_(qs);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEachCA_Qubit__AdjCtl__S_(register : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    {
                        let _array_id_46451 : Qubit[] = register;
                        let _len_id_46456 : Int = Length(_array_id_46451);
                        mutable _index_id_46460 : Int = 0;
                        while _index_id_46460 < _len_id_46456 {
                            let item : Qubit = _array_id_46451[_index_id_46460];
                            S(item);
                            _index_id_46460 += 1;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46479 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46482 : Int = _range_id_46479::Start;
                            let _step_id_46487 : Int = _range_id_46479::Step;
                            let _end_id_46492 : Int = _range_id_46479::End;
                            while _step_id_46487 > 0 and _index_id_46482 <= _end_id_46492 or _step_id_46487 < 0 and _index_id_46482 >= _end_id_46492 {
                                let _index : Int = _index_id_46482;
                                let item : Qubit = _array[_index];
                                Adjoint S(item);
                                _index_id_46482 += _step_id_46487;
                            }

                        }

                    }

                }
                controlled (ctls, ...) {
                    {
                        let _array_id_46522 : Qubit[] = register;
                        let _len_id_46527 : Int = Length(_array_id_46522);
                        mutable _index_id_46531 : Int = 0;
                        while _index_id_46531 < _len_id_46527 {
                            let item : Qubit = _array_id_46522[_index_id_46531];
                            Controlled S(ctls, item);
                            _index_id_46531 += 1;
                        }

                    }

                }
                controlled adjoint (ctls, ...) {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46550 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46553 : Int = _range_id_46550::Start;
                            let _step_id_46558 : Int = _range_id_46550::Step;
                            let _end_id_46563 : Int = _range_id_46550::End;
                            while _step_id_46558 > 0 and _index_id_46553 <= _end_id_46563 or _step_id_46558 < 0 and _index_id_46553 >= _end_id_46563 {
                                let _index : Int = _index_id_46553;
                                let item : Qubit = _array[_index];
                                Controlled Adjoint S(ctls, item);
                                _index_id_46553 += _step_id_46558;
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn controlled_apply_to_each_ca_keeps_body_callable_static() {
    let source = r#"
        open Std.Canon;

        operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            ApplyToEachCA(H, inputQubits);
        }

        operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            ApplyToEachCA(X, inputQubits);
        }

        @EntryPoint()
        operation Main() : Unit {
            use qs = Qubit[3];
            let register = [qs[1], qs[2]];
            Controlled PrepareUniform([qs[0]], register);
            Controlled PrepareAllOnes([qs[0]], register);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    ApplyToEachCA_Qubit__AdjCtl_(H, inputQubits);
                }
                adjoint ... {
                    Adjoint ApplyToEachCA_Qubit__AdjCtl_(H, inputQubits);
                }
                controlled (ctls, ...) {
                    Controlled ApplyToEachCA_Qubit__AdjCtl_(ctls, (H, inputQubits));
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl_(ctls, (H, inputQubits));
                }
            }
            operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    ApplyToEachCA_Qubit__AdjCtl_(X, inputQubits);
                }
                adjoint ... {
                    Adjoint ApplyToEachCA_Qubit__AdjCtl_(X, inputQubits);
                }
                controlled (ctls, ...) {
                    Controlled ApplyToEachCA_Qubit__AdjCtl_(ctls, (X, inputQubits));
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl_(ctls, (X, inputQubits));
                }
            }
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                let register : Qubit[] = [qs[1], qs[2]];
                Controlled PrepareUniform([qs[0]], register);
                Controlled PrepareAllOnes([qs[0]], register);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    ApplyToEachCA_Qubit__AdjCtl__H_(inputQubits);
                }
                adjoint ... {
                    Adjoint ApplyToEachCA_Qubit__AdjCtl__H_(inputQubits);
                }
                controlled (ctls, ...) {
                    Controlled ApplyToEachCA_Qubit__AdjCtl__H_(ctls, inputQubits);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl__H_(ctls, inputQubits);
                }
            }
            operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    ApplyToEachCA_Qubit__AdjCtl__X_(inputQubits);
                }
                adjoint ... {
                    Adjoint ApplyToEachCA_Qubit__AdjCtl__X_(inputQubits);
                }
                controlled (ctls, ...) {
                    Controlled ApplyToEachCA_Qubit__AdjCtl__X_(ctls, inputQubits);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl__X_(ctls, inputQubits);
                }
            }
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                let register : Qubit[] = [qs[1], qs[2]];
                Controlled PrepareUniform([qs[0]], register);
                Controlled PrepareAllOnes([qs[0]], register);
                ReleaseQubitArray(qs);
            }
            operation ApplyToEachCA_Qubit__AdjCtl__X_(register : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    {
                        let _array_id_46451 : Qubit[] = register;
                        let _len_id_46456 : Int = Length(_array_id_46451);
                        mutable _index_id_46460 : Int = 0;
                        while _index_id_46460 < _len_id_46456 {
                            let item : Qubit = _array_id_46451[_index_id_46460];
                            X(item);
                            _index_id_46460 += 1;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46479 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46482 : Int = _range_id_46479::Start;
                            let _step_id_46487 : Int = _range_id_46479::Step;
                            let _end_id_46492 : Int = _range_id_46479::End;
                            while _step_id_46487 > 0 and _index_id_46482 <= _end_id_46492 or _step_id_46487 < 0 and _index_id_46482 >= _end_id_46492 {
                                let _index : Int = _index_id_46482;
                                let item : Qubit = _array[_index];
                                Adjoint X(item);
                                _index_id_46482 += _step_id_46487;
                            }

                        }

                    }

                }
                controlled (ctls, ...) {
                    {
                        let _array_id_46522 : Qubit[] = register;
                        let _len_id_46527 : Int = Length(_array_id_46522);
                        mutable _index_id_46531 : Int = 0;
                        while _index_id_46531 < _len_id_46527 {
                            let item : Qubit = _array_id_46522[_index_id_46531];
                            Controlled X(ctls, item);
                            _index_id_46531 += 1;
                        }

                    }

                }
                controlled adjoint (ctls, ...) {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46550 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46553 : Int = _range_id_46550::Start;
                            let _step_id_46558 : Int = _range_id_46550::Step;
                            let _end_id_46563 : Int = _range_id_46550::End;
                            while _step_id_46558 > 0 and _index_id_46553 <= _end_id_46563 or _step_id_46558 < 0 and _index_id_46553 >= _end_id_46563 {
                                let _index : Int = _index_id_46553;
                                let item : Qubit = _array[_index];
                                Controlled Adjoint X(ctls, item);
                                _index_id_46553 += _step_id_46558;
                            }

                        }

                    }

                }
            }
            operation ApplyToEachCA_Qubit__AdjCtl__H_(register : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    {
                        let _array_id_46451 : Qubit[] = register;
                        let _len_id_46456 : Int = Length(_array_id_46451);
                        mutable _index_id_46460 : Int = 0;
                        while _index_id_46460 < _len_id_46456 {
                            let item : Qubit = _array_id_46451[_index_id_46460];
                            H(item);
                            _index_id_46460 += 1;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46479 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46482 : Int = _range_id_46479::Start;
                            let _step_id_46487 : Int = _range_id_46479::Step;
                            let _end_id_46492 : Int = _range_id_46479::End;
                            while _step_id_46487 > 0 and _index_id_46482 <= _end_id_46492 or _step_id_46487 < 0 and _index_id_46482 >= _end_id_46492 {
                                let _index : Int = _index_id_46482;
                                let item : Qubit = _array[_index];
                                Adjoint H(item);
                                _index_id_46482 += _step_id_46487;
                            }

                        }

                    }

                }
                controlled (ctls, ...) {
                    {
                        let _array_id_46522 : Qubit[] = register;
                        let _len_id_46527 : Int = Length(_array_id_46522);
                        mutable _index_id_46531 : Int = 0;
                        while _index_id_46531 < _len_id_46527 {
                            let item : Qubit = _array_id_46522[_index_id_46531];
                            Controlled H(ctls, item);
                            _index_id_46531 += 1;
                        }

                    }

                }
                controlled adjoint (ctls, ...) {
                    {
                        let _array : Qubit[] = register;
                        {
                            let _range_id_46550 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_46553 : Int = _range_id_46550::Start;
                            let _step_id_46558 : Int = _range_id_46550::Step;
                            let _end_id_46563 : Int = _range_id_46550::End;
                            while _step_id_46558 > 0 and _index_id_46553 <= _end_id_46563 or _step_id_46558 < 0 and _index_id_46553 >= _end_id_46563 {
                                let _index : Int = _index_id_46553;
                                let item : Qubit = _array[_index];
                                Controlled Adjoint H(ctls, item);
                                _index_id_46553 += _step_id_46558;
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn cross_package_mapped_defunctionalizes() {
    let source = r#"
        open Std.Arrays;
        function Double(x : Int) : Int { x * 2 }
        @EntryPoint()
        operation Main() : Unit {
            let arr = [1, 2, 3];
            let _ = Mapped(Double, arr);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            function Double(x : Int) : Int {
                x * 2
            }
            operation Main() : Unit {
                let arr : Int[] = [1, 2, 3];
                let _ : Int[] = Mapped_Int__Int_(Double, arr);
            }
            // entry
            Main()

            AFTER:
            function Double(x : Int) : Int {
                x * 2
            }
            operation Main() : Unit {
                let arr : Int[] = [1, 2, 3];
                let _ : Int[] = Mapped_Int__Int__Double_(arr);
            }
            function Mapped_Int__Int__Double_(array : Int[]) : Int[] {
                mutable mapped : Int[] = [];
                {
                    let _array_id_45810 : Int[] = array;
                    let _len_id_45815 : Int = Length(_array_id_45810);
                    mutable _index_id_45819 : Int = 0;
                    while _index_id_45819 < _len_id_45815 {
                        let element : Int = _array_id_45810[_index_id_45819];
                        mapped += [Double(element)];
                        _index_id_45819 += 1;
                    }

                }

                mapped
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn cross_package_for_each_defunctionalizes() {
    let source = r#"
        open Std.Arrays;
        operation Main() : Unit {
            use qs = Qubit[3];
            ForEach(H, qs);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ForEach_Qubit__Unit__AdjCtl_(H, qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                ForEach_Qubit__Unit__AdjCtl__H_(qs);
                ReleaseQubitArray(qs);
            }
            operation ForEach_Qubit__Unit__AdjCtl__H_(array : Qubit[]) : Unit[] {
                mutable output : Unit[] = [];
                {
                    let _array_id_45582 : Qubit[] = array;
                    let _len_id_45587 : Int = Length(_array_id_45582);
                    mutable _index_id_45591 : Int = 0;
                    while _index_id_45591 < _len_id_45587 {
                        let element : Qubit = _array_id_45582[_index_id_45591];
                        output += [H(element)];
                        _index_id_45591 += 1;
                    }

                }

                output
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn stdlib_hof_specialized_with_concrete_callable() {
    let source = r#"
        open Microsoft.Quantum.Arrays;

        operation Main() : Int[] {
            let arr = [1, 2, 3];
            Mapped(x -> x + 1, arr)
        }
        "#;
    check(
        source,
        &expect![[r#"
            .lambda_2: input_ty=(Int,)
            Main: input_ty=Unit
            Mapped<Int, Int>{closure}: input_ty=(Int)[]"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Int[] {
                let arr : Int[] = [1, 2, 3];
                Mapped_Int__Int_(/ * closure item = 2 captures = [] * / _lambda_2, arr)
            }
            function _lambda_2(x : Int, ) : Int {
                x + 1
            }
            // entry
            Main()

            AFTER:
            operation Main() : Int[] {
                let arr : Int[] = [1, 2, 3];
                Mapped_Int__Int__closure_(arr)
            }
            function _lambda_2(x : Int, ) : Int {
                x + 1
            }
            function Mapped_Int__Int__closure_(array : Int[]) : Int[] {
                mutable mapped : Int[] = [];
                {
                    let _array_id_45810 : Int[] = array;
                    let _len_id_45815 : Int = Length(_array_id_45810);
                    mutable _index_id_45819 : Int = 0;
                    while _index_id_45819 < _len_id_45815 {
                        let element : Int = _array_id_45810[_index_id_45819];
                        mapped += [_lambda_2(element, )];
                        _index_id_45819 += 1;
                    }

                }

                mapped
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn lambda_expression_sample_shape_has_no_defunctionalization_errors() {
    let source = r#"
        import Std.Arrays.*;

        operation Main() : Unit {
            let add = (x, y) -> x + y;
            let _ = add(2, 3);

            use control = Qubit();
            let cnotOnControl = q => CNOT(control, q);

            let intArray = [1, 2, 3, 4, 5];
            let _ = Fold(add, 0, intArray);
            let _ = Mapped(x -> x + 1, intArray);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let add : ((Int, Int) -> Int) = / * closure item = 2 captures = [] * / _lambda_2;
                let _ : Int = add(2, 3);
                let control : Qubit = __quantum__rt__qubit_allocate();
                let cnotOnControl : (Qubit => Unit) = / * closure item = 3 captures = [control] * / _lambda_3;
                let intArray : Int[] = [1, 2, 3, 4, 5];
                let _ : Int = Fold_Int__Int_(add, 0, intArray);
                let _ : Int[] = Mapped_Int__Int_(/ * closure item = 4 captures = [] * / _lambda_4, intArray);
                __quantum__rt__qubit_release(control);
            }
            function _lambda_2((x : Int, y : Int), ) : Int {
                x + y
            }
            operation _lambda_3(control : Qubit, q : Qubit) : Unit {
                CNOT(control, q)
            }
            function _lambda_4(x : Int, ) : Int {
                x + 1
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let _ : Int = _lambda_2((2, 3), );
                let control : Qubit = __quantum__rt__qubit_allocate();
                let intArray : Int[] = [1, 2, 3, 4, 5];
                let _ : Int = Fold_Int__Int__closure_(0, intArray);
                let _ : Int[] = Mapped_Int__Int__closure_(intArray);
                __quantum__rt__qubit_release(control);
            }
            function _lambda_2((x : Int, y : Int), ) : Int {
                x + y
            }
            operation _lambda_3(control : Qubit, q : Qubit) : Unit {
                CNOT(control, q)
            }
            function _lambda_4(x : Int, ) : Int {
                x + 1
            }
            function Fold_Int__Int__closure_(state : Int, array : Int[]) : Int {
                mutable current : Int = state;
                {
                    let _array_id_45554 : Int[] = array;
                    let _len_id_45559 : Int = Length(_array_id_45554);
                    mutable _index_id_45563 : Int = 0;
                    while _index_id_45563 < _len_id_45559 {
                        let element : Int = _array_id_45554[_index_id_45563];
                        current = _lambda_2((current, element), );
                        _index_id_45563 += 1;
                    }

                }

                current
            }
            function Mapped_Int__Int__closure_(array : Int[]) : Int[] {
                mutable mapped : Int[] = [];
                {
                    let _array_id_45810 : Int[] = array;
                    let _len_id_45815 : Int = Length(_array_id_45810);
                    mutable _index_id_45819 : Int = 0;
                    while _index_id_45819 < _len_id_45815 {
                        let element : Int = _array_id_45810[_index_id_45819];
                        mapped += [_lambda_4(element, )];
                        _index_id_45819 += 1;
                    }

                }

                mapped
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn partial_application_sample_shape_has_no_defunctionalization_errors() {
    let source = r#"
        import Std.Arrays.*;

        function Main() : Unit {
            let incrementByOne = Add(_, 1);
            let incrementByOneLambda = x -> Add(x, 1);

            let _ = incrementByOne(4);

            let sumAndAddOne = AddMany(_, _, _, 1);
            let sumAndAddOneLambda = (a, b, c) -> AddMany(a, b, c, 1);

            let intArray = [1, 2, 3, 4, 5];
            let _ = Mapped(Add(_, 1), intArray);
        }

        function Add(x : Int, y : Int) : Int {
            return x + y;
        }

        function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
            return a + b + c + d;
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            function Main() : Unit {
                let incrementByOne : (Int -> Int) = {
                    let arg : Int = 1;
                    / * closure item = 4 captures = [arg] * / _lambda_4
                };
                let incrementByOneLambda : (Int -> Int) = / * closure item = 5 captures = [] * / _lambda_5;
                let _ : Int = incrementByOne(4);
                let sumAndAddOne : ((Int, Int, Int) -> Int) = {
                    let arg : Int = 1;
                    / * closure item = 6 captures = [arg] * / _lambda_6
                };
                let sumAndAddOneLambda : ((Int, Int, Int) -> Int) = / * closure item = 7 captures = [] * / _lambda_7;
                let intArray : Int[] = [1, 2, 3, 4, 5];
                let _ : Int[] = Mapped_Int__Int_({
                    let arg : Int = 1;
                    / * closure item = 8 captures = [arg] * / _lambda_8
                }, intArray);
            }
            function Add(x : Int, y : Int) : Int {
                return x + y;
            }
            function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
                return a + b + c + d;
            }
            function _lambda_4(arg : Int, hole : Int) : Int {
                Add(hole, arg)
            }
            function _lambda_5(x : Int, ) : Int {
                Add(x, 1)
            }
            function _lambda_6(arg : Int, (hole : Int, hole : Int, hole : Int)) : Int {
                AddMany(hole, hole, hole, arg)
            }
            function _lambda_7((a : Int, b : Int, c : Int), ) : Int {
                AddMany(a, b, c, 1)
            }
            function _lambda_8(arg : Int, hole : Int) : Int {
                Add(hole, arg)
            }
            // entry
            Main()

            AFTER:
            function Main() : Unit {
                let _ : Int = _lambda_4(1, 4);
                let intArray : Int[] = [1, 2, 3, 4, 5];
                let _ : Int[] = Mapped_Int__Int__closure_(intArray, 1);
            }
            function Add(x : Int, y : Int) : Int {
                return x + y;
            }
            function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
                return a + b + c + d;
            }
            function _lambda_4(arg : Int, hole : Int) : Int {
                Add(hole, arg)
            }
            function _lambda_5(x : Int, ) : Int {
                Add(x, 1)
            }
            function _lambda_6(arg : Int, (hole : Int, hole : Int, hole : Int)) : Int {
                AddMany(hole, hole, hole, arg)
            }
            function _lambda_7((a : Int, b : Int, c : Int), ) : Int {
                AddMany(a, b, c, 1)
            }
            function _lambda_8(arg : Int, hole : Int) : Int {
                Add(hole, arg)
            }
            function Mapped_Int__Int__closure_(array : Int[], __capture_0 : Int) : Int[] {
                mutable mapped : Int[] = [];
                {
                    let _array_id_45810 : Int[] = array;
                    let _len_id_45815 : Int = Length(_array_id_45810);
                    mutable _index_id_45819 : Int = 0;
                    while _index_id_45819 < _len_id_45815 {
                        let element : Int = _array_id_45810[_index_id_45819];
                        mapped += [_lambda_8(__capture_0, element)];
                        _index_id_45819 += 1;
                    }

                }

                mapped
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn cross_package_callable_value_defunctionalized() {
    let lib_source = indoc! {"
        namespace TestLib {
            function ApplyFunc(f: Int -> Int, x: Int) : Int { f(x) }
            function Double(x: Int) : Int { x * 2 }
            export ApplyFunc, Double;
        }
    "};

    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            ApplyFunc(Double, 5)
        }
    "};

    let (_store, _pkg_id) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        crate::test_utils::PipelineStage::Defunc,
    );
}

#[test]
fn cross_package_callable_value_semantic_equivalence() {
    let lib_source = indoc! {"
        namespace TestLib {
            function ApplyFunc(f: Int -> Int, x: Int) : Int { f(x) }
            function Double(x: Int) : Int { x * 2 }
            export ApplyFunc, Double;
        }
    "};

    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            ApplyFunc(Double, 5)
        }
    "};

    crate::test_utils::check_semantic_equivalence_with_library(lib_source, user_source);
}

/// The same library HOF is called with the same global callable from two
/// packages: the user entry and a sibling library function. Defunctionalization
/// keys the specialization on the callee, not the caller package, so both call
/// sites dedup to a single specialization. The program stays correct regardless
/// of which package that shared specialization lands in.
#[test]
fn cross_package_same_hof_same_global_from_two_packages_is_correct() {
    let lib_source = indoc! {"
        namespace TestLib {
            function Inc(x : Int) : Int { x + 1 }
            function ApplyFn(f : Int -> Int, x : Int) : Int { f(x) }
            function LibUsesInc(x : Int) : Int { ApplyFn(Inc, x) }
            export Inc, ApplyFn, LibUsesInc;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            ApplyFn(Inc, 10) + LibUsesInc(20)
        }
    "};

    // Inc(10) + Inc(20) = 32, both before and after the transforms.
    crate::test_utils::check_semantic_equivalence_with_library(lib_source, user_source);
}
