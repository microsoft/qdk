// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the CUDA MPS engine's rules on
//! [`HostTensors`], its tensor calls on the host: the conformance kit,
//! operators applied as an MPO (`with_dev_mpo_far`) against the exact
//! state and against the CPU engine whose MPO path it ports, and the QR
//! moves (`with_dev_qr_moves`) against the `GateSplit` moves and the CPU
//! engine.

mod host_tensors;

use crate::cuda_mps_engine::CudaMpsEngine;
use host_tensors::HostTensors;
use num_complex::Complex64;
use qdk_cpu_mps::CpuMpsEngine;
use tensornet::reference::ExactReference;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

const TOLERANCE: f64 = 1e-10;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

fn on_host() -> CudaMpsEngine<HostTensors> {
    CudaMpsEngine::with_tensors(HostTensors)
}

fn on_host_with_mpo() -> CudaMpsEngine<HostTensors> {
    on_host().with_dev_mpo_far(true)
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

    /// `count` distinct sites of a chain of `sites`, in a random order.
    fn distinct(&mut self, sites: usize, count: usize) -> Vec<usize> {
        let mut chosen = Vec::with_capacity(count);
        while chosen.len() < count {
            let site = self.below(sites);
            if !chosen.contains(&site) {
                chosen.push(site);
            }
        }
        chosen
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
        let gate = multiply(&cx, &gate, 4);
        multiply(&kron(&self.one_site(), &self.one_site()), &gate, 4)
    }
}

fn kron(a: &[Complex64; 4], b: &[Complex64; 4]) -> Vec<Complex64> {
    let mut product = vec![c(0.0, 0.0); 16];
    for (row, col) in (0..4).flat_map(|row| (0..4).map(move |col| (row, col))) {
        product[row * 4 + col] = a[(row / 2) * 2 + col / 2] * b[(row % 2) * 2 + col % 2];
    }
    product
}

/// `a·b` of two row-major `dim × dim` matrices.
fn multiply(a: &[Complex64], b: &[Complex64], dim: usize) -> Vec<Complex64> {
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

/// One gate of a circuit.
struct Gate(Vec<usize>, Vec<Complex64>);

/// A random circuit on `sites`, as the engine under test applies it and as
/// gates on two sites at most applied in turn: one-site gates, two-site
/// gates on any pair (neighbours and distant sites), and three-site
/// operators on any three sites, in any order, each the product of two
/// two-site gates (the second sharing a site with the first, as a
/// controlled Pauli rotation of weight two fused in order).
fn circuit(sites: usize, operations: usize, seed: u64) -> (Vec<Gate>, Vec<Gate>) {
    let mut random = SplitMix(seed);
    let (mut fused, mut in_turn) = (Vec::new(), Vec::new());
    for _ in 0..operations {
        let kind = random.next();
        if kind < 0.2 {
            let site = random.below(sites);
            let gate = random.one_site().to_vec();
            fused.push(Gate(vec![site], gate.clone()));
            in_turn.push(Gate(vec![site], gate));
        } else if kind < 0.55 {
            let pair = random.distinct(sites, 2);
            let gate = random.two_site();
            fused.push(Gate(pair.clone(), gate.clone()));
            in_turn.push(Gate(pair, gate));
        } else {
            let three = random.distinct(sites, 3);
            let (first, second) = (random.two_site(), random.two_site());
            let block = multiply(&on_three(&second, 1, 2), &on_three(&first, 0, 1), 8);
            fused.push(Gate(three.clone(), block));
            in_turn.push(Gate(vec![three[0], three[1]], first));
            in_turn.push(Gate(vec![three[1], three[2]], second));
        }
    }
    (fused, in_turn)
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
/// site given the more likely values of the first two, the observable's
/// expectation, the shape and the retained weight.
#[derive(Debug)]
struct Reads {
    probabilities: Vec<f64>,
    conditional: f64,
    expectation: Complex64,
    shape: MpsShape,
    retained_weight: f64,
}

fn run<E: MpsEngine>(engine: &E, capacity: &MpsShape, gates: &[Gate]) -> Reads
where
    E::Error: std::fmt::Debug,
{
    let mut state = engine.prepare(capacity).expect("a qubit chain is held");
    for Gate(sites, matrix) in gates {
        engine
            .apply(&mut state, sites, matrix)
            .expect("a unitary gate applies");
    }
    let sites = capacity.site_count();
    let probabilities: Vec<f64> = (0..sites)
        .map(|site| {
            engine
                .probability_of_one(&state, site)
                .expect("a site in range")
        })
        .collect();
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
    }
}

fn close_within(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * b.abs().max(1.0)
}

/// The values two runs read agree: P(One), the conditional, the
/// expectation and the retained weight.
fn assert_values_agree(port: &Reads, other: &Reads) {
    assert_values_agree_within(port, other, TOLERANCE);
}

fn assert_values_agree_within(port: &Reads, other: &Reads, tolerance: f64) {
    let close = |a, b| close_within(a, b, tolerance);
    assert_eq!(port.probabilities.len(), other.probabilities.len());
    for (site, (a, b)) in port
        .probabilities
        .iter()
        .zip(&other.probabilities)
        .enumerate()
    {
        assert!(close(*a, *b), "P(One) of site {site}: {a} against {b}");
    }
    assert!(
        close(port.conditional, other.conditional),
        "conditional: {} against {}",
        port.conditional,
        other.conditional
    );
    assert!(
        (port.expectation - other.expectation).norm() <= tolerance,
        "expectation: {} against {}",
        port.expectation,
        other.expectation
    );
    assert!(
        close(port.retained_weight, other.retained_weight),
        "retained weight: {} against {}",
        port.retained_weight,
        other.retained_weight
    );
}

#[test]
fn the_engine_on_host_tensors_conforms_to_the_kit() {
    let failures = tensornet::conformance::check(&on_host());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Below the capacity an operator applied as an MPO, on three sites in any
/// order or on a distant pair, leaves the state its gates applied in turn
/// leave: the exact state's reads.
#[test]
fn operators_as_an_mpo_below_the_capacity_read_as_their_gates_in_turn() {
    let capacity = MpsShape::qubit_chain(7, 64).expect("a qubit chain");
    for seed in [1, 2, 3] {
        let (fused, in_turn) = circuit(7, 40, seed);
        let port = run(&on_host_with_mpo(), &capacity, &fused);
        let exact = run(&ExactReference::new(), &capacity, &in_turn);
        assert!(
            port.retained_weight > 1.0 - TOLERANCE,
            "nothing truncated but rounding"
        );
        assert_values_agree(&port, &exact);
    }
}

/// A capacity's bond truncates an operator's span where the CPU engine's
/// MPO path truncates it, and keeps the same shape and weight.
#[test]
fn a_capped_bond_truncates_operators_as_an_mpo_as_on_the_cpu_engine() {
    let capacity = MpsShape::qubit_chain(7, 3).expect("a qubit chain");
    for seed in [4, 5, 6] {
        let (fused, _) = circuit(7, 40, seed);
        let port = run(&on_host_with_mpo(), &capacity, &fused);
        let cpu = run(
            &CpuMpsEngine::default().with_dev_mpo_far(true),
            &capacity,
            &fused,
        );
        assert!(port.retained_weight < 1.0, "the cap truncates");
        assert_eq!(port.shape, cpu.shape, "the chain's shape");
        assert_values_agree(&port, &cpu);
    }
}

/// Without the flag an operator on three sites is refused, as the
/// contract says.
#[test]
fn without_the_flag_an_operator_on_three_sites_is_refused() {
    let capacity = MpsShape::qubit_chain(5, 8).expect("a qubit chain");
    let (fused, _) = circuit(5, 40, 9);
    let Gate(sites, matrix) = fused
        .iter()
        .find(|gate| gate.0.len() == 3)
        .expect("a three-site operator in 40 operations");
    let engine = on_host();
    let mut state = engine.prepare(&capacity).expect("a qubit chain is held");
    assert!(engine.apply(&mut state, sites, matrix).is_err());
}

/// The tolerance between the QR moves and the `GateSplit` moves: both
/// exact, so rounding apart.
const QR_TOLERANCE: f64 = 1e-12;

/// QR moves leave the state the `GateSplit` moves leave, below the
/// capacity and at a capped bond, through routed pairs and operators as an
/// MPO alike; and with them every centre move is a QR move and every
/// `GateSplit` a truncating split.
#[test]
fn qr_moves_read_as_gate_split_moves() {
    for bond in [64, 3] {
        let capacity = MpsShape::qubit_chain(7, bond).expect("a qubit chain");
        for seed in [11, 12] {
            let (fused, in_turn) = circuit(7, 40, seed);
            for (gates, mpo) in [(&fused, true), (&in_turn, false)] {
                let by_split = on_host().with_dev_mpo_far(mpo);
                let by_qr = on_host().with_dev_mpo_far(mpo).with_dev_qr_moves(true);
                let split_reads = run(&by_split, &capacity, gates);
                let qr_reads = run(&by_qr, &capacity, gates);
                assert_values_agree_within(&qr_reads, &split_reads, QR_TOLERANCE);
                assert_eq!(qr_reads.shape, split_reads.shape, "the chain's shape");
                let counters = by_qr.counters();
                assert!(counters.qr_moves > 0, "the centre moved");
                assert_eq!(counters.qr_moves, counters.centre_moves);
                assert_eq!(counters.gate_splits, counters.splits);
            }
        }
    }
}

/// At a capped bond, QR moves keep the CPU engine's shape and reads, whose
/// moves they port.
#[test]
fn qr_moves_at_a_capped_bond_read_as_the_cpu_engine() {
    let capacity = MpsShape::qubit_chain(7, 3).expect("a qubit chain");
    for seed in [13, 14] {
        let (_, in_turn) = circuit(7, 40, seed);
        let port = run(&on_host().with_dev_qr_moves(true), &capacity, &in_turn);
        let cpu = run(&CpuMpsEngine::default(), &capacity, &in_turn);
        assert!(port.retained_weight < 1.0, "the cap truncates");
        assert_eq!(port.shape, cpu.shape, "the chain's shape");
        assert_values_agree(&port, &cpu);
    }
}

/// On the polyene case's QPE prefix (N = 8, 17 qubits in the identity
/// order, its trial state and the start of its first controlled step), QR
/// moves read as `GateSplit` moves.
#[test]
fn qr_moves_read_as_gate_split_moves_on_the_polyene_prefix() {
    let gates = polyene_prefix(300);
    let capacity = MpsShape::qubit_chain(17, 64).expect("a qubit chain");
    let by_qr = on_host().with_dev_qr_moves(true);
    let qr_reads = run(&by_qr, &capacity, &gates);
    let split_reads = run(&on_host(), &capacity, &gates);
    assert_values_agree_within(&qr_reads, &split_reads, QR_TOLERANCE);
    assert!(by_qr.counters().qr_moves > 0, "the centre moved");
}

/// The polyene N = 8 prefix's trial state, H on the ancilla, and the first
/// `step_gates` gates of its controlled step, from the example's
/// `ppp8.circuit`.
fn polyene_prefix(step_gates: usize) -> Vec<Gate> {
    let text = include_str!("../../examples/polyene_qpe_prefix/ppp8.circuit");
    let (mut gates, mut in_step) = (Vec::new(), false);
    let mut step = 0;
    let real = |x: f64| c(x, 0.0);
    let h = std::f64::consts::FRAC_1_SQRT_2;
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let qubit = |at: usize| -> usize { words[at].parse().expect("a qubit") };
        let angle = || -> f64 { words[1].parse().expect("an angle") };
        let gate = match words.as_slice() {
            [first, ..] if first.starts_with('#') => continue,
            [] | ["trial"] => continue,
            ["controlled_step", _] => {
                in_step = true;
                gates.push(Gate(vec![0], [h, h, h, -h].map(real).to_vec()));
                continue;
            }
            ["cx", ..] => {
                let mut cx = vec![real(0.0); 16];
                for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
                    cx[4 * row + col] = real(1.0);
                }
                Gate(vec![qubit(1), qubit(2)], cx)
            }
            ["h", _] => Gate(vec![qubit(1)], [h, h, h, -h].map(real).to_vec()),
            ["x", _] => Gate(vec![qubit(1)], [0.0, 1.0, 1.0, 0.0].map(real).to_vec()),
            ["s" | "sadj", _] => {
                let phase = if words[0] == "s" { 1.0 } else { -1.0 };
                let zero = real(0.0);
                Gate(vec![qubit(1)], vec![real(1.0), zero, zero, c(0.0, phase)])
            }
            ["rz", _, _] => {
                let half = angle() / 2.0;
                let zero = real(0.0);
                Gate(
                    vec![qubit(2)],
                    vec![
                        Complex64::from_polar(1.0, -half),
                        zero,
                        zero,
                        Complex64::from_polar(1.0, half),
                    ],
                )
            }
            ["ry", _, _] => {
                let (sin, cos) = (angle() / 2.0).sin_cos();
                Gate(vec![qubit(2)], [cos, -sin, sin, cos].map(real).to_vec())
            }
            other => panic!("a gate this test does not read: {other:?}"),
        };
        if in_step {
            if step == step_gates {
                break;
            }
            step += 1;
        }
        gates.push(gate);
    }
    gates
}
