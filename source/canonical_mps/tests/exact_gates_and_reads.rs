// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine against `tensornet`'s exact reference: every one-qubit gate
//! of the QDK's intrinsic set, alone and controlled, and the engine's
//! probabilities, Z strings and projections, on the same entangled
//! states. Gates on more than two qubits, allocation, release and drawing
//! values are the simulation method's (the layer above the engine), not
//! the engine's.

use canonical_mps::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use num_complex::Complex64;
use std::f64::consts::FRAC_1_SQRT_2;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

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
struct Both {
    engine: CpuMpsEngine,
    state: CanonicalMpsState<CpuAlgebra>,
    reference: ExactReference,
    exact_state: ReferenceState,
}

impl Both {
    /// Five qubits entangled by gates on neighbours and on distant sites,
    /// with a complex state on each, so no symmetry hides a wrong gate.
    fn scrambled() -> Self {
        let engine = CpuMpsEngine::new(CpuAlgebra);
        let reference = ExactReference::new();
        let capacity = MpsShape::qubit_chain(SITES, 1 << SITES).expect("a chain");
        let state = engine.prepare(&capacity).expect("prepare");
        let exact_state = reference.prepare(&capacity).expect("prepare");
        let mut both = Self {
            engine,
            state,
            reference,
            exact_state,
        };
        for site in 0..SITES {
            let x = f64::from(u32::try_from(site).expect("a small site"));
            both.apply(&[site], &Gate::Ry(0.3 + 0.37 * x).matrix());
            both.apply(&[site], &Gate::Rz(0.2 + 0.51 * x).matrix());
        }
        for (control, target) in [(0, 1), (3, 1), (2, 4), (4, 0)] {
            both.apply(&[control, target], &controlled(Gate::X.matrix()));
        }
        both.apply(&[2], &Gate::Rx(0.9).matrix());
        both
    }

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

/// Each one-qubit gate on an entangled state, on the first, a middle and
/// the last site.
#[test]
fn one_qubit_gates_give_the_exact_state() {
    for gate in GATES {
        for site in [0, 2, 4] {
            let mut both = Both::scrambled();
            both.apply(&[site], &gate.matrix());
            both.assert_same_state(&format!("{gate:?} on {site}"));
        }
    }
}

/// Each one-qubit gate, controlled, on an entangled state: controls on
/// either side of the target, on neighbours and further apart.
#[test]
fn controlled_gates_give_the_exact_state() {
    for gate in GATES {
        for (control, target) in [(0, 3), (4, 1), (2, 3), (3, 2)] {
            let mut both = Both::scrambled();
            both.apply(&[control, target], &controlled(gate.matrix()));
            both.assert_same_state(&format!("controlled {gate:?} on ({control}, {target})"));
        }
    }
}

/// A controlled phase of 0.8 on an entangled state, the control below the
/// target and above it, on distant sites.
#[test]
fn a_controlled_phase_gives_the_exact_state() {
    let phase = Complex64::from_polar(1.0, 0.8);
    let one = Complex64::new(1.0, 0.0);
    for (control, target) in [(1, 4), (3, 0)] {
        let mut both = Both::scrambled();
        let matrix = controlled([one, c(0.0, 0.0), c(0.0, 0.0), phase]);
        both.apply(&[control, target], &matrix);
        both.assert_same_state(&format!("phase on ({control}, {target})"));
    }
}

/// A SWAP on an entangled state, on distant sites and on neighbours, in
/// either order: each Pauli string then reads what the same string with
/// the two sites exchanged read before, on the reference left unswapped.
#[test]
fn a_swap_exchanges_two_sites() {
    let mut swap = vec![c(0.0, 0.0); 16];
    for (row, col) in [(0, 0), (1, 2), (2, 1), (3, 3)] {
        swap[4 * row + col] = c(1.0, 0.0);
    }
    for (a, b) in [(0, 4), (3, 1), (2, 3)] {
        let mut both = Both::scrambled();
        both.engine
            .apply(&mut both.state, &[a, b], &swap)
            .expect("a two-site gate");
        let exchanged = |site| match site {
            s if s == a => b,
            s if s == b => a,
            s => s,
        };
        both.assert_same_state_on(&format!("SWAP on ({a}, {b})"), exchanged);
    }
}

/// P(One) of every site of an entangled state.
#[test]
fn probabilities_of_one_are_exact() {
    let both = Both::scrambled();
    for site in 0..SITES {
        let p = both
            .engine
            .probability_of_one(&both.state, site)
            .expect("P(One)");
        let expected = both
            .reference
            .probability_of_one(&both.exact_state, site)
            .expect("P(One)");
        assert!(
            (p - expected).abs() < 1e-12,
            "site {site}: {p} vs {expected}"
        );
    }
}

/// Z strings on two to four sites of an entangled state, their factors in
/// any order.
#[test]
fn z_string_expectations_are_exact() {
    let both = Both::scrambled();
    for sites in [&[0, 4][..], &[1, 2, 3], &[4, 0, 2, 1]] {
        let observable = string(sites.iter().map(|&site| (site, Pauli::Z)).collect());
        let z = both
            .engine
            .expectation(&both.state, &observable)
            .expect("an expectation");
        let expected = both
            .reference
            .expectation(&both.exact_state, &observable)
            .expect("an expectation");
        assert!(
            (z - expected).norm() < 1e-12,
            "{sites:?}: {z} vs {expected}"
        );
    }
}

/// Three measurements in turn, each projected on a value: each projection
/// returns the probability the reference's projection returns.
#[test]
fn a_projection_returns_the_exact_probability() {
    let mut both = Both::scrambled();
    for (site, one) in [(3, true), (0, false), (4, true)] {
        let probability = both
            .reference
            .project(&mut both.exact_state, site, one)
            .expect("a possible value");
        let p = both
            .engine
            .project(&mut both.state, site, one)
            .expect("a possible value");
        assert!((p - probability).abs() < 1e-12, "{p} vs {probability}");
    }
}

/// Three measurements in turn, each projected on a value: each leaves the
/// engine in the reference's collapsed state.
#[test]
fn a_projection_leaves_the_collapsed_state() {
    let mut both = Both::scrambled();
    for (site, one) in [(3, true), (0, false), (4, true)] {
        both.reference
            .project(&mut both.exact_state, site, one)
            .expect("a possible value");
        both.engine
            .project(&mut both.state, site, one)
            .expect("a possible value");
        let weight = both
            .engine
            .retained_weight(&both.state)
            .expect("the weight");
        assert!(
            (weight - 1.0).abs() < 1e-12,
            "precondition: nothing was truncated, so the states compare \
             exactly ({weight})"
        );
        both.assert_same_state(&format!("collapse of {site} on {one}"));
    }
}
