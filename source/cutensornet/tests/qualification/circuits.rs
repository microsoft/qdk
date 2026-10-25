// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The circuits, capacities and observables the qualification runs: plain
//! matrices on sites, as `MpsEngine::apply` takes them, drawn from fixed
//! seeds so that every run applies the same operators.

use num_complex::Complex64;
use std::f64::consts::FRAC_1_SQRT_2;
use tensornet::{MpsShape, MpsShapeError, Pauli, PauliSum, PauliTerm};

/// One operator: its sites and its row-major matrix, `sites[0]` the most
/// significant bit (`MpsEngine::apply`'s convention).
#[derive(Clone, Debug)]
pub struct Operator {
    pub sites: Vec<usize>,
    pub matrix: Vec<Complex64>,
}

impl Operator {
    fn new(sites: Vec<usize>, matrix: Vec<Complex64>) -> Self {
        Self { sites, matrix }
    }

    /// The inverse of a unitary operator: its conjugate transpose.
    pub fn adjoint(&self) -> Self {
        let dimension = 1 << self.sites.len();
        let matrix = (0..dimension * dimension)
            .map(|at| self.matrix[(at % dimension) * dimension + at / dimension].conj())
            .collect();
        Self::new(self.sites.clone(), matrix)
    }
}

pub fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

pub fn hadamard(site: usize) -> Operator {
    let h = FRAC_1_SQRT_2;
    Operator::new(
        vec![site],
        vec![c(h, 0.0), c(h, 0.0), c(h, 0.0), c(-h, 0.0)],
    )
}

/// `Ry(θ)`: from |0⟩, P(One) is `sin²(θ/2)`.
pub fn ry(site: usize, theta: f64) -> Operator {
    let (s, k) = (theta / 2.0).sin_cos();
    Operator::new(
        vec![site],
        vec![c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)],
    )
}

/// The `Ry` angle whose P(One) from |0⟩ is `probability`.
pub fn ry_angle(probability: f64) -> f64 {
    2.0 * probability.sqrt().asin()
}

/// `Rz(φ)·Ry(θ)·Rz(λ)`: any one-site unitary, up to a phase.
fn one_site(site: usize, phi: f64, theta: f64, lambda: f64) -> Operator {
    let (s, k) = (theta / 2.0).sin_cos();
    let phase = |angle: f64| Complex64::from_polar(1.0, angle / 2.0);
    Operator::new(
        vec![site],
        vec![
            k * phase(-phi - lambda),
            -s * phase(-phi + lambda),
            s * phase(phi - lambda),
            k * phase(phi + lambda),
        ],
    )
}

pub fn cnot(control: usize, target: usize) -> Operator {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    Operator::new(
        vec![control, target],
        vec![l, o, o, o, o, l, o, o, o, o, o, l, o, o, l, o],
    )
}

/// Controlled-Z: diagonal, so it entangles without changing a probability
/// in Z.
pub fn cz(first: usize, second: usize) -> Operator {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    Operator::new(
        vec![first, second],
        vec![l, o, o, o, o, l, o, o, o, o, l, o, o, o, o, -l],
    )
}

/// Controlled-Y: complex, and not symmetric under swapping its sites.
fn controlled_y(control: usize, target: usize) -> Operator {
    let (o, l, i) = (c(0.0, 0.0), c(1.0, 0.0), c(0.0, 1.0));
    Operator::new(
        vec![control, target],
        vec![l, o, o, o, o, l, o, o, o, o, o, -i, o, o, i, o],
    )
}

/// The `SplitMix64` generator: a fixed sequence for a seed, so that a run
/// repeats without a dependency.
pub struct Draws(u64);

impl Draws {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        // The top 53 bits, exactly representable, scaled by 2⁻⁵³.
        let bits = u32::try_from(self.next() >> 32).unwrap_or(0);
        let low = u32::try_from((self.next() >> 43) & 0x1F_FFFF).unwrap_or(0);
        (f64::from(bits) * f64::from(1u32 << 21) + f64::from(low)) / 9_007_199_254_740_992.0
    }

    fn angle(&mut self) -> f64 {
        std::f64::consts::TAU * self.unit()
    }

    /// Uniform in `0..bound`, for a small `bound`.
    pub fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound.max(1)).unwrap_or(u64::MAX);
        usize::try_from(self.next() % bound).unwrap_or(0)
    }
}

/// A random one-site unitary on a random site, then a CNOT or controlled-Y
/// on a random pair of distinct sites, adjacent or not.
pub fn random_operators(draws: &mut Draws, width: usize) -> [Operator; 2] {
    let site = draws.below(width);
    let single = one_site(site, draws.angle(), draws.angle(), draws.angle());
    let first = draws.below(width);
    let second = (first + 1 + draws.below(width - 1)) % width;
    let pair = if draws.unit() < 0.5 {
        cnot(first, second)
    } else {
        controlled_y(first, second)
    };
    [single, pair]
}

/// `layers` layers on `width` sites: a random one-site unitary on every
/// site, then CNOTs and controlled-Ys on alternate neighbouring pairs, and,
/// with `long_range`, one CNOT between the chain's ends, which an MPS engine
/// must route across every bond. Generic enough that every amplitude and
/// every bond is exercised; the same seed gives the same operators.
pub fn brickwork(width: usize, layers: usize, long_range: bool, seed: u64) -> Vec<Operator> {
    let mut draws = Draws::new(seed);
    let mut operators = Vec::new();
    for layer in 0..layers {
        for site in 0..width {
            operators.push(one_site(site, draws.angle(), draws.angle(), draws.angle()));
        }
        for left in (layer % 2..width.saturating_sub(1)).step_by(2) {
            operators.push(if (left / 2 + layer) % 2 == 0 {
                cnot(left, left + 1)
            } else {
                controlled_y(left + 1, left)
            });
        }
        if long_range && width > 2 {
            operators.push(cnot(width - 1, 0));
        }
    }
    operators
}

/// The brickwork layers after which the bond at the chain's middle can
/// reach `chi`: each cut gets a two-site gate every other layer, and a gate
/// at most doubles a bond, so `2·log₂ χ` layers, and two more.
pub fn layers_to_saturate(chi: usize) -> usize {
    2 * usize::try_from(chi.max(2).ilog2()).unwrap_or(0) + 2
}

/// A capacity of `width` qubits whose bond at each cut is the smaller of
/// `chi` and what an exact chain needs there.
pub fn capacity(width: usize, chi: usize) -> Result<MpsShape, MpsShapeError> {
    let bond = |cut: usize| {
        let smaller = (cut + 1).min(width - cut - 1);
        u32::try_from(smaller)
            .ok()
            .and_then(|smaller| 1usize.checked_shl(smaller))
            .filter(|&exact| exact > 0)
            .map_or(chi, |exact| exact.min(chi))
    };
    let sites = (0..width)
        .map(|site| {
            let left = site.checked_sub(1).map(bond);
            let right = (site + 1 < width).then(|| bond(site));
            left.into_iter().chain([2]).chain(right).collect()
        })
        .collect();
    MpsShape::new(sites)
}

/// A one-term observable with coefficient one.
pub fn string(factors: Vec<(usize, Pauli)>) -> Option<PauliSum> {
    let term = PauliTerm::new(c(1.0, 0.0), factors).ok()?;
    PauliSum::new(c(0.0, 0.0), vec![term]).ok()
}

/// The observables the tolerance is measured on: X, Y and Z of every site,
/// XX, YY, ZZ and XY of every neighbouring pair, and Z on every site.
pub fn observables(width: usize) -> Vec<(String, PauliSum)> {
    let mut list = Vec::new();
    for site in 0..width {
        for (pauli, name) in [(Pauli::X, "X"), (Pauli::Y, "Y"), (Pauli::Z, "Z")] {
            list.extend(string(vec![(site, pauli)]).map(|o| (format!("{name}{site}"), o)));
        }
    }
    let pairs = [
        (Pauli::X, Pauli::X, "XX"),
        (Pauli::Y, Pauli::Y, "YY"),
        (Pauli::Z, Pauli::Z, "ZZ"),
        (Pauli::X, Pauli::Y, "XY"),
    ];
    for left in 0..width.saturating_sub(1) {
        for (a, b, name) in pairs {
            list.extend(
                string(vec![(left, a), (left + 1, b)]).map(|o| (format!("{name}{left}"), o)),
            );
        }
    }
    list.extend(
        string((0..width).map(|site| (site, Pauli::Z)).collect()).map(|o| ("Z_all".to_string(), o)),
    );
    list
}

/// The projector on `given`'s values, `Π (I ± Z)/2`, as a sum of `2^t`
/// Pauli strings: it reads a condition's weight through the engine's
/// general expectation, not through its conditional probability.
pub fn projector(given: &[(usize, bool)]) -> Option<PauliSum> {
    let count = i32::try_from(given.len()).ok()?;
    let scale = 0.5f64.powi(count);
    let mut terms = Vec::new();
    for subset in 1..1usize << given.len() {
        let mut sign = 1.0;
        let mut factors = Vec::new();
        for (bit, &(site, one)) in given.iter().enumerate() {
            if subset >> bit & 1 == 1 {
                factors.push((site, Pauli::Z));
                if one {
                    sign = -sign;
                }
            }
        }
        terms.push(PauliTerm::new(c(sign * scale, 0.0), factors).ok()?);
    }
    PauliSum::new(c(scale, 0.0), terms).ok()
}

#[cfg(test)]
mod tests {
    use super::{capacity, cnot, hadamard, one_site, projector, ry};
    use tensornet::{MpsEngine, reference::ExactReference};

    #[test]
    fn the_adjoint_undoes_a_gate() {
        let gate = one_site(0, 0.3, 1.1, -0.4);
        let inverse = gate.adjoint();
        for row in 0..2 {
            for column in 0..2 {
                let value: num_complex::Complex64 = (0..2)
                    .map(|k| inverse.matrix[row * 2 + k] * gate.matrix[k * 2 + column])
                    .sum();
                let expected = if row == column { 1.0 } else { 0.0 };
                assert!(
                    (value - expected).norm() < 1e-15,
                    "{row}, {column}: {value}"
                );
            }
        }
    }

    #[test]
    fn a_capacity_never_exceeds_the_exact_bond() {
        let shape = capacity(6, 4).unwrap_or_else(|e| panic!("{e}"));
        let bonds: Vec<_> = (0..5).filter_map(|cut| shape.bond_dim(cut)).collect();
        assert_eq!(bonds, [2, 4, 4, 4, 2]);
    }

    #[test]
    fn the_projector_weighs_its_condition() {
        let reference = ExactReference::new();
        let shape = capacity(3, 4).unwrap_or_else(|e| panic!("{e}"));
        let mut state = reference.prepare(&shape).unwrap_or_else(|e| panic!("{e}"));
        for operator in [hadamard(0), cnot(0, 1), ry(2, 1.0)] {
            reference
                .apply(&mut state, &operator.sites, &operator.matrix)
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let weight = |given: &[(usize, bool)]| {
            let observable = projector(given).unwrap_or_else(|| panic!("a projector"));
            reference
                .expectation(&state, &observable)
                .unwrap_or_else(|e| panic!("{e}"))
                .re
        };
        assert!(weight(&[(0, true), (1, false)]).abs() < 1e-15);
        assert!((weight(&[(0, true), (1, true)]) - 0.5).abs() < 1e-15);
        let p2 = 0.5f64.sin().powi(2);
        assert!((weight(&[(0, false), (2, true)]) - 0.5 * p2).abs() < 1e-15);
    }
}
