// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the CUDA MPS engine on the real
//! cuTensorNet, on CUDA device 0: its `GateSplit` self-checks, `tensornet`'s
//! conformance kit, and agreement with the CPU engine whose algorithm it
//! ports, on random circuits of seven sites (distant gates, projections,
//! a capped bond), to 1e-10; and, with `with_dev_mpo_far`, operators on
//! three sites applied as an MPO, against the CPU engine's MPO path.
//!
//! It needs an NVIDIA GPU, the CUDA Runtime 12.9 and cuTensorNet 2.13, so
//! every test is ignored by default; run them with `--ignored --nocapture`
//! on such a host (`examples/cuda-mps-dev.sh`'s `kit` part does).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use num_complex::Complex64;
use qdk_cpu_mps::CpuMpsEngine;
use qdk_cutensornet::CudaMpsEngine;
use qdk_cutensornet::cuda_mps_engine::{DeviceTensors, self_check};
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
#[allow(clippy::float_cmp, reason = "no truncation keeps the weight 1 exactly")]
fn gates_below_the_capacity_read_as_on_the_cpu_engine() {
    let capacity = MpsShape::qubit_chain(7, 64).expect("a qubit chain");
    for seed in [1, 2, 3] {
        let operations = circuit(7, 60, seed, false);
        let port = run(&on_the_gpu(), &capacity, &operations);
        let cpu = run(&CpuMpsEngine::default(), &capacity, &operations);
        assert_eq!(port.retained_weight, 1.0, "nothing truncated");
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
