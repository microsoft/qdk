// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Contract tests that validate `run_pipeline` output satisfies the PostAll
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

use qsc_fir_transforms::{
    invariants, run_pipeline,
    test_utils::{assert_no_pipeline_errors, compile_to_fir},
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
    let errors = run_pipeline(&mut store, pkg_id);
    assert_no_pipeline_errors("run_pipeline", &errors);
    (store, pkg_id)
}

// ---------------------------------------------------------------------------
// PostAll invariant contract tests
// ---------------------------------------------------------------------------

/// Core contract test: verifies that `run_pipeline` output on a minimal entry
/// point satisfies the full PostAll invariant suite expected by downstream
/// consumers (codegen, language service, RCA).
///
/// Postconditions asserted by `InvariantLevel::PostAll`:
/// - No `Ty::Param` in reachable code (monomorphization completed).
/// - No `ExprKind::Return` in reachable code (return unification completed).
/// - No `Ty::Arrow` params / `ExprKind::Closure` (defunctionalization completed).
/// - No `Ty::Udt` / `ExprKind::Struct` / `Field::Path` (UDT erasure completed).
/// - All exec-graph ranges populated (exec-graph rebuild completed).
#[test]
fn run_pipeline_output_satisfies_post_all_invariants() {
    let (store, pkg_id) = compile_and_run_full_pipeline(
        r#"
        @EntryPoint()
        operation Main() : Int { 42 }
        "#,
    );

    // Panics with a descriptive message if any PostAll invariant is violated.
    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

/// Verifies that a program using higher-order functions satisfies PostAll
/// invariants -- exercises the defunctionalization contract specifically.
#[test]
fn run_pipeline_defunctionalized_output_satisfies_post_all_invariants() {
    let (store, pkg_id) = compile_and_run_full_pipeline(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Reset(q);
        }
        "#,
    );

    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

/// Verifies that a program with early returns satisfies PostAll invariants --
/// exercises the return-unification contract specifically.
#[test]
fn run_pipeline_return_unified_output_satisfies_post_all_invariants() {
    let (store, pkg_id) = compile_and_run_full_pipeline(
        r#"
        operation EarlyReturn(flag : Bool) : Int {
            if flag { return 1; }
            0
        }

        @EntryPoint()
        operation Main() : Int {
            EarlyReturn(true)
        }
        "#,
    );

    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

/// Verifies that a program using user-defined types satisfies PostAll
/// invariants -- exercises the UDT erasure contract specifically.
#[test]
fn run_pipeline_udt_erased_output_satisfies_post_all_invariants() {
    let (store, pkg_id) = compile_and_run_full_pipeline(
        r#"
        newtype Pair = (First : Int, Second : Int);

        @EntryPoint()
        operation Main() : Int {
            let p = Pair(1, 2);
            p::First
        }
        "#,
    );

    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

/// Verifies that a program with generic functions satisfies PostAll invariants
/// -- exercises the monomorphization contract specifically.
#[test]
fn run_pipeline_monomorphized_output_satisfies_post_all_invariants() {
    let (store, pkg_id) = compile_and_run_full_pipeline(
        r#"
        function Identity<'T>(x : 'T) : 'T { x }

        @EntryPoint()
        operation Main() : Int { Identity(42) }
        "#,
    );

    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}
