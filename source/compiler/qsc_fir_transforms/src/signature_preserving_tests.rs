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
use crate::test_utils::{assert_panics_with, compile_to_fir};
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

fn callable_id_by_name(package: &Package, name: &str) -> LocalItemId {
    package
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == name => Some(item_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable {name} should exist"))
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
