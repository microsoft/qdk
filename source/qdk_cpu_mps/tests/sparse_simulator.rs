// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine against the QDK's sparse simulator: every one- and two-qubit
//! gate it applies, its probabilities, parities and forced collapses, on
//! the same states. Gates on more than two qubits, allocation, release and
//! drawing values are the method's, not the engine's.

use num_complex::Complex64;
use qdk_cpu_mps::{CanonicalMps, CpuMpsEngine};
use qdk_simulators::SparseStateSim;
use std::f64::consts::FRAC_1_SQRT_2;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

const SITES: usize = 5;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// A one-qubit gate of the sparse simulator.
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

    fn on_sparse(self, sim: &mut SparseStateSim, target: usize) {
        match self {
            Self::X => sim.x(target),
            Self::Y => sim.y(target),
            Self::Z => sim.z(target),
            Self::H => sim.h(target),
            Self::S => sim.s(target),
            Self::SAdj => sim.sadj(target),
            Self::T => sim.t(target),
            Self::TAdj => sim.tadj(target),
            Self::Rx(theta) => sim.rx(theta, target),
            Self::Ry(theta) => sim.ry(theta, target),
            Self::Rz(theta) => sim.rz(theta, target),
        }
    }

    fn controlled_on_sparse(self, sim: &mut SparseStateSim, control: usize, target: usize) {
        let controls = &[control];
        match self {
            Self::X => sim.mcx(controls, target),
            Self::Y => sim.mcy(controls, target),
            Self::Z => sim.mcz(controls, target),
            Self::H => sim.mch(controls, target),
            Self::S => sim.mcs(controls, target),
            Self::SAdj => sim.mcsadj(controls, target),
            Self::T => sim.mct(controls, target),
            Self::TAdj => sim.mctadj(controls, target),
            Self::Rx(theta) => sim.mcrx(controls, theta, target),
            Self::Ry(theta) => sim.mcry(controls, theta, target),
            Self::Rz(theta) => sim.mcrz(controls, theta, target),
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

/// The engine's state and the sparse simulator's, on the same circuit.
struct Both {
    engine: CpuMpsEngine,
    state: CanonicalMps,
    sim: SparseStateSim,
}

impl Both {
    /// Five qubits entangled by gates on neighbours and on distant sites,
    /// with a complex state on each, so no symmetry hides a wrong gate.
    fn scrambled() -> Self {
        let engine = CpuMpsEngine::default();
        let capacity = MpsShape::qubit_chain(SITES, 1 << SITES).expect("a chain");
        let state = engine.prepare(&capacity).expect("prepare");
        let mut sim = SparseStateSim::new(None);
        for _ in 0..SITES {
            let _ = sim.allocate();
        }
        let mut both = Self { engine, state, sim };
        for site in 0..SITES {
            let x = f64::from(u32::try_from(site).expect("a small site"));
            both.one(Gate::Ry(0.3 + 0.37 * x), site);
            both.one(Gate::Rz(0.2 + 0.51 * x), site);
        }
        for (control, target) in [(0, 1), (3, 1), (2, 4), (4, 0)] {
            both.controlled(Gate::X, control, target);
        }
        both.one(Gate::Rx(0.9), 2);
        both
    }

    fn one(&mut self, gate: Gate, site: usize) {
        self.engine
            .apply(&mut self.state, &[site], &gate.matrix())
            .expect("a one-site gate");
        gate.on_sparse(&mut self.sim, site);
    }

    fn controlled(&mut self, gate: Gate, control: usize, target: usize) {
        self.engine
            .apply(
                &mut self.state,
                &[control, target],
                &controlled(gate.matrix()),
            )
            .expect("a two-site gate");
        gate.controlled_on_sparse(&mut self.sim, control, target);
    }

    /// The sparse simulator's amplitudes, basis state `i` with qubit `k` as
    /// bit `k`.
    fn amplitudes(&mut self) -> Vec<Complex64> {
        let (entries, qubits) = self.sim.get_state();
        let mut amplitudes = vec![c(0.0, 0.0); 1 << qubits];
        for (index, amplitude) in entries {
            let index = index.iter_u64_digits().next().unwrap_or(0);
            amplitudes[usize::try_from(index).expect("a small index")] = amplitude;
        }
        amplitudes
    }

    /// Every Pauli string's expectation agrees, so the two states are the
    /// same up to a global phase.
    fn assert_same_state(&mut self, case: &str) {
        let amplitudes = self.amplitudes();
        let norm: f64 = amplitudes.iter().map(Complex64::norm_sqr).sum();
        for code in 1..4_usize.pow(u32::try_from(SITES).expect("small")) {
            let factors = pauli_factors(code);
            let observable = PauliSum::new(
                c(0.0, 0.0),
                vec![PauliTerm::new(c(1.0, 0.0), factors.clone()).expect("a term")],
            )
            .expect("an observable");
            let value = self
                .engine
                .expectation(&self.state, &observable)
                .expect("an expectation");
            let expected = expectation(&amplitudes, &factors) / norm;
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

/// `⟨ψ|P|ψ⟩` for the Pauli string `factors`, on amplitudes with site `k` as
/// bit `k`.
fn expectation(amplitudes: &[Complex64], factors: &[(usize, Pauli)]) -> Complex64 {
    let flips = factors
        .iter()
        .filter(|(_, p)| matches!(p, Pauli::X | Pauli::Y))
        .fold(0, |mask, (site, _)| mask | 1 << site);
    (0..amplitudes.len())
        .map(|index| {
            let factor = factors.iter().fold(c(1.0, 0.0), |factor, &(site, p)| {
                let one = index >> site & 1 == 1;
                let sign = if one { -1.0 } else { 1.0 };
                factor
                    * match p {
                        Pauli::X => c(1.0, 0.0),
                        Pauli::Y => c(0.0, sign),
                        Pauli::Z => c(sign, 0.0),
                    }
            });
            amplitudes[index ^ flips].conj() * factor * amplitudes[index]
        })
        .sum()
}

/// Each of the sparse simulator's one-qubit gates on an entangled state, on
/// the first, a middle and the last site.
#[test]
fn one_qubit_gates_give_the_exact_state() {
    for gate in GATES {
        for site in [0, 2, 4] {
            let mut both = Both::scrambled();
            both.one(gate, site);
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
            both.controlled(gate, control, target);
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
        both.engine
            .apply(&mut both.state, &[control, target], &matrix)
            .expect("a two-site gate");
        both.sim.mcphase(&[control], phase, target);
        both.assert_same_state(&format!("phase on ({control}, {target})"));
    }
}

/// A SWAP on an entangled state, on distant sites and on neighbours, in
/// either order; the sparse simulator swaps two qubits by swapping their
/// identifiers.
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
        both.sim.swap_qubit_ids(a, b);
        both.assert_same_state(&format!("SWAP on ({a}, {b})"));
    }
}

/// P(One) of every site of an entangled state.
#[test]
fn probabilities_of_one_are_exact() {
    let mut both = Both::scrambled();
    for site in 0..SITES {
        let p = both
            .engine
            .probability_of_one(&both.state, site)
            .expect("P(One)");
        let expected = both.sim.joint_probability(&[site]);
        assert!(
            (p - expected).abs() < 1e-12,
            "site {site}: {p} vs {expected}"
        );
    }
}

/// Z strings on two to four sites of an entangled state, in any order: the
/// sparse simulator's joint probability is P(odd parity), which is
/// `(1 − ⟨Z…Z⟩) / 2` of the engine's expectation.
#[test]
fn z_string_expectations_are_exact() {
    let mut both = Both::scrambled();
    for sites in [&[0, 4][..], &[1, 2, 3], &[4, 0, 2, 1]] {
        let factors = sites.iter().map(|&site| (site, Pauli::Z)).collect();
        let observable = PauliSum::new(
            c(0.0, 0.0),
            vec![PauliTerm::new(c(1.0, 0.0), factors).expect("a term")],
        )
        .expect("an observable");
        let z = both
            .engine
            .expectation(&both.state, &observable)
            .expect("an expectation");
        let odd = (1.0 - z.re) / 2.0;
        let expected = both.sim.joint_probability(sites);
        assert!(
            (odd - expected).abs() < 1e-12,
            "{sites:?}: {odd} vs {expected}"
        );
    }
}

/// Three measurements in turn, each projected on a value: each projection
/// returns the probability the sparse simulator's forced collapse reports.
#[test]
fn a_projection_returns_the_exact_probability() {
    let mut both = Both::scrambled();
    for (site, one) in [(3, true), (0, false), (4, true)] {
        let probability = both.sim.force_collapse(one, site);
        let p = both
            .engine
            .project(&mut both.state, site, one)
            .expect("a possible value");
        assert!((p - probability).abs() < 1e-12, "{p} vs {probability}");
    }
}

/// Three measurements in turn, each projected on a value: each leaves the
/// engine in the sparse simulator's collapsed state, of unit weight.
#[test]
fn a_projection_leaves_the_collapsed_state() {
    let mut both = Both::scrambled();
    for (site, one) in [(3, true), (0, false), (4, true)] {
        both.sim.force_collapse(one, site);
        both.engine
            .project(&mut both.state, site, one)
            .expect("a possible value");
        both.assert_same_state(&format!("collapse of {site} on {one}"));
        let weight = both
            .engine
            .retained_weight(&both.state)
            .expect("the weight");
        assert!((weight - 1.0).abs() < 1e-12, "{weight}");
    }
}
