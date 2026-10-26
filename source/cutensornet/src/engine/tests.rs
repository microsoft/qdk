//! Host tests of the cuTensorNet engine, on the CPU test double of the
//! native API: conformance with `tensornet::reference::ExactReference`, the
//! refusals that are this engine's own, the order of native calls, and that
//! every native object is released. Numerical behavior on a GPU is not
//! tested here.

use super::{
    CuTensorNetEngine, CuTensorNetState, EngineError, NativeApi,
    execution::{MpsExecutionApi, StateU32Configuration},
    policy::{ExecutionPolicy, MpsGauge, RESTART_PERIOD, SvdAlgorithm},
    profile::{EngineVariant, NativeHandle},
    resources::SessionApi,
    test_double::{NativeDouble, uniform_capacity},
};
use num_complex::Complex64;
use std::{num::NonZeroUsize, sync::Arc};
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm, SampleSeed, conformance};

/// The engine as it ships: it restarts each state every [`RESTART_PERIOD`]
/// computes on a new handle.
fn engine() -> (CuTensorNetEngine, Arc<NativeDouble>) {
    engine_with(ExecutionPolicy::base_qualification())
}

fn policy_without_restart() -> ExecutionPolicy {
    ExecutionPolicy::base_qualification()
        .with_restart_period(None)
        .with_fresh_handle_at_restart(false)
}

/// The engine with no restart: every state stays on the engine's handle.
fn engine_without_restart() -> (CuTensorNetEngine, Arc<NativeDouble>) {
    engine_with(policy_without_restart())
}

fn engine_without_capture() -> (CuTensorNetEngine, Arc<NativeDouble>) {
    engine_with(policy_without_restart().without_capture())
}

fn engine_with(policy: ExecutionPolicy) -> (CuTensorNetEngine, Arc<NativeDouble>) {
    let native = Arc::new(NativeDouble::new());
    let api: Arc<dyn NativeApi> = native.clone();
    let engine = CuTensorNetEngine::with_api(api, policy)
        .unwrap_or_else(|error| panic!("the test double opens a session: {error}"));
    (engine, native)
}

fn c(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

fn hadamard() -> Vec<Complex64> {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    vec![c(h), c(h), c(h), c(-h)]
}

fn cnot() -> Vec<Complex64> {
    let mut m = vec![c(0.0); 16];
    for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
        m[row * 4 + col] = c(1.0);
    }
    m
}

fn ok<T>(result: Result<T, EngineError>) -> T {
    result.unwrap_or_else(|error| panic!("the engine call succeeds: {error}"))
}

fn is_invalid_request<T: std::fmt::Debug>(result: &Result<T, EngineError>) -> bool {
    matches!(result, Err(EngineError::InvalidRequest { .. }))
}

#[test]
fn the_engine_conforms_to_the_exact_reference() {
    let (engine, native) = engine();
    let failures = conformance::check(&engine);
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

/// Except for the retained weight: without capture each compute truncates
/// the whole history again, after a projection whose probability was read
/// on an earlier truncation, so the projection changes the weight.
#[test]
fn the_engine_without_capture_conforms_to_the_exact_reference() {
    let (engine, native) = engine_without_capture();
    let (weight, failures): (Vec<_>, Vec<_>) = conformance::check(&engine)
        .into_iter()
        .partition(|failure| failure.case == "retained weight");
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(
        matches!(weight.as_slice(), [failure] if failure.message.starts_with("a projection changed")),
        "{weight:#?}"
    );
    assert!(native.count("compute_state") > 0);
    assert_eq!(native.count("capture_mps"), 0);
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

/// The calls that compute, capture, apply, and free, in order.
fn compute_events(native: &NativeDouble) -> Vec<&'static str> {
    native
        .events()
        .into_iter()
        .filter(|event| {
            [
                "apply_tensor_operator",
                "compute_state",
                "capture_mps",
                "free",
            ]
            .contains(event)
        })
        .collect()
}

#[test]
fn each_compute_is_captured_before_the_previous_one_is_freed() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    let first = compute_events(&native);
    assert_eq!(
        first.iter().rev().find(|event| **event != "free"),
        Some(&"capture_mps"),
        "{first:?}"
    );
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    let p1 = ok(engine.probability_of_one(&state, 1));
    assert!((p1 - 0.5).abs() < 1e-12, "{p1}");
    let events = compute_events(&native);
    let second = &events[first.len()..];
    let compute = second
        .iter()
        .position(|event| *event == "compute_state")
        .unwrap_or_else(|| panic!("{second:?}"));
    // The compute's own scratch is freed first, then the compute is
    // captured, and only then are the previous outputs freed.
    assert_eq!(
        second[compute + 1..compute + 3],
        ["free", "capture_mps"],
        "{second:?}"
    );
    assert!(
        !second[..compute].contains(&"free"),
        "the captured start is freed before it is read: {second:?}"
    );
    assert!(second[compute + 3..].contains(&"free"), "{second:?}");
    assert_eq!(native.count("compute_state"), 2);
    assert_eq!(native.count("capture_mps"), 2);
}

#[test]
fn without_capture_every_compute_starts_from_the_first_operator() {
    let (engine, native) = engine_without_capture();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    let p1 = ok(engine.probability_of_one(&state, 1));
    assert!((p1 - 0.5).abs() < 1e-12, "{p1}");
    assert_eq!(native.count("compute_state"), 2);
    assert_eq!(native.count("capture_mps"), 0);
}

#[test]
fn a_failed_capture_poisons_the_state_and_cleans_up() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    native.fail("capture_mps", 2);
    assert!(matches!(
        engine.probability_of_one(&state, 1),
        Err(EngineError::NativeCallFailed {
            operation: "capture_mps",
            ..
        })
    ));
    assert!(matches!(
        engine.probability_of_one(&state, 0),
        Err(EngineError::Poisoned { .. })
    ));
    drop(state);
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

#[test]
fn a_fork_after_a_capture_replays_every_operator() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.project(&mut state, 0, true, 0.5));
    ok(engine.probability_of_one(&state, 1));
    assert_eq!(native.count("capture_mps"), 2);
    let copy = ok(engine.fork(&state));
    assert_eq!(native.count("apply_tensor_operator"), 6);
    assert!((ok(engine.probability_of_one(&copy, 1)) - 1.0).abs() < 1e-12);
    ok(copy.close());
    ok(engine.apply(&mut state, &[1], &hadamard()));
    assert!((ok(engine.probability_of_one(&state, 1)) - 0.5).abs() < 1e-12);
}

fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![c(k), c(-s), c(s), c(k)]
}

#[test]
fn reads_divide_by_the_norm_the_library_reports() {
    let (engine, _native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 1)));
    // cos(0.35)|00⟩ + sin(0.35)|11⟩, truncated to |00⟩ with norm cos²(0.35).
    ok(engine.apply(&mut state, &[0], &ry(0.7)));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    let kept = 0.35f64.cos().powi(2);
    let weight = ok(engine.retained_weight(&state));
    assert!((weight - kept).abs() < 1e-12, "{weight}, not {kept}");
    ok(engine.apply(&mut state, &[0], &hadamard()));
    let p1 = ok(engine.probability_of_one(&state, 0));
    assert!((p1 - 0.5).abs() < 1e-12, "{p1}");
    let x0 = PauliTerm::new(c(1.0), vec![(0, Pauli::X)])
        .and_then(|term| PauliSum::new(c(0.0), vec![term]))
        .unwrap_or_else(|e| panic!("{e}"));
    let value = ok(engine.expectation(&state, &x0));
    assert!((value - c(1.0)).norm() < 1e-12, "{value}");
    let weight = ok(engine.retained_weight(&state));
    assert!((weight - kept).abs() < 1e-12, "a unitary keeps the weight");
}

#[test]
fn the_retained_weight_falls_under_a_small_bond_and_projections_keep_it() {
    let (engine, _native) = engine();
    let sites = 6;
    let mut state = ok(engine.prepare(&uniform_capacity(sites, 2)));
    let mut weights = vec![ok(engine.retained_weight(&state))];
    for layer in 0..8 {
        for site in 0..sites {
            let angle = 1.1 + 0.37 * f64::from(u8::try_from(site + layer).unwrap_or(0));
            ok(engine.apply(&mut state, &[site], &ry(angle)));
        }
        for site in (layer % 2..sites - 1).step_by(2) {
            ok(engine.apply(&mut state, &[site, site + 1], &cnot()));
        }
        let weight = ok(engine.retained_weight(&state));
        // An edge site, so that the projection leaves the rest entangled.
        let edge = if layer % 2 == 0 { 0 } else { sites - 1 };
        let p1 = ok(engine.probability_of_one(&state, edge));
        let (one, p) = if p1 >= 0.5 {
            (true, p1)
        } else {
            (false, 1.0 - p1)
        };
        ok(engine.project(&mut state, edge, one, p));
        let projected = ok(engine.retained_weight(&state));
        assert!(
            (projected - weight).abs() < 1e-12,
            "a projection changed the weight from {weight} to {projected}"
        );
        weights.push(projected);
    }
    assert!((weights[0] - 1.0).abs() < 1e-12, "{weights:?}");
    assert!(
        weights.windows(2).all(|pair| pair[1] <= pair[0] + 1e-12),
        "the weight rose: {weights:?}"
    );
    let last = weights.last().copied().unwrap_or(1.0);
    assert!(last > 0.0 && last < 0.9, "the weight fell: {weights:?}");
    let shape = ok(engine.shape(&state));
    assert!(shape.fits_within(&uniform_capacity(sites, 2)), "{shape:?}");
}

#[test]
fn operators_beyond_two_sites_and_capacities_of_qudits_are_refused() {
    let (engine, _native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 4)));
    let identity: Vec<Complex64> = (0..64)
        .map(|i| c(if i % 9 == 0 { 1.0 } else { 0.0 }))
        .collect();
    assert!(is_invalid_request(&engine.apply(
        &mut state,
        &[0, 1, 2],
        &identity
    )));
    let nan = vec![c(f64::NAN); 4];
    assert!(is_invalid_request(&engine.apply(&mut state, &[0], &nan)));
    let qutrits = MpsShape::new(vec![vec![3, 1], vec![1, 3]]).unwrap_or_else(|e| panic!("{e}"));
    assert!(is_invalid_request(&engine.prepare(&qutrits)));
    // The refusals left the state usable.
    ok(engine.apply(&mut state, &[0], &hadamard()));
    assert!((ok(engine.probability_of_one(&state, 0)) - 0.5).abs() < 1e-12);
}

#[test]
fn a_condition_of_probability_zero_and_a_bad_projection_are_refused() {
    let (engine, _native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    assert!(is_invalid_request(&engine.conditional_probability_of_one(
        &state,
        0,
        &[(1, true)]
    )));
    for probability in [0.0, -0.5, 1.5, f64::NAN] {
        assert!(is_invalid_request(&engine.project(
            &mut state,
            0,
            false,
            probability
        )));
    }
    assert!((ok(engine.probability_of_one(&state, 1))).abs() < 1e-12);
}

#[test]
fn the_unitary_flag_is_set_for_unitaries_and_projections_are_not() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[1, 0], &cnot()));
    ok(engine.apply(&mut state, &[1], &[c(2.0), c(0.0), c(0.0), c(1.0)]));
    ok(engine.project(&mut state, 0, true, 0.5));
    assert_eq!(
        native.applied(),
        [
            (vec![0], true),
            (vec![1, 0], true),
            (vec![1], false),
            (vec![0], false)
        ]
    );
}

#[test]
fn the_state_is_finalized_once_and_computed_again_only_after_a_change() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 4)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    ok(engine.probability_of_one(&state, 1));
    ok(engine.shape(&state));
    assert_eq!(native.count("finalize_mps"), 1);
    assert_eq!(native.count("compute_state"), 1);
    assert_eq!(native.count("configure_state_f64"), 2);
    assert_eq!(native.count("configure_state_u32"), 3);
    let lifecycle: Vec<&str> = native
        .events()
        .into_iter()
        .filter(|event| {
            [
                "apply_tensor_operator",
                "finalize_mps",
                "configure_state_f64",
                "configure_state_u32",
                "prepare_state",
                "set_workspace",
                "compute_state",
            ]
            .contains(event)
        })
        .take(10)
        .collect();
    assert_eq!(
        lifecycle,
        [
            "apply_tensor_operator",
            "finalize_mps",
            "configure_state_f64",
            "configure_state_f64",
            "configure_state_u32",
            "configure_state_u32",
            "configure_state_u32",
            "prepare_state",
            "set_workspace",
            "compute_state"
        ]
    );
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.probability_of_one(&state, 1));
    ok(engine.project(&mut state, 1, true, 0.5));
    let given = ok(engine.conditional_probability_of_one(&state, 0, &[(1, true)]));
    assert!((given - 1.0).abs() < 1e-12, "{given}");
    assert_eq!(native.count("finalize_mps"), 1);
    assert_eq!(native.count("compute_state"), 3);
    // A Query releases its descriptors and buffers before it returns.
    assert_eq!(
        native.count("create_expectation"),
        native.count("destroy_expectation")
    );
    assert_eq!(
        native.count("create_network_operator"),
        native.count("destroy_network_operator")
    );
}

#[test]
fn a_fork_replays_the_operators_on_a_new_state() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.project(&mut state, 0, true, 0.5));
    let copy = ok(engine.fork(&state));
    assert_eq!(native.count("create_state"), 2);
    assert_eq!(native.count("apply_tensor_operator"), 6);
    assert!((ok(engine.probability_of_one(&copy, 1)) - 1.0).abs() < 1e-12);
    ok(copy.close());
    assert!((ok(engine.probability_of_one(&state, 1)) - 1.0).abs() < 1e-12);
}

#[test]
fn sampling_draws_through_the_native_sampler_and_releases_it() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    let seed = SampleSeed::try_from(3).unwrap_or_else(|e| panic!("{e}"));
    let values = ok(engine.sample(&state, &[1, 0], 50, seed));
    assert_eq!(values.len(), 100);
    assert!(values.chunks_exact(2).all(|shot| shot[0] == shot[1]));
    assert_eq!(native.count("create_sampler"), 1);
    assert_eq!(native.count("destroy_sampler"), 1);
    assert!(ok(engine.sample(&state, &[0], 0, seed)).is_empty());
    assert_eq!(native.count("create_sampler"), 1, "no sampler for no shots");
}

#[test]
fn a_native_failure_poisons_the_state_and_cleans_up() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    native.fail("compute_state", 1);
    assert!(matches!(
        engine.probability_of_one(&state, 0),
        Err(EngineError::NativeCallFailed {
            operation: "compute_state",
            ..
        })
    ));
    assert!(matches!(
        engine.probability_of_one(&state, 0),
        Err(EngineError::Poisoned { .. })
    ));
    assert!(matches!(
        engine.apply(&mut state, &[1], &hadamard()),
        Err(EngineError::Poisoned { .. })
    ));
    drop(state);
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

#[test]
fn a_failed_query_releases_its_objects_and_leaves_the_state_usable() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.shape(&state));
    let live = native.live_objects();
    native.fail("compute_expectation", 1);
    assert!(engine.probability_of_one(&state, 0).is_err());
    assert_eq!(native.live_objects(), live);
    assert!((ok(engine.probability_of_one(&state, 0)) - 0.5).abs() < 1e-12);
}

#[test]
fn a_state_of_another_engine_is_refused() {
    let (first, _) = engine();
    let (second, _) = engine();
    let state = ok(first.prepare(&uniform_capacity(2, 2)));
    assert!(is_invalid_request(&second.probability_of_one(&state, 0)));
    assert!(ok(first.probability_of_one(&state, 0)).abs() < 1e-12);
}

#[test]
fn closing_releases_every_native_object() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    let copy = ok(engine.fork(&state));
    ok(engine.shape(&copy));
    let refused = engine.close();
    assert!(is_invalid_request(&refused), "two states are open");
    // The engine's stream and handle outlive it while a state needs them.
    assert!(native.live_objects() > 2);
    ok(state.close());
    drop(copy);
    assert_eq!(native.live_objects(), 0);
    assert_eq!(native.count("destroy_handle"), 1);
    assert_eq!(native.count("destroy_stream"), 1);
}

#[test]
fn a_bond_cap_below_the_exact_chain_is_the_realized_bound() {
    let (engine, _native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(4, 1)));
    ok(engine.apply(&mut state, &[1], &hadamard()));
    ok(engine.apply(&mut state, &[1, 2], &cnot()));
    let shape = ok(engine.shape(&state));
    assert!(shape.fits_within(&uniform_capacity(4, 1)), "{shape:?}");
}

#[test]
fn repeated_reads_and_computes_do_not_grow_device_memory() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    let seed = SampleSeed::try_from(5).unwrap_or_else(|e| panic!("{e}"));
    ok(engine.sample(&state, &[0, 1], 4, seed));
    let live = native.live_objects();
    for _ in 0..3 {
        ok(engine.sample(&state, &[0, 1], 4, seed));
        ok(engine.probability_of_one(&state, 0));
        ok(engine.conditional_probability_of_one(&state, 1, &[(0, true)]));
    }
    assert_eq!(native.live_objects(), live, "reads free what they allocate");
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.probability_of_one(&state, 1));
    let recomputed = native.live_objects();
    for _ in 0..3 {
        ok(engine.apply(&mut state, &[1], &hadamard()));
        ok(engine.probability_of_one(&state, 1));
    }
    assert_eq!(native.count("compute_state"), 5);
    // Each operator keeps its tensor; the last compute's buffers replace the
    // previous ones.
    assert_eq!(native.live_objects(), recomputed + 3);
}

#[test]
fn a_compute_frees_its_scratch_once_the_stream_has_finished_it() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    for site in 0..3 {
        ok(engine.apply(&mut state, &[site], &hadamard()));
        ok(engine.probability_of_one(&state, site));
        assert_eq!(native.live_scratch(), 0, "no scratch outlives its call");
    }
    let computes = native.computes();
    assert_eq!(computes.len(), 3);
    assert!(
        computes.iter().all(|&(live, _)| live == 1),
        "a compute holds only its own scratch: {computes:?}"
    );
}

#[test]
fn a_compute_prepares_within_the_budget_and_the_free_memory() {
    let (engine, native) = engine();
    let budget = ExecutionPolicy::base_qualification().maximum_workspace_bytes;
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    native.set_free_memory(1 << 20);
    ok(engine.apply(&mut state, &[1], &hadamard()));
    ok(engine.probability_of_one(&state, 1));
    assert_eq!(native.prepare_limits(), [budget, 1 << 20]);
}

/// Ported from ④'s `workspace_recommendation_above_policy_fails_before_scratch_allocation`
/// and `workspace_recommendation_above_free_memory_fails_before_scratch_allocation`.
#[test]
fn a_workspace_above_the_budget_or_the_free_memory_fails_before_any_scratch() {
    let mut tight = ExecutionPolicy::base_qualification();
    tight.maximum_workspace_bytes = 1000;
    for (policy, free) in [
        (tight, None),
        (ExecutionPolicy::base_qualification(), Some(1000)),
    ] {
        let (engine, native) = engine_with(policy);
        if let Some(bytes) = free {
            native.set_free_memory(bytes);
        }
        let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
        ok(engine.apply(&mut state, &[0], &hadamard()));
        let error = engine.probability_of_one(&state, 0).err();
        assert!(
            matches!(
                error,
                Some(EngineError::WorkspaceLimitExceeded {
                    required: 1024,
                    maximum: 1000
                })
            ),
            "{error:?}"
        );
        assert_eq!(native.count("set_workspace"), 0);
        assert_eq!(native.live_scratch(), 0);
    }
}

#[test]
fn the_minimum_workspace_policy_gives_each_compute_the_minimum() {
    for (policy, bytes) in [
        (ExecutionPolicy::base_qualification(), 1024),
        (
            ExecutionPolicy::base_qualification().with_minimum_compute_workspace(),
            512,
        ),
    ] {
        let (engine, native) = engine_with(policy);
        let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
        ok(engine.apply(&mut state, &[0], &hadamard()));
        ok(engine.probability_of_one(&state, 0));
        assert_eq!(native.computes(), [(1, bytes)]);
        let profile = ok(engine.profile(&state));
        assert_eq!(
            (
                profile.minimum_workspace_bytes,
                profile.recommended_workspace_bytes
            ),
            (512, 1024)
        );
        assert_eq!(profile.computes, 1);
    }
}

/// Restarting every `period` computes on the handle the state is on.
fn restarting(period: usize) -> (CuTensorNetEngine, Arc<NativeDouble>) {
    engine_with(
        ExecutionPolicy::base_qualification()
            .with_restart_period(NonZeroUsize::new(period))
            .with_fresh_handle_at_restart(false),
    )
}

/// Restarting every `period` computes, each time on a new handle.
fn restarting_on_fresh_handles(period: usize) -> (CuTensorNetEngine, Arc<NativeDouble>) {
    engine_with(
        ExecutionPolicy::base_qualification()
            .with_restart_period(NonZeroUsize::new(period))
            .with_fresh_handle_at_restart(true),
    )
}

#[test]
fn the_engine_restarting_its_state_conforms_to_the_exact_reference() {
    for period in [1, 2, 3] {
        for fresh in [false, true] {
            let (engine, native) = if fresh {
                restarting_on_fresh_handles(period)
            } else {
                restarting(period)
            };
            let failures = conformance::check(&engine);
            assert!(
                failures.is_empty(),
                "every {period}, {fresh}: {failures:#?}"
            );
            assert!(native.count("initialize_mps") > 0, "every {period}");
            assert_eq!(native.count("create_handle") > 1, fresh, "every {period}");
            assert_eq!(native.live_objects(), 2, "only the stream and the handle");
            ok(engine.close());
            assert_eq!(native.live_objects(), 0);
        }
    }
}

/// Everything a long truncated walk reads, step by step: probabilities, the
/// retained weight, the shape, samples and a fork's probability.
fn walk(engine: &CuTensorNetEngine) -> Vec<f64> {
    let sites = 5;
    let mut state = ok(engine.prepare(&uniform_capacity(sites, 2)));
    let seed = SampleSeed::try_from(11).unwrap_or_else(|e| panic!("{e}"));
    let mut read = Vec::new();
    for layer in 0..10 {
        for site in 0..sites {
            let angle = 0.9 + 0.41 * f64::from(u8::try_from(site + 2 * layer).unwrap_or(0));
            ok(engine.apply(&mut state, &[site], &ry(angle)));
        }
        for site in (layer % 2..sites - 1).step_by(2) {
            ok(engine.apply(&mut state, &[site, site + 1], &cnot()));
        }
        let edge = if layer % 2 == 0 { 0 } else { sites - 1 };
        let p1 = ok(engine.probability_of_one(&state, edge));
        read.push(p1);
        read.push(ok(engine.retained_weight(&state)));
        read.push(ok(engine.conditional_probability_of_one(
            &state,
            2,
            &[(edge, p1 >= 0.5)],
        )));
        let shape = ok(engine.shape(&state));
        read.extend(
            (0..sites - 1)
                .map(|cut| f64::from(u32::try_from(shape.bond_dim(cut).unwrap_or(0)).unwrap_or(0))),
        );
        let samples = ok(engine.sample(&state, &[0, 2, 4], 3, seed));
        read.extend(samples.into_iter().map(f64::from));
        let (one, p) = if p1 >= 0.5 {
            (true, p1)
        } else {
            (false, 1.0 - p1)
        };
        ok(engine.project(&mut state, edge, one, p));
    }
    let copy = ok(engine.fork(&state));
    read.push(ok(engine.probability_of_one(&copy, 2)));
    ok(copy.close());
    ok(state.close());
    read
}

#[test]
fn a_restart_changes_no_value_through_truncations_and_projections() {
    let (captured, _native) = engine_without_restart();
    let expected = walk(&captured);
    for (period, fresh) in [1, 2, 3, 50]
        .into_iter()
        .flat_map(|p| [(p, false), (p, true)])
    {
        let (engine, native) = if fresh {
            restarting_on_fresh_handles(period)
        } else {
            restarting(period)
        };
        let read = walk(&engine);
        assert_eq!(read.len(), expected.len());
        for (at, (value, wanted)) in read.iter().zip(&expected).enumerate() {
            assert!(
                (value - wanted).abs() < 1e-12,
                "every {period}, {fresh}, read {at}: {value}, not {wanted}"
            );
        }
        let restarts = native.count("initialize_mps");
        assert_eq!(restarts > 0, period < 50, "every {period}: {restarts}");
        assert_eq!(
            native.count("create_handle") - 1,
            if fresh { restarts } else { 0 },
            "every {period}, {fresh}"
        );
        assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    }
}

#[test]
fn a_restart_releases_the_old_state_and_its_operator_tensors() {
    let (engine, native) = restarting(1);
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.probability_of_one(&state, 1));
    // Two operator tensors, then one after each restart.
    let live = native.live_objects() - 1;
    for step in 0..5 {
        ok(engine.apply(&mut state, &[step % 3], &hadamard()));
        ok(engine.probability_of_one(&state, step % 3));
        assert_eq!(native.live_objects(), live, "step {step}");
        let profile = ok(engine.profile(&state));
        assert_eq!(profile.operator_tensors, 1, "only the new state's");
        assert_eq!(profile.restarts, step + 1);
    }
    assert_eq!(native.count("create_state"), 6);
    assert_eq!(native.count("destroy_state"), 5);
    assert_eq!(native.count("finalize_mps"), 6);
    // Each restart reads the last compute, and only after it is the
    // previous MPS freed: the new state's first compute reads it.
    let events: Vec<_> = native
        .events()
        .into_iter()
        .filter(|event| ["initialize_mps", "destroy_state", "compute_state"].contains(event))
        .collect();
    assert_eq!(
        events[..4],
        [
            "compute_state",
            "initialize_mps",
            "destroy_state",
            "compute_state"
        ],
        "{events:?}"
    );
    ok(state.close());
    assert_eq!(native.live_objects(), 2);
}

#[test]
fn a_failed_restart_poisons_the_state_and_cleans_up() {
    let (engine, native) = restarting(1);
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    native.fail("initialize_mps", 1);
    assert!(matches!(
        engine.apply(&mut state, &[0, 1], &cnot()),
        Err(EngineError::NativeCallFailed {
            operation: "initialize_mps",
            ..
        })
    ));
    assert!(matches!(
        engine.probability_of_one(&state, 0),
        Err(EngineError::Poisoned { .. })
    ));
    assert_eq!(
        native.count("destroy_state"),
        1,
        "the new state is released"
    );
    drop(state);
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

#[test]
fn a_restart_without_capture_is_refused() {
    let native = Arc::new(NativeDouble::new());
    let api: Arc<dyn NativeApi> = native.clone();
    let policy = ExecutionPolicy::base_qualification().without_capture();
    assert!(matches!(
        CuTensorNetEngine::with_api(api, policy),
        Err(EngineError::InvalidExecutionPolicy { .. })
    ));
    assert_eq!(native.live_objects(), 0);
}

/// The native calls of a restart on a fresh handle.
fn handle_events(native: &NativeDouble) -> Vec<&'static str> {
    native
        .events()
        .into_iter()
        .filter(|event| {
            [
                "create_handle",
                "destroy_handle",
                "create_state",
                "initialize_mps",
                "destroy_state",
                "create_workspace",
                "destroy_workspace",
            ]
            .contains(event)
        })
        .collect()
}

#[test]
fn a_fresh_handle_restart_moves_the_state_and_destroys_the_old_handle_last() {
    let (engine, native) = restarting_on_fresh_handles(1);
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.probability_of_one(&state, 1));
    assert_eq!(native.live_handles(), 1, "the session's");
    let live = native.live_objects();
    for step in 0..4 {
        let before = handle_events(&native).len();
        ok(engine.apply(&mut state, &[step % 3], &hadamard()));
        let restart: Vec<_> = handle_events(&native)[before..].to_vec();
        let mut expected = vec![
            "create_handle",
            "create_state",
            "initialize_mps",
            "create_workspace",
            "destroy_state",
            "destroy_workspace",
        ];
        if step > 0 {
            expected.push("destroy_handle");
        }
        assert_eq!(restart, expected, "step {step}");
        ok(engine.probability_of_one(&state, step % 3));
        assert_eq!(native.live_handles(), 2, "the session's and the state's");
        // The old handle is replaced, not kept: against the first state,
        // one handle more and one operator tensor fewer.
        assert_eq!(native.live_objects(), live, "step {step}");
        let profile = ok(engine.profile(&state));
        assert_eq!(
            (
                profile.restarts,
                profile.fresh_handles,
                profile.handle_computes
            ),
            (step + 1, step + 1, 1),
            "the new handle has done one compute"
        );
        assert_eq!(profile.computes, step + 2);
    }
    ok(state.close());
    assert_eq!(native.live_handles(), 1, "the state's handle is destroyed");
    assert_eq!(
        handle_events(&native).last(),
        Some(&"destroy_handle"),
        "after the state and its workspace"
    );
    assert_eq!(native.count("destroy_handle"), 4);
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

#[test]
fn the_profile_counts_the_computes_of_the_handle_the_state_is_on() {
    let (engine, _native) = engine();
    let mut first = ok(engine.prepare(&uniform_capacity(2, 2)));
    let mut second = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut first, &[0], &hadamard()));
    ok(engine.probability_of_one(&first, 0));
    ok(engine.apply(&mut second, &[0], &hadamard()));
    ok(engine.probability_of_one(&second, 0));
    ok(engine.apply(&mut second, &[1], &hadamard()));
    ok(engine.probability_of_one(&second, 1));
    let profile = ok(engine.profile(&first));
    assert_eq!((profile.computes, profile.handle_computes), (1, 3));
    assert_eq!(profile.fresh_handles, 0);
    ok(first.close());
    ok(second.close());
}

#[test]
fn forks_made_before_and_after_a_fresh_handle_restart_stay_usable() {
    let reads = |engine: &CuTensorNetEngine| -> Vec<f64> {
        let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
        ok(engine.apply(&mut state, &[0], &ry(0.7)));
        ok(engine.probability_of_one(&state, 0));
        let mut before = ok(engine.fork(&state));
        ok(engine.apply(&mut state, &[0, 1], &cnot()));
        ok(engine.probability_of_one(&state, 1));
        let mut after = ok(engine.fork(&state));
        ok(engine.apply(&mut state, &[1, 2], &cnot()));
        ok(engine.apply(&mut before, &[2], &ry(1.1)));
        ok(engine.apply(&mut after, &[2], &ry(0.3)));
        let seed = SampleSeed::try_from(5).unwrap_or_else(|e| panic!("{e}"));
        let mut read = vec![
            ok(engine.probability_of_one(&state, 2)),
            ok(engine.probability_of_one(&before, 2)),
            ok(engine.probability_of_one(&after, 2)),
        ];
        read.extend(
            ok(engine.sample(&state, &[0, 1, 2], 4, seed))
                .into_iter()
                .map(f64::from),
        );
        for state in [state, before, after] {
            ok(state.close());
        }
        read
    };
    let (plain, _plain_native) = engine_without_restart();
    let expected = reads(&plain);
    let (engine, native) = restarting_on_fresh_handles(1);
    let read = reads(&engine);
    assert!(native.count("create_handle") > 2, "the state restarted");
    for (value, wanted) in read.iter().zip(&expected) {
        assert!((value - wanted).abs() < 1e-12, "{read:?} != {expected:?}");
    }
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
}

#[test]
fn a_failed_fresh_handle_restart_poisons_the_state_and_releases_what_it_created() {
    for operation in [
        "create_handle",
        "create_state",
        "initialize_mps",
        "configure_state_u32",
        "create_workspace",
    ] {
        let (engine, native) = restarting_on_fresh_handles(1);
        let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
        ok(engine.apply(&mut state, &[0], &hadamard()));
        ok(engine.probability_of_one(&state, 0));
        native.fail(operation, native.count(operation) + 1);
        let failed = engine.apply(&mut state, &[0, 1], &cnot());
        assert!(
            matches!(
                &failed,
                Err(EngineError::NativeCallFailed { operation: at, .. }) if *at == operation
            ),
            "{operation}: {failed:?}"
        );
        assert!(
            matches!(
                engine.probability_of_one(&state, 0),
                Err(EngineError::Poisoned { .. })
            ),
            "{operation}"
        );
        assert_eq!(
            native.live_handles(),
            1,
            "{operation}: the new handle is released"
        );
        drop(state);
        assert_eq!(native.live_objects(), 2, "{operation}");
        ok(engine.close());
        assert_eq!(native.live_objects(), 0, "{operation}");
    }
}

/// One step of a long shot: a gate, then a read, which computes.
fn gate_and_read(engine: &CuTensorNetEngine, state: &mut CuTensorNetState, step: usize) -> f64 {
    let site = step % 3;
    let angle = 0.3 + 0.17 * f64::from(u8::try_from(step % 23).unwrap_or(0));
    ok(engine.apply(state, &[site], &ry(angle)));
    if step % 2 == 1 {
        ok(engine.apply(state, &[site.min(1), site.min(1) + 1], &cnot()));
    }
    ok(engine.probability_of_one(state, site))
}

#[test]
fn the_engine_restarts_each_state_on_a_fresh_handle_every_ten_computes() {
    assert_eq!(RESTART_PERIOD, 10);
    let (engine, native) = engine();
    let (reference, _reference_native) = engine_without_restart();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    let mut expected = ok(reference.prepare(&uniform_capacity(3, 2)));
    let mut step = 0;
    let mut advance = |state: &mut CuTensorNetState, expected: &mut CuTensorNetState| {
        let read = gate_and_read(&engine, state, step);
        let wanted = gate_and_read(&reference, expected, step);
        assert!(
            (read - wanted).abs() < 1e-12,
            "step {step}: {read}, not {wanted}"
        );
        step += 1;
    };
    for _ in 0..RESTART_PERIOD {
        advance(&mut state, &mut expected);
    }
    let profile = ok(engine.profile(&state));
    assert_eq!(
        (profile.computes, profile.restarts, profile.fresh_handles),
        (RESTART_PERIOD, 0, 0),
        "no restart before the period is reached"
    );
    assert_eq!(
        profile.handle_computes, RESTART_PERIOD,
        "the engine's handle"
    );
    assert_eq!(native.live_handles(), 1, "the engine's");
    for period in 1..=3 {
        for _ in 0..RESTART_PERIOD {
            advance(&mut state, &mut expected);
        }
        let profile = ok(engine.profile(&state));
        assert_eq!(
            (
                profile.restarts,
                profile.fresh_handles,
                profile.handle_computes
            ),
            (period, period, RESTART_PERIOD),
            "the state restarts before the first operator after every period"
        );
        assert_eq!(native.live_handles(), 2, "the engine's and the state's");
        assert_eq!(
            native.count("destroy_handle"),
            period - 1,
            "each previous own handle"
        );
    }
    ok(state.close());
    ok(expected.close());
    assert_eq!(native.live_handles(), 1, "the state's handle is destroyed");
    assert_eq!(native.count("destroy_handle"), 3);
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

#[test]
fn forks_stay_usable_across_their_parent_restarts() {
    let run = |engine: &CuTensorNetEngine| -> Vec<f64> {
        let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
        let mut forks = Vec::new();
        let mut read = Vec::new();
        for step in 0..3 * RESTART_PERIOD + 2 {
            read.push(gate_and_read(engine, &mut state, step));
            if step % 7 == 3 {
                forks.push(ok(engine.fork(&state)));
            }
        }
        for (at, fork) in forks.iter_mut().enumerate() {
            for step in 0..=RESTART_PERIOD {
                read.push(gate_and_read(engine, fork, step + at));
            }
        }
        read.push(gate_and_read(engine, &mut state, 0));
        for fork in forks {
            ok(fork.close());
        }
        ok(state.close());
        read
    };
    let (reference, _reference_native) = engine_without_restart();
    let expected = run(&reference);
    let (engine, native) = engine();
    let read = run(&engine);
    assert!(
        native.count("create_handle") > 4,
        "the parent restarted three times, and the forks"
    );
    assert_eq!(read.len(), expected.len());
    for (at, (value, wanted)) in read.iter().zip(&expected).enumerate() {
        assert!(
            (value - wanted).abs() < 1e-12,
            "read {at}: {value}, not {wanted}"
        );
    }
    assert_eq!(native.live_objects(), 2, "only the stream and the handle");
    ok(engine.close());
    assert_eq!(native.live_objects(), 0);
}

#[test]
fn the_default_engine_variant_is_the_engine() {
    let policy = ExecutionPolicy::base_qualification();
    assert_eq!(policy.with_variant(EngineVariant::default()), policy);
    let no_restart = EngineVariant {
        restart_period: None,
        fresh_handle_at_restart: false,
        ..EngineVariant::default()
    };
    assert_eq!(policy.with_variant(no_restart), policy_without_restart());
    let (engine, native) = engine_with(policy.with_variant(EngineVariant::default()));
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    for step in 0..=RESTART_PERIOD {
        gate_and_read(&engine, &mut state, step);
    }
    let profile = ok(engine.profile(&state));
    assert_eq!((profile.restarts, profile.fresh_handles), (1, 1));
    assert_eq!(native.computes().last(), Some(&(1, 1024)), "recommended");
    ok(state.close());
    ok(engine.close());
}

/// The 32-bit configurations each native state of `engine` gets, over
/// enough computes for one restart: the first state's and its successor's.
fn configurations_across_a_restart(
    engine: &CuTensorNetEngine,
    native: &NativeDouble,
) -> Vec<Vec<StateU32Configuration>> {
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    for step in 0..=RESTART_PERIOD {
        gate_and_read(engine, &mut state, step);
    }
    assert_eq!(ok(engine.profile(&state)).restarts, 1);
    ok(state.close());
    native.u32_configurations()
}

#[test]
fn the_default_svd_algorithm_and_gauge_are_gesvd_and_simple() {
    let (engine, native) = engine();
    let expected = [
        StateU32Configuration::SvdNormalizationNone,
        StateU32Configuration::SvdAlgorithm(SvdAlgorithm::Gesvd),
        StateU32Configuration::MpsGauge(MpsGauge::Simple),
    ];
    assert_eq!(
        configurations_across_a_restart(&engine, &native),
        [expected, expected],
        "the state and the state its restart builds"
    );
    ok(engine.close());
}

#[test]
fn the_policys_svd_algorithm_and_gauge_configure_each_state_and_each_restart() {
    let algorithms = [
        SvdAlgorithm::Gesvd,
        SvdAlgorithm::Gesvdj,
        SvdAlgorithm::Gesvdp,
        SvdAlgorithm::Gesvdr,
    ];
    for algorithm in algorithms {
        for gauge in [MpsGauge::Simple, MpsGauge::Free] {
            for fresh in [true, false] {
                let policy = ExecutionPolicy::base_qualification()
                    .with_svd_algorithm(algorithm)
                    .with_gauge(gauge)
                    .with_fresh_handle_at_restart(fresh);
                let (engine, native) = engine_with(policy);
                let expected = [
                    StateU32Configuration::SvdNormalizationNone,
                    StateU32Configuration::SvdAlgorithm(algorithm),
                    StateU32Configuration::MpsGauge(gauge),
                ];
                assert_eq!(
                    configurations_across_a_restart(&engine, &native),
                    [expected, expected],
                    "{algorithm:?}, {gauge:?}, fresh handle {fresh}"
                );
                ok(engine.close());
                assert_eq!(native.live_objects(), 0);
            }
        }
    }
}

#[test]
fn an_engine_variants_svd_algorithm_and_gauge_reach_the_state() {
    let variant = EngineVariant {
        svd_algorithm: SvdAlgorithm::Gesvdj,
        gauge: MpsGauge::Free,
        ..EngineVariant::default()
    };
    let policy = ExecutionPolicy::base_qualification();
    assert_eq!(
        policy.with_variant(variant),
        policy
            .with_svd_algorithm(SvdAlgorithm::Gesvdj)
            .with_gauge(MpsGauge::Free)
    );
    let (engine, native) = engine_with(policy.with_variant(variant));
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    gate_and_read(&engine, &mut state, 0);
    assert_eq!(
        native.u32_configurations(),
        [[
            StateU32Configuration::SvdNormalizationNone,
            StateU32Configuration::SvdAlgorithm(SvdAlgorithm::Gesvdj),
            StateU32Configuration::MpsGauge(MpsGauge::Free),
        ]]
    );
    ok(state.close());
    ok(engine.close());
}

#[test]
fn a_fresh_handle_without_a_restart_is_refused() {
    let native = Arc::new(NativeDouble::new());
    let api: Arc<dyn NativeApi> = native.clone();
    let policy = ExecutionPolicy::base_qualification().with_restart_period(None);
    assert!(matches!(
        CuTensorNetEngine::with_api(api, policy),
        Err(EngineError::InvalidExecutionPolicy { .. })
    ));
    assert_eq!(native.live_objects(), 0);
}

/// The double holds the engine to what it assumes of cuTensorNet: an
/// object is used with the handle it was created on, and a handle outlives
/// its objects.
#[test]
fn the_double_refuses_an_object_used_with_another_handle() {
    let native = NativeDouble::new();
    let first = ok(native.create_handle());
    let second = ok(native.create_handle());
    let state = ok(native.create_state(first, &[2, 2]));
    let refused = |result: Result<(), EngineError>, message: &str| {
        assert!(
            matches!(&result, Err(EngineError::NativeCallFailed { message: m, .. }) if m == message),
            "{result:?}"
        );
    };
    refused(
        native.capture_mps(second, state),
        "the object belongs to another handle",
    );
    refused(
        native.destroy_handle(first),
        "an object of the handle is still live",
    );
    ok(native.destroy_state(state));
    refused(native.capture_mps(first, state), "no such object");
    ok(native.destroy_handle(first));
    ok(native.destroy_handle(second));
    assert_eq!(native.live_objects(), 0);
}

#[test]
fn the_workspace_probe_prepares_a_new_state_and_releases_it() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    let p1 = ok(engine.probability_of_one(&state, 1));
    let live = native.live_objects();
    let state_computes = native.count("compute_state");
    let gate = hadamard();
    let batch: [(&[usize], &[Complex64]); 2] = [(&[2], &gate), (&[1], &gate)];
    for handle in [NativeHandle::Engine, NativeHandle::Fresh] {
        for operators in [&batch[..0], &batch[..]] {
            let handles = native.count("create_handle");
            let applied = native.applied().len();
            let prepares = native.count("prepare_state");
            let sizes = ok(engine.fresh_state_workspace(&state, handle, operators));
            assert_eq!(
                (sizes.minimum_bytes, sizes.recommended_bytes),
                (512, 1024),
                "{handle:?}"
            );
            let (created, on_handle) = match handle {
                NativeHandle::Engine => (0, 1),
                NativeHandle::Fresh => (1, 0),
            };
            assert_eq!(sizes.handle_computes, on_handle, "{handle:?}");
            assert_eq!(native.count("create_handle") - handles, created);
            assert_eq!(native.applied().len() - applied, operators.len());
            assert_eq!(native.count("prepare_state") - prepares, 1);
            assert_eq!(native.live_objects(), live, "{handle:?}: nothing leaks");
        }
    }
    assert_eq!(
        native.count("compute_state"),
        state_computes,
        "the probe computes nothing"
    );
    assert_eq!(
        ok(engine.probability_of_one(&state, 1)).to_bits(),
        p1.to_bits()
    );
    assert_eq!(
        native.count("compute_state"),
        state_computes,
        "nor the state again"
    );
    ok(engine.apply(&mut state, &[1, 2], &cnot()));
    let p2 = ok(engine.probability_of_one(&state, 2));
    assert!((p2 - 0.5).abs() < 1e-12, "{p2}");
    assert_eq!(ok(engine.profile(&state)).computes, 2);
    ok(state.close());
    assert_eq!(native.live_objects(), 2);
}

#[test]
fn the_workspace_probe_is_refused_before_a_compute_and_for_an_operator_apply_refuses() {
    let (engine, native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    let live = native.live_objects();
    assert!(is_invalid_request(&engine.fresh_state_workspace(
        &state,
        NativeHandle::Fresh,
        &[]
    )));
    assert_eq!(native.live_objects(), live);
    ok(engine.probability_of_one(&state, 0));
    let live = native.live_objects();
    let gate = cnot();
    for operators in [
        [(&[0_usize, 1, 2][..], &gate[..])],
        [(&[0, 0][..], &gate[..])],
        [(&[0][..], &gate[..])],
        [(&[3][..], &hadamard()[..])],
    ] {
        assert!(is_invalid_request(&engine.fresh_state_workspace(
            &state,
            NativeHandle::Fresh,
            &operators
        )));
    }
    assert_eq!(native.live_objects(), live);
    assert_eq!(native.count("create_handle"), 1, "refused up front");
    ok(state.close());
}

#[test]
fn a_failed_workspace_probe_releases_what_it_created_and_leaves_the_state_usable() {
    for operation in [
        "create_handle",
        "create_state",
        "initialize_mps",
        "apply_tensor_operator",
        "create_workspace",
        "prepare_state",
        "workspace_size",
    ] {
        let (engine, native) = engine();
        let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
        ok(engine.apply(&mut state, &[0], &hadamard()));
        let p0 = ok(engine.probability_of_one(&state, 0));
        let live = native.live_objects();
        native.fail(operation, native.count(operation) + 1);
        let gate = hadamard();
        let failed = engine.fresh_state_workspace(&state, NativeHandle::Fresh, &[(&[1], &gate)]);
        assert!(
            matches!(
                &failed,
                Err(EngineError::NativeCallFailed { operation: at, .. }) if *at == operation
            ),
            "{operation}: {failed:?}"
        );
        assert_eq!(native.live_objects(), live, "{operation}");
        assert_eq!(
            ok(engine.probability_of_one(&state, 0)).to_bits(),
            p0.to_bits(),
            "{operation}"
        );
        ok(engine.apply(&mut state, &[0, 1], &cnot()));
        ok(engine.probability_of_one(&state, 1));
        ok(state.close());
        ok(engine.close());
        assert_eq!(native.live_objects(), 0, "{operation}");
    }
}
