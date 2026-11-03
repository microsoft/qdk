// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Contract tests that validate `run_pipeline` output satisfies the `PostAll`
//! invariants expected by downstream consumers (codegen, language service, RCA).
//!
//! Each test compiles a representative Q# program, runs the full FIR transform
//! pipeline, and then calls [`invariants::check`] with [`InvariantLevel::PostAll`]
//! to assert that all structural postconditions hold.
//!
//! These tests are intentionally kept separate from the stage-parity tests in
//! `pipeline_integration.rs` so that contract regressions are easy to triage:
//! a failure here means a downstream consumer may receive malformed FIR.
//!
//! ## Compilation pattern
//!
//! Tests use `compile_to_fir` with `@EntryPoint()` in the source (the same
//! pattern as `pipeline_integration.rs`). This produces a package with a
//! concrete `entry` expression so that `invariants::check` runs the full
//! reachability-based checks rather than returning early.

use qsc_fir::fir::{ItemKind, PackageId, PackageStore};
use qsc_fir_transforms::{
    invariants,
    test_utils::{assert_full_pipeline_succeeds, assert_no_pipeline_errors, compile_to_fir},
};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Compiles `source` (which must contain `@EntryPoint()`) through the full FIR
/// transform pipeline and returns the store + package id.
///
/// Panics if the pipeline reports any errors.
fn compile_and_run_full_pipeline(
    source: &str,
) -> (qsc_fir::fir::PackageStore, qsc_fir::fir::PackageId) {
    let (mut store, pkg_id) = compile_to_fir(source);
    assert_full_pipeline_succeeds("pipeline_contracts::run_pipeline(Full)", &mut store, pkg_id);
    (store, pkg_id)
}

// ---------------------------------------------------------------------------
// PostAll invariant contract tests
// ---------------------------------------------------------------------------

/// Core contract test: verifies that `run_pipeline` output on a minimal entry
/// point satisfies the full `PostAll` invariant suite expected by downstream
/// consumers (codegen, language service, RCA).
///
/// Postconditions asserted by `InvariantLevel::PostAll`
/// (the full set actually exercised by the invariant runner, not just the
/// per-pass type bans):
/// - All ID references inside blocks/stmts/exprs/pats resolve to existing
///   arena entries on the target package (and on every reachable external
///   package, via the `PostUdtErase`+ package-closure walk).
/// - Synthesized callable-input tuple patterns match their callable-input
///   types (argument promotion shape contract).
/// - Local-variable bindings are consistent: every `LocalVarId` use has a
///   matching binding pattern of the same type in scope.
/// - Per-spec `SpecDecl` input/output types match their parent
///   `CallableDecl` signature.
/// - Every `ExprKind::Call` argument and return type matches the resolved
///   callee signature (with controlled-functor input wrappers applied),
///   per the post-arg-promote call-shape contract.
/// - `Package.entry_exec_graph` is structurally well-formed in both
///   `ExecGraphConfig::NoDebug` and `ExecGraphConfig::Debug` configurations,
///   and every reachable callable specialization's `exec_graph` is
///   structurally well-formed in both configurations.
/// - All earlier-stage type bans hold: no `Ty::Param`, no `ExprKind::Return`,
///   no `Ty::Arrow` params / `ExprKind::Closure`, no `Ty::Udt` /
///   `ExprKind::Struct`, no `Field::Path` in `UpdateField`/`AssignField`,
///   no `BinOp(Eq/Neq)` on tuple operands, and no `Ty::Infer` / `Ty::Err`
///   anywhere in checked types.
///
/// This is the authoritative contract test for simple entry-point invariant
/// verification; do not duplicate in other test files.
#[test]
fn run_pipeline_output_satisfies_post_all_invariants() {
    let (store, pkg_id) = compile_and_run_full_pipeline(
        r#"
        @EntryPoint()
        operation Main() : Int { 42 }
        "#,
    );

    // Panics with a descriptive message if any `PostAll` invariant is violated.
    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

/// Verifies that a single program exercising every major transform contract
/// at once -- monomorphization (generic `Identity`), UDT erasure (`newtype
/// Pair`), defunctionalization (callable-typed `Apply` argument), and
/// return-unification (`EarlyReturn` early `return`) -- still satisfies the
/// full `PostAll` invariant suite.
///
/// This intentionally combines what were previously four near-identical
/// single-feature contract tests (defunc / return-unify / UDT / mono) into one
/// representative anchor. The combined program forces all four transforms to
/// run in the same pipeline invocation, so a contract regression in any single
/// transform -- or in their interaction -- surfaces here. The authoritative
/// minimal anchor above (`run_pipeline_output_satisfies_post_all_invariants`)
/// remains as the simplest-possible entry-point contract.
#[test]
fn run_pipeline_combined_features_output_satisfies_post_all_invariants() {
    let (store, pkg_id) = compile_and_run_full_pipeline(
        r#"
        function Identity<'T>(x : 'T) : 'T { x }

        newtype Pair = (First : Int, Second : Int);

        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation EarlyReturn(flag : Bool) : Int {
            if flag { return 1; }
            0
        }

        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            Apply(H, q);
            Reset(q);
            let p = Pair(Identity(1), 2);
            p::First + EarlyReturn(true)
        }
        "#,
    );

    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

// ---------------------------------------------------------------------------
// FIR-stage optimization point
// ---------------------------------------------------------------------------

/// Names every callable item still present in `package_id`.
fn callable_names(store: &PackageStore, package_id: PackageId) -> Vec<String> {
    store
        .get(package_id)
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) => Some(decl.name.name.to_string()),
            ItemKind::Ty(..) => None,
        })
        .collect()
}

/// The optimization point runs exactly once, and the invariant level it
/// advertises is asserted where the point actually is rather than in prose.
#[test]
fn the_fir_stage_optimization_point_holds_post_arg_promote() {
    let (mut store, pkg_id) = compile_to_fir(
        r#"
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            H(q);
            H(q);
        }
        "#,
    );

    let mut times_reached = 0;
    let result = qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize(
        &mut store,
        pkg_id,
        |store, package_id| {
            times_reached += 1;
            invariants::check(
                store,
                package_id,
                invariants::InvariantLevel::PostArgPromote,
            );
        },
    );

    assert_no_pipeline_errors("run_pipeline_with_diagnostics_and_optimize", &result.errors);
    assert_eq!(times_reached, 1);
    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

/// The point sits before the backend stages, which is what lets a stage that
/// retires a call leave the resulting cleanup to the pipeline.
///
/// Item DCE, node GC, and the exec graph rebuild are the same `finalize_pipeline`
/// call in that order, so observing pre-DCE state at the point observes
/// pre-rebuild state too.
#[test]
fn the_fir_stage_optimization_point_runs_before_item_dce() {
    let (mut store, pkg_id) = compile_to_fir(
        r#"
        operation NeverCalled() : Unit {
            use q = Qubit();
            X(q);
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            H(q);
        }
        "#,
    );

    let mut seen_at_the_point = Vec::new();
    let result = qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize(
        &mut store,
        pkg_id,
        |store, package_id| seen_at_the_point = callable_names(store, package_id),
    );

    assert_no_pipeline_errors("run_pipeline_with_diagnostics_and_optimize", &result.errors);
    assert!(
        seen_at_the_point.iter().any(|name| name == "NeverCalled"),
        "the point should see the entry-unreachable callable, saw {seen_at_the_point:?}"
    );
    assert!(
        !callable_names(&store, pkg_id)
            .iter()
            .any(|name| name == "NeverCalled"),
        "item DCE should still remove it after the point"
    );
}
