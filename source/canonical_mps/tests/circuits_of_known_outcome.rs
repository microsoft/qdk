// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Circuits whose outcome is known exactly, on chains up to more sites
//! than a state vector holds, with gates on distant sites across the
//! chain: a GHZ state, whose reads are known at any width; mirror
//! circuits, which return every qubit to zero; and Clifford circuits,
//! whose every read is 0, ½ or 1, and is `tensornet`'s exact reference's
//! where it holds the chain.

use canonical_mps::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use num_complex::Complex64;
use std::f64::consts::FRAC_1_SQRT_2;
use tensornet::reference::ExactReference;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// The widths of the chains, from a few sites to more than a state vector
/// holds.
const WIDTHS: [usize; 4] = [6, 12, 24, 48];

/// The largest distance between the two sites of a gate, so that the bonds
/// stay small whatever the width.
const REACH: usize = 4;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// A deterministic stream of pseudo-random numbers: `SplitMix64` (Steele,
/// Lea and Flood, 2014), from its seed.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).expect("a small bound"))
            .expect("below a usize")
    }

    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }

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

fn exact_chain(
    engine: &CpuMpsEngine,
    width: usize,
    max_bond_dimension: usize,
) -> CanonicalMpsState<CpuAlgebra> {
    let capacity = MpsShape::qubit_chain(width, max_bond_dimension).expect("a chain");
    engine.prepare(&capacity).expect("prepare")
}

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
/// shot of a simulation method would: the engine's probability of one,
/// given the values drawn so far, is exactly 0, ½ or 1, as every read of
/// a stabilizer state is, and, on the chains the exact reference holds,
/// is the reference's.
#[test]
fn clifford_shots_read_exact_probabilities() {
    let engine = CpuMpsEngine::new(CpuAlgebra);
    let reference = ExactReference::new();
    for width in WIDTHS {
        for shot in 0..3 {
            let case = format!("width {width}, shot {shot}");
            let mut stream = Stream(u64::try_from(1000 * width + shot).expect("small"));
            let mut state = exact_chain(&engine, width, 256);
            let mut exact_state = (width <= ExactReference::MAX_SITES).then(|| {
                let capacity = MpsShape::qubit_chain(width, 256).expect("a chain");
                reference.prepare(&capacity).expect("prepare")
            });
            for _ in 0..3 * width {
                let (sites, matrix) = Clifford::random(&mut stream, width).sites_and_matrix();
                engine
                    .apply(&mut state, &sites, &matrix)
                    .expect("a Clifford gate");
                if let Some(exact_state) = &mut exact_state {
                    reference
                        .apply(exact_state, &sites, &matrix)
                        .expect("a Clifford gate");
                }
            }
            assert_nothing_truncated(&engine, &state, &case);
            for site in 0..width {
                let p = engine.probability_of_one(&state, site).expect("P(One)");
                let nearest = (2.0 * p).round() / 2.0;
                assert!(
                    (p - nearest).abs() < 1e-10,
                    "{case}, site {site}: {p} is not 0, ½ or 1"
                );
                if let Some(exact_state) = &exact_state {
                    let expected = reference
                        .probability_of_one(exact_state, site)
                        .expect("P(One)");
                    assert!(
                        (p - expected).abs() < 1e-10,
                        "{case}, site {site}: {p} vs {expected}"
                    );
                }
                let one = if (p - 0.5).abs() < 0.25 {
                    stream.coin()
                } else {
                    p > 0.5
                };
                engine
                    .project(&mut state, site, one)
                    .expect("a possible value");
                if let Some(exact_state) = &mut exact_state {
                    reference
                        .project(exact_state, site, one)
                        .expect("a possible value");
                }
            }
        }
    }
}

/// A GHZ state, `(|0…0⟩ + |1…1⟩)/√2`, spread over the whole chain from
/// its middle by CNOTs on sites up to [`REACH`] apart, the control on
/// either side of the target: every site reads P(One) = ½; X on every
/// site reads 1, which no mixture of `|0…0⟩` and `|1…1⟩` does; and a site
/// projected on a value, with probability ½, leaves every other site
/// reading that value.
#[test]
fn a_ghz_state_across_the_chain_reads_its_exact_values() {
    let engine = CpuMpsEngine::new(CpuAlgebra);
    let half = c(FRAC_1_SQRT_2, 0.0);
    let cnot = permutation([0, 1, 3, 2]);
    for width in WIDTHS {
        let case = format!("width {width}");
        let mut stream = Stream(u64::try_from(7 * width).expect("small"));
        let mut state = exact_chain(&engine, width, 256);
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
        assert_nothing_truncated(&engine, &state, &case);
        for site in 0..width {
            let p = engine.probability_of_one(&state, site).expect("P(One)");
            assert!((p - 0.5).abs() < 1e-10, "{case}, site {site}: {p}");
        }
        let every_x = PauliSum::new(
            c(0.0, 0.0),
            vec![
                PauliTerm::new(
                    c(1.0, 0.0),
                    (0..width).map(|site| (site, Pauli::X)).collect(),
                )
                .expect("a term"),
            ],
        )
        .expect("an observable");
        let x = engine.expectation(&state, &every_x).expect("⟨X…X⟩");
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

/// A random circuit of rotations and entangling gates, then its inverse,
/// leaves every qubit at zero: no probability of one, and only zeros drawn.
#[test]
fn a_mirror_circuit_returns_every_qubit_to_zero() {
    let engine = CpuMpsEngine::new(CpuAlgebra);
    for width in WIDTHS {
        let mut stream = Stream(u64::try_from(width).expect("small"));
        let mut state = exact_chain(&engine, width, 256);
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

/// A one-site rotation, or an entangling two-site gate on sites at most
/// [`REACH`] apart.
fn random_gate(stream: &mut Stream, width: usize) -> (Vec<usize>, Vec<Complex64>) {
    if stream.coin() {
        let (theta, phi) = (stream.angle(), stream.angle());
        let (cos, sin) = ((theta / 2.0).cos(), (theta / 2.0).sin());
        let phase = Complex64::from_polar(1.0, phi);
        let matrix = vec![c(cos, 0.0), -phase.conj() * sin, phase * sin, c(cos, 0.0)];
        (vec![stream.below(width)], matrix)
    } else {
        let (a, b) = stream.pair(width);
        let theta = stream.angle();
        let (cos, sin) = ((theta / 2.0).cos(), (theta / 2.0).sin());
        // exp(−iθ/2 · X⊗X), followed by a controlled phase.
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
}

fn adjoint(matrix: &[Complex64]) -> Vec<Complex64> {
    let n = if matrix.len() == 4 { 2 } else { 4 };
    (0..n * n)
        .map(|at| matrix[(at % n) * n + at / n].conj())
        .collect()
}
