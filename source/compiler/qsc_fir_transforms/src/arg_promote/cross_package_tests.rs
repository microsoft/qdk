// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Cross-package argument promotion: a tuple-parameter callable declared in a
//! library package is flattened in lockstep with every call site in every
//! reachable package (user + library), projection temps are minted into the
//! caller's package, and the cross-package call-shape invariant validates the
//! rewrite end-to-end.

use super::*;
use crate::test_utils::{
    PipelineStage, assert_panics_with, check_semantic_equivalence_with_library,
    compile_and_run_pipeline_to_with_library, compile_to_fir_with_library, find_callable,
    find_library_callable,
};
use indoc::indoc;
use qsc_fir::fir::{ExprId, ExprKind, ItemKind, PackageLookup, PatKind, StoreItemId};
use qsc_fir::ty::{Prim, Ty};

/// Returns the display string of a reachable callable's input pattern type.
fn callable_input_ty_string(store: &PackageStore, sid: StoreItemId) -> String {
    let package = store.get(sid.package);
    let ItemKind::Callable(decl) = &package.get_item(sid.item).kind else {
        panic!("expected callable");
    };
    package.get_pat(decl.input).ty.to_string()
}

/// Collects the display strings of the argument types of every direct call to
/// `callee` found in `caller_pkg` (entry expression and all callable bodies).
fn call_arg_type_strings_to(
    store: &PackageStore,
    caller_pkg: PackageId,
    callee: StoreItemId,
) -> Vec<String> {
    let package = store.get(caller_pkg);
    let mut arg_tys = Vec::new();
    let mut visit = |_expr_id, expr: &qsc_fir::fir::Expr| {
        if let ExprKind::Call(callee_id, arg_id) = expr.kind
            && let Some(resolved) = resolve_direct_item_callee(package, callee_id)
            && resolved.item_id == callee
        {
            arg_tys.push(package.get_expr(arg_id).ty.to_string());
        }
    };
    if let Some(entry) = package.entry {
        crate::walk_utils::for_each_expr(package, entry, &mut visit);
    }
    for item in package.items.values() {
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut visit,
            );
        }
    }
    arg_tys
}

/// Returns `true` if `pkg` contains any synthesized argument-promotion
/// projection temporary in its pattern arena.
fn package_has_arg_promote_temp(store: &PackageStore, pkg: PackageId) -> bool {
    store.get(pkg).pats.values().any(|pat| {
        matches!(&pat.kind, PatKind::Bind(ident) if ident.name.starts_with(ARG_PROMOTE_TMP_NAME))
    })
}

/// A library callable whose nested-tuple input is flattened, called from both
/// the user package and another library callable. Every call site is rewritten
/// to the flat argument shape, no call site retains the original nested tuple,
/// and behavior is unchanged end-to-end.
#[test]
fn cross_package_library_callee_flattened_and_all_call_sites_rewritten() {
    let lib_source = indoc! {"
        namespace TestLib {
            function Op(p : (Int, (Int, Int))) : Int {
                let (a, (b, c)) = p;
                a + b + c
            }
            function CallOpInLib(x : Int) : Int {
                Op((x, (x + 1, x + 2)))
            }
            export Op, CallOpInLib;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        function Main() : Int { Op((3, (4, 5))) + CallOpInLib(10) }
    "};

    let (store, pkg_id) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);

    let op = find_library_callable(&store, pkg_id, "Op");
    let lib_pkg = op.package;

    // The library callee's nested input tuple is dissolved into flat scalars.
    assert_eq!(
        callable_input_ty_string(&store, op),
        "(Int, Int, Int)",
        "library callee input should be flattened across the package boundary"
    );

    // Every call site — the one in the user entry package and the one in the
    // sibling library callable — is rewritten to the flat argument shape, so no
    // call retains the original nested `(Int, (Int, Int))` tuple.
    let user_args = call_arg_type_strings_to(&store, pkg_id, op);
    let lib_args = call_arg_type_strings_to(&store, lib_pkg, op);
    assert_eq!(user_args, vec!["(Int, Int, Int)".to_string()]);
    assert_eq!(lib_args, vec!["(Int, Int, Int)".to_string()]);

    // The pipeline runs the cross-package PostArgPromote call-shape invariant as
    // part of `PipelineStage::Full`; reaching this point means it passed for the
    // library call site too. Assert it again explicitly for clarity.
    crate::invariants::check(
        &store,
        pkg_id,
        crate::invariants::InvariantLevel::PostArgPromote,
    );

    check_semantic_equivalence_with_library(lib_source, user_source);
}

/// A library callable whose only call site is inside the library (the user
/// entry never calls it directly) is still promoted across the package boundary,
/// and its in-library call site is rewritten. Promotion does not require an
/// entry-package call site.
#[test]
fn cross_package_foreign_only_candidate_is_promoted_and_library_call_site_rewritten() {
    let lib_source = indoc! {"
        namespace TestLib {
            function Op(t : (Int, (Int, Int))) : Int {
                let (a, (b, c)) = t;
                a + b + c
            }
            function CallOpInLib(x : Int) : Int {
                Op((x, (x + 1, x + 2)))
            }
            export CallOpInLib;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        function Main() : Int { CallOpInLib(10) }
    "};

    let (store, pkg_id) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);

    let op = find_library_callable(&store, pkg_id, "Op");
    let lib_pkg = op.package;
    assert_ne!(lib_pkg, pkg_id, "Op must live in a foreign package");

    // The foreign-only callee's nested input tuple is dissolved into flat scalars.
    assert_eq!(
        callable_input_ty_string(&store, op),
        "(Int, Int, Int)",
        "foreign-only candidate input should be flattened across the package boundary"
    );

    // Its only call site (inside the library) is rewritten to the flat shape,
    // and the user package never calls it directly.
    let lib_args = call_arg_type_strings_to(&store, lib_pkg, op);
    assert_eq!(lib_args, vec!["(Int, Int, Int)".to_string()]);
    let user_args = call_arg_type_strings_to(&store, pkg_id, op);
    assert!(
        user_args.is_empty(),
        "user package should not call Op directly"
    );

    check_semantic_equivalence_with_library(lib_source, user_source);
}

/// Argument promotion flattens a nested tuple parameter defined in a leaf
/// library (libB) reached through an intermediate library (libA), proving the
/// cross-package transform propagates across an entry → libA → libB chain of
/// distinct packages.
#[test]
fn cross_package_three_package_chain_flattens_leaf_library_callee() {
    let lib_b = indoc! {"
        namespace LibB {
            function Sum(t : (Int, (Int, Int))) : Int {
                let (a, (b, c)) = t;
                a + b + c
            }
            export Sum;
        }
    "};
    let lib_a = indoc! {"
        namespace LibA {
            import LibB.*;
            function UseSum(x : Int) : Int { Sum((x, (x + 1, x + 2))) }
            export UseSum;
        }
    "};
    let user = indoc! {"
        import LibA.*;
        @EntryPoint()
        function Main() : Int { UseSum(10) }
    "};

    let (store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to_with_two_libraries(
        lib_b,
        lib_a,
        user,
        PipelineStage::Full,
    );

    let sum = find_library_callable(&store, pkg_id, "Sum");
    let use_sum = find_library_callable(&store, pkg_id, "UseSum");
    assert_ne!(sum.package, pkg_id, "Sum (libB) must be a foreign package");
    assert_ne!(
        use_sum.package, pkg_id,
        "UseSum (libA) must be a foreign package"
    );
    assert_ne!(
        sum.package, use_sum.package,
        "Sum (libB) and UseSum (libA) must live in distinct packages"
    );

    // The leaf-library callee's nested tuple parameter is flattened across two
    // package hops.
    assert_eq!(
        callable_input_ty_string(&store, sum),
        "(Int, Int, Int)",
        "leaf-library callee input should be flattened through the chain"
    );

    // libA's call site to libB's `Sum` is rewritten to the flat argument shape.
    let lib_a_args = call_arg_type_strings_to(&store, use_sum.package, sum);
    assert_eq!(lib_a_args, vec!["(Int, Int, Int)".to_string()]);
}

/// A controlled cross-package call site projects the promoted payload to its
/// flattened leaves while preserving the control layer.
#[test]
fn cross_package_controlled_call_payload_projected_controls_preserved() {
    let lib_source = indoc! {"
        namespace TestLib {
            operation Op(p : (Qubit, (Qubit, Qubit))) : Unit is Ctl {
                let (a, (b, c)) = p;
                CNOT(a, b);
                CNOT(b, c);
            }
            export Op;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        import Std.Measurement.*;
        @EntryPoint()
        operation Main() : Result {
            use ctl = Qubit();
            use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
            Controlled Op([ctl], (q0, (q1, q2)));
            let r = MResetZ(q0);
            Reset(ctl);
            Reset(q1);
            Reset(q2);
            r
        }
    "};

    let (store, pkg_id) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);

    let op = find_library_callable(&store, pkg_id, "Op");
    assert_eq!(
        callable_input_ty_string(&store, op),
        "(Qubit, Qubit, Qubit)",
        "controlled library callee payload should be flattened"
    );

    // Locate the `Controlled Op(...)` call in the user entry package and confirm
    // its argument is still a `(controls, payload)` tuple whose controls layer
    // is preserved and whose payload now carries the flat leaf shape.
    let user_package = store.get(pkg_id);
    let controlled_arg_id = find_cross_package_functor_call_arg(user_package, "Main", op, 1);
    let ExprKind::Tuple(items) = &user_package.get_expr(controlled_arg_id).kind else {
        panic!("controlled argument should remain a controls/payload tuple");
    };
    let [controls_id, payload_id] = items.as_slice() else {
        panic!("controlled argument should have controls and payload elements");
    };
    assert!(
        matches!(user_package.get_expr(*controls_id).ty, Ty::Array(_)),
        "controls layer should be preserved in the first tuple position"
    );
    assert_eq!(
        user_package.get_expr(*payload_id).ty.to_string(),
        "(Qubit, Qubit, Qubit)",
        "controlled payload should be projected to the promoted callee's flat input"
    );

    crate::invariants::check(
        &store,
        pkg_id,
        crate::invariants::InvariantLevel::PostArgPromote,
    );

    check_semantic_equivalence_with_library(lib_source, user_source);
}

/// Finds the argument expression of a direct call to `callee` (resolved across
/// package boundaries) with the given controlled depth, inside `caller_name`.
fn find_cross_package_functor_call_arg(
    package: &qsc_fir::fir::Package,
    caller_name: &str,
    callee: StoreItemId,
    controlled_depth: usize,
) -> ExprId {
    let callable = find_callable(package, caller_name);
    let mut found = None;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &callable.implementation,
        &mut |_expr_id, expr| {
            if found.is_some() {
                return;
            }
            if let ExprKind::Call(callee_id, arg_id) = expr.kind
                && let Some(resolved) = resolve_direct_item_callee(package, callee_id)
                && resolved.item_id == callee
                && resolved.controlled_depth == controlled_depth
            {
                found = Some(arg_id);
            }
        },
    );
    found.unwrap_or_else(|| panic!("controlled call to callee not found in '{caller_name}'"))
}

/// A call site whose argument is not safe to project repeatedly (here, the
/// result of a foreign call) mints its projection temporary into the *caller's*
/// package — not the callee's owning package.
#[test]
fn cross_package_projection_temp_minted_into_caller_package() {
    let lib_source = indoc! {"
        namespace TestLib {
            function Op(p : (Int, (Int, Int))) : Int {
                let (a, (b, c)) = p;
                a + b + c
            }
            function MakeNested(x : Int) : (Int, (Int, Int)) {
                (x, (x + 1, x + 2))
            }
            export Op, MakeNested;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        function Main() : Int { Op(MakeNested(3)) }
    "};

    let (store, pkg_id) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);

    let op = find_library_callable(&store, pkg_id, "Op");
    let lib_pkg = op.package;
    assert_eq!(callable_input_ty_string(&store, op), "(Int, Int, Int)");

    // The user call `Op(MakeNested(3))` materializes the foreign call result into
    // a projection temporary so it is projected exactly once. That temporary is
    // minted from the caller (user) package's assigner, landing in the user
    // package's arena, never the library callee's package.
    assert!(
        package_has_arg_promote_temp(&store, pkg_id),
        "projection temporary should be minted into the caller (user) package"
    );
    assert!(
        !package_has_arg_promote_temp(&store, lib_pkg),
        "library package should not receive the caller's projection temporary"
    );

    check_semantic_equivalence_with_library(lib_source, user_source);
}

/// A library callable used as a first-class value in the user package is
/// recorded by the cross-package safety filter (keyed by its own
/// `StoreItemId`) and is therefore excluded from flattening, while a sibling
/// callable used only via direct calls is not excluded.
#[test]
fn cross_package_first_class_library_callable_excluded_by_union_filter() {
    let lib_source = indoc! {"
        namespace TestLib {
            function FirstClass(p : (Int, Int)) : Int {
                let (a, b) = p;
                a + b
            }
            function DirectOnly(p : (Int, Int)) : Int {
                let (a, b) = p;
                a * b
            }
            export FirstClass, DirectOnly;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        function Main() : Int {
            let f = FirstClass;
            f((3, 4)) + DirectOnly((5, 6))
        }
    "};

    // Use the untransformed FIR so the first-class `let f = FirstClass;` arrow
    // reference is still present when the safety filter scans the closure.
    let (store, pkg_id) = compile_to_fir_with_library(lib_source, user_source);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);

    let first_class = collect_first_class_callables(&store, pkg_id, &reachable);
    let first_class_sid = find_library_callable(&store, pkg_id, "FirstClass");
    let direct_only_sid = find_library_callable(&store, pkg_id, "DirectOnly");

    assert!(
        first_class.contains(&first_class_sid),
        "a library callable used first-class in the user package must be unioned \
         into the safety filter by its own StoreItemId"
    );
    assert!(
        !first_class.contains(&direct_only_sid),
        "a library callable used only via direct calls must not be treated as first-class"
    );
}

/// Deliberately corrupts a *library* call site so its argument no longer matches
/// the promoted callee's input, then confirms the cross-package
/// `check_call_shape_matches_callee` invariant catches it. With the
/// argument-promotion stage check still entry-only this library call site would
/// be silently skipped; the lockstep flip to cross-package scope is what makes
/// the mismatch observable.
#[test]
fn cross_package_stale_library_call_site_caught_by_call_shape_check() {
    let lib_source = indoc! {"
        namespace TestLib {
            function Op(p : (Int, (Int, Int))) : Int {
                let (a, (b, c)) = p;
                a + b + c
            }
            function CallOpInLib(x : Int) : Int {
                Op((x, (x + 1, x + 2)))
            }
            export Op, CallOpInLib;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        function Main() : Int { Op((3, (4, 5))) + CallOpInLib(10) }
    "};

    let (mut store, pkg_id) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);

    let op = find_library_callable(&store, pkg_id, "Op");
    let lib_pkg = op.package;

    // Corrupt the `Op(...)` argument inside the *library* callable `CallOpInLib`
    // so it no longer matches the flattened callee input.
    let arg_id = call_arg_expr_id_to(&store, lib_pkg, "CallOpInLib", op);
    store
        .get_mut(lib_pkg)
        .exprs
        .get_mut(arg_id)
        .expect("library call argument should exist")
        .ty = Ty::Prim(Prim::Int);

    assert_panics_with("PostArgPromote/PostAll call invariant violation", || {
        crate::invariants::check(
            &store,
            pkg_id,
            crate::invariants::InvariantLevel::PostArgPromote,
        );
    });
}

/// Returns the argument `ExprId` of the first direct call to `callee` found in
/// the named callable of `caller_pkg`.
fn call_arg_expr_id_to(
    store: &PackageStore,
    caller_pkg: PackageId,
    caller_name: &str,
    callee: StoreItemId,
) -> ExprId {
    let package = store.get(caller_pkg);
    let callable = find_callable(package, caller_name);
    let mut found = None;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &callable.implementation,
        &mut |_expr_id, expr| {
            if found.is_some() {
                return;
            }
            if let ExprKind::Call(callee_id, arg_id) = expr.kind
                && let Some(resolved) = resolve_direct_item_callee(package, callee_id)
                && resolved.item_id == callee
            {
                found = Some(arg_id);
            }
        },
    );
    found.unwrap_or_else(|| panic!("call to callee not found in '{caller_name}'"))
}
