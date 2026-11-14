// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the CUDA MPS engine on the real
//! cuTensorNet, on CUDA device 0: its `GateSplit` self-checks, `tensornet`'s
//! conformance kit, and agreement with the CPU engine whose algorithm it
//! ports, on random circuits of seven sites (distant gates, projections,
//! a capped bond), to 1e-10; and, with `with_dev_mpo_far`, operators on
//! three sites applied as an MPO, against the CPU engine's MPO path; and
//! the `GateSplit` centre moves (`with_dev_qr_moves(false)`; the default
//! moves are QR moves) and the Jacobi SVD (`with_dev_svd_algo`) against
//! the CPU engine. One test shows that cuTensorNet's polar SVD (GESVDP),
//! which the engine refuses, is still defective: it passes while the
//! defect is present.
//!
//! It needs an NVIDIA GPU, the CUDA Runtime 12.9 and cuTensorNet 2.13, so
//! every test is ignored by default; run them with `--ignored --nocapture`
//! on such a host (`examples/cuda-mps-dev.sh`'s `kit` part does).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use num_complex::Complex64;
use qdk_cpu_mps::CpuMpsEngine;
use qdk_cutensornet::CudaMpsEngine;
use qdk_cutensornet::cuda_mps_engine::{DeviceTensors, self_check};
use qdk_cutensornet::dev_gate_split::TensorSvdAlgo;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// The engine on device 0.
fn on_the_gpu() -> CudaMpsEngine<DeviceTensors> {
    let availability = qdk_cutensornet::discover().expect("the audited libraries are installed");
    CudaMpsEngine::new(&availability).expect("the tensor API on CUDA device 0")
}

const TOLERANCE: f64 = 1e-10;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// A seeded stream of numbers, so each circuit is the same on every run.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss, reason = "53 bits of a uniform")]
        let uniform = (z >> 11) as f64 / (1u64 << 53) as f64;
        uniform
    }

    fn angle(&mut self) -> f64 {
        std::f64::consts::TAU * self.next()
    }

    fn below(&mut self, n: usize) -> usize {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a small index from a uniform"
        )]
        let index = (self.next() * n as f64) as usize;
        index.min(n - 1)
    }

    /// A one-site unitary `Rz(φ)·Ry(θ)·Rz(λ)` with a global phase.
    fn one_site(&mut self) -> [Complex64; 4] {
        let (theta, phi, lambda) = (self.angle(), self.angle(), self.angle());
        let (cos, sin) = ((theta / 2.0).cos(), (theta / 2.0).sin());
        [
            c(cos, 0.0),
            -Complex64::from_polar(sin, lambda),
            Complex64::from_polar(sin, phi),
            Complex64::from_polar(cos, phi + lambda),
        ]
    }

    /// A two-site unitary: one-site unitaries around `exp(i·a·Z⊗Z)` and a CX.
    fn two_site(&mut self) -> Vec<Complex64> {
        let a = self.angle();
        let zz = [
            Complex64::from_polar(1.0, a),
            Complex64::from_polar(1.0, -a),
            Complex64::from_polar(1.0, -a),
            Complex64::from_polar(1.0, a),
        ];
        let mut gate = kron(&self.one_site(), &self.one_site());
        for (row, phase) in zz.iter().enumerate() {
            for col in 0..4 {
                gate[row * 4 + col] *= phase;
            }
        }
        let cx = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0,
        ]
        .map(|x| c(x, 0.0));
        let gate = multiply(&cx, &gate);
        multiply(&kron(&self.one_site(), &self.one_site()), &gate)
    }
}

fn kron(a: &[Complex64; 4], b: &[Complex64; 4]) -> Vec<Complex64> {
    let mut product = vec![c(0.0, 0.0); 16];
    for (row, col) in (0..4).flat_map(|row| (0..4).map(move |col| (row, col))) {
        product[row * 4 + col] = a[(row / 2) * 2 + col / 2] * b[(row % 2) * 2 + col % 2];
    }
    product
}

fn multiply(a: &[Complex64], b: &[Complex64]) -> Vec<Complex64> {
    multiply_square(a, b, 4)
}

/// `a·b` of two row-major `dim × dim` matrices.
fn multiply_square(a: &[Complex64], b: &[Complex64], dim: usize) -> Vec<Complex64> {
    let mut product = vec![c(0.0, 0.0); dim * dim];
    for (row, col) in (0..dim).flat_map(|row| (0..dim).map(move |col| (row, col))) {
        product[row * dim + col] = (0..dim).map(|k| a[row * dim + k] * b[k * dim + col]).sum();
    }
    product
}

/// The two-site `gate` on the operands `first` and `second` (positions of
/// three, `first` its most significant bit) as an 8×8 matrix on the three,
/// row-major, position 0 the most significant bit.
fn on_three(gate: &[Complex64], first: usize, second: usize) -> Vec<Complex64> {
    let bit = |x: usize, position: usize| (x >> (2 - position)) & 1;
    let other = 3 - first - second;
    let mut matrix = vec![c(0.0, 0.0); 64];
    for row in 0..8 {
        for col in 0..8 {
            if bit(row, other) == bit(col, other) {
                let r = 2 * bit(row, first) + bit(row, second);
                let k = 2 * bit(col, first) + bit(col, second);
                matrix[row * 8 + col] = gate[r * 4 + k];
            }
        }
    }
    matrix
}

/// A random circuit of fused operators on `sites`: one-site gates, two-site
/// gates on any pair, and three-site operators on any three sites in any
/// order, each the product of two two-site gates sharing a site (a
/// controlled Pauli rotation of weight two fused in order).
fn fused_circuit(sites: usize, operations: usize, seed: u64) -> Vec<Operation> {
    let mut random = SplitMix(seed);
    let mut distinct = |count: usize| {
        let mut chosen = Vec::with_capacity(count);
        while chosen.len() < count {
            let site = random.below(sites);
            if !chosen.contains(&site) {
                chosen.push(site);
            }
        }
        chosen
    };
    let mut kinds = SplitMix(seed ^ 0x5eed);
    let mut gates = SplitMix(seed.wrapping_mul(31));
    (0..operations)
        .map(|_| {
            let kind = kinds.next();
            if kind < 0.2 {
                Operation::Gate(distinct(1), gates.one_site().to_vec())
            } else if kind < 0.55 {
                Operation::Gate(distinct(2), gates.two_site())
            } else {
                let (first, second) = (gates.two_site(), gates.two_site());
                let block = multiply_square(&on_three(&second, 1, 2), &on_three(&first, 0, 1), 8);
                Operation::Gate(distinct(3), block)
            }
        })
        .collect()
}

/// One operation of a circuit.
enum Operation {
    Gate(Vec<usize>, Vec<Complex64>),
    Project(usize, bool),
}

/// A random circuit on `sites`: one-site gates, two-site gates on any pair
/// (neighbours and distant sites), and, when `projections`, a projection
/// every tenth operation.
fn circuit(sites: usize, operations: usize, seed: u64, projections: bool) -> Vec<Operation> {
    let mut random = SplitMix(seed);
    (0..operations)
        .map(|at| {
            if projections && at % 10 == 9 {
                return Operation::Project(random.below(sites), random.next() < 0.5);
            }
            let first = random.below(sites);
            if random.next() < 0.3 {
                return Operation::Gate(vec![first], random.one_site().to_vec());
            }
            let second = (first + 1 + random.below(sites - 1)) % sites;
            Operation::Gate(vec![first, second], random.two_site())
        })
        .collect()
}

/// An observable with terms on one, two and three sites, distant included.
fn observable(sites: usize) -> PauliSum {
    let term = |coefficient, factors| PauliTerm::new(coefficient, factors).expect("a valid term");
    PauliSum::new(
        c(0.25, 0.0),
        vec![
            term(c(0.5, 0.0), vec![(0, Pauli::Z)]),
            term(c(-0.75, 0.0), vec![(sites - 1, Pauli::X), (1, Pauli::Y)]),
            term(c(1.25, 0.0), vec![(2, Pauli::Z), (3, Pauli::Z)]),
            term(
                c(0.5, 0.0),
                vec![(0, Pauli::X), (2, Pauli::Y), (sites - 1, Pauli::Z)],
            ),
            term(c(0.1, 0.0), vec![(1, Pauli::X), (sites - 2, Pauli::X)]),
        ],
    )
    .expect("a valid sum")
}

/// What a run of a circuit reads: each site's P(One), P(One) of the last
/// site given the more likely values of the first two, the observable's expectation, the shape, the
/// retained weight and each projection's probability.
#[derive(Debug)]
struct Reads {
    probabilities: Vec<f64>,
    conditional: f64,
    expectation: Complex64,
    shape: MpsShape,
    retained_weight: f64,
    projections: Vec<f64>,
}

fn run<E: MpsEngine>(engine: &E, capacity: &MpsShape, operations: &[Operation]) -> Reads {
    let mut state = engine.prepare(capacity).expect("a qubit chain is held");
    let mut projections = Vec::new();
    for operation in operations {
        match operation {
            Operation::Gate(sites, matrix) => {
                engine
                    .apply(&mut state, sites, matrix)
                    .expect("a unitary gate applies");
            }
            Operation::Project(site, one) => {
                if let Ok(p) = engine.project(&mut state, *site, *one) {
                    projections.push(p);
                }
            }
        }
    }
    let sites = capacity.site_count();
    let probabilities: Vec<f64> = (0..sites)
        .map(|site| {
            engine
                .probability_of_one(&state, site)
                .expect("a site in range")
        })
        .collect();
    // Each condition's more likely value, so that a projection earlier in
    // the circuit leaves the condition a probability that means something.
    let given = [(0, probabilities[0] >= 0.5), (1, probabilities[1] >= 0.5)];
    Reads {
        conditional: engine
            .conditional_probability_of_one(&state, sites - 1, &given)
            .expect("a random state gives the condition a probability"),
        probabilities,
        expectation: engine
            .expectation(&state, &observable(sites))
            .expect("an observable on the chain"),
        shape: engine.shape(&state).expect("the chain's shape"),
        retained_weight: engine.retained_weight(&state).expect("the retained weight"),
        projections,
    }
}

fn assert_agree(port: &Reads, cpu: &Reads) {
    let close = |a: f64, b: f64| (a - b).abs() <= TOLERANCE * b.abs().max(1.0);
    assert_eq!(port.probabilities.len(), cpu.probabilities.len());
    for (site, (a, b)) in port
        .probabilities
        .iter()
        .zip(&cpu.probabilities)
        .enumerate()
    {
        assert!(close(*a, *b), "P(One) of site {site}: {a} against {b}");
    }
    assert!(
        close(port.conditional, cpu.conditional),
        "conditional: {} against {}",
        port.conditional,
        cpu.conditional
    );
    assert!(
        (port.expectation - cpu.expectation).norm() <= TOLERANCE,
        "expectation: {} against {}",
        port.expectation,
        cpu.expectation
    );
    assert_eq!(port.shape, cpu.shape, "the chain's shape");
    assert!(
        close(port.retained_weight, cpu.retained_weight),
        "retained weight: {} against {}",
        port.retained_weight,
        cpu.retained_weight
    );
    assert_eq!(port.projections.len(), cpu.projections.len());
    for (a, b) in port.projections.iter().zip(&cpu.projections) {
        assert!(close(*a, *b), "projection: {a} against {b}");
    }
}

#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn gate_split_passes_its_self_checks() {
    let availability = qdk_cutensornet::discover().expect("the audited libraries are installed");
    let tensors = DeviceTensors::open(&availability).expect("the tensor API on CUDA device 0");
    let checks = self_check(&tensors);
    for check in &checks {
        println!(
            "SELF_CHECK {} passed={} {}",
            check.name, check.passed, check.detail
        );
    }
    assert!(
        checks.iter().all(|check| check.passed),
        "a GateSplit self-check failed"
    );
}

#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_engine_on_the_gpu_conforms_to_the_kit() {
    let failures = tensornet::conformance::check(&on_the_gpu());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Below the capacity nothing is truncated but the cutoff's values: the
/// port reads what the CPU engine reads, distant gates routed and fused.
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn gates_below_the_capacity_read_as_on_the_cpu_engine() {
    let capacity = MpsShape::qubit_chain(7, 64).expect("a qubit chain");
    for seed in [1, 2, 3] {
        let operations = circuit(7, 60, seed, false);
        let port = run(&on_the_gpu(), &capacity, &operations);
        let cpu = run(&CpuMpsEngine::default(), &capacity, &operations);
        // The cutoff may drop rounding-level singular values (a weight of
        // 1 − 4e-16 on the A100), so the weight is 1 up to rounding.
        assert!(
            port.retained_weight > 1.0 - TOLERANCE,
            "nothing truncated but rounding: {}",
            port.retained_weight
        );
        assert_agree(&port, &cpu);
    }
}

/// A capacity's bond truncates where the CPU engine truncates, and keeps
/// the same weight.
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_capped_bond_truncates_as_on_the_cpu_engine() {
    let capacity = MpsShape::qubit_chain(7, 3).expect("a qubit chain");
    for seed in [4, 5, 6] {
        let operations = circuit(7, 60, seed, false);
        let port = run(&on_the_gpu(), &capacity, &operations);
        let cpu = run(&CpuMpsEngine::default(), &capacity, &operations);
        assert!(port.retained_weight < 1.0, "the cap truncates");
        assert_agree(&port, &cpu);
    }
}

/// Projections between gates normalise as the CPU engine's do, so the
/// gates after them read the same.
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn projections_between_gates_read_as_on_the_cpu_engine() {
    for max_bond in [64, 3] {
        let capacity = MpsShape::qubit_chain(7, max_bond).expect("a qubit chain");
        for seed in [7, 8] {
            let operations = circuit(7, 60, seed, true);
            let port = run(&on_the_gpu(), &capacity, &operations);
            let cpu = run(&CpuMpsEngine::default(), &capacity, &operations);
            assert!(!port.projections.is_empty(), "a projection ran");
            assert_agree(&port, &cpu);
        }
    }
}

/// With `with_dev_mpo_far`, operators on three sites in any order and on
/// distant pairs, applied as an MPO with `GateSplit`s on the device, read
/// as the CPU engine's MPO path reads them, below the capacity and capped.
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn operators_as_an_mpo_read_as_on_the_cpu_engine() {
    for max_bond in [64, 3] {
        let capacity = MpsShape::qubit_chain(7, max_bond).expect("a qubit chain");
        for seed in [9, 10, 11] {
            let operations = fused_circuit(7, 40, seed);
            let port = run(&on_the_gpu().with_dev_mpo_far(true), &capacity, &operations);
            let cpu = run(
                &CpuMpsEngine::default().with_dev_mpo_far(true),
                &capacity,
                &operations,
            );
            assert_agree(&port, &cpu);
        }
    }
}

/// With `with_dev_qr_moves(false)`, every exact centre move a `GateSplit`
/// with no cutoff instead of the default QR move, routed gates,
/// projections and operators as an MPO read as on the CPU engine, below
/// the capacity and capped; the other tests run the default QR moves.
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn gate_split_moves_read_as_on_the_cpu_engine() {
    for max_bond in [64, 3] {
        let capacity = MpsShape::qubit_chain(7, max_bond).expect("a qubit chain");
        for seed in [12, 13] {
            let operations = circuit(7, 60, seed, true);
            let engine = on_the_gpu().with_dev_qr_moves(false);
            let port = run(&engine, &capacity, &operations);
            let cpu = run(&CpuMpsEngine::default(), &capacity, &operations);
            assert_agree(&port, &cpu);
            let counters = engine.counters();
            assert!(counters.centre_moves > 0, "the centre moved");
            assert_eq!(counters.qr_moves, 0, "by GateSplits only");
            let operations = fused_circuit(7, 40, seed);
            let port = run(
                &on_the_gpu().with_dev_mpo_far(true).with_dev_qr_moves(false),
                &capacity,
                &operations,
            );
            let cpu = run(
                &CpuMpsEngine::default().with_dev_mpo_far(true),
                &capacity,
                &operations,
            );
            assert_agree(&port, &cpu);
        }
    }
}

/// With `with_dev_svd_algo(Gesvdj)`, cuSOLVER's Jacobi SVD, the engine
/// reads as the CPU engine below the capacity; GESVDP and GESVDR are
/// refused.
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_jacobi_svd_reads_as_on_the_cpu_engine() {
    let capacity = MpsShape::qubit_chain(7, 64).expect("a qubit chain");
    let engine = on_the_gpu()
        .with_dev_svd_algo(TensorSvdAlgo::Gesvdj)
        .expect("GESVDJ is accepted");
    for seed in [14, 15] {
        let operations = circuit(7, 60, seed, false);
        let port = run(&engine, &capacity, &operations);
        let cpu = run(&CpuMpsEngine::default(), &capacity, &operations);
        assert_agree(&port, &cpu);
    }
    for refused in [TensorSvdAlgo::Gesvdp, TensorSvdAlgo::Gesvdr] {
        assert!(
            on_the_gpu().with_dev_svd_algo(refused).is_err(),
            "{} is refused",
            refused.name()
        );
    }
}

/// The largest difference between two runs' reads, relative as in
/// `assert_agree`; infinite when their shapes or projections differ.
fn disagreement(port: &Reads, cpu: &Reads) -> f64 {
    if port.shape != cpu.shape || port.projections.len() != cpu.projections.len() {
        return f64::INFINITY;
    }
    let relative = |a: f64, b: f64| (a - b).abs() / b.abs().max(1.0);
    port.probabilities
        .iter()
        .zip(&cpu.probabilities)
        .chain([
            (&port.conditional, &cpu.conditional),
            (&port.retained_weight, &cpu.retained_weight),
        ])
        .chain(port.projections.iter().zip(&cpu.projections))
        .map(|(a, b)| relative(*a, *b))
        .fold((port.expectation - cpu.expectation).norm(), f64::max)
}

/// The defect of cuTensorNet's GESVDP that makes the engine refuse it
/// (`DeviceTensors::svd_algo_refusal`) is still present. With GESVDP,
/// reached past the refusal (`set_dev_svd_algo_despite_refusal`), the
/// kit's former GESVDP cases (routed gates with projections, and operators
/// as an MPO, below the capacity and capped), which cuTensorNet 2.13.0
/// failed by 2.2e-7, show at least one of its symptoms: a split's singular
/// values perturbed by more than the engine's 1e-10 (GESVDP's own
/// `errSigma`), reads that differ from the CPU engine's by more than 1e-10,
/// or a failed call. It prints the libraries' versions, the largest
/// perturbation and difference, and the first failure (`GESVDP_DEFECT`).
///
/// It passes while the defect is present. When it fails, the loaded
/// cuTensorNet shows none of the symptoms on these cases: revisit the
/// refusal in `src/cuda_mps_engine/device_tensors.rs`, and before
/// accepting GESVDP again, check that an H2 shot on GESVDP (`h2_dev`) no
/// longer fails with `CUSOLVER_ERROR`, which this test does not reproduce.
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_polar_svd_still_perturbs_rank_deficient_splits() {
    let availability = qdk_cutensornet::discover().expect("the audited libraries are installed");
    let version = |library: &str| {
        availability
            .report()
            .library(library)
            .and_then(|found| found.version(library))
            .map_or_else(|| "unread".to_string(), |value| value.to_string())
    };
    let gesvdp = || {
        let tensors = DeviceTensors::open(&availability).expect("the tensor API on CUDA device 0");
        tensors.set_dev_svd_algo_despite_refusal(TensorSvdAlgo::Gesvdp);
        CudaMpsEngine::with_tensors(tensors)
    };
    let (mut perturbation, mut difference) = (0.0_f64, 0.0_f64);
    let mut failure: Option<String> = None;
    for max_bond in [64, 3] {
        let capacity = MpsShape::qubit_chain(7, max_bond).expect("a qubit chain");
        for seed in [16, 17] {
            for (engine, cpu_engine, operations) in [
                (
                    gesvdp(),
                    CpuMpsEngine::default(),
                    circuit(7, 60, seed, true),
                ),
                (
                    gesvdp().with_dev_mpo_far(true),
                    CpuMpsEngine::default().with_dev_mpo_far(true),
                    fused_circuit(7, 40, seed),
                ),
            ] {
                // A failed call panics in `run`; it is one of the symptoms.
                let port = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&engine, &capacity, &operations)
                }));
                match port {
                    Ok(port) => {
                        let cpu = run(&cpu_engine, &capacity, &operations);
                        difference = difference.max(disagreement(&port, &cpu));
                    }
                    Err(panic) => {
                        let message = panic
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_string()))
                            .unwrap_or_else(|| "a panic".to_string());
                        failure.get_or_insert(message.replace(['\n', '"'], " "));
                    }
                }
                perturbation = perturbation.max(engine.counters().largest_svd_perturbation);
            }
        }
    }
    println!(
        "GESVDP_DEFECT cutensornet_version={} cudart_version={} largest_perturbation={perturbation:.3e} \
         largest_difference={difference:.3e} tolerance={TOLERANCE:.0e} failure=\"{}\"",
        version("cuTensorNet"),
        version("CUDA Runtime"),
        failure.as_deref().unwrap_or("none"),
    );
    assert!(
        perturbation > TOLERANCE || difference > TOLERANCE || failure.is_some(),
        "GESVDP perturbed no split beyond {TOLERANCE:.0e} (largest {perturbation:.3e}), its reads \
         differ from the CPU engine's by {difference:.3e} at most, and no call failed: the defect \
         is gone on this cuTensorNet, so revisit the engine's refusal of GESVDP"
    );
}
