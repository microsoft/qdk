// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Circuits whose outcome is known exactly, on chains up to more sites
//! than a state vector holds, with gates on distant sites across the
//! chain:
//!
//! - a GHZ state, whose reads are known at any width;
//! - mirror circuits, which return every qubit to zero;
//! - Clifford circuits, whose every read is 0, ½ or 1. Where
//!   `tensornet`'s exact reference holds the chain, each read is the
//!   reference's.

use crate::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use num_complex::Complex64;
use std::f64::consts::FRAC_1_SQRT_2;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::sampling::SplitMix64;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// The widths of the chains, from a few sites to more than a state vector
/// holds.
const WIDTHS: [usize; 4] = [6, 12, 24, 48];

/// The largest distance between the two sites of a gate, so that the bonds
/// stay small whatever the width.
const REACH: usize = 4;

/// The largest bond of every chain.
const BOND: usize = 256;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// A deterministic stream of pseudo-random numbers, `tensornet`'s
/// [`SplitMix64`], with the draws the circuits need.
struct Stream(SplitMix64);

impl Stream {
    /// The stream of `seed`.
    fn new(seed: u64) -> Self {
        Self(SplitMix64::new(seed))
    }

    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0.next_u64()
    }

    /// An index in `0..bound`.
    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).expect("a small bound"))
            .expect("below a usize")
    }

    /// A fair coin.
    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }

    /// An angle in `[−π, π)`, on a grid of `2^24` steps.
    fn angle(&mut self) -> f64 {
        let unit =
            f64::from(u32::try_from(self.next() >> 40).expect("24 bits")) / f64::from(1 << 24);
        (2.0 * unit - 1.0) * std::f64::consts::PI
    }

    /// Two distinct sites at most [`REACH`] apart, in either order.
    fn pair(&mut self, width: usize) -> (usize, usize) {
        let low = self.below(width - 1);
        let high = (low + 1 + self.below(REACH)).min(width - 1);
        if self.coin() {
            (low, high)
        } else {
            (high, low)
        }
    }
}

/// A gate of a Clifford circuit.
#[derive(Clone, Copy, Debug)]
enum Clifford {
    H(usize),
    S(usize),
    SAdj(usize),
    X(usize),
    Y(usize),
    Z(usize),
    Cx(usize, usize),
    Cz(usize, usize),
    Swap(usize, usize),
}

impl Clifford {
    /// A gate drawn uniformly from the nine kinds, on a random site or
    /// on a [`Stream::pair`]; both are drawn whatever the kind.
    fn random(stream: &mut Stream, width: usize) -> Self {
        let site = stream.below(width);
        let (a, b) = stream.pair(width);
        match stream.below(9) {
            0 => Self::H(site),
            1 => Self::S(site),
            2 => Self::SAdj(site),
            3 => Self::X(site),
            4 => Self::Y(site),
            5 => Self::Z(site),
            6 => Self::Cx(a, b),
            7 => Self::Cz(a, b),
            _ => Self::Swap(a, b),
        }
    }

    /// The sites and the row-major matrix of the gate.
    fn sites_and_matrix(self) -> (Vec<usize>, Vec<Complex64>) {
        let (zero, one, i) = (c(0.0, 0.0), c(1.0, 0.0), c(0.0, 1.0));
        let half = c(FRAC_1_SQRT_2, 0.0);
        match self {
            Self::H(site) => (vec![site], vec![half, half, half, -half]),
            Self::S(site) => (vec![site], vec![one, zero, zero, i]),
            Self::SAdj(site) => (vec![site], vec![one, zero, zero, -i]),
            Self::X(site) => (vec![site], vec![zero, one, one, zero]),
            Self::Y(site) => (vec![site], vec![zero, -i, i, zero]),
            Self::Z(site) => (vec![site], vec![one, zero, zero, -one]),
            Self::Cx(control, target) => (vec![control, target], permutation([0, 1, 3, 2])),
            Self::Swap(left, right) => (vec![left, right], permutation([0, 2, 1, 3])),
            Self::Cz(control, target) => {
                let mut matrix = permutation([0, 1, 2, 3]);
                matrix[15] = -one;
                (vec![control, target], matrix)
            }
        }
    }
}

/// The two-site permutation matrix sending basis state `j` to `to[j]`.
fn permutation(to: [usize; 4]) -> Vec<Complex64> {
    let mut matrix = vec![c(0.0, 0.0); 16];
    for (from, &row) in to.iter().enumerate() {
        matrix[4 * row + from] = c(1.0, 0.0);
    }
    matrix
}

/// The engine's chain of `width` qubits in `|0…0⟩`, every bond at most
/// [`BOND`]: large enough that the Clifford and GHZ circuits truncate
/// nothing, which [`assert_nothing_truncated`] checks.
fn chain_of_zeros(engine: &CpuMpsEngine, width: usize) -> CanonicalMpsState<CpuAlgebra> {
    let capacity = MpsShape::qubit_chain(width, BOND).expect("a chain");
    engine.prepare(&capacity).expect("prepare")
}

/// Checks that `state` dropped no weight, so that its reads are exact.
fn assert_nothing_truncated(
    engine: &CpuMpsEngine,
    state: &CanonicalMpsState<CpuAlgebra>,
    case: &str,
) {
    let weight = engine.retained_weight(state).expect("the weight");
    assert!(
        (weight - 1.0).abs() < 1e-12,
        "precondition: nothing was truncated, so the reads are exact ({case}, \
         weight {weight})"
    );
}

/// Each shot of a random Clifford circuit reads every site in turn, as a
/// shot of a simulation method would. The engine's probability of one,
/// given the values drawn so far, is exactly 0, ½ or 1, as every read of
/// a stabilizer state is. On the chains the exact reference holds, it is
/// the reference's.
#[test]
fn clifford_shots_read_exact_probabilities() {
    let engine = CpuMpsEngine::new(CpuAlgebra);
    for width in WIDTHS {
        for shot in 0..3 {
            let case = format!("width {width}, shot {shot}");
            let mut stream = Stream::new(u64::try_from(1000 * width + shot).expect("small"));
            let mut chain = ComparedChain::of_zeros(&engine, width);
            for _ in 0..3 * width {
                let (sites, matrix) = Clifford::random(&mut stream, width).sites_and_matrix();
                chain.apply(&sites, &matrix);
            }
            assert_nothing_truncated(&engine, &chain.state, &case);
            for site in 0..width {
                let p = chain.probability_of_one(site);
                chain.assert_stabilizer_read(site, p, &case);
                let one = if (p - 0.5).abs() < 0.25 {
                    stream.coin()
                } else {
                    p > 0.5
                };
                chain.project(site, one);
            }
        }
    }
}

/// The engine's chain and, where [`ExactReference`] holds the chain, the
/// reference's state beside it, given the same gates and projections.
struct ComparedChain<'e> {
    engine: &'e CpuMpsEngine,
    state: CanonicalMpsState<CpuAlgebra>,
    reference: ExactReference,
    exact_state: Option<ReferenceState>,
}

impl<'e> ComparedChain<'e> {
    /// A chain of `width` qubits in `|0…0⟩` on `engine`, and on the
    /// reference when it holds `width` sites.
    fn of_zeros(engine: &'e CpuMpsEngine, width: usize) -> Self {
        let state = chain_of_zeros(engine, width);
        let reference = ExactReference::new();
        let exact_state = (width <= ExactReference::MAX_SITES).then(|| {
            let capacity = MpsShape::qubit_chain(width, BOND).expect("a chain");
            reference.prepare(&capacity).expect("prepare")
        });
        Self {
            engine,
            state,
            reference,
            exact_state,
        }
    }

    /// Applies the Clifford gate `matrix` on `sites` to both states.
    fn apply(&mut self, sites: &[usize], matrix: &[Complex64]) {
        self.engine
            .apply(&mut self.state, sites, matrix)
            .expect("a Clifford gate");
        if let Some(exact_state) = &mut self.exact_state {
            self.reference
                .apply(exact_state, sites, matrix)
                .expect("a Clifford gate");
        }
    }

    /// The engine's P(One) of `site`.
    fn probability_of_one(&self, site: usize) -> f64 {
        self.engine
            .probability_of_one(&self.state, site)
            .expect("P(One)")
    }

    /// Asserts that `p`, the engine's P(One) of `site`, is 0, ½ or 1, as
    /// every read of a stabilizer state is, and is the reference's.
    fn assert_stabilizer_read(&self, site: usize, p: f64, case: &str) {
        let nearest = (2.0 * p).round() / 2.0;
        assert!(
            (p - nearest).abs() < 1e-10,
            "{case}, site {site}: {p} is not 0, ½ or 1"
        );
        if let Some(exact_state) = &self.exact_state {
            let expected = self
                .reference
                .probability_of_one(exact_state, site)
                .expect("P(One)");
            assert!(
                (p - expected).abs() < 1e-10,
                "{case}, site {site}: {p} vs {expected}"
            );
        }
    }

    /// Projects `site` of both states on One when `one` is true, on Zero
    /// when it is false.
    fn project(&mut self, site: usize, one: bool) {
        self.engine
            .project(&mut self.state, site, one)
            .expect("a possible value");
        if let Some(exact_state) = &mut self.exact_state {
            self.reference
                .project(exact_state, site, one)
                .expect("a possible value");
        }
    }
}

/// A GHZ state, `(|0…0⟩ + |1…1⟩)/√2`, spread over the whole chain from
/// its middle by CNOTs on sites up to [`REACH`] apart, the control on
/// either side of the target.
///
/// - Every site reads P(One) = ½.
/// - X on every site reads 1, which no mixture of `|0…0⟩` and `|1…1⟩` does.
/// - A site projected on a value, with probability ½, leaves every other
///   site reading that value.
#[test]
fn a_ghz_state_across_the_chain_reads_its_exact_values() {
    let engine = CpuMpsEngine::new(CpuAlgebra);
    for width in WIDTHS {
        let case = format!("width {width}");
        let mut stream = Stream::new(u64::try_from(7 * width).expect("small"));
        let mut state = ghz_across_the_chain(&engine, width, &mut stream);
        assert_nothing_truncated(&engine, &state, &case);
        for site in 0..width {
            let p = engine.probability_of_one(&state, site).expect("P(One)");
            assert!((p - 0.5).abs() < 1e-10, "{case}, site {site}: {p}");
        }
        let x = engine
            .expectation(&state, &x_on_every_site(width))
            .expect("⟨X…X⟩");
        assert!((x - 1.0).norm() < 1e-10, "{case}: ⟨X…X⟩ = {x}");
        let (measured, one) = (stream.below(width), stream.coin());
        let p = engine
            .project(&mut state, measured, one)
            .expect("a possible value");
        assert!((p - 0.5).abs() < 1e-10, "{case}: projected with {p}");
        let expected = if one { 1.0 } else { 0.0 };
        for site in 0..width {
            let p = engine.probability_of_one(&state, site).expect("P(One)");
            assert!(
                (p - expected).abs() < 1e-10,
                "{case}, site {site} after site {measured} read {one}: {p}"
            );
        }
    }
}

/// The GHZ state on `width` sites: H on the middle site, then CNOTs that
/// grow the entangled block one site at a time on each side, each from a
/// control in the block up to [`REACH`] sites from its target.
fn ghz_across_the_chain(
    engine: &CpuMpsEngine,
    width: usize,
    stream: &mut Stream,
) -> CanonicalMpsState<CpuAlgebra> {
    let half = c(FRAC_1_SQRT_2, 0.0);
    let cnot = permutation([0, 1, 3, 2]);
    let mut state = chain_of_zeros(engine, width);
    let middle = width / 2;
    engine
        .apply(&mut state, &[middle], &[half, half, half, -half])
        .expect("H");
    let (mut low, mut high) = (middle, middle);
    while low > 0 || high < width - 1 {
        if high < width - 1 {
            let target = high + 1;
            let control = target - 1 - stream.below(REACH.min(target - low));
            engine
                .apply(&mut state, &[control, target], &cnot)
                .expect("a CNOT");
            high = target;
        }
        if low > 0 {
            let target = low - 1;
            let control = target + 1 + stream.below(REACH.min(high - target));
            engine
                .apply(&mut state, &[control, target], &cnot)
                .expect("a CNOT");
            low = target;
        }
    }
    state
}

/// The observable `X ⊗ … ⊗ X` on `width` sites.
fn x_on_every_site(width: usize) -> PauliSum {
    let factors = (0..width).map(|site| (site, Pauli::X)).collect();
    PauliSum::new(
        c(0.0, 0.0),
        vec![PauliTerm::new(c(1.0, 0.0), factors).expect("a term")],
    )
    .expect("an observable")
}

/// A random circuit of rotations and entangling gates, then its inverse,
/// leaves every qubit at zero: no probability of one, and only zeros drawn.
#[test]
fn a_mirror_circuit_returns_every_qubit_to_zero() {
    let engine = CpuMpsEngine::new(CpuAlgebra);
    for width in WIDTHS {
        let mut stream = Stream::new(u64::try_from(width).expect("small"));
        let mut state = chain_of_zeros(&engine, width);
        let circuit: Vec<(Vec<usize>, Vec<Complex64>)> = (0..3 * width)
            .map(|_| random_gate(&mut stream, width))
            .collect();
        for (sites, matrix) in &circuit {
            engine.apply(&mut state, sites, matrix).expect("a gate");
        }
        for (sites, matrix) in circuit.iter().rev() {
            engine
                .apply(&mut state, sites, &adjoint(matrix))
                .expect("an inverse gate");
        }
        for site in 0..width {
            let p = engine.probability_of_one(&state, site).expect("P(One)");
            assert!(p < 1e-9, "width {width}, site {site}: {p}");
        }
        let sites: Vec<usize> = (0..width).collect();
        let values = engine.sample(&state, &sites, 20, 3).expect("a sample");
        assert!(
            values.iter().all(|&one| !one),
            "width {width}: a one was drawn"
        );
    }
}

/// A one-site rotation or an entangling two-site gate, chosen by a coin.
fn random_gate(stream: &mut Stream, width: usize) -> (Vec<usize>, Vec<Complex64>) {
    if stream.coin() {
        random_rotation(stream, width)
    } else {
        random_entangler(stream, width)
    }
}

/// A rotation of a random angle about a random axis of the XY plane, on a
/// random site.
fn random_rotation(stream: &mut Stream, width: usize) -> (Vec<usize>, Vec<Complex64>) {
    let (theta, phi) = (stream.angle(), stream.angle());
    let (cos, sin) = ((theta / 2.0).cos(), (theta / 2.0).sin());
    let phase = Complex64::from_polar(1.0, phi);
    let matrix = vec![c(cos, 0.0), -phase.conj() * sin, phase * sin, c(cos, 0.0)];
    (vec![stream.below(width)], matrix)
}

/// `exp(−iθ/2 · X⊗X)`, followed by a controlled phase, both of random
/// angles, on two sites at most [`REACH`] apart.
fn random_entangler(stream: &mut Stream, width: usize) -> (Vec<usize>, Vec<Complex64>) {
    let (a, b) = stream.pair(width);
    let theta = stream.angle();
    let (cos, sin) = ((theta / 2.0).cos(), (theta / 2.0).sin());
    let mut matrix = vec![c(0.0, 0.0); 16];
    for row in 0..4 {
        matrix[4 * row + row] = c(cos, 0.0);
        matrix[4 * row + (3 - row)] = c(0.0, -sin);
    }
    let phase = Complex64::from_polar(1.0, stream.angle());
    for col in 0..4 {
        matrix[12 + col] *= phase;
    }
    (vec![a, b], matrix)
}

/// The adjoint of a one- or two-site gate's row-major `matrix`.
fn adjoint(matrix: &[Complex64]) -> Vec<Complex64> {
    let n = if matrix.len() == 4 { 2 } else { 4 };
    (0..n * n)
        .map(|at| matrix[(at % n) * n + at / n].conj())
        .collect()
}
