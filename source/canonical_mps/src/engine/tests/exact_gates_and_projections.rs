// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine against `tensornet`'s exact reference: every one-qubit gate
//! of the QDK's intrinsic set, alone and controlled, a SWAP, and the
//! engine's projections, on the same entangled states. The engine's
//! conformance circuits and the expectation's tests compare its other reads
//! with the reference. Gates on more than two qubits, allocation, release
//! and drawing values are the simulation method's (the layer above the
//! engine), not the engine's.

use crate::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use num_complex::Complex64;
use std::f64::consts::FRAC_1_SQRT_2;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// The sites of every chain.
const SITES: usize = 5;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// A one-qubit gate of the QDK's intrinsic set.
#[derive(Clone, Copy, Debug)]
enum Gate {
    X,
    Y,
    Z,
    H,
    S,
    SAdj,
    T,
    TAdj,
    Rx(f64),
    Ry(f64),
    Rz(f64),
}

const GATES: [Gate; 11] = [
    Gate::X,
    Gate::Y,
    Gate::Z,
    Gate::H,
    Gate::S,
    Gate::SAdj,
    Gate::T,
    Gate::TAdj,
    Gate::Rx(0.7),
    Gate::Ry(-1.1),
    Gate::Rz(0.4),
];

impl Gate {
    /// The gate's matrix, row-major.
    fn matrix(self) -> [Complex64; 4] {
        let (zero, one) = (c(0.0, 0.0), c(1.0, 0.0));
        let half = c(FRAC_1_SQRT_2, 0.0);
        let rotation = |theta: f64| (c((theta / 2.0).cos(), 0.0), (theta / 2.0).sin());
        match self {
            Self::X => [zero, one, one, zero],
            Self::Y => [zero, c(0.0, -1.0), c(0.0, 1.0), zero],
            Self::Z => [one, zero, zero, -one],
            Self::H => [half, half, half, -half],
            Self::S => [one, zero, zero, c(0.0, 1.0)],
            Self::SAdj => [one, zero, zero, c(0.0, -1.0)],
            Self::T => [one, zero, zero, c(FRAC_1_SQRT_2, FRAC_1_SQRT_2)],
            Self::TAdj => [one, zero, zero, c(FRAC_1_SQRT_2, -FRAC_1_SQRT_2)],
            Self::Rx(theta) => {
                let (cos, sin) = rotation(theta);
                [cos, c(0.0, -sin), c(0.0, -sin), cos]
            }
            Self::Ry(theta) => {
                let (cos, sin) = rotation(theta);
                [cos, c(-sin, 0.0), c(sin, 0.0), cos]
            }
            Self::Rz(theta) => [
                Complex64::from_polar(1.0, -theta / 2.0),
                zero,
                zero,
                Complex64::from_polar(1.0, theta / 2.0),
            ],
        }
    }
}

/// `m` controlled by the first of two sites, row-major.
fn controlled(m: [Complex64; 4]) -> Vec<Complex64> {
    let mut gate = vec![c(0.0, 0.0); 16];
    gate[0] = c(1.0, 0.0);
    gate[5] = c(1.0, 0.0);
    for (at, &x) in m.iter().enumerate() {
        gate[4 * (2 + at / 2) + 2 + at % 2] = x;
    }
    gate
}

/// The engine's state and the exact reference's, on the same circuit.
struct ComparedStates {
    engine: CpuMpsEngine,
    state: CanonicalMpsState<CpuAlgebra>,
    reference: ExactReference,
    exact_state: ReferenceState,
}

impl ComparedStates {
    /// Five qubits entangled by gates on neighbours and on distant sites,
    /// with a complex state on each, so no symmetry hides a wrong gate.
    fn scrambled() -> Self {
        let engine = CpuMpsEngine::new(CpuAlgebra);
        let reference = ExactReference::new();
        let capacity = MpsShape::qubit_chain(SITES, 1 << SITES).expect("a chain");
        let state = engine.prepare(&capacity).expect("prepare");
        let exact_state = reference.prepare(&capacity).expect("prepare");
        let mut states = Self {
            engine,
            state,
            reference,
            exact_state,
        };
        for site in 0..SITES {
            let x = f64::from(u32::try_from(site).expect("a small site"));
            states.apply(&[site], &Gate::Ry(0.3 + 0.37 * x).matrix());
            states.apply(&[site], &Gate::Rz(0.2 + 0.51 * x).matrix());
        }
        for (control, target) in [(0, 1), (3, 1), (2, 4), (4, 0)] {
            states.apply(&[control, target], &controlled(Gate::X.matrix()));
        }
        states.apply(&[2], &Gate::Rx(0.9).matrix());
        states
    }

    /// Applies the gate `matrix` on `sites` to both states.
    fn apply(&mut self, sites: &[usize], matrix: &[Complex64]) {
        self.engine
            .apply(&mut self.state, sites, matrix)
            .expect("a gate");
        self.reference
            .apply(&mut self.exact_state, sites, matrix)
            .expect("a gate");
    }

    /// Every Pauli string's expectation agrees, so the two states are the
    /// same up to a global phase.
    fn assert_same_state(&self, case: &str) {
        self.assert_same_state_on(case, |site| site);
    }

    /// Every Pauli string's expectation on the engine agrees with the
    /// reference's on the same string, each factor moved to
    /// `exact_site(site)`.
    fn assert_same_state_on(&self, case: &str, exact_site: impl Fn(usize) -> usize) {
        for code in 1..4_usize.pow(u32::try_from(SITES).expect("small")) {
            let factors = pauli_factors(code);
            let moved = factors.iter().map(|&(site, p)| (exact_site(site), p));
            let value = self
                .engine
                .expectation(&self.state, &string(factors.clone()))
                .expect("an expectation");
            let expected = self
                .reference
                .expectation(&self.exact_state, &string(moved.collect()))
                .expect("an expectation");
            assert!(
                (value - expected).norm() < 1e-10,
                "{case}, {factors:?}: {value} vs {expected}"
            );
        }
    }
}

/// The Pauli string of `code`, two bits per site.
fn pauli_factors(code: usize) -> Vec<(usize, Pauli)> {
    let paulis = [None, Some(Pauli::X), Some(Pauli::Y), Some(Pauli::Z)];
    (0..SITES)
        .filter_map(|site| paulis[(code >> (2 * site)) % 4].map(|p| (site, p)))
        .collect()
}

/// The observable of one Pauli string, `factors`, with coefficient 1.
fn string(factors: Vec<(usize, Pauli)>) -> PauliSum {
    PauliSum::new(
        c(0.0, 0.0),
        vec![PauliTerm::new(c(1.0, 0.0), factors).expect("a term")],
    )
    .expect("an observable")
}

/// Each one-qubit gate of the intrinsic set on an entangled state, alone on
/// the first, a middle and the last site, and controlled, with controls on
/// either side of the target, on neighbours and further apart; and a
/// controlled phase of 0.8, the control below the target and above it, on
/// distant sites.
#[test]
fn every_gate_alone_or_controlled_gives_the_exact_state() {
    let cases = GATES
        .into_iter()
        .flat_map(|gate| gate_alone(gate).chain(gate_controlled(gate)))
        .chain(controlled_phase(0.8));
    for GateCase {
        label,
        sites,
        matrix,
    } in cases
    {
        let mut states = ComparedStates::scrambled();
        states.apply(&sites, &matrix);
        states.assert_same_state(&label);
    }
}

/// A gate on given sites, and its label in a failure's message.
struct GateCase {
    label: String,
    sites: Vec<usize>,
    matrix: Vec<Complex64>,
}

/// `gate` alone on the first, a middle and the last site.
fn gate_alone(gate: Gate) -> impl Iterator<Item = GateCase> {
    [0, 2, 4].into_iter().map(move |site| GateCase {
        label: format!("{gate:?} on {site}"),
        sites: vec![site],
        matrix: gate.matrix().to_vec(),
    })
}

/// `gate` controlled, the control below and above the target, on
/// neighbours and further apart.
fn gate_controlled(gate: Gate) -> impl Iterator<Item = GateCase> {
    [(0, 3), (4, 1), (2, 3), (3, 2)]
        .into_iter()
        .map(move |(control, target)| GateCase {
            label: format!("controlled {gate:?} on ({control}, {target})"),
            sites: vec![control, target],
            matrix: controlled(gate.matrix()),
        })
}

/// The phase `theta` controlled, the control below and above the target,
/// on distant sites.
fn controlled_phase(theta: f64) -> impl Iterator<Item = GateCase> {
    let phase = [
        c(1.0, 0.0),
        c(0.0, 0.0),
        c(0.0, 0.0),
        Complex64::from_polar(1.0, theta),
    ];
    [(1, 4), (3, 0)]
        .into_iter()
        .map(move |(control, target)| GateCase {
            label: format!("phase on ({control}, {target})"),
            sites: vec![control, target],
            matrix: controlled(phase),
        })
}

/// A SWAP on an entangled state, on distant sites and on neighbours, in
/// either order: each Pauli string then reads what the same string with
/// the two sites exchanged read before, on the reference left unswapped.
#[test]
fn a_swap_exchanges_two_sites() {
    let swap = swap();
    for (a, b) in [(0, 4), (3, 1), (2, 3)] {
        let mut states = ComparedStates::scrambled();
        states
            .engine
            .apply(&mut states.state, &[a, b], &swap)
            .expect("a two-site gate");
        let exchanged = |site| match site {
            s if s == a => b,
            s if s == b => a,
            s => s,
        };
        states.assert_same_state_on(&format!("SWAP on ({a}, {b})"), exchanged);
    }
}

/// The SWAP's matrix, row-major.
fn swap() -> Vec<Complex64> {
    let mut swap = vec![c(0.0, 0.0); 16];
    for (row, col) in [(0, 0), (1, 2), (2, 1), (3, 3)] {
        swap[4 * row + col] = c(1.0, 0.0);
    }
    swap
}

/// Three measurements in turn, each projected on a value: each leaves the
/// engine in the reference's collapsed state.
#[test]
fn a_projection_leaves_the_collapsed_state() {
    let mut states = ComparedStates::scrambled();
    for (site, one) in [(3, true), (0, false), (4, true)] {
        states
            .reference
            .project(&mut states.exact_state, site, one)
            .expect("a possible value");
        states
            .engine
            .project(&mut states.state, site, one)
            .expect("a possible value");
        let weight = states
            .engine
            .retained_weight(&states.state)
            .expect("the weight");
        assert!(
            (weight - 1.0).abs() < 1e-12,
            "precondition: nothing was truncated, so the states compare \
             exactly ({weight})"
        );
        states.assert_same_state(&format!("collapse of {site} on {one}"));
    }
}
