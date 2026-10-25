// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ExactReference`]: the [`MpsEngine`] every other engine is checked
//! against, a full state vector that never truncates.
//!
//! It holds all 2ⁿ amplitudes of a state, so it gives the exact answer of
//! every operation, and it reports the exact chain: the bond at each cut is
//! the Schmidt rank of the state there. It is for tests and small examples,
//! up to [`ExactReference::MAX_SITES`] sites, and depends on nothing but
//! this crate, so it runs wherever the crate builds.
//!
//! Its arithmetic is fixed, so a simulator that runs on it gets the same
//! probabilities, bit for bit, as a state vector that applies the same
//! matrices in the same order:
//!
//! - basis state `i` has site `k` as bit `k` of `i`;
//! - [`apply`](MpsEngine::apply) computes each new amplitude as
//!   `M[r][0]·a₀ + M[r][1]·a₁ + …`, in column order;
//! - [`probability_of_one`](MpsEngine::probability_of_one) sums `re² + im²`
//!   over the basis states with the site set, in index order.
//!
//! A capacity is accepted whatever its bonds: the reference never truncates,
//! so a state whose exact chain outgrows its capacity is reported by
//! [`shape`](MpsEngine::shape), for the caller to check.

use crate::{Mps, MpsEngine, MpsError, Pauli, PauliSum, SampleSeed};
use num_complex::Complex64;
use std::collections::BTreeMap;

/// Why the reference refused an operation.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ReferenceError {
    /// A capacity with more sites than the reference holds.
    #[error("the exact reference holds at most {max} sites, not {requested}")]
    TooManySites {
        /// The sites asked for.
        requested: usize,
        /// [`ExactReference::MAX_SITES`].
        max: usize,
    },
    /// A capacity whose site is not two-level: the reference holds qubits.
    #[error("site {site} has physical extent {extent}, not 2")]
    NotQubit {
        /// The site.
        site: usize,
        /// Its physical extent.
        extent: usize,
    },
    /// A site the state does not have.
    #[error("site {site} is not among the state's {sites}")]
    SiteOutOfRange {
        /// The site named.
        site: usize,
        /// How many sites the state has.
        sites: usize,
    },
    /// A site named twice where distinct sites are needed.
    #[error("site {0} is named twice")]
    RepeatedSite(usize),
    /// A matrix whose length is not `4^sites`.
    #[error("a matrix on {sites} sites has {expected} elements, not {actual}")]
    MatrixSize {
        /// How many sites it acts on.
        sites: usize,
        /// `4^sites`.
        expected: usize,
        /// Its length.
        actual: usize,
    },
    /// A probability outside (0, 1] (to rounding), or not finite.
    #[error("{0} is not the probability of a value that can be drawn")]
    InvalidProbability(f64),
    /// A condition that has probability zero, so no conditional exists.
    #[error("the condition has probability zero")]
    ZeroProbabilityCondition,
    /// The exact chain is not an [`Mps`]; unreachable for a state of at
    /// least one site, kept so that no path panics.
    #[error(transparent)]
    Shape(#[from] MpsError),
}

/// How far past 1 a probability may be: a sum of 2ⁿ squares of a
/// normalized state rounds to slightly more than 1.
const PROBABILITY_ROUNDING: f64 = 1e-9;

/// The exact reference engine. It holds no state of its own, so one value
/// serves any number of states.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactReference;

impl ExactReference {
    /// The most sites a state holds: 2²⁰ amplitudes, 16 MiB.
    pub const MAX_SITES: usize = 20;

    /// The reference.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// A state of [`ExactReference`]: all 2ⁿ amplitudes, basis state `i` with
/// site `k` as bit `k`.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceState {
    re: Vec<f64>,
    im: Vec<f64>,
    sites: usize,
}

impl ReferenceState {
    /// How many sites it has.
    #[must_use]
    pub fn sites(&self) -> usize {
        self.sites
    }

    /// The amplitude of basis state `index`, zero past the last.
    #[must_use]
    pub fn amplitude(&self, index: usize) -> Complex64 {
        Complex64::new(
            self.re.get(index).copied().unwrap_or(0.0),
            self.im.get(index).copied().unwrap_or(0.0),
        )
    }

    fn check_site(&self, site: usize) -> Result<usize, ReferenceError> {
        if site < self.sites {
            Ok(1 << site)
        } else {
            Err(ReferenceError::SiteOutOfRange {
                site,
                sites: self.sites,
            })
        }
    }

    /// The probability of basis state `i`, `re² + im²`.
    fn probability(&self, i: usize) -> f64 {
        let (r, m) = (
            self.re.get(i).copied().unwrap_or(0.0),
            self.im.get(i).copied().unwrap_or(0.0),
        );
        r * r + m * m
    }

    /// ⟨ψ|P|ψ⟩ for the Pauli string with X on the sites of `x` and Z on
    /// those of `z` (Y on both): P|i⟩ = i^#Y · (−1)^|i ∧ z| · |i ⊕ x⟩, from
    /// Y|b⟩ = i(−1)^b|1−b⟩. Only the real part is returned, because P is
    /// Hermitian, so the imaginary part is rounding.
    fn pauli_expectation(&self, x: usize, z: usize) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for i in 0..self.re.len() {
            let (a, b) = (self.amplitude(i), self.amplitude(i ^ x));
            // conj(ψ_{i⊕x})·ψ_i, signed by the Z part.
            let sign = if (i & z).count_ones().is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            re += sign * (b.re * a.re + b.im * a.im);
            im += sign * (b.re * a.im - b.im * a.re);
        }
        match (x & z).count_ones() % 4 {
            0 => re,
            1 => -im,
            2 => -re,
            _ => im,
        }
    }

    /// The rank of the state as a matrix with sites 0..=cut as rows and
    /// the rest as columns: the bond an exact chain needs at that cut.
    /// Gaussian elimination with partial pivoting; entries below 1e-10
    /// count as zero, since amplitudes of a normalized state that small
    /// carry no weight.
    fn schmidt_rank(&self, cut: usize) -> usize {
        const ZERO: f64 = 1e-10;
        let low = cut + 1;
        let (rows, cols) = (1usize << low, 1usize << (self.sites - low));
        let amplitude = |index: usize| {
            let a = self.amplitude(index);
            (a.re, a.im)
        };
        // Eliminate along the smaller side: rank ≤ min(rows, cols).
        let mut matrix: Vec<Vec<(f64, f64)>> = if rows <= cols {
            (0..rows)
                .map(|row| (0..cols).map(|col| amplitude(row | (col << low))).collect())
                .collect()
        } else {
            (0..cols)
                .map(|col| (0..rows).map(|row| amplitude(row | (col << low))).collect())
                .collect()
        };
        let (short, long) = (rows.min(cols), rows.max(cols));
        let norm = |value: (f64, f64)| value.0.hypot(value.1);
        let mut rank = 0;
        for col in 0..long {
            if rank == short {
                break;
            }
            let at = |matrix: &[Vec<(f64, f64)>], row: usize| {
                matrix
                    .get(row)
                    .and_then(|entries| entries.get(col))
                    .copied()
                    .unwrap_or((0.0, 0.0))
            };
            let Some(pivot) = (rank..short)
                .max_by(|&x, &y| norm(at(&matrix, x)).total_cmp(&norm(at(&matrix, y))))
            else {
                break;
            };
            let pivot_value = at(&matrix, pivot);
            if norm(pivot_value) < ZERO {
                continue;
            }
            matrix.swap(rank, pivot);
            let (pr, pi) = pivot_value;
            let denominator = pr * pr + pi * pi;
            let (top, below) = matrix.split_at_mut(rank + 1);
            let Some(pivot_row) = top.last() else { break };
            for row in below.iter_mut() {
                let (xr, xi) = row.get(col).copied().unwrap_or((0.0, 0.0));
                // factor = x / pivot
                let factor = (
                    (xr * pr + xi * pi) / denominator,
                    (xi * pr - xr * pi) / denominator,
                );
                if norm(factor) == 0.0 {
                    continue;
                }
                for (entry, &above) in row.iter_mut().zip(pivot_row).skip(col) {
                    entry.0 -= factor.0 * above.0 - factor.1 * above.1;
                    entry.1 -= factor.0 * above.1 + factor.1 * above.0;
                }
            }
            rank += 1;
        }
        rank.max(1)
    }
}

/// The value drawn as a uniform in [0, 1) from a `SplitMix64` stream, the
/// reference's own sampler RNG: small, and fixed by this crate rather than
/// by a dependency's version.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_uniform(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // The top 53 bits, as a multiple of 2⁻⁵³.
        #[allow(clippy::cast_precision_loss)]
        let uniform = (z >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
        uniform
    }
}

impl MpsEngine for ExactReference {
    type State = ReferenceState;
    type Error = ReferenceError;

    fn prepare(&self, capacity: &Mps) -> Result<ReferenceState, ReferenceError> {
        let sites = capacity.site_count();
        if sites > Self::MAX_SITES {
            return Err(ReferenceError::TooManySites {
                requested: sites,
                max: Self::MAX_SITES,
            });
        }
        if let Some((site, extent)) = (0..sites)
            .map(|s| (s, capacity.physical_dim(s).unwrap_or(0)))
            .find(|&(_, extent)| extent != 2)
        {
            return Err(ReferenceError::NotQubit { site, extent });
        }
        let n = 1usize << sites;
        let mut re = vec![0.0; n];
        if let Some(first) = re.first_mut() {
            *first = 1.0;
        }
        Ok(ReferenceState {
            re,
            im: vec![0.0; n],
            sites,
        })
    }

    fn apply(
        &self,
        state: &mut ReferenceState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), ReferenceError> {
        let k = sites.len();
        let mut bits = Vec::with_capacity(k);
        for (at, &site) in sites.iter().enumerate() {
            if sites[..at].contains(&site) {
                return Err(ReferenceError::RepeatedSite(site));
            }
            bits.push(state.check_site(site)?);
        }
        let dim = 1usize << k;
        if matrix.len() != dim * dim {
            return Err(ReferenceError::MatrixSize {
                sites: k,
                expected: dim * dim,
                actual: matrix.len(),
            });
        }
        // The offset of local index `c` (sites[0] its most significant bit)
        // from a basis state with every site of `sites` clear.
        let offsets: Vec<usize> = (0..dim)
            .map(|c| {
                bits.iter()
                    .enumerate()
                    .filter(|&(j, _)| c >> (k - 1 - j) & 1 == 1)
                    .fold(0, |o, (_, &bit)| o | bit)
            })
            .collect();
        let mask = bits.iter().fold(0, |m, &bit| m | bit);
        let mut column = vec![Complex64::new(0.0, 0.0); dim];
        for base in (0..state.re.len()).filter(|i| i & mask == 0) {
            for (a, &o) in column.iter_mut().zip(&offsets) {
                *a = state.amplitude(base | o);
            }
            for (row, &o) in matrix.chunks_exact(dim).zip(&offsets) {
                let mut terms = row.iter().zip(&column).map(|(m, a)| m * a);
                let first = terms.next().unwrap_or_default();
                let value = terms.fold(first, |sum, t| sum + t);
                if let (Some(re), Some(im)) =
                    (state.re.get_mut(base | o), state.im.get_mut(base | o))
                {
                    (*re, *im) = (value.re, value.im);
                }
            }
        }
        Ok(())
    }

    fn probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
    ) -> Result<f64, ReferenceError> {
        let bit = state.check_site(site)?;
        Ok((0..state.re.len())
            .filter(|i| i & bit != 0)
            .map(|i| state.probability(i))
            .sum())
    }

    /// Sums the probabilities of the basis states that agree with `given`,
    /// as a marginal with projected modes contracts them: no copy.
    fn conditional_probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, ReferenceError> {
        let bit = state.check_site(site)?;
        let (mut mask, mut want) = (0usize, 0usize);
        for &(at, one) in given {
            let b = state.check_site(at)?;
            mask |= b;
            if one {
                want |= b;
            }
        }
        let (mut total, mut ones) = (0.0, 0.0);
        for i in (0..state.re.len()).filter(|&i| i & mask == want) {
            let p = state.probability(i);
            total += p;
            if i & bit != 0 {
                ones += p;
            }
        }
        if total > 0.0 {
            Ok(ones / total)
        } else {
            Err(ReferenceError::ZeroProbabilityCondition)
        }
    }

    fn project(
        &self,
        state: &mut ReferenceState,
        site: usize,
        one: bool,
        probability: f64,
    ) -> Result<(), ReferenceError> {
        let bit = state.check_site(site)?;
        if !(probability > 0.0 && probability <= 1.0 + PROBABILITY_ROUNDING) {
            return Err(ReferenceError::InvalidProbability(probability));
        }
        let norm = probability.sqrt();
        for (i, (re, im)) in state.re.iter_mut().zip(state.im.iter_mut()).enumerate() {
            if (i & bit != 0) == one {
                *re /= norm;
                *im /= norm;
            } else {
                (*re, *im) = (0.0, 0.0);
            }
        }
        Ok(())
    }

    fn fork(&self, state: &ReferenceState) -> Result<ReferenceState, ReferenceError> {
        Ok(state.clone())
    }

    /// Draws each shot's values in order, One when a uniform from a
    /// `SplitMix64` stream seeded with `seed` is below P(One | the shot's
    /// earlier values). Shots that drew the same values so far share that
    /// probability, so it is computed once per distinct prefix.
    fn sample(
        &self,
        state: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: SampleSeed,
    ) -> Result<Vec<bool>, ReferenceError> {
        for (at, &site) in sites.iter().enumerate() {
            if sites[..at].contains(&site) {
                return Err(ReferenceError::RepeatedSite(site));
            }
            state.check_site(site)?;
        }
        let mut rng = SplitMix64(u64::from(seed.get()));
        let mut known: BTreeMap<Vec<(usize, bool)>, f64> = BTreeMap::new();
        let mut values = Vec::with_capacity(shots * sites.len());
        for _ in 0..shots {
            let mut prefix = Vec::with_capacity(sites.len());
            for &site in sites {
                let p1 = if let Some(&p1) = known.get(&prefix) {
                    p1
                } else {
                    let p1 = self.conditional_probability_of_one(state, site, &prefix)?;
                    known.insert(prefix.clone(), p1);
                    p1
                };
                let one = rng.next_uniform() < p1;
                values.push(one);
                prefix.push((site, one));
            }
        }
        Ok(values)
    }

    fn expectation(
        &self,
        state: &ReferenceState,
        observable: &PauliSum,
    ) -> Result<Complex64, ReferenceError> {
        observable
            .terms()
            .iter()
            .try_fold(observable.identity(), |sum, term| {
                let (mut x, mut z) = (0usize, 0usize);
                for &(site, pauli) in term.factors() {
                    let bit = state.check_site(site)?;
                    match pauli {
                        Pauli::X => x |= bit,
                        Pauli::Y => (x, z) = (x | bit, z | bit),
                        Pauli::Z => z |= bit,
                    }
                }
                Ok(sum + term.coefficient() * state.pauli_expectation(x, z))
            })
    }

    fn shape(&self, state: &ReferenceState) -> Result<Mps, ReferenceError> {
        let n = state.sites;
        let bonds: Vec<usize> = (0..n.saturating_sub(1))
            .map(|cut| state.schmidt_rank(cut))
            .collect();
        let sites = (0..n)
            .map(|site| {
                let left = site.checked_sub(1).and_then(|c| bonds.get(c).copied());
                let right = bonds.get(site).copied();
                left.into_iter().chain([2]).chain(right).collect()
            })
            .collect();
        Ok(Mps::new(sites)?)
    }
}

#[cfg(test)]
mod tests;
