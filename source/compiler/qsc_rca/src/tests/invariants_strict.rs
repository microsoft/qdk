// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Strict-invariant regression tests for the default-generator-set correction.
//!
//! Each test exercises a code shape that was a known source of
//! `actual==0 && expected>0` arity skew prior to the default-generator-set
//! correction and
//! relied on the old tolerance rule in
//! `invariants::check_entry`. With the tolerance removed and strict `==`
//! enforced both in `lib.rs` (`generate_application_compute_kind`) and
//! `invariants.rs` (`debug_assert!(actual == expected, ..)`), any future
//! regression that re-introduces arity-0 saves over spec-owned stmts/blocks
//! will panic here in debug builds.
//!
//! Each test:
//! 1. Runs the RCA pipeline to completion via `CompilationContext` (or
//!    `PipelineContext` when post-FIR-transform behavior is being covered).
//!    Both contexts call into `Analyzer::analyze_all` / `analyze_package`,
//!    which runs `assert_arity_consistency` under `#[cfg(debug_assertions)]`.
//! 2. Adds an explicit positive arity check on a representative callable so a
//!    silent regression (invariant disabled or weakened) still surfaces as a
//!    test failure.

use qsc_data_structures::target::Profile;

use super::{CompilationContext, PackageStoreSearch, PipelineContext};
use crate::{ComputePropertiesLookup, ItemComputeProperties};

/// Returns the `dynamic_param_applications` length recorded for the body spec
/// of the callable named `callable_name` in `context`.
fn body_arity(context: &CompilationContext, callable_name: &str) -> usize {
    let id = context
        .fir_store
        .find_callable_id_by_name(callable_name)
        .unwrap_or_else(|| panic!("callable {callable_name} should exist"));
    let ItemComputeProperties::Callable(props) = context.get_compute_properties().get_item(id)
    else {
        panic!("{callable_name} should be a callable item");
    };
    props.body.dynamic_param_applications.len()
}

/// Class 1 (arity 1): `@SimulatableIntrinsic` operation whose body stmts are
/// written via `set_all_stmts_in_block_to_default`. Under the old
/// `ApplicationGeneratorSet::default()` writes, every stmt in the body was
/// saved at arity 0; the debug invariant reported expected arity 1 and
/// tolerated the skew. The default-generator-set correction now saves
/// arity-matched generators directly.
#[test]
fn simulatable_intrinsic_arity_one_body_matches_input_params() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        @SimulatableIntrinsic()
        operation SimIntrinsic1(q : Qubit) : Unit {
            H(q);
            let x = 1;
            Message($"x = {x}");
        }"#,
    );
    assert_eq!(
        body_arity(&context, "SimIntrinsic1"),
        1,
        "SimulatableIntrinsic body arity must match input-pat arity",
    );
}

/// Class 1 (arity 2): same as above with a two-parameter input pat.
#[test]
fn simulatable_intrinsic_arity_two_body_matches_input_params() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        @SimulatableIntrinsic()
        operation SimIntrinsic2(q : Qubit, i : Int) : Unit {
            H(q);
            let y = i + 1;
            Message($"y = {y}");
        }"#,
    );
    assert_eq!(
        body_arity(&context, "SimIntrinsic2"),
        2,
        "SimulatableIntrinsic body arity must match input-pat arity",
    );
}

/// Class 1 (arity 3, mixed scalar/array): covers the `ParamApplication::Array`
/// construction path inside `default_application_generator_set_for_callable`.
#[test]
fn simulatable_intrinsic_arity_three_with_array_param_body_matches_input_params() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        @SimulatableIntrinsic()
        operation SimIntrinsic3(q : Qubit, i : Int, arr : Int[]) : Unit {
            H(q);
            let z = i + Length(arr);
            Message($"z = {z}");
        }"#,
    );
    assert_eq!(
        body_arity(&context, "SimIntrinsic3"),
        3,
        "SimulatableIntrinsic body arity must match input-pat arity",
    );
}

/// Class 2: `@Test` callable with a non-trivial measurement-driven body.
/// Previously the body stmts were saved at arity 0 by the top-level sweep
/// (`@Test` bodies are not entered by the main analyzer path). The body is
/// arity 0 because `@Test` callables take no parameters, but the regression
/// target here is that the invariant runs to completion on a `@Test` body
/// without triggering any intermediate skew on inner stmts/blocks.
#[test]
fn test_attribute_callable_body_reaches_strict_invariant() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        @Test()
        operation TestSample() : Int {
            use q = Qubit();
            mutable a = 0;
            if M(q) == Zero {
                set a = 1;
            }
            Message($"a = {a}");
            return a;
        }"#,
    );
    assert_eq!(
        body_arity(&context, "TestSample"),
        0,
        "@Test callable body arity must match the empty input pat",
    );
}

/// End-to-end fixture: a minimal reduction of `samples/algorithms/DeutschJozsa.qs`
/// exercising multiple callables, a dynamic measurement loop, and an array
/// parameter. This is Class 3 coverage — prior to the narrowing of
/// `unanalyzed_stmts`, the top-level sweep would overwrite spec-body stmts at
/// arity 0 for programs of this shape.
#[test]
fn deutsch_jozsa_shape_passes_strict_invariant() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        operation ConstantOracle(qs : Qubit[], target : Qubit) : Unit is Adj + Ctl {
            body ... { }
            adjoint self;
        }

        operation BalancedOracle(qs : Qubit[], target : Qubit) : Unit is Adj + Ctl {
            body ... {
                for q in qs {
                    CNOT(q, target);
                }
            }
        }

        operation DeutschJozsaMini(oracle : (Qubit[], Qubit) => Unit is Adj + Ctl, n : Int) : Bool {
            use qs = Qubit[n];
            use target = Qubit();
            X(target);
            H(target);
            for q in qs {
                H(q);
            }
            oracle(qs, target);
            for q in qs {
                H(q);
            }
            mutable isConstant = true;
            for q in qs {
                if M(q) == One {
                    set isConstant = false;
                }
            }
            Reset(target);
            ResetAll(qs);
            return isConstant;
        }

        operation MainMini() : Bool[] {
            [
                DeutschJozsaMini(ConstantOracle, 3),
                DeutschJozsaMini(BalancedOracle, 3)
            ]
        }"#,
    );
    assert_eq!(
        body_arity(&context, "DeutschJozsaMini"),
        2,
        "DeutschJozsaMini takes (oracle, n) — body arity must be 2",
    );
    assert_eq!(
        body_arity(&context, "MainMini"),
        0,
        "MainMini has no input parameters — body arity must be 0",
    );
    assert_eq!(
        body_arity(&context, "ConstantOracle"),
        2,
        "ConstantOracle takes (qs, target) — body arity must be 2",
    );
}

/// Mutual recursion: cyclic callables are analyzed by the dedicated
/// `cyclic_callables::Analyzer` pass, which pre-populates spec-body
/// generators at arity N. Historically the subsequent `TopLevelContext`
/// sweep could overwrite these at arity 0 when a cyclic spec-body stmt was
/// not tracked as "already analyzed". Phase 2's spec-owned-stmt filter
/// prevents the overwrite; this test guards against a regression.
#[test]
fn mutual_recursion_passes_strict_invariant() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        function Ping(n : Int) : Int {
            if n <= 0 {
                return 0;
            }
            return Pong(n - 1);
        }

        function Pong(n : Int) : Int {
            if n <= 0 {
                return 0;
            }
            return Ping(n - 1);
        }"#,
    );
    assert_eq!(
        body_arity(&context, "Ping"),
        1,
        "Ping body arity must match its single Int input parameter",
    );
    assert_eq!(
        body_arity(&context, "Pong"),
        1,
        "Pong body arity must match its single Int input parameter",
    );
}

/// Dynamic return via an early-exit inside a measurement-driven branch. This
/// exercises the `return_unify` FIR pass. Uses `PipelineContext` to force the
/// FIR transform pipeline (including GC) to run before RCA.
#[test]
fn dynamic_return_pipeline_passes_strict_invariant() {
    let source = r#"
        namespace Test {
            operation DynReturnStrict(qs : Qubit[]) : Result[] {
                mutable results = [Zero, size = Length(qs)];
                mutable i = 0;
                while i < Length(qs) {
                    if M(qs[i]) == One {
                        return results;
                    }
                    set i += 1;
                }
                results
            }
        }
    "#;
    let entry = "{ use qs = Qubit[2]; Test.DynReturnStrict(qs) }";
    let context = PipelineContext::new(source, entry, Profile::AdaptiveRIF.into());
    let dyn_return_id = context
        .fir_store
        .find_callable_id_by_name("DynReturnStrict")
        .expect("DynReturnStrict should exist after pipeline lowering");
    let ItemComputeProperties::Callable(props) =
        context.get_compute_properties().get_item(dyn_return_id)
    else {
        panic!("DynReturnStrict should be a callable item");
    };
    assert_eq!(
        props.body.dynamic_param_applications.len(),
        1,
        "DynReturnStrict body arity must match its single Qubit[] input parameter",
    );
}
