//! Native numerical qualification of MPS execution against a real A100.
//!
//! These are integration tests in everything but location. They need an
//! x86-64 Linux host, the real cuTensorNet and CUDA runtime libraries, and a
//! GPU; they answer "does the simulation still produce the right numbers",
//! which is a different question from "is the FFI surface intact" that
//! `tests/availability.rs` answers. Every one is `#[ignore]`d so a plain
//! `cargo test` stays fast, and `scripts/validate-on-cuda-host.sh
//! --qualification` is what runs them.
//!
//! They live inside the crate rather than under `tests/` because they drive
//! `MpsSession`, `Circuit`, `ExecutionPolicy` and friends directly, and every one
//! of those is crate-private. Moving the file to `tests/` would mean promoting
//! the whole simulation core to the crate's public contract to buy nothing but
//! a different directory.
//!
//! Each case sweeps its parameters from a table pinned in the test body, so
//! running them takes no configuration.

use super::{MpsTarget, convert_layout};
use crate::library::MpsSession;
use crate::simulation::{
    Circuit, Gate, SimulationResult, branch, circuit::StateReadout, policy::ExecutionPolicy,
    query::AdjacentZQuery,
};
use num_complex::Complex64;
use std::{f64::consts::FRAC_1_SQRT_2, sync::Arc, time::Instant};

fn circuit_with_gates(qubit_count: u32, gates: &[Gate]) -> Circuit {
    let mut circuit = Circuit::new(qubit_count).expect("qualification width should be valid");
    for gate in gates {
        circuit
            .push(*gate)
            .expect("qualification gate should be valid");
    }
    circuit
}

fn maximum_amplitude_error(actual: &[Complex64], expected: &[Complex64]) -> f64 {
    assert_eq!(actual.len(), expected.len());
    actual
        .iter()
        .zip(expected)
        .map(|(actual, expected)| (*actual - expected).norm())
        .fold(0.0_f64, f64::max)
}

fn basis_state(width: usize, index: usize) -> Vec<Complex64> {
    let length = 1_usize
        .checked_shl(u32::try_from(width).expect("small qualification width should fit u32"))
        .expect("small qualification state should fit usize");
    let mut state = vec![Complex64::new(0.0, 0.0); length];
    state[index] = Complex64::new(1.0, 0.0);
    state
}

fn b0_qualification_cases() -> Vec<(&'static str, Circuit, Vec<Complex64>)> {
    let mut cases = b0_basis_order_cases();
    cases.extend(b0_rotation_cases());
    cases
}

fn b0_basis_order_cases() -> Vec<(&'static str, Circuit, Vec<Complex64>)> {
    vec![
        (
            "x-q0",
            circuit_with_gates(2, &[Gate::X { target: 0 }]),
            basis_state(2, 1),
        ),
        (
            "x-q1",
            circuit_with_gates(2, &[Gate::X { target: 1 }]),
            basis_state(2, 2),
        ),
        cnot_case("cnot-0-1-active", 0, 0, 1, 3),
        cnot_case("cnot-0-1-inactive", 1, 0, 1, 2),
        cnot_case("cnot-1-0-active", 1, 1, 0, 3),
        cnot_case("cnot-1-0-inactive", 0, 1, 0, 1),
        (
            "six-qubit-ordering",
            circuit_with_gates(
                6,
                &[
                    Gate::X { target: 0 },
                    Gate::X { target: 1 },
                    Gate::X { target: 3 },
                ],
            ),
            basis_state(6, 11),
        ),
    ]
}

fn cnot_case(
    label: &'static str,
    prepared_qubit: u32,
    control: u32,
    target: u32,
    expected_index: usize,
) -> (&'static str, Circuit, Vec<Complex64>) {
    (
        label,
        circuit_with_gates(
            2,
            &[
                Gate::X {
                    target: prepared_qubit,
                },
                Gate::Cnot { control, target },
            ],
        ),
        basis_state(2, expected_index),
    )
}

fn b0_rotation_cases() -> Vec<(&'static str, Circuit, Vec<Complex64>)> {
    let rotation_angle = 0.731;
    let (rotation_sine, rotation_cosine) = (rotation_angle / 2.0_f64).sin_cos();
    let mut rotation_expected = basis_state(2, 0);
    rotation_expected[0] = Complex64::new(rotation_cosine, 0.0);
    rotation_expected[1] = Complex64::new(0.0, -rotation_sine);

    let phase_angle = 0.913;
    let (phase_sine, phase_cosine) = (phase_angle / 2.0_f64).sin_cos();
    let mut phase_expected = basis_state(2, 0);
    phase_expected[0] = Complex64::new(phase_cosine, 0.0);
    phase_expected[1] = Complex64::new(0.0, -phase_sine);

    vec![
        (
            "asymmetric-rx",
            circuit_with_gates(
                2,
                &[Gate::Rx {
                    theta: rotation_angle,
                    target: 0,
                }],
            ),
            rotation_expected,
        ),
        (
            "complex-phase-interference",
            circuit_with_gates(
                2,
                &[
                    Gate::H { target: 0 },
                    Gate::Rz {
                        theta: phase_angle,
                        target: 0,
                    },
                    Gate::H { target: 0 },
                ],
            ),
            phase_expected,
        ),
    ]
}

fn b1_width_circuit(width: u32) -> Circuit {
    let gates = if width == 2 {
        vec![Gate::X { target: 0 }]
    } else if width == 3 {
        vec![Gate::X { target: 0 }, Gate::X { target: 2 }]
    } else {
        vec![
            Gate::X { target: 0 },
            Gate::X { target: width / 3 },
            Gate::X { target: width - 1 },
        ]
    };
    circuit_with_gates(width, &gates)
}

fn validate_b1_width_result(width: usize, result: &SimulationResult, readout: StateReadout) {
    let expected_target = MpsTarget::new(width, 128).expect("B1 target should be valid");
    let expected_target = convert_layout("expected target", &expected_target.extents)
        .expect("target extents should fit usize");
    assert_eq!(result.report.policy, ExecutionPolicy::base_qualification());
    assert_eq!(result.report.target_extents, expected_target);
    assert!(result.report.maximum_bond <= 128);
    assert_eq!(
        result.report.workspace.requested_maximum_bytes,
        68_719_476_736
    );
    assert!(
        result.report.workspace.native_recommended_bytes
            <= result.report.workspace.requested_maximum_bytes
    );
    assert_eq!(
        result.report.workspace.allocated_bytes,
        result.report.workspace.native_recommended_bytes
    );
    if readout == StateReadout::MetadataOnly {
        assert_eq!(result.amplitudes(), None);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn base_profile_a100_qualification() {
    let availability = crate::discover().expect("native libraries should be available");
    let report = availability.report().clone();
    let policy = ExecutionPolicy::bell_regression()
        .validate()
        .expect("Bell policy should be valid");
    let mut session = MpsSession::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");
    let mut circuit = Circuit::new(2).expect("two-qubit fixture should be valid");
    circuit
        .push(Gate::H { target: 0 })
        .expect("Hadamard should be valid");
    circuit
        .push(Gate::Cnot {
            control: 0,
            target: 1,
        })
        .expect("CNOT should be valid");

    let expected = [
        Complex64::new(FRAC_1_SQRT_2, 0.0),
        Complex64::new(0.0, 0.0),
        Complex64::new(0.0, 0.0),
        Complex64::new(FRAC_1_SQRT_2, 0.0),
    ];
    let started = Instant::now();
    let result = match session.simulate(&circuit, StateReadout::FullAmplitudes) {
        Ok(result) => result,
        Err(error) => {
            let elapsed = started.elapsed();
            let cleanup = session.close();
            eprintln!("simulation_error={error}");
            eprintln!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
            eprintln!("cleanup={cleanup:?}");
            panic!("Base Profile replay failed");
        }
    };
    let elapsed = started.elapsed();
    let actual = result
        .amplitudes()
        .expect("full-amplitude readout should return amplitudes");
    let maximum_error = maximum_amplitude_error(actual, &expected);

    println!(
        "cutensornet_library={}",
        report.cutensornet_library.display()
    );
    println!("cudart_library={}", report.cuda_runtime_library.display());
    println!("cutensornet_version={}", report.cutensornet_version);
    println!(
        "cutensornet_cudart_version={}",
        report.cutensornet_cuda_runtime_version
    );
    println!("cudart_version={}", report.cuda_runtime_version);
    println!("cuda_driver_version={}", report.cuda_driver_version);
    println!("device_ordinal={}", session.device_ordinal());
    println!("circuit={circuit:?}");
    println!("basis_index=q0+2*q1");
    println!("matrix_storage=textbook-row-major-null-native-strides");
    println!("expected={expected:?}");
    println!("actual={actual:?}");
    println!("maximum_amplitude_error={maximum_error:.17e}");
    println!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
    let cleanup = session.close();
    println!("cleanup={cleanup:?}");
    cleanup.expect("native session cleanup should succeed");
    assert!(
        maximum_error <= 1.0e-12,
        "maximum error was {maximum_error}"
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b0_a100_ordering_and_gate_qualification() {
    let availability = crate::discover().expect("native libraries should be available");
    let policy = ExecutionPolicy::bell_regression()
        .validate()
        .expect("B0 policy should be valid");
    let mut session = MpsSession::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");
    for (label, circuit, expected) in b0_qualification_cases() {
        let started = Instant::now();
        let result = session
            .simulate(&circuit, StateReadout::FullAmplitudes)
            .unwrap_or_else(|error| panic!("{label} failed: {error}"));
        let elapsed = started.elapsed();
        let error = maximum_amplitude_error(
            result
                .amplitudes()
                .expect("full-amplitude readout should return amplitudes"),
            &expected,
        );
        println!("case={label}");
        println!("circuit={circuit:?}");
        println!("maximum_amplitude_error={error:.17e}");
        println!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
        assert!(error <= 1.0e-12, "{label} maximum error was {error}");
    }

    println!("ordering_qdk_semantic_q0_to_q5=[1,1,0,1,0,0]");
    println!("ordering_nonzero_dense_amplitude_index=11");
    println!("ordering_conventional_binary_q5_to_q0=001011");
    let cleanup = session.close();
    println!("cleanup={cleanup:?}");
    cleanup.expect("native session cleanup should succeed");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b1_a100_width_qualification() {
    let availability = crate::discover().expect("native libraries should be available");
    let policy = ExecutionPolicy::base_qualification();
    let mut session = MpsSession::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");

    for width in [2_u32, 3, 63, 64, 128] {
        let circuit = b1_width_circuit(width);
        let readout = if width <= 3 {
            StateReadout::FullAmplitudes
        } else {
            StateReadout::MetadataOnly
        };
        let started = Instant::now();
        let result = session
            .simulate(&circuit, readout)
            .unwrap_or_else(|error| panic!("width {width} failed: {error}"));
        let elapsed = started.elapsed();
        let width_usize = usize::try_from(width).expect("width should fit usize");
        validate_b1_width_result(width_usize, &result, readout);

        if width <= 3 {
            let expected_index = if width == 2 { 1 } else { 5 };
            let expected = basis_state(width_usize, expected_index);
            let error = maximum_amplitude_error(
                result
                    .amplitudes()
                    .expect("small-width readout should return amplitudes"),
                &expected,
            );
            println!("maximum_amplitude_error={error:.17e}");
            assert!(error <= 1.0e-12, "width {width} error was {error}");
        }

        println!("width={width}");
        println!("readout={readout:?}");
        println!("policy={:?}", result.report.policy);
        println!("target_extents={:?}", result.report.target_extents);
        println!("realized_extents={:?}", result.report.realized_extents);
        println!("maximum_bond={}", result.report.maximum_bond);
        println!("workspace={:?}", result.report.workspace);
        println!("simulation_elapsed_seconds={:.9}", elapsed.as_secs_f64());
    }

    let cleanup = session.close();
    println!("cleanup={cleanup:?}");
    cleanup.expect("native session cleanup should succeed");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b2_a100_trotter_query_qualification() {
    // The three widths pin extensivity: the expectation grows by a constant 2.014_143_502 per
    // four added sites, so a per-site error would move one width without moving the others.
    for (width, expected) in [
        (12, 4.332_869_154_633),
        (16, 6.347_012_657_087),
        (20, 8.361_156_159_877),
    ] {
        run_trotter_query_qualification(
            "B2",
            width,
            8,
            expected,
            1.0e-9,
            ExecutionPolicy::base_qualification(),
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b3_a100_matched_bond_qualification() {
    run_trotter_query_qualification(
        "B3",
        128,
        16,
        60.319_518_034_172_646,
        1.0e-10,
        ExecutionPolicy::b3_matched_bond_qualification(),
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires the pinned CUDA 12.9/cuTensorNet 2.13 A100 environment"]
#[allow(
    clippy::used_underscore_binding,
    reason = "the Phase 2 library guard is intentionally unused outside this internal fixture"
)]
fn b4_a100_convergence_qualification() {
    // Convergence is the claim, so the whole ladder has to run: the expectation must still be
    // moving between caps 32 and 64, and must have stopped moving by 128, where truncation is
    // already cutoff-driven rather than cap-driven. Caps 128 and 256 therefore share a constant.
    for (bond_cap, expected) in [
        (32, 122.350_509_319_616),
        (64, 122.350_509_321_997),
        (128, 122.350_509_322_001),
        (256, 122.350_509_322_001),
    ] {
        run_trotter_query_qualification(
            "B4",
            256,
            16,
            expected,
            1.0e-9,
            ExecutionPolicy::b4_convergence_qualification(bond_cap),
        );
    }
}

#[allow(
    clippy::too_many_lines,
    clippy::used_underscore_binding,
    reason = "the hardware qualification emits one complete traceable cell record and retains the Phase 2 library guard"
)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_trotter_query_qualification(
    stage: &str,
    width: u32,
    steps: u32,
    expected: f64,
    relative_error_limit: f64,
    policy: ExecutionPolicy,
) {
    /// A Pauli-Z expectation is real by construction; anything above this is not round-off.
    const IMAGINARY_PART_LIMIT: f64 = 1.0e-12;

    // Every stage below sweeps at least one parameter, so the stage name alone no longer says
    // which case failed. Name the whole point in the sweep instead.
    let case = format!(
        "{stage} qualification (width {width}, {steps} Trotter steps, bond cap {})",
        policy.bond_cap
    );

    let total_started = Instant::now();

    let discovery_started = Instant::now();
    let availability = crate::discover().expect("native libraries should be available");
    let discovery_seconds = discovery_started.elapsed().as_secs_f64();
    let session_started = Instant::now();
    let mut session = MpsSession::new(Arc::clone(&availability.libraries), policy)
        .expect("native session should be created");
    let session_creation_seconds = session_started.elapsed().as_secs_f64();
    let fixture_started = Instant::now();
    let circuit = Circuit::trotter_domain_wall(width, steps, 0.3)
        .unwrap_or_else(|error| panic!("{case}: Trotter fixture construction failed: {error}"));
    let query = AdjacentZQuery::new(width)
        .unwrap_or_else(|error| panic!("{case}: query construction failed: {error}"));
    let canonical_description = circuit.canonical_description();
    let fixture_construction_seconds = fixture_started.elapsed().as_secs_f64();

    let execution_started = Instant::now();
    let result = session
        .simulate_and_query(&circuit, &query)
        .unwrap_or_else(|error| panic!("{case} failed: {error}"));
    let execution_seconds = execution_started.elapsed().as_secs_f64();
    let host_validation_started = Instant::now();
    let normalized = result.query.normalized_expectation;
    let relative_error = ((normalized.re - expected) / expected).abs();
    assert!(
        normalized.im.abs() <= IMAGINARY_PART_LIMIT,
        "{case}: the query is a sum of Hermitian Pauli-Z terms, so its expectation must be real, \
         but the normalized expectation {normalized:?} carries an imaginary part of magnitude \
         {:.3e}, above the {IMAGINARY_PART_LIMIT:.3e} allowed for double-precision round-off. \
         A non-zero imaginary part means the contracted state is no longer a valid wavefunction \
         or the query operator was assembled incorrectly.",
        normalized.im.abs()
    );
    assert!(
        relative_error <= relative_error_limit,
        "{case}: the normalized expectation should reproduce the pinned reference value \
         {expected:.15} to a relative error of at most {relative_error_limit:.3e}, but measured \
         {:.15}, a relative error of {relative_error:.3e} ({:.1}x the limit). Either the \
         truncation policy no longer converges to the reference, or the state evolution changed.",
        normalized.re,
        relative_error / relative_error_limit
    );
    let host_validation_seconds = host_validation_started.elapsed().as_secs_f64();
    let session_cleanup_started = Instant::now();
    let cleanup = session.close();
    let session_cleanup_seconds = session_cleanup_started.elapsed().as_secs_f64();
    cleanup.expect("native session cleanup should succeed");

    println!("stage={stage}");
    println!("width={width}");
    println!("steps={steps}");
    println!("operation_count={}", circuit.gates().len());
    println!("canonical_circuit={canonical_description}");
    println!("query_term_count={}", query.terms.len());
    println!("expected_query={expected:.15}");
    println!("raw_expectation={:?}", result.query.raw_expectation);
    println!("squared_norm={:?}", result.query.squared_norm);
    println!("normalized_expectation={normalized:?}");
    println!("relative_error={relative_error:.17e}");
    println!("relative_error_limit={relative_error_limit:.17e}");
    println!("hyper_samples={}", result.query.hyper_samples);
    println!("policy={:?}", result.state.report.policy);
    println!("target_extents={:?}", result.state.report.target_extents);
    println!(
        "realized_extents={:?}",
        result.state.report.realized_extents
    );
    println!("maximum_bond={}", result.state.report.maximum_bond);
    println!("state_workspace={:?}", result.state.report.workspace);
    println!("query_workspace={:?}", result.query.workspace);
    println!("state_timings={:?}", result.state.report.timings);
    println!("query_timings={:?}", result.query.timings);
    println!("discovery_seconds={discovery_seconds:.9}");
    println!("session_creation_seconds={session_creation_seconds:.9}");
    println!("fixture_construction_seconds={fixture_construction_seconds:.9}");
    println!("execution_seconds={execution_seconds:.9}");
    println!(
        "through_query_completion_seconds={:.9}",
        result.through_query_completion_seconds
    );
    println!("host_validation_seconds={host_validation_seconds:.9}");
    println!(
        "replay_cleanup_seconds={:.9}",
        result.replay_cleanup_seconds
    );
    println!("session_cleanup_seconds={session_cleanup_seconds:.9}");
    println!(
        "total_process_test_seconds={:.9}",
        total_started.elapsed().as_secs_f64()
    );
    println!("cleanup=Ok(())");
}

fn apply_sparse_circuit(simulator: &mut qdk_simulators::SparseStateSim, circuit: &Circuit) {
    for gate in circuit.gates() {
        match *gate {
            Gate::X { target } => simulator.x(target as usize),
            Gate::H { target } => simulator.h(target as usize),
            Gate::Rx { theta, target } => simulator.rx(theta, target as usize),
            Gate::Rz { theta, target } => simulator.rz(theta, target as usize),
            Gate::Cnot { control, target } => {
                simulator.mcx(&[control as usize], target as usize);
            }
            Gate::Rzz { theta, q1, q2 } => {
                // Rzz(theta) = CNOT(q1, q2) . (I (x) Rz(theta)) . CNOT(q1, q2)
                let (q1, q2) = (q1 as usize, q2 as usize);
                simulator.mcx(&[q1], q2);
                simulator.rz(theta, q2);
                simulator.mcx(&[q1], q2);
            }
        }
    }
}

fn sparse_dense_state(
    simulator: &mut qdk_simulators::SparseStateSim,
    expected_width: usize,
) -> Vec<Complex64> {
    let (state, width) = simulator.get_state();
    assert_eq!(width, expected_width);
    let mut dense = vec![Complex64::new(0.0, 0.0); 1 << width];
    for (basis, amplitude) in state {
        let digits = basis.to_u64_digits();
        let index = match digits.as_slice() {
            [] => 0,
            [low] => usize::try_from(*low).expect("small basis index should fit usize"),
            _ => panic!("small basis index should fit one u64 digit"),
        };
        dense[index] = amplitude;
    }
    dense
}

fn maximum_global_phase_error(actual: &[Complex64], expected: &[Complex64]) -> f64 {
    assert_eq!(actual.len(), expected.len());
    let Some((actual_reference, expected_reference)) = actual
        .iter()
        .zip(expected)
        .find(|(actual, expected)| actual.norm() > 1.0e-14 && expected.norm() > 1.0e-14)
    else {
        return maximum_amplitude_error(actual, expected);
    };
    let phase = *actual_reference / *expected_reference;
    let phase = phase / phase.norm();
    actual
        .iter()
        .zip(expected)
        .map(|(actual, expected)| (*actual - phase * *expected).norm())
        .fold(0.0_f64, f64::max)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires a CUDA 12.9/cuTensorNet 2.13 GPU environment"]
#[allow(
    clippy::too_many_lines,
    clippy::used_underscore_binding,
    reason = "the private qualification keeps one readable end-to-end evidence transaction"
)]
fn b5_branch_capture_and_continuation_matches_qdk_sparse_oracle() {
    use branch::{BranchRequest, SelectedBranch};
    use qdk_simulators::SparseStateSim;

    /// Three qubits held exactly: every quantity below is analytic up to double round-off.
    const ORACLE_LIMIT: f64 = 1.0e-12;

    let theta = 0.7;
    let initial = circuit_with_gates(
        3,
        &[
            Gate::Rx { theta, target: 0 },
            Gate::Cnot {
                control: 0,
                target: 1,
            },
            Gate::Cnot {
                control: 1,
                target: 2,
            },
        ],
    );
    let continuation = circuit_with_gates(
        3,
        &[
            Gate::H { target: 2 },
            Gate::Cnot {
                control: 2,
                target: 1,
            },
            Gate::Rz {
                theta: 0.41,
                target: 0,
            },
            Gate::Cnot {
                control: 1,
                target: 0,
            },
        ],
    );
    let query = AdjacentZQuery::new(3).expect("three-qubit Query should be valid");
    let availability = crate::discover().expect("native libraries should be available");
    let policy = ExecutionPolicy::bell_regression();

    // Both outcomes have to run: they take different native paths (different projected masses,
    // a different projector, and a different renormalization), so passing on one says nothing
    // about the other.
    for selected in [SelectedBranch::Zero, SelectedBranch::One] {
        let label = match selected {
            SelectedBranch::Zero => "zero",
            SelectedBranch::One => "one",
        };
        let case = format!("B5 qualification (forced branch {label})");

        let mut oracle = SparseStateSim::new(None);
        for expected in 0..3 {
            let allocated = oracle.allocate();
            assert_eq!(
                allocated, expected,
                "{case}: the oracle must hand out qubits 0, 1, 2 in order for its basis indices \
                 to line up with the native chain, but allocation {expected} returned {allocated}"
            );
        }
        apply_sparse_circuit(&mut oracle, &initial);
        let expected_initial = sparse_dense_state(&mut oracle, 3);
        let expected_norm = expected_initial
            .iter()
            .map(Complex64::norm_sqr)
            .sum::<f64>();
        let expected_q1 = expected_initial
            .iter()
            .enumerate()
            .filter(|(basis, _)| basis & 1 != 0)
            .map(|(_, amplitude)| amplitude.norm_sqr())
            .sum::<f64>();
        let expected_q0 = expected_norm - expected_q1;
        let expected_q0_analytic = (theta / 2.0_f64).cos().powi(2);
        let expected_q1_analytic = (theta / 2.0_f64).sin().powi(2);
        assert!(
            (expected_q0 - expected_q0_analytic).abs() <= ORACLE_LIMIT,
            "{case}: the oracle is the reference, so it must itself be right before anything is \
             compared against it. After Rx({theta}) on qubit 0 the |0> mass is cos^2(theta/2) = \
             {expected_q0_analytic:.17e}, but the sparse oracle reports {expected_q0:.17e}, off \
             by {:.3e} (limit {ORACLE_LIMIT:.3e}).",
            (expected_q0 - expected_q0_analytic).abs()
        );
        assert!(
            (expected_q1 - expected_q1_analytic).abs() <= ORACLE_LIMIT,
            "{case}: the oracle is the reference, so it must itself be right before anything is \
             compared against it. After Rx({theta}) on qubit 0 the |1> mass is sin^2(theta/2) = \
             {expected_q1_analytic:.17e}, but the sparse oracle reports {expected_q1:.17e}, off \
             by {:.3e} (limit {ORACLE_LIMIT:.3e}).",
            (expected_q1 - expected_q1_analytic).abs()
        );
        assert!(
            expected_q0 > expected_q1 && expected_q1 > 0.0,
            "{case}: the fixture only exercises branch selection if both outcomes are reachable \
             and unequal, so that forcing one is a real choice. With theta = {theta} the masses \
             should satisfy q0 > q1 > 0, but the oracle reports q0 = {expected_q0:.17e} and \
             q1 = {expected_q1:.17e}."
        );
        let forced_probability = oracle.force_collapse(selected == SelectedBranch::One, 0);
        let expected_post_projection = sparse_dense_state(&mut oracle, 3);
        let post_projection_norm = expected_post_projection
            .iter()
            .map(Complex64::norm_sqr)
            .sum::<f64>();
        assert!(
            (post_projection_norm - 1.0).abs() <= ORACLE_LIMIT,
            "{case}: projection must renormalize, so the oracle state after forcing the branch \
             should have unit squared norm, but it has {post_projection_norm:.17e}, off by \
             {:.3e} (limit {ORACLE_LIMIT:.3e}).",
            (post_projection_norm - 1.0).abs()
        );
        apply_sparse_circuit(&mut oracle, &continuation);
        let expected_final = sparse_dense_state(&mut oracle, 3);
        let expected_query = (0..2)
            .map(|left| 1.0 - 2.0 * oracle.joint_probability(&[left, left + 1]))
            .sum::<f64>();

        let mut session = MpsSession::new(Arc::clone(&availability.libraries), policy)
            .unwrap_or_else(|error| panic!("{case}: native session creation failed: {error}"));
        let result = session
            .simulate_with_branch(
                &initial,
                BranchRequest { mode: 0, selected },
                &continuation,
                &query,
            )
            .unwrap_or_else(|error| panic!("{case}: branch replay failed: {error}"));
        session
            .close()
            .unwrap_or_else(|error| panic!("{case}: native session cleanup failed: {error}"));

        let tolerance = 1.0e-12;
        let agrees = |quantity: &str, native: f64, oracle: f64, why: &str| {
            assert!(
                (native - oracle).abs() <= tolerance,
                "{case}: {quantity} must match the sparse-state oracle because {why}. The native \
                 run reports {native:.17e} against the oracle value {oracle:.17e}, a difference \
                 of {:.3e} (limit {tolerance:.3e}).",
                (native - oracle).abs()
            );
        };

        agrees(
            "the total squared norm before projection",
            result.report.masses.norm,
            expected_norm,
            "the initial circuit is unitary and the bond cap is wide enough to hold the state \
             exactly",
        );
        agrees(
            "the |0> mass on the measured qubit",
            result.report.masses.q0,
            expected_q0,
            "the native reduced density is computed from the same state the oracle holds",
        );
        agrees(
            "the |1> mass on the measured qubit",
            result.report.masses.q1,
            expected_q1,
            "the native reduced density is computed from the same state the oracle holds",
        );
        agrees(
            "the sum of the normalized branch probabilities",
            result.report.masses.p0 + result.report.masses.p1,
            1.0,
            "the two outcomes are exhaustive and mutually exclusive",
        );
        agrees(
            "the probability reported for the forced branch",
            result.report.probability,
            forced_probability,
            "forcing an outcome must still report the probability that outcome actually had",
        );
        agrees(
            "the log probability reported for the forced branch",
            result.report.log_probability,
            forced_probability.ln(),
            "the log probability is the logarithm of the reported probability",
        );

        let initial_error = maximum_global_phase_error(
            result
                .initial_state
                .amplitudes()
                .expect("initial amplitudes should be retained"),
            &expected_initial,
        );
        let projection_error = maximum_global_phase_error(
            result
                .post_projection_state
                .amplitudes()
                .expect("post-projection amplitudes should be retained"),
            &expected_post_projection,
        );
        let continuation_error = maximum_global_phase_error(
            result
                .continuation_state
                .amplitudes()
                .expect("continuation amplitudes should be retained"),
            &expected_final,
        );
        let matches_state = |stage: &str, error: f64| {
            assert!(
                error <= tolerance,
                "{case}: the captured {stage} state must equal the oracle state up to a global \
                 phase, which is the only freedom a wavefunction has. The largest per-amplitude \
                 deviation after removing that phase is {error:.3e}, above the {tolerance:.3e} \
                 expected from double-precision round-off."
            );
        };

        matches_state("initial", initial_error);
        matches_state("post-projection", projection_error);
        matches_state("continuation", continuation_error);

        agrees(
            "the squared norm of the continuation state",
            result.query.squared_norm.re,
            1.0,
            "projection renormalizes and the continuation circuit is unitary",
        );
        agrees(
            "the raw query expectation",
            result.query.raw_expectation.re,
            expected_query,
            "the state is already normalized, so raw and normalized expectations coincide",
        );
        agrees(
            "the normalized query expectation",
            result.query.normalized_expectation.re,
            expected_query,
            "the adjacent-Z query is evaluated on the same continuation state the oracle holds",
        );

        println!("b5_branch={label}");
        println!("b5_expected_norm={expected_norm:.17e}");
        println!("b5_native_norm={:.17e}", result.report.masses.norm);
        println!("b5_expected_q0={expected_q0:.17e}");
        println!("b5_native_q0={:.17e}", result.report.masses.q0);
        println!("b5_expected_q1={expected_q1:.17e}");
        println!("b5_native_q1={:.17e}", result.report.masses.q1);
        println!("b5_native_p0={:.17e}", result.report.masses.p0);
        println!("b5_native_p1={:.17e}", result.report.masses.p1);
        println!("b5_selected_probability={:.17e}", result.report.probability);
        println!(
            "b5_selected_log_probability={:.17e}",
            result.report.log_probability
        );
        println!("b5_initial_max_phase_error={initial_error:.17e}");
        println!("b5_projection_max_phase_error={projection_error:.17e}");
        println!("b5_continuation_max_phase_error={continuation_error:.17e}");
        println!("b5_expected_query={expected_query:.17e}");
        println!(
            "b5_native_raw_query={:.17e}",
            result.query.raw_expectation.re
        );
        println!(
            "b5_native_normalized_query={:.17e}",
            result.query.normalized_expectation.re
        );
        println!("b5_query_norm={:.17e}", result.query.squared_norm.re);
        println!(
            "b5_initial_execution_seconds={:.17e}",
            result.report.timings.initial_execution_seconds
        );
        println!(
            "b5_first_barrier_synchronization_seconds={:.17e}",
            result.report.timings.first_barrier_synchronization_seconds
        );
        println!(
            "b5_first_capture_seconds={:.17e}",
            result.report.timings.first_capture_seconds
        );
        println!(
            "b5_mass_computation_seconds={:.17e}",
            result.report.timings.mass_computation_seconds
        );
        println!(
            "b5_mass_synchronization_seconds={:.17e}",
            result.report.timings.mass_synchronization_seconds
        );
        println!(
            "b5_projection_registration_seconds={:.17e}",
            result.report.timings.projection_registration_seconds
        );
        println!(
            "b5_projection_preparation_compute_seconds={:.17e}",
            result.report.timings.projection_preparation_compute_seconds
        );
        println!(
            "b5_projection_barrier_synchronization_seconds={:.17e}",
            result
                .report
                .timings
                .projection_barrier_synchronization_seconds
        );
        println!(
            "b5_projection_capture_seconds={:.17e}",
            result.report.timings.projection_capture_seconds
        );
        println!(
            "b5_continuation_registration_seconds={:.17e}",
            result.report.timings.continuation_registration_seconds
        );
        println!(
            "b5_continuation_preparation_compute_seconds={:.17e}",
            result
                .report
                .timings
                .continuation_preparation_compute_seconds
        );
        println!(
            "b5_continuation_barrier_synchronization_seconds={:.17e}",
            result
                .report
                .timings
                .continuation_barrier_synchronization_seconds
        );
        println!(
            "b5_branch_query_seconds={:.17e}",
            result.report.timings.query_seconds
        );
        println!(
            "b5_branch_cleanup_seconds={:.17e}",
            result.report.timings.cleanup_seconds
        );
        println!(
            "b5_branch_total_wall_seconds={:.17e}",
            result.report.timings.total_wall_seconds
        );
        println!(
            "b5_query_construction_seconds={:.17e}",
            result.query.timings.construction_seconds
        );
        println!(
            "b5_query_preparation_path_planning_seconds={:.17e}",
            result.query.timings.preparation_path_planning_seconds
        );
        println!(
            "b5_query_workspace_allocation_attachment_seconds={:.17e}",
            result.query.timings.workspace_allocation_attachment_seconds
        );
        println!(
            "b5_query_compute_call_seconds={:.17e}",
            result.query.timings.compute_call_seconds
        );
        println!(
            "b5_query_synchronization_seconds={:.17e}",
            result.query.timings.synchronization_seconds
        );
        println!(
            "b5_query_output_validation_seconds={:.17e}",
            result.query.timings.output_validation_seconds
        );
        println!(
            "b5_initial_workspace={:?}",
            result.initial_state.report.workspace
        );
        println!(
            "b5_projection_workspace={:?}",
            result.post_projection_state.report.workspace
        );
        println!(
            "b5_continuation_workspace={:?}",
            result.continuation_state.report.workspace
        );
        println!("b5_query_workspace={:?}", result.query.workspace);
        println!("b5_session_cleanup=ok");
    }
}
