// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the cuTensorNet engine, on fake libraries that simulate the
//! native API on the CPU (`src/engine/native_calls/fake_libraries.rs`),
//! which enforce cuTensorNet's call order and compute each state as a state
//! vector: the engine's answers, its refusals, the order of its native
//! calls, its forks, its restarts and its scratch, and that every native
//! object is released.
//! Numerical behaviour on a GPU is not tested here.

mod dev_tuning;
mod device_session;
mod native_calls;

use crate::engine::{
    CuTensorNetEngine, CuTensorNetState, DISCARDED_WEIGHT_CUTOFF, EXPECTATION_HYPER_SAMPLES,
    EngineError, RESTART_PERIOD,
    native_calls::{
        NativeCalls, StateF64Configuration, StateU32Configuration,
        fake_libraries::{FakeLibraries, WORKSPACE_BYTES},
    },
};
use num_complex::Complex64;
use std::sync::Arc;
use tensornet::{MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, PauliTerm, Refusal};

fn engine() -> (CuTensorNetEngine, Arc<FakeLibraries>) {
    let libraries = Arc::new(FakeLibraries::new());
    let native_calls: Arc<dyn NativeCalls> = libraries.clone();
    let engine = CuTensorNetEngine::with_native_calls(native_calls)
        .unwrap_or_else(|error| panic!("the fake libraries open a session: {error}"));
    (engine, libraries)
}

/// The capacity of `sites` qubits with every bond `bond`.
fn uniform_capacity(sites: usize, bond: usize) -> MpsShape {
    let shape = (0..sites)
        .map(|site| match site {
            0 => vec![2, bond],
            _ if site + 1 == sites => vec![bond, 2],
            _ => vec![bond, 2, bond],
        })
        .collect();
    MpsShape::new(shape).unwrap_or_else(|error| panic!("a valid capacity: {error}"))
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

fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![c(k), c(-s), c(s), c(k)]
}

fn ok<T>(result: Result<T, EngineError>) -> T {
    result.unwrap_or_else(|error| panic!("the engine call succeeds: {error}"))
}

fn refusal<T: std::fmt::Debug>(result: Result<T, EngineError>) -> Refusal {
    match result {
        Err(error) => error
            .refusal()
            .cloned()
            .unwrap_or_else(|| panic!("a refusal, not {error}")),
        Ok(value) => panic!("a refusal, not {value:?}"),
    }
}

/// A Bell pair: H on site 0, then CX from 0 to 1.
fn bell(engine: &CuTensorNetEngine) -> CuTensorNetState {
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    state
}

/// cos(0.35)|00⟩ + sin(0.35)|11⟩ on a capacity of bond 1, which keeps
/// |00⟩ with the weight cos²(0.35).
fn truncated_pair(engine: &CuTensorNetEngine) -> CuTensorNetState {
    let mut state = ok(engine.prepare(&uniform_capacity(2, 1)));
    ok(engine.apply(&mut state, &[0], &ry(0.7)));
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    state
}

fn z(site: usize) -> PauliSum {
    PauliTerm::new(c(1.0), vec![(site, Pauli::Z)])
        .and_then(|term| PauliSum::new(c(0.0), vec![term]))
        .unwrap_or_else(|error| panic!("one Z factor is a Pauli sum: {error}"))
}

/// A Bell pair reads One on either site half the time: P(One) reads the
/// computed state.
#[test]
fn a_bell_pairs_probability_of_one_is_a_half() {
    let (engine, _libraries) = engine();
    let state = bell(&engine);
    let p = ok(engine.probability_of_one(&state, 1));
    assert!((p - 0.5).abs() < 1e-12, "{p}");
}

/// A Bell pair's ⟨Z₀⟩ is zero: an expectation reads the computed state.
#[test]
fn a_bell_pairs_z_expectation_is_zero() {
    let (engine, _libraries) = engine();
    let state = bell(&engine);
    let value = ok(engine.expectation(&state, &z(0)));
    assert!(value.norm() < 1e-12, "{value}");
}

/// The two sites of a Bell pair always agree, so every shot sampled from
/// it reads the same value on both, in the order the sites were asked for.
#[test]
fn a_bell_pairs_samples_agree_on_both_sites() {
    let (engine, _libraries) = engine();
    let state = bell(&engine);
    let values = ok(engine.sample(&state, &[1, 0], 50, 3));
    assert_eq!(values.len(), 100, "two values per shot, 50 shots");
    assert!(values.chunks_exact(2).all(|shot| shot[0] == shot[1]));
}

/// A Bell pair needs a bond of two, so its realized chain is the whole
/// capacity of two qubits with bond two.
#[test]
fn a_bell_pairs_realized_chain_has_bond_two() {
    let (engine, _libraries) = engine();
    let state = bell(&engine);
    assert_eq!(ok(engine.shape(&state)), uniform_capacity(2, 2));
}

/// A truncated MPS is not normalized, so every read divides by the norm
/// the library reports beside it, and that norm is the retained weight.
#[test]
fn reads_divide_by_the_norm_the_library_reports() {
    let (engine, _libraries) = engine();
    let mut state = truncated_pair(&engine);
    let kept = 0.35f64.cos().powi(2);
    let weight = ok(engine.retained_weight(&state));
    assert!((weight - kept).abs() < 1e-12, "{weight}, not {kept}");
    ok(engine.apply(&mut state, &[0], &hadamard()));
    assert!((ok(engine.probability_of_one(&state, 0)) - 0.5).abs() < 1e-12);
    assert!((ok(engine.expectation(&state, &z(1))) - c(1.0)).norm() < 1e-12);
}

/// A projection divides by √p, so it keeps the norm, and with it the
/// retained weight, while truncations lower it.
#[test]
fn projections_keep_the_retained_weight() {
    let (engine, _libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 1)));
    ok(engine.apply(&mut state, &[1], &ry(1.1)));
    ok(engine.apply(&mut state, &[1, 2], &cnot()));
    ok(engine.apply(&mut state, &[0], &ry(0.4)));
    let weight = ok(engine.retained_weight(&state));
    assert!(weight < 0.99, "the bond truncated: {weight}");
    ok(engine.project(&mut state, 0, false));
    let projected = ok(engine.retained_weight(&state));
    assert!(
        (projected - weight).abs() < 1e-12,
        "{projected}, not {weight}"
    );
}

/// The engine refuses with the contract's checks: an operator on three
/// sites as too many sites, whatever its matrix.
#[test]
fn an_operator_on_three_sites_is_refused_as_too_many_sites() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 4)));
    assert_eq!(
        refusal(engine.apply(&mut state, &[0, 1, 2], &[c(1.0)])),
        Refusal::TooManySites { sites: 3 }
    );
    assert_eq!(libraries.count("allocate"), 0, "no operator tensor");
    assert!(libraries.applied().is_empty());
}

/// Development only: with `with_dev_wide_operators`, an operator on three
/// sites far apart and out of order goes to the library as one MPO on
/// their modes ascending, never as a three-mode tensor operator, which
/// cuTensorNet's MPS simulation does not take (the fake refuses it), with
/// `sites[0]` its most significant bit: an entangling operator (a Toffoli
/// after rotations by three angles) leaves the state the dense operator
/// gives, and the MPO is released with the state.
#[test]
fn with_the_dev_flag_an_operator_on_three_sites_applies_as_one_mpo() {
    let (engine, libraries) = engine();
    let engine = engine.with_dev_wide_operators(true);
    let live = libraries.live_objects();
    let mut state = ok(engine.prepare(&uniform_capacity(5, 4)));
    let angles = [0.3, 1.1, 2.0];
    let factors: Vec<Vec<Complex64>> = angles.iter().map(|&theta| ry(theta)).collect();
    let bit = |x: usize, at: usize| (x >> (2 - at)) & 1;
    let mut rotations = vec![c(0.0); 64];
    for (row, col) in (0..8).flat_map(|row| (0..8).map(move |col| (row, col))) {
        rotations[row * 8 + col] = (0..3)
            .map(|at| factors[at][2 * bit(row, at) + bit(col, at)])
            .product();
    }
    // Toffoli: the last bit flips when the first two are one.
    let toffoli = |row: usize| if row >= 6 { row ^ 1 } else { row };
    let mut matrix = vec![c(0.0); 64];
    for (row, col) in (0..8).flat_map(|row| (0..8).map(move |col| (row, col))) {
        matrix[row * 8 + col] = rotations[toffoli(row) * 8 + col];
    }
    ok(engine.apply(&mut state, &[4, 0, 2], &matrix));
    assert_eq!(
        libraries.applied(),
        vec![(vec![0, 2, 4], true)],
        "one MPO, its modes ascending"
    );
    let p = |theta: f64| (theta / 2.0_f64).sin().powi(2);
    let (p4, p0, p2) = (p(angles[0]), p(angles[1]), p(angles[2]));
    let expected = [(4, p4), (0, p0), (2, p2 + p4 * p0 * (1.0 - 2.0 * p2))];
    for (site, expected) in expected {
        let probability = ok(engine.probability_of_one(&state, site));
        assert!(
            (probability - expected).abs() < 1e-12,
            "site {site}: {probability}, not {expected}"
        );
    }
    drop(state);
    assert_eq!(libraries.live_objects(), live, "the MPO released");
}

/// The engine's sites are qubits: a capacity of qutrits is refused by the
/// contract's check before any native state is created.
#[test]
fn a_capacity_of_qutrits_is_refused_as_not_two_level() {
    let (engine, libraries) = engine();
    let qutrits = MpsShape::new(vec![vec![3, 1], vec![1, 3]])
        .unwrap_or_else(|error| panic!("two qutrits form a chain: {error}"));
    assert_eq!(
        refusal(engine.prepare(&qutrits)),
        Refusal::NotTwoLevel { site: 0, levels: 3 }
    );
    assert_eq!(
        libraries.count("create_state"),
        0,
        "a refused capacity creates no native state"
    );
}

/// The padding site a lone qubit needs on cuTensorNet is out of range, and
/// `shape` shows the lone site alone.
#[test]
fn a_lone_qubit_has_one_site() {
    let (engine, _libraries) = engine();
    let lone = MpsShape::qubit_chain(1, 1)
        .unwrap_or_else(|error| panic!("a one-qubit chain is a valid capacity: {error}"));
    let mut state = ok(engine.prepare(&lone));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    assert!((ok(engine.probability_of_one(&state, 0)) - 0.5).abs() < 1e-12);
    assert_eq!(
        refusal(engine.probability_of_one(&state, 1)),
        Refusal::SiteOutOfRange { site: 1, sites: 1 }
    );
    assert_eq!(ok(engine.shape(&state)), lone);
}

/// `project` returns the probability of the value it projected on, and
/// leaves the state normalized on it.
#[test]
fn a_projection_returns_the_probability_of_its_value() {
    let (engine, _libraries) = engine();
    let mut state = truncated_pair(&engine);
    ok(engine.apply(&mut state, &[0], &ry(1.0)));
    let p_one = (0.5f64).sin().powi(2);
    let p = ok(engine.project(&mut state, 0, true));
    assert!((p - p_one).abs() < 1e-12, "{p}, not {p_one}");
    assert!((ok(engine.probability_of_one(&state, 0)) - 1.0).abs() < 1e-12);
}

/// A measurement reads P(One) and projects: the projection reuses the
/// probability just read, so the measurement costs one expectation.
#[test]
fn a_projection_reuses_the_probability_just_read() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    let p_one = ok(engine.probability_of_one(&state, 1));
    let reads = libraries.count("compute_expectation");
    assert!((ok(engine.project(&mut state, 1, false)) - (1.0 - p_one)).abs() < 1e-15);
    assert_eq!(libraries.count("compute_expectation"), reads);
}

/// A probability read before an operator is stale after it.
#[test]
fn a_projection_after_an_operator_reads_its_probability_again() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    ok(engine.probability_of_one(&state, 1));
    ok(engine.apply(&mut state, &[1], &ry(0.6)));
    let reads = libraries.count("compute_expectation");
    let p = ok(engine.project(&mut state, 1, true));
    assert_eq!(libraries.count("compute_expectation"), reads + 1);
    let expected = 0.5 * (0.3f64.sin().powi(2) + 0.3f64.cos().powi(2));
    assert!((p - expected).abs() < 1e-12, "{p}, not {expected}");
}

/// Projecting site 0 of |00⟩ on One, a value of probability zero, is
/// refused, and no projector reaches the state.
#[test]
fn a_projection_on_an_impossible_value_is_refused_and_changes_nothing() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    assert_eq!(
        refusal(engine.project(&mut state, 0, true)),
        Refusal::ZeroProbabilityOutcome
    );
    assert!(libraries.applied().is_empty(), "no projector was applied");
    assert!(ok(engine.probability_of_one(&state, 0)).abs() < 1e-12);
}

/// P(One) of site 0 given that site 1 reads One has no answer on |00⟩:
/// the condition's probability is zero.
#[test]
fn a_condition_of_probability_zero_is_refused() {
    let (engine, _libraries) = engine();
    let state = ok(engine.prepare(&uniform_capacity(2, 2)));
    assert_eq!(
        refusal(engine.conditional_probability_of_one(&state, 0, &[(1, true)])),
        Refusal::ZeroProbabilityOutcome
    );
}

/// A conditional probability is two expectations on the state: no fork,
/// no projection.
#[test]
fn a_conditional_probability_neither_forks_nor_projects() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    let given = ok(engine.conditional_probability_of_one(&state, 1, &[(0, true)]));
    assert!((given - 1.0).abs() < 1e-12, "{given}");
    assert_eq!(libraries.count("create_state"), 1);
    assert_eq!(libraries.count("compute_expectation"), 2);
    assert_eq!(libraries.applied().len(), 2, "the two gates only");
}

/// cuTensorNet is told whether each operator is unitary: the gates of a
/// Bell pair are, the projection that measures it is not.
#[test]
fn operators_are_marked_unitary_and_projections_are_not() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    ok(engine.project(&mut state, 0, true));
    assert_eq!(
        libraries.applied(),
        [(vec![0], true), (vec![0, 1], true), (vec![0], false)]
    );
}

/// Every state is finalized once, and computed again only after a change.
#[test]
fn a_state_is_computed_again_only_after_a_change() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 4)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.probability_of_one(&state, 0));
    ok(engine.retained_weight(&state));
    ok(engine.shape(&state));
    assert_eq!(libraries.count("compute_state"), 1);
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.probability_of_one(&state, 1));
    assert_eq!(libraries.count("compute_state"), 2);
    assert_eq!(libraries.count("finalize_mps"), 1);
}

/// The calls that compute, capture, apply and free, in order.
fn compute_events(libraries: &FakeLibraries) -> Vec<&'static str> {
    libraries
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

/// Each compute is captured, so the next compute applies only the new
/// operators; the previous outputs, which the captured start is read from,
/// are freed only after the next capture.
#[test]
fn each_compute_is_captured_before_the_previous_outputs_are_freed() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.shape(&state));
    let first = compute_events(&libraries).len();
    ok(engine.apply(&mut state, &[0, 1], &cnot()));
    ok(engine.shape(&state));
    let events = compute_events(&libraries);
    assert_eq!(
        events[first..],
        [
            "apply_tensor_operator",
            "compute_state",
            "capture_mps",
            "free",
            "free"
        ]
    );
}

/// A compute frees the previous compute's outputs; when one of those frees
/// fails, the others are still made, so once the state and the engine are
/// dropped only the buffer whose free failed is left.
#[test]
fn a_failed_free_of_one_previous_output_still_frees_the_others() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    ok(engine.probability_of_one(&state, 0));
    ok(engine.apply(&mut state, &[0], &hadamard()));
    libraries.fail("free", libraries.count("free") + 1);
    assert!(matches!(
        engine.probability_of_one(&state, 0),
        Err(EngineError::NativeCallFailed {
            operation: "free",
            ..
        })
    ));
    drop(state);
    drop(engine);
    assert_eq!(libraries.live_objects(), 1, "the buffer whose free failed");
}

/// Every state, a restarted or a forked one too, truncates with the
/// discarded-weight cutoff, unnormalized, with GESVD and the simple gauge.
#[test]
fn every_native_state_is_configured_as_the_engine_fixes() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    for step in 0..=RESTART_PERIOD {
        ok(engine.apply(&mut state, &[step % 2], &hadamard()));
        ok(engine.shape(&state));
    }
    let fork = ok(engine.fork(&state));
    ok(engine.shape(&fork));
    let configured = [
        StateU32Configuration::SvdNormalizationNone,
        StateU32Configuration::SvdAlgorithmGesvd,
        StateU32Configuration::MpsGaugeSimple,
    ];
    assert_eq!(libraries.u32_configurations(), [configured; 3]);
    assert_eq!(
        libraries.f64_configurations(),
        [StateF64Configuration::SvdDiscardedWeightCutoff(DISCARDED_WEIGHT_CUTOFF); 3]
    );
}

/// The native calls of a fork, and of a restart.
fn lifecycle_events(libraries: &FakeLibraries) -> Vec<&'static str> {
    libraries
        .events()
        .into_iter()
        .filter(|event| {
            [
                "create_handle",
                "destroy_handle",
                "create_state",
                "destroy_state",
                "copy_on_device",
                "initialize_mps",
                "apply_tensor_operator",
            ]
            .contains(event)
        })
        .collect()
}

/// A fork copies the computed MPS, one tensor per site, and starts a new
/// state from the copy: it applies no operator again.
#[test]
fn a_fork_starts_from_a_copy_of_the_computed_mps() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    let before = lifecycle_events(&libraries).len();
    let fork = ok(engine.fork(&state));
    assert_eq!(
        lifecycle_events(&libraries)[before..],
        [
            "create_state",
            "copy_on_device",
            "copy_on_device",
            "initialize_mps"
        ]
    );
    let given = ok(engine.conditional_probability_of_one(&fork, 1, &[(0, true)]));
    assert!((given - 1.0).abs() < 1e-12, "{given}");
}

/// A fork holds the state it copies, truncated: a state recomputed from
/// |0…0⟩ by the same operators would truncate along another path, and
/// keep another weight.
#[test]
fn a_fork_of_a_truncated_state_keeps_its_retained_weight() {
    let (engine, _libraries) = engine();
    let gates = |engine: &CuTensorNetEngine, state: &mut CuTensorNetState, read: bool| {
        ok(engine.apply(state, &[0], &ry(1.2)));
        ok(engine.apply(state, &[0, 1], &cnot()));
        if read {
            ok(engine.shape(state));
        }
        ok(engine.apply(state, &[1], &ry(0.9)));
        ok(engine.apply(state, &[0, 1], &cnot()));
    };
    let mut stepped = ok(engine.prepare(&uniform_capacity(2, 1)));
    gates(&engine, &mut stepped, true);
    let mut at_once = ok(engine.prepare(&uniform_capacity(2, 1)));
    gates(&engine, &mut at_once, false);
    let weight = ok(engine.retained_weight(&stepped));
    let replayed = ok(engine.retained_weight(&at_once));
    assert!(
        (weight - replayed).abs() > 1e-3,
        "the circuit truncates along two paths: {weight}, {replayed}"
    );
    let fork = ok(engine.fork(&stepped));
    let forked = ok(engine.retained_weight(&fork));
    assert!((forked - weight).abs() < 1e-12, "{forked}, not {weight}");
}

/// The fork's tensors are its own: its parent may change, recompute,
/// restart and be dropped.
#[test]
fn a_fork_outlives_its_parents_changes() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    let fork = ok(engine.fork(&state));
    for step in 0..=RESTART_PERIOD {
        ok(engine.apply(&mut state, &[step % 2], &hadamard()));
        ok(engine.shape(&state));
    }
    assert!(
        libraries.count("initialize_mps") > 1,
        "the parent restarted"
    );
    drop(state);
    let given = ok(engine.conditional_probability_of_one(&fork, 0, &[(1, false)]));
    assert!(given.abs() < 1e-12, "{given}");
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

/// A state restarts before the first operator after every
/// [`RESTART_PERIOD`] computes, on a handle of its own, and destroys its
/// previous own handle; never the engine's.
#[test]
fn a_state_restarts_on_a_fresh_handle_every_restart_period() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    let mut step = 0;
    for period in 0..3 {
        for _ in 0..RESTART_PERIOD {
            gate_and_read(&engine, &mut state, step);
            step += 1;
        }
        assert_eq!(libraries.count("initialize_mps"), period, "after {step}");
    }
    gate_and_read(&engine, &mut state, step);
    assert_eq!(libraries.count("initialize_mps"), 3);
    assert_eq!(libraries.live_handles(), 2, "the engine's and the state's");
    assert_eq!(
        libraries.count("destroy_handle"),
        2,
        "each earlier own handle"
    );
}

/// Capture is equivalent to a new state started from the computed MPS, so
/// two states whose restarts fall at different steps read the same values
/// through truncations and projections.
#[test]
fn a_restart_changes_no_value() {
    let (engine, libraries) = engine();
    let walk = |offset: usize| -> Vec<f64> {
        let mut state = ok(engine.prepare(&uniform_capacity(4, 2)));
        for _ in 0..offset {
            ok(engine.shape(&state));
            ok(engine.apply(&mut state, &[3], &hadamard()));
            ok(engine.apply(&mut state, &[3], &hadamard()));
        }
        let mut read = Vec::new();
        for step in 0..3 * RESTART_PERIOD {
            read.push(gate_and_read(&engine, &mut state, step));
            read.push(ok(engine.retained_weight(&state)));
            if step % 5 == 4 {
                read.push(ok(engine.project(&mut state, step % 3, true)));
            }
        }
        read
    };
    let early = walk(0);
    let late = walk(RESTART_PERIOD / 2);
    assert!(
        libraries.count("initialize_mps") >= 4,
        "both states restarted"
    );
    for (at, (value, wanted)) in late.iter().zip(&early).enumerate() {
        assert!(
            (value - wanted).abs() < 1e-12,
            "read {at}: {value}, not {wanted}"
        );
    }
}

/// A restart releases the old state, its operator tensors and its
/// handle: a long shot holds what one period needs.
#[test]
fn a_restart_releases_what_the_old_state_held() {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    for step in 0..=RESTART_PERIOD {
        gate_and_read(&engine, &mut state, step % RESTART_PERIOD);
    }
    let live = libraries.live_objects();
    for step in RESTART_PERIOD + 1..=2 * RESTART_PERIOD {
        gate_and_read(&engine, &mut state, step % RESTART_PERIOD);
    }
    assert_eq!(libraries.count("initialize_mps"), 2);
    assert_eq!(libraries.live_objects(), live);
}

/// The native calls a restart makes, in steps, any of which may fail.
const RESTART_STEPS: [&str; 5] = [
    "create_handle",
    "create_state",
    "initialize_mps",
    "configure_state_u32",
    "create_workspace",
];

/// A state whose restart failed, and what a test reads of it.
struct FailedRestart {
    engine: CuTensorNetEngine,
    libraries: Arc<FakeLibraries>,
    state: CuTensorNetState,
    /// The native objects live before the restart.
    live: usize,
    /// What the operator that triggered the restart returned.
    failed: Result<(), EngineError>,
}

/// A state due to restart, whose restart fails at `operation`.
fn failed_restart(operation: &'static str) -> FailedRestart {
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    for step in 0..RESTART_PERIOD {
        ok(engine.apply(&mut state, &[step % 2], &hadamard()));
        ok(engine.shape(&state));
    }
    let live = libraries.live_objects();
    libraries.fail(operation, libraries.count(operation) + 1);
    let failed = engine.apply(&mut state, &[0], &hadamard());
    FailedRestart {
        engine,
        libraries,
        state,
        live,
        failed,
    }
}

/// A restart replaces the native state in steps; the operator whose
/// restart fails at any step reports that step's native failure.
#[test]
fn a_failed_restart_reports_the_native_failure() {
    for operation in RESTART_STEPS {
        let failed = failed_restart(operation).failed;
        assert!(
            matches!(
                &failed,
                Err(EngineError::NativeCallFailed { operation: at, .. }) if *at == operation
            ),
            "{operation}: {failed:?}"
        );
    }
}

/// A restart that fails at any step may leave the native state half
/// replaced, so the state is unusable after it.
#[test]
fn a_failed_restart_poisons_the_state() {
    for operation in RESTART_STEPS {
        let restart = failed_restart(operation);
        assert!(
            matches!(
                restart.engine.shape(&restart.state),
                Err(EngineError::Poisoned { .. })
            ),
            "{operation}"
        );
    }
}

/// A restart that fails at any step releases what it had created: the
/// state holds what it held before.
#[test]
fn a_failed_restart_releases_what_it_created() {
    for operation in RESTART_STEPS {
        let restart = failed_restart(operation);
        assert_eq!(
            restart.libraries.live_objects(),
            restart.live,
            "{operation}"
        );
    }
}

/// The state holds one scratch buffer, attached to every descriptor it
/// uses, across computes and expectations (a sample is expectations).
#[test]
fn a_state_holds_one_scratch_buffer() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    for site in 0..2 {
        ok(engine.probability_of_one(&state, site));
        ok(engine.sample(&state, &[0, 1], 4, 7));
        ok(engine.apply(&mut state, &[site], &hadamard()));
        ok(engine.shape(&state));
    }
    assert_eq!(libraries.live_scratch(), 1);
    assert_eq!(libraries.scratch_buffers(), 1, "never replaced");
}

/// When a call needs more scratch than the state holds, the state attaches
/// a larger buffer before it frees the smaller one, so no descriptor holds
/// freed memory (the fake refuses to free attached scratch).
#[test]
fn a_state_grows_its_scratch_when_a_call_needs_more() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    ok(engine.shape(&state));
    libraries.set_workspace_bytes(4 * WORKSPACE_BYTES);
    ok(engine.apply(&mut state, &[0], &hadamard()));
    ok(engine.shape(&state));
    assert_eq!(libraries.live_scratch(), 1);
    assert_eq!(libraries.scratch_buffers(), 2);
    let attached = libraries.computes().last().map(|&(_, bytes)| bytes);
    assert_eq!(attached, usize::try_from(4 * WORKSPACE_BYTES).ok());
}

/// A call is told it may use the larger of the device's free memory and
/// the scratch the state holds, not their sum.
#[test]
fn a_call_may_take_the_larger_of_the_free_memory_and_the_held_scratch() {
    let held = usize::try_from(WORKSPACE_BYTES).unwrap_or(0);
    for (free, limit) in [(1 << 20, 1 << 20), (100, held)] {
        let (engine, libraries) = engine();
        let mut state = bell(&engine);
        ok(engine.shape(&state));
        libraries.set_free_memory(free);
        ok(engine.apply(&mut state, &[0], &hadamard()));
        ok(engine.shape(&state));
        assert_eq!(libraries.prepare_limits()[1], limit, "{free} bytes free");
    }
}

/// The larger buffer is allocated before the smaller is freed, so a
/// growth must fit in the free memory alone.
#[test]
fn a_growth_beyond_the_free_memory_fails() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    ok(engine.shape(&state));
    libraries.set_free_memory(1500);
    libraries.set_workspace_bytes(2 * WORKSPACE_BYTES);
    ok(engine.apply(&mut state, &[0], &hadamard()));
    assert!(matches!(
        engine.shape(&state),
        Err(EngineError::WorkspaceLimitExceeded {
            required: 2048,
            maximum: 1500
        })
    ));
}

/// A compute whose scratch exceeds the device's free memory fails with
/// both sizes, before any scratch is allocated.
#[test]
fn a_workspace_above_the_free_memory_fails_before_any_scratch() {
    let (engine, libraries) = engine();
    libraries.set_free_memory(1000);
    let state = bell(&engine);
    assert!(matches!(
        engine.shape(&state),
        Err(EngineError::WorkspaceLimitExceeded {
            required: 1024,
            maximum: 1000
        })
    ));
    assert_eq!(libraries.scratch_buffers(), 0);
}

/// A failed compute may leave the native state half changed, so the
/// state's later calls are refused as poisoned.
#[test]
fn a_native_failure_while_computing_poisons_the_state() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    libraries.fail("compute_state", 1);
    assert!(matches!(
        engine.probability_of_one(&state, 0),
        Err(EngineError::NativeCallFailed {
            operation: "compute_state",
            ..
        })
    ));
    assert!(matches!(
        engine.apply(&mut state, &[1], &hadamard()),
        Err(EngineError::Poisoned { .. })
    ));
}

/// A read changes nothing in the state: when its expectation fails, its
/// objects are released and the next read is answered.
#[test]
fn a_failed_read_releases_its_objects_and_leaves_the_state_usable() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    ok(engine.shape(&state));
    let live = libraries.live_objects();
    libraries.fail("compute_expectation", 1);
    assert!(engine.probability_of_one(&state, 0).is_err());
    assert_eq!(libraries.live_objects(), live);
    assert!((ok(engine.probability_of_one(&state, 0)) - 0.5).abs() < 1e-12);
}

/// A state belongs to the engine that prepared it, whose session holds its
/// native objects: another engine refuses it.
#[test]
fn a_state_of_another_engine_is_refused() {
    let (first, _) = engine();
    let (second, _) = engine();
    let state = ok(first.prepare(&uniform_capacity(2, 2)));
    assert!(matches!(
        second.probability_of_one(&state, 0),
        Err(EngineError::ForeignState)
    ));
}

/// CUDA's current device is per thread, and an MPS simulation method may
/// call the engine on a thread that last used another device: every
/// `MpsEngine` method, and the drop of a state, selects the engine's device
/// before any other native call.
#[test]
fn every_call_selects_the_engines_device_first() {
    type Call = fn(&CuTensorNetEngine, &mut CuTensorNetState);
    let (engine, libraries) = engine();
    let mut prepared = None;
    let first = first_native_call(&libraries, || {
        prepared = Some(ok(engine.prepare(&uniform_capacity(2, 2))));
    });
    assert_eq!(first, Some("set_device"), "prepare");
    let mut state = prepared.expect("prepare succeeded");
    let calls: [(&str, Call); 9] = [
        ("apply", |engine, state| {
            ok(engine.apply(state, &[0], &hadamard()));
        }),
        ("probability_of_one", |engine, state| {
            ok(engine.probability_of_one(state, 0));
        }),
        ("project", |engine, state| {
            ok(engine.project(state, 0, true));
        }),
        ("conditional_probability_of_one", |engine, state| {
            ok(engine.conditional_probability_of_one(state, 1, &[(0, true)]));
        }),
        ("sample", |engine, state| {
            ok(engine.sample(state, &[0, 1], 2, 5));
        }),
        ("expectation", |engine, state| {
            ok(engine.expectation(state, &z(0)));
        }),
        ("shape", |engine, state| {
            ok(engine.shape(state));
        }),
        ("retained_weight", |engine, state| {
            ok(engine.retained_weight(state));
        }),
        ("fork", |engine, state| {
            ok(engine.fork(state));
        }),
    ];
    for (name, call) in calls {
        let first = first_native_call(&libraries, || call(&engine, &mut state));
        assert_eq!(first, Some("set_device"), "{name}");
    }
    let first = first_native_call(&libraries, move || drop(state));
    assert_eq!(first, Some("set_device"), "drop");
}

/// The engine checks each call against the contract (`tensornet::checks`)
/// before any native call, so a call naming a site the state does not have
/// is refused and leaves the device untouched.
#[test]
fn a_refused_call_makes_no_native_call() {
    type Call = fn(&CuTensorNetEngine, &mut CuTensorNetState) -> Refusal;
    let (engine, libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    let calls: [(&str, Call); 6] = [
        ("apply", |engine, state| {
            refusal(engine.apply(state, &[2], &hadamard()))
        }),
        ("probability_of_one", |engine, state| {
            refusal(engine.probability_of_one(state, 2))
        }),
        ("project", |engine, state| {
            refusal(engine.project(state, 2, true))
        }),
        ("conditional_probability_of_one", |engine, state| {
            refusal(engine.conditional_probability_of_one(state, 2, &[(0, true)]))
        }),
        ("sample", |engine, state| {
            refusal(engine.sample(state, &[2], 1, 5))
        }),
        ("expectation", |engine, state| {
            refusal(engine.expectation(state, &z(2)))
        }),
    ];
    for (name, call) in calls {
        let mut refused = None;
        let first = first_native_call(&libraries, || refused = Some(call(&engine, &mut state)));
        assert_eq!(
            refused,
            Some(Refusal::SiteOutOfRange { site: 2, sites: 2 }),
            "{name} refuses site 2 of 2"
        );
        assert_eq!(first, None, "{name}");
    }
}

/// The first native call `run` makes, if any.
fn first_native_call(libraries: &FakeLibraries, run: impl FnOnce()) -> Option<&'static str> {
    let before = libraries.events().len();
    run();
    libraries.events().get(before).copied()
}

/// Dropping the states and the engine releases every native object, the
/// engine's stream and handle last, once no state needs them.
#[test]
fn dropping_releases_every_native_object() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    ok(engine.sample(&state, &[0, 1], 2, 1));
    let mut fork = ok(engine.fork(&state));
    // At once: the fork's copies must be done with the parent's tensors.
    drop(state);
    for step in 0..=RESTART_PERIOD {
        ok(engine.apply(&mut fork, &[step % 2], &hadamard()));
        ok(engine.shape(&fork));
    }
    drop(engine);
    assert!(libraries.live_objects() > 2, "the state keeps the session");
    drop(fork);
    assert_eq!(libraries.live_objects(), 0);
}

fn sum(identity: f64, terms: &[(f64, usize, Pauli)]) -> PauliSum {
    let terms = terms
        .iter()
        .map(|&(coefficient, site, pauli)| {
            PauliTerm::new(c(coefficient), vec![(site, pauli)])
                .unwrap_or_else(|error| panic!("one factor is a term: {error}"))
        })
        .collect();
    PauliSum::new(c(identity), terms)
        .unwrap_or_else(|error| panic!("terms on distinct sites form a sum: {error}"))
}

/// ⟨0.5·I + 2·Z₀ − 3·X₁⟩ on |1⟩|+⟩ is 0.5 − 2 − 3: the identity's
/// coefficient is added on the host, and each term weighs by its own.
#[test]
fn an_expectation_adds_the_identity_and_weighs_each_term() {
    let (engine, _libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    let x = [c(0.0), c(1.0), c(1.0), c(0.0)];
    ok(engine.apply(&mut state, &[0], &x));
    ok(engine.apply(&mut state, &[1], &hadamard()));
    let observable = sum(0.5, &[(2.0, 0, Pauli::Z), (-3.0, 1, Pauli::X)]);
    let value = ok(engine.expectation(&state, &observable));
    assert!((value - c(-4.5)).norm() < 1e-12, "{value}");
}

/// Only a condition of probability exactly zero is refused: one of
/// probability 1e-200 is answered.
#[test]
fn a_condition_of_tiny_probability_is_answered() {
    let (engine, _libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &ry(2.0 * 1e-100_f64.asin())));
    ok(engine.apply(&mut state, &[1], &ry(0.7)));
    let given = ok(engine.conditional_probability_of_one(&state, 1, &[(0, true)]));
    let expected = 0.35f64.sin().powi(2);
    assert!((given - expected).abs() < 1e-12, "{given}, not {expected}");
}

/// A projection reuses only the probability read for its own site.
#[test]
fn a_projection_after_reading_another_site_reads_its_own() {
    let (engine, _libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.apply(&mut state, &[0], &ry(0.4)));
    ok(engine.apply(&mut state, &[1], &ry(1.2)));
    ok(engine.probability_of_one(&state, 0));
    let p = ok(engine.project(&mut state, 1, true));
    let expected = 0.6f64.sin().powi(2);
    assert!((p - expected).abs() < 1e-12, "{p}, not {expected}");
}

/// A fork holds the operators applied since the last read: the state is
/// computed before it is copied.
#[test]
fn a_fork_holds_the_operators_applied_since_the_last_read() {
    let (engine, _libraries) = engine();
    let mut state = ok(engine.prepare(&uniform_capacity(2, 2)));
    ok(engine.shape(&state));
    ok(engine.apply(&mut state, &[0], &[c(0.0), c(1.0), c(1.0), c(0.0)]));
    let fork = ok(engine.fork(&state));
    assert!((ok(engine.probability_of_one(&fork, 0)) - 1.0).abs() < 1e-12);
}

/// A fork's copies are freed once its first compute is captured, as any
/// computed MPS once the next is.
#[test]
fn a_forks_copies_are_freed_after_its_first_compute() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    let fork = ok(engine.fork(&state));
    assert_eq!(libraries.live_copies(), 2);
    ok(engine.shape(&fork));
    assert_eq!(libraries.live_copies(), 0);
}

/// A scratch buffer whose replacement could not be attached may still be
/// attached, so it is freed with the state, after the descriptors.
#[test]
fn a_scratch_whose_replacement_failed_is_freed_with_the_state() {
    let (engine, libraries) = engine();
    let mut state = bell(&engine);
    ok(engine.shape(&state));
    libraries.set_workspace_bytes(2 * WORKSPACE_BYTES);
    ok(engine.apply(&mut state, &[0], &hadamard()));
    libraries.fail("set_workspace", libraries.count("set_workspace") + 1);
    assert!(engine.shape(&state).is_err());
    drop(state);
    assert_eq!(
        libraries.live_objects(),
        2,
        "only the stream and the handle"
    );
}

/// The retained weight is the squared norm the library reports, which
/// must be positive, finite and real.
#[test]
fn a_norm_that_is_not_positive_and_real_is_an_invalid_native_result() {
    for norm in [c(0.0), c(-1e-18), c(f64::NAN), Complex64::new(1.0, 1e-6)] {
        let (engine, libraries) = engine();
        let state = bell(&engine);
        libraries.set_squared_norm(norm);
        assert!(
            matches!(
                engine.retained_weight(&state),
                Err(EngineError::InvalidNativeResult { .. })
            ),
            "{norm}"
        );
    }
}

/// Rounding may leave a ratio of expectations above one; a probability is
/// clamped to [0, 1].
#[test]
fn a_probability_is_clamped_to_one() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    libraries.set_squared_norm(c(0.4));
    let p = ok(engine.probability_of_one(&state, 0));
    assert!((p - 1.0).abs() < 1e-15, "{p}");
}

/// A capacity of four qubits with bond 64 asks more than any cut can
/// carry: the library is given each cut's largest bond, 2, 4 and 2, so it
/// sizes no output beyond what the state can reach.
#[test]
fn a_capacity_bond_is_lowered_to_what_its_cut_can_carry() {
    let (engine, libraries) = engine();
    let state = ok(engine.prepare(&uniform_capacity(4, 64)));
    ok(engine.shape(&state));
    assert_eq!(libraries.finalized_bonds(), [vec![2, 4, 2]]);
}

/// A capacity of four qubits with bond 3 asks less than the middle cut
/// can carry: the library is given that bond there, and the edges' 2.
#[test]
fn a_capacity_bond_within_what_its_cut_can_carry_is_kept() {
    let (engine, libraries) = engine();
    let state = ok(engine.prepare(&uniform_capacity(4, 3)));
    ok(engine.shape(&state));
    assert_eq!(libraries.finalized_bonds(), [vec![2, 3, 2]]);
}

/// A library that realized a bond of 2 for a capacity of bond 1 broke the
/// capacity it was given; the engine reports it rather than read a chain
/// it did not ask for.
#[test]
fn a_realized_bond_beyond_the_capacity_is_an_invalid_native_result() {
    let (engine, libraries) = engine();
    let state = ok(engine.prepare(&uniform_capacity(2, 1)));
    libraries.misreport_extents(0, &[2, 2]);
    libraries.misreport_extents(1, &[2, 2]);
    let realized = engine.shape(&state);
    assert!(
        matches!(realized, Err(EngineError::InvalidNativeResult { .. })),
        "{realized:?}"
    );
}

/// The engine's sites are qubits: a library that realized a site of three
/// levels did not compute the state it was given.
#[test]
fn a_realized_site_of_three_levels_is_an_invalid_native_result() {
    let (engine, libraries) = engine();
    let state = ok(engine.prepare(&uniform_capacity(2, 2)));
    libraries.misreport_extents(0, &[3, 1]);
    libraries.misreport_extents(1, &[1, 2]);
    let realized = engine.shape(&state);
    assert!(
        matches!(realized, Err(EngineError::InvalidNativeResult { .. })),
        "{realized:?}"
    );
}

/// ⟨Z₀⟩ of |00⟩ over a squared norm of 1e-320, positive but tiny,
/// overflows: a read that is not finite is the library's fault, not an
/// answer.
#[test]
fn a_normalized_expectation_that_overflows_is_an_invalid_native_result() {
    let (engine, libraries) = engine();
    let state = ok(engine.prepare(&uniform_capacity(2, 2)));
    libraries.set_squared_norm(c(1e-320));
    let value = engine.expectation(&state, &z(0));
    assert!(
        matches!(value, Err(EngineError::InvalidNativeResult { .. })),
        "{value:?}"
    );
}

/// A condition whose weight the library returns as NaN is the library's
/// fault, not a condition of probability zero, so it is not refused as
/// one.
#[test]
fn a_nan_from_the_library_is_an_invalid_native_result_not_a_refusal() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    ok(engine.shape(&state));
    libraries.queue_expectation_values(&[c(f64::NAN)]);
    let given = engine.conditional_probability_of_one(&state, 1, &[(0, true)]);
    assert!(
        matches!(given, Err(EngineError::InvalidNativeResult { .. })),
        "{given:?}"
    );
}

/// The engine expects every compute to ask for scratch, and reports a size
/// of zero as the library's fault rather than compute with none attached.
#[test]
fn a_compute_workspace_of_no_bytes_is_an_invalid_native_result() {
    let (engine, libraries) = engine();
    let state = ok(engine.prepare(&uniform_capacity(2, 2)));
    libraries.set_workspace_bytes(0);
    let realized = engine.shape(&state);
    assert!(
        matches!(realized, Err(EngineError::InvalidNativeResult { .. })),
        "{realized:?}"
    );
}

/// An expectation that asks for a negative scratch size is the library's
/// fault: the engine reports it rather than run the expectation.
#[test]
fn a_negative_expectation_workspace_is_an_invalid_native_result() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    ok(engine.shape(&state));
    libraries.set_workspace_bytes(-1);
    let p = engine.probability_of_one(&state, 0);
    assert!(
        matches!(p, Err(EngineError::InvalidNativeResult { .. })),
        "{p:?}"
    );
}

/// An observable that is a multiple of the identity, 0.7·I, has that
/// coefficient as its expectation on every state, so the engine answers it
/// without a native call; cuTensorNet's documentation does not cover an
/// expectation of an operator with no product.
#[test]
fn an_observable_of_the_identity_alone_makes_no_native_call() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    let identity = PauliSum::new(c(0.7), vec![])
        .unwrap_or_else(|error| panic!("the identity alone is a Pauli sum: {error}"));
    let value = ok(engine.expectation(&state, &identity));
    assert!((value - c(0.7)).norm() < 1e-12, "{value}");
    assert_eq!(libraries.count("create_network_operator"), 0);
    assert_eq!(libraries.count("compute_state"), 0, "nothing computed");
}

/// ⟨Z₀Z₁ + X₀⟩ has three factors but two matrices, Z and X: each is
/// uploaded once and shared by the factors that use it.
#[test]
fn an_expectation_uploads_each_distinct_factor_matrix_once() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    ok(engine.shape(&state));
    let zz = PauliTerm::new(c(1.0), vec![(0, Pauli::Z), (1, Pauli::Z)])
        .unwrap_or_else(|error| panic!("Z on two sites is a term: {error}"));
    let x = PauliTerm::new(c(1.0), vec![(0, Pauli::X)])
        .unwrap_or_else(|error| panic!("one X factor is a term: {error}"));
    let observable = PauliSum::new(c(0.0), vec![zz, x])
        .unwrap_or_else(|error| panic!("two terms form a Pauli sum: {error}"));
    let uploads = libraries.count("copy_to_device");
    ok(engine.expectation(&state, &observable));
    assert_eq!(libraries.count("copy_to_device") - uploads, 2);
}

/// Rounding may leave the ratio of a conditional probability's two
/// expectations outside [0, 1]; it is clamped to the nearest bound.
#[test]
fn a_conditional_probability_is_clamped_to_zero_and_one() {
    for (numerator, expected) in [(0.6, 1.0), (-0.1, 0.0)] {
        let (engine, libraries) = engine();
        let state = bell(&engine);
        ok(engine.shape(&state));
        libraries.queue_expectation_values(&[c(0.5), c(numerator)]);
        let p = ok(engine.conditional_probability_of_one(&state, 1, &[(0, true)]));
        assert!((p - expected).abs() < 1e-15, "{numerator}: {p}");
    }
}

/// Every expectation, whatever read it serves, searches its contraction
/// path with the engine's fixed hyper-samples (`policy.rs`).
#[test]
fn every_expectation_takes_the_engines_hyper_samples() {
    let (engine, libraries) = engine();
    let state = bell(&engine);
    ok(engine.probability_of_one(&state, 0));
    ok(engine.conditional_probability_of_one(&state, 1, &[(0, true)]));
    ok(engine.expectation(&state, &z(1)));
    ok(engine.retained_weight(&state));
    assert_eq!(
        libraries.hyper_samples(),
        [EXPECTATION_HYPER_SAMPLES; 5],
        "one expectation per read, two for the conditional"
    );
}

/// Development check, not for the PR: `tensornet`'s conformance kit on the
/// engine over the double.
#[test]
fn the_engine_conforms_to_the_kit() {
    let (engine, native) = engine();
    let failures = tensornet::conformance::check(&engine);
    assert!(failures.is_empty(), "{failures:#?}");
    drop(engine);
    assert_eq!(native.live_objects(), 0);
}
