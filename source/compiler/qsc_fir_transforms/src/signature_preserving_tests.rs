// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Unit tests for the body-only signature-preserving sub-pipeline
//! ([`crate::run_signature_preserving_subpipeline`]) used by the codegen
//! `ReinvokeOriginal` pinned path.
//!
//! Each test compiles a package whose target callable is **not** entry-reachable
//! and is therefore skipped by the main pipeline's body-rewriting passes
//! (`monomorphize` / `return_unify` / `defunctionalize` / `arg_promote`). The
//! callable is pinned through the main `Full` pipeline, then re-processed by the
//! sub-pipeline so its early `return`s are rewritten into single-exit form while
//! its signature (including the un-defunctionalized arrow argument) is preserved.

use indoc::indoc;
use qsc_fir::fir::{
    ExprKind, ItemKind, LocalItemId, Package, PackageId, PackageLookup, PackageStore, StoreItemId,
};
use qsc_fir::ty::{Prim, Ty};

use crate::invariants::{self, InvariantLevel};
use crate::test_utils::{
    assert_panics_with, callable_id_by_name, compile_to_fir, compile_to_fir_with_library,
    find_library_callable,
};
use crate::walk_utils::for_each_expr_in_callable_impl;
use crate::{
    PipelineStage, run_pipeline_to_with_diagnostics, run_signature_preserving_subpipeline,
};

/// A pinned (non-entry-reachable) operation that takes an arrow-typed argument
/// and early-returns inside a measurement-dependent (dynamic) branch. The `op`
/// parameter is never defunctionalized because the callable is not
/// entry-reachable during the main pipeline, mirroring the codegen
/// `ReinvokeOriginal` pinned target.
const PINNED_ARROW_EARLY_RETURN: &str = indoc! {"
    namespace Test {
        import Std.Measurement.*;
        @EntryPoint()
        operation Main() : Int { 42 }
        operation Pinned(op : (Qubit => Unit)) : Int {
            use q = Qubit();
            op(q);
            let r = MResetZ(q);
            if r == One {
                return 1;
            }
            return 2;
        }
    }
"};

/// Locates a callable by name in any package of `store`, regardless of
/// reachability. Used to seed a callable that is not entry-reachable.
fn find_callable_in_any_package(store: &PackageStore, name: &str) -> StoreItemId {
    for (package, pkg) in store {
        for (item, item_decl) in pkg.items.iter() {
            if let ItemKind::Callable(decl) = &item_decl.kind
                && decl.name.name.as_ref() == name
            {
                return StoreItemId { package, item };
            }
        }
    }
    panic!("callable {name} not found in any package");
}

fn callable_has_return(package: &Package, item: LocalItemId) -> bool {
    let ItemKind::Callable(decl) = &package.get_item(item).kind else {
        panic!("expected item {item:?} to be a callable");
    };
    let mut found = false;
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_id, expr| {
        if matches!(expr.kind, ExprKind::Return(_)) {
            found = true;
        }
    });
    found
}

/// Compiles `source`, runs the main `Full` pipeline with the `target` callable
/// pinned, and returns the store, package id, and the pinned callable's
/// `StoreItemId`. The pinned callable is not entry-reachable, so the main
/// pipeline leaves its body-rewriting work to the sub-pipeline.
fn prepare_pinned(source: &str, target: &str) -> (PackageStore, PackageId, StoreItemId) {
    let (mut store, pkg_id) = compile_to_fir(source);
    let pinned_local = callable_id_by_name(store.get(pkg_id), target);
    let pinned_store_id = StoreItemId {
        package: pkg_id,
        item: pinned_local,
    };
    let result = run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::Full,
        &[pinned_store_id],
    );
    assert!(
        result.is_success(),
        "main pipeline should succeed: {:?}",
        result.errors
    );
    (store, pkg_id, pinned_store_id)
}

#[test]
fn subpipeline_rewrites_pinned_early_dynamic_return() {
    let (mut store, pkg_id, pinned) = prepare_pinned(PINNED_ARROW_EARLY_RETURN, "Pinned");

    // The main pipeline skips the non-entry-reachable pinned body, so the early
    // returns are still present before the sub-pipeline runs.
    assert!(
        callable_has_return(store.get(pkg_id), pinned.item),
        "pinned body should retain early returns after the main pipeline"
    );

    let result = run_signature_preserving_subpipeline(&mut store, pkg_id, &[pinned]);
    assert!(
        result.is_success(),
        "sub-pipeline should succeed: {:?}",
        result.errors
    );

    // return_unify rewrote the early returns into single-exit form.
    assert!(
        !callable_has_return(store.get(pkg_id), pinned.item),
        "sub-pipeline should remove all Return nodes from the pinned body"
    );
}

#[test]
fn subpipeline_preserves_arrow_signature() {
    let (mut store, pkg_id, pinned) = prepare_pinned(PINNED_ARROW_EARLY_RETURN, "Pinned");

    let result = run_signature_preserving_subpipeline(&mut store, pkg_id, &[pinned]);
    assert!(result.is_success(), "sub-pipeline should succeed");

    // The arrow-typed input is excluded from defunctionalization and argument
    // promotion (the callable is not entry-reachable), and the output stays Int.
    let package = store.get(pkg_id);
    let ItemKind::Callable(decl) = &package.get_item(pinned.item).kind else {
        panic!("expected Pinned to be a callable");
    };
    let input_ty = &package.get_pat(decl.input).ty;
    assert!(
        matches!(input_ty, Ty::Arrow(_)),
        "pinned input signature should remain an arrow type, found {input_ty}"
    );
    assert_eq!(
        decl.output,
        Ty::Prim(Prim::Int),
        "pinned output signature should remain Int"
    );
}

#[test]
fn post_signature_preserving_check_rejects_residual_return() {
    // The pinned body still contains early returns because the sub-pipeline has
    // not run. The seed-rooted PostSignaturePreserving check must reject them.
    let (store, pkg_id, pinned) = prepare_pinned(PINNED_ARROW_EARLY_RETURN, "Pinned");
    assert_panics_with("ExprKind::Return found", || {
        invariants::check_with_seeds(
            &store,
            pkg_id,
            InvariantLevel::PostSignaturePreserving,
            &[pinned],
        );
    });
}

#[test]
fn post_signature_preserving_check_accepts_rewritten_pinned_body() {
    let (mut store, pkg_id, pinned) = prepare_pinned(PINNED_ARROW_EARLY_RETURN, "Pinned");
    let result = run_signature_preserving_subpipeline(&mut store, pkg_id, &[pinned]);
    assert!(result.is_success(), "sub-pipeline should succeed");

    // After the sub-pipeline rewrites the body, the seed-rooted check accepts
    // the preserved arrow residue and the single-exit form without panicking.
    invariants::check_with_seeds(
        &store,
        pkg_id,
        InvariantLevel::PostSignaturePreserving,
        &[pinned],
    );
}

/// Edge case: a pinned callable that early-returns a user-defined type inside a
/// dynamic branch. UDT erasure runs over the whole package arena in the main
/// pipeline (it is not entry-reachability-scoped), so the pinned body's UDT is
/// already lowered to a tuple before the sub-pipeline runs. This test asserts
/// the sub-pipeline lowers the body to single-exit form rather than silently
/// leaving a residual return.
#[test]
fn subpipeline_rewrites_pinned_udt_returning_early_return() {
    const SOURCE: &str = indoc! {"
        namespace Test {
            import Std.Measurement.*;
            newtype Wrapper = Int;
            @EntryPoint()
            operation Main() : Int { 42 }
            operation PinnedUdt(op : (Qubit => Unit)) : Wrapper {
                use q = Qubit();
                op(q);
                let r = MResetZ(q);
                if r == One {
                    return Wrapper(1);
                }
                return Wrapper(2);
            }
        }
    "};

    let (mut store, pkg_id, pinned) = prepare_pinned(SOURCE, "PinnedUdt");
    let result = run_signature_preserving_subpipeline(&mut store, pkg_id, &[pinned]);
    assert!(
        result.is_success(),
        "sub-pipeline should succeed on UDT-returning pinned body: {:?}",
        result.errors
    );
    assert!(
        !callable_has_return(store.get(pkg_id), pinned.item),
        "sub-pipeline should remove all Return nodes from the UDT-returning pinned body"
    );
}

/// A pinned (non-entry-reachable) operation whose input is a plain value type
/// (not an arrow), still early-returning inside a measurement-dependent branch.
/// Mirrors the `ReinvokeOriginal` pinned path for a target that takes no
/// arrow-typed argument, confirming the sub-pipeline rewrites the body to
/// single-exit form and preserves the non-arrow signature.
const PINNED_NON_ARROW_EARLY_RETURN: &str = indoc! {"
    namespace Test {
        import Std.Measurement.*;
        @EntryPoint()
        operation Main() : Int { 42 }
        operation Pinned(q : Qubit) : Int {
            let r = MResetZ(q);
            if r == One {
                return 1;
            }
            return 2;
        }
    }
"};

#[test]
fn subpipeline_rewrites_pinned_non_arrow_early_return() {
    let (mut store, pkg_id, pinned) = prepare_pinned(PINNED_NON_ARROW_EARLY_RETURN, "Pinned");

    // The main pipeline skips the non-entry-reachable pinned body, so the early
    // returns are still present before the sub-pipeline runs.
    assert!(
        callable_has_return(store.get(pkg_id), pinned.item),
        "pinned body should retain early returns after the main pipeline"
    );

    let result = run_signature_preserving_subpipeline(&mut store, pkg_id, &[pinned]);
    assert!(
        result.is_success(),
        "sub-pipeline should succeed: {:?}",
        result.errors
    );

    // The body is rewritten to single-exit form.
    assert!(
        !callable_has_return(store.get(pkg_id), pinned.item),
        "sub-pipeline should remove all Return nodes from the non-arrow pinned body"
    );

    // The plain `Qubit` input and `Int` output signature are preserved.
    let package = store.get(pkg_id);
    let ItemKind::Callable(decl) = &package.get_item(pinned.item).kind else {
        panic!("expected Pinned to be a callable");
    };
    assert_eq!(
        package.get_pat(decl.input).ty,
        Ty::Prim(Prim::Qubit),
        "pinned non-arrow input signature should be preserved"
    );
    assert_eq!(
        decl.output,
        Ty::Prim(Prim::Int),
        "pinned output signature should remain Int"
    );

    invariants::check_with_seeds(
        &store,
        pkg_id,
        InvariantLevel::PostSignaturePreserving,
        &[pinned],
    );
}

/// A pinned (non-entry-reachable) operation with multiple early returns spread
/// across several dynamic branches (a nested `if`/`else`), each guarded by a
/// distinct measurement. The sub-pipeline must collapse all of them into a
/// single-exit form.
const PINNED_MULTI_BRANCH_EARLY_RETURN: &str = indoc! {"
    namespace Test {
        import Std.Measurement.*;
        @EntryPoint()
        operation Main() : Int { 42 }
        operation Pinned(op : (Qubit => Unit)) : Int {
            use q = Qubit();
            op(q);
            let r = MResetZ(q);
            if r == One {
                return 1;
            } else {
                let s = MResetZ(q);
                if s == One {
                    return 2;
                }
            }
            return 3;
        }
    }
"};

#[test]
fn subpipeline_rewrites_pinned_multiple_branch_early_returns() {
    let (mut store, pkg_id, pinned) = prepare_pinned(PINNED_MULTI_BRANCH_EARLY_RETURN, "Pinned");

    // Sanity: the body really does carry multiple Return nodes before the
    // sub-pipeline runs.
    let return_count = |store: &PackageStore| {
        let package = store.get(pkg_id);
        let ItemKind::Callable(decl) = &package.get_item(pinned.item).kind else {
            panic!("expected Pinned to be a callable");
        };
        let mut count = 0;
        for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_id, expr| {
            if matches!(expr.kind, ExprKind::Return(_)) {
                count += 1;
            }
        });
        count
    };
    assert!(
        return_count(&store) >= 3,
        "pinned body should carry the three branch early returns before the sub-pipeline, got {}",
        return_count(&store)
    );

    let result = run_signature_preserving_subpipeline(&mut store, pkg_id, &[pinned]);
    assert!(
        result.is_success(),
        "sub-pipeline should succeed: {:?}",
        result.errors
    );

    // Every early return across every branch is rewritten into single-exit form.
    assert_eq!(
        return_count(&store),
        0,
        "sub-pipeline should remove all Return nodes from the multi-branch pinned body"
    );

    invariants::check_with_seeds(
        &store,
        pkg_id,
        InvariantLevel::PostSignaturePreserving,
        &[pinned],
    );
}

/// A library operation with a measurement-dependent early return. Because the
/// user entry point calls it directly, it is entry-reachable and the main
/// pipeline rewrites its early returns into single-exit form in place, in the
/// library package — even though it lives in a different package than the
/// entry point.
const CROSS_PACKAGE_LIBRARY: &str = indoc! {"
    namespace Lib {
        import Std.Measurement.*;
        operation LibEarlyReturn(q : Qubit) : Int {
            let r = MResetZ(q);
            if r == One {
                return 1;
            }
            return 2;
        }
        export LibEarlyReturn;
    }
"};

/// A user program whose entry point calls the library operation directly (so
/// it is entry-reachable and transformed by the main pipeline) and whose
/// non-entry-reachable `Pinned` operation also calls it. `Pinned` takes an
/// arrow-typed argument (excluding it from defunctionalization) and
/// early-returns, mirroring the codegen `ReinvokeOriginal` pinned target whose
/// transitive callees include a cross-package-transformed library operation.
const CROSS_PACKAGE_USER: &str = indoc! {"
    namespace Test {
        import Lib.*;
        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            LibEarlyReturn(q)
        }
        operation Pinned(op : (Qubit => Unit)) : Int {
            use q = Qubit();
            op(q);
            let v = LibEarlyReturn(q);
            if v == 1 {
                return 10;
            }
            return 20;
        }
    }
"};

/// Compiles `lib_source` + `user_source`, runs the main `Full` pipeline with
/// the user-package `target` callable pinned, and returns the store, user
/// package id, and the pinned callable's `StoreItemId`. The pinned callable is
/// not entry-reachable, so the main pipeline leaves its body-rewriting work to
/// the sub-pipeline while still transforming the entry-reachable library
/// callees it shares with the entry point.
fn prepare_pinned_with_library(
    lib_source: &str,
    user_source: &str,
    target: &str,
) -> (PackageStore, PackageId, StoreItemId) {
    let (mut store, user_pkg_id) = compile_to_fir_with_library(lib_source, user_source);
    let pinned_local = callable_id_by_name(store.get(user_pkg_id), target);
    let pinned_store_id = StoreItemId {
        package: user_pkg_id,
        item: pinned_local,
    };
    let result = run_pipeline_to_with_diagnostics(
        &mut store,
        user_pkg_id,
        PipelineStage::Full,
        &[pinned_store_id],
    );
    assert!(
        result.is_success(),
        "main pipeline should succeed: {:?}",
        result.errors
    );
    (store, user_pkg_id, pinned_store_id)
}

/// Cross-package consistency between the body-only sub-pipeline and the main
/// pipeline's cross-package transformation: a pinned, non-entry-reachable
/// target whose transitive callees include a library operation that the main
/// pipeline already rewrote in its own (foreign) package. Running the
/// sub-pipeline on the pinned target must rewrite the pinned body to
/// single-exit form without disturbing the already-transformed library callee,
/// and the library callee must stay correctly transformed (single-exit body,
/// preserved signature) throughout.
#[test]
fn cross_package_subpipeline_preserves_pinned_and_library_callee() {
    let (mut store, user_pkg_id, pinned) =
        prepare_pinned_with_library(CROSS_PACKAGE_LIBRARY, CROSS_PACKAGE_USER, "Pinned");

    let lib_callee = find_library_callable(&store, user_pkg_id, "LibEarlyReturn");
    assert_ne!(
        lib_callee.package, user_pkg_id,
        "library callee should live in a foreign (library) package"
    );

    // The main pipeline transformed the entry-reachable library callee in place
    // (its early returns are gone) but skipped the non-entry-reachable pinned
    // body (its early returns remain).
    assert!(
        !callable_has_return(store.get(lib_callee.package), lib_callee.item),
        "main pipeline should remove early returns from the cross-package library callee"
    );
    assert!(
        callable_has_return(store.get(user_pkg_id), pinned.item),
        "pinned body should retain early returns after the main pipeline"
    );

    // Record the library callee's signature so we can confirm the pinned-target
    // sub-pipeline leaves it untouched.
    let (lib_input_before, lib_output_before) = {
        let package = store.get(lib_callee.package);
        let ItemKind::Callable(decl) = &package.get_item(lib_callee.item).kind else {
            panic!("expected LibEarlyReturn to be a callable");
        };
        (package.get_pat(decl.input).ty.clone(), decl.output.clone())
    };

    let result = run_signature_preserving_subpipeline(&mut store, user_pkg_id, &[pinned]);
    assert!(
        result.is_success(),
        "sub-pipeline should succeed: {:?}",
        result.errors
    );

    // (a) The sub-pipeline rewrote the pinned body into single-exit form.
    assert!(
        !callable_has_return(store.get(user_pkg_id), pinned.item),
        "sub-pipeline should remove all Return nodes from the pinned body"
    );
    let pinned_input_ty = {
        let package = store.get(user_pkg_id);
        let ItemKind::Callable(decl) = &package.get_item(pinned.item).kind else {
            panic!("expected Pinned to be a callable");
        };
        package.get_pat(decl.input).ty.clone()
    };
    assert!(
        matches!(pinned_input_ty, Ty::Arrow(_)),
        "pinned input signature should remain an arrow type, found {pinned_input_ty}"
    );

    // (b) The cross-package library callee is unchanged by the pinned-target
    // sub-pipeline: still single-exit, same signature. The two transformation
    // paths do not corrupt each other.
    assert!(
        !callable_has_return(store.get(lib_callee.package), lib_callee.item),
        "library callee should stay single-exit after the pinned sub-pipeline"
    );
    let package = store.get(lib_callee.package);
    let ItemKind::Callable(decl) = &package.get_item(lib_callee.item).kind else {
        panic!("expected LibEarlyReturn to be a callable");
    };
    assert_eq!(
        package.get_pat(decl.input).ty,
        lib_input_before,
        "library callee input signature should be unchanged by the pinned sub-pipeline"
    );
    assert_eq!(
        decl.output, lib_output_before,
        "library callee output signature should be unchanged by the pinned sub-pipeline"
    );

    // The post-sub-pipeline seed-rooted check accepts the rewritten pinned body
    // (single-exit, preserved arrow residue) without panicking.
    invariants::check_with_seeds(
        &store,
        user_pkg_id,
        InvariantLevel::PostSignaturePreserving,
        &[pinned],
    );
}

/// A foreign (library) callable that is not entry-reachable but is pinned (so it
/// survives DCE) and then seeded into the sub-pipeline. The seed lives in a
/// foreign package, so the seed-rooted passes must process it in that package
/// via the per-package assigner pool. Without the cross-package seeded path the
/// body keeps its early returns and the seed-rooted check rejects it.
#[test]
fn subpipeline_rewrites_non_entry_reachable_foreign_seed() {
    const LIB: &str = indoc! {"
        namespace Lib {
            import Std.Measurement.*;
            operation LibPinned(op : (Qubit => Unit), q : Qubit) : Int {
                op(q);
                let r = MResetZ(q);
                if r == One {
                    return 1;
                }
                return 2;
            }
            export LibPinned;
        }
    "};
    const USER: &str = indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Int { 42 }
        }
    "};

    let (mut store, user_pkg_id) = compile_to_fir_with_library(LIB, USER);
    let seed = find_callable_in_any_package(&store, "LibPinned");
    assert_ne!(
        seed.package, user_pkg_id,
        "the seed must live in a foreign (library) package"
    );

    // Pin the foreign callable through the main pipeline so item DCE keeps it.
    // It is not entry-reachable, so the main pipeline never return-unifies it.
    let result =
        run_pipeline_to_with_diagnostics(&mut store, user_pkg_id, PipelineStage::Full, &[seed]);
    assert!(
        result.is_success(),
        "main pipeline should succeed: {:?}",
        result.errors
    );
    assert!(
        callable_has_return(store.get(seed.package), seed.item),
        "non-entry-reachable foreign body should retain early returns after the main pipeline"
    );

    // The seed-rooted sub-pipeline must rewrite the foreign seed body in its own
    // package, removing the early returns.
    let result = run_signature_preserving_subpipeline(&mut store, user_pkg_id, &[seed]);
    assert!(
        result.is_success(),
        "sub-pipeline should succeed on a foreign seed: {:?}",
        result.errors
    );
    assert!(
        !callable_has_return(store.get(seed.package), seed.item),
        "sub-pipeline should remove all Return nodes from the foreign seed body"
    );

    // The seed-rooted check accepts the rewritten foreign body.
    invariants::check_with_seeds(
        &store,
        user_pkg_id,
        InvariantLevel::PostSignaturePreserving,
        &[seed],
    );
}
