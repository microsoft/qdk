//! Host tests of the cuTensorNet engine, on the CPU test double of the
//! native API: conformance with `tensornet::reference::ExactReference`, the
//! refusals that are this engine's own, the order of native calls, and that
//! every native object is released. Numerical behavior on a GPU is not
//! tested here.

use super::{
    CuTensorNetEngine, EngineError, NativeApi,
    policy::ExecutionPolicy,
    test_double::{NORM_SCALE, NativeDouble, uniform_capacity},
};
use num_complex::Complex64;
use std::sync::Arc;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm, SampleSeed, conformance};

fn engine() -> (CuTensorNetEngine, Arc<NativeDouble>) {
    engine_with(ExecutionPolicy::base_qualification())
}

fn engine_without_capture() -> (CuTensorNetEngine, Arc<NativeDouble>) {
    engine_with(ExecutionPolicy::base_qualification().without_capture())
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

#[test]
fn the_engine_without_capture_conforms_to_the_exact_reference() {
    let (engine, native) = engine_without_capture();
    let failures = conformance::check(&engine);
    assert!(failures.is_empty(), "{failures:#?}");
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
    assert_eq!(second[compute + 1], "capture_mps", "{second:?}");
    assert!(
        !second[..compute].contains(&"free"),
        "the captured start is freed before it is read: {second:?}"
    );
    assert!(second[compute + 2..].contains(&"free"), "{second:?}");
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

#[test]
fn reads_divide_by_the_norm_the_library_reports() {
    assert!((NORM_SCALE - 1.0).abs() > 0.1, "the double scales the norm");
    let (engine, _native) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    let p1 = ok(engine.probability_of_one(&state, 0));
    assert!((p1 - 0.5).abs() < 1e-12, "{p1}");
    let x0 = PauliTerm::new(c(1.0), vec![(0, Pauli::X)])
        .and_then(|term| PauliSum::new(c(0.0), vec![term]))
        .unwrap_or_else(|e| panic!("{e}"));
    let value = ok(engine.expectation(&state, &x0));
    assert!((value - c(1.0)).norm() < 1e-12, "{value}");
}

#[test]
fn operators_beyond_two_sites_and_one_site_capacities_are_refused() {
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
    let lone = MpsShape::new(vec![vec![2]]).unwrap_or_else(|e| panic!("{e}"));
    assert!(is_invalid_request(&engine.prepare(&lone)));
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
