// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ExactReference`]: the [`MpsEngine`] every other engine is checked
//! against, a full state vector that never truncates.
//!
//! It holds all 2ⁿ amplitudes of a state, so it gives the exact answer of
//! every operation, and it reports the exact chain: the bond at each cut is
//! the Schmidt rank of the state there, found by an elimination that counts
//! a pivot below 1e-10 as zero. It is for tests and small examples
//! (feature `conformance`), up to [`ExactReference::MAX_SITES`]
//! sites, and depends on nothing but this crate, so it runs wherever the
//! crate builds.
//!
//! Its arithmetic is its own, with no library beneath it, so its answers do
//! not depend on a library's version:
//!
//! - basis state `i` has site `k` as bit `k` of `i`, the order its dense
//!   vector is stored in, on which no engine call depends;
//! - its reads divide by ⟨ψ|ψ⟩, so they are of the normalized state:
//!   `apply` accepts a matrix within [`UNITARITY_TOLERANCE`] of unitary,
//!   which may change the norm by as much, and such changes add up (sixteen
//!   gates `diag(1, 1 + 4·10⁻¹⁰)` raise it by 1.3·10⁻⁸); the identity's
//!   coefficient of an observable is added as it is;
//! - it squares amplitudes as they are, so its conditional probabilities
//!   and projections hold, to rounding, for conditions and outcomes whose
//!   probability is well above [`f64::MIN_POSITIVE`] (about 2.2·10⁻³⁰⁸;
//!   one of about 10⁻³⁰⁰ is pinned); near and below it, squared amplitudes
//!   lose precision or underflow, so a conditional probability may be off
//!   or read 0 and a projection may lose weight (untested).
//!
//! [`UNITARITY_TOLERANCE`]: crate::checks::UNITARITY_TOLERANCE
//!
//! A capacity is accepted whatever its bonds: the reference never truncates,
//! so a state whose exact chain outgrows its capacity is reported by
//! [`shape`](MpsEngine::shape), for the caller to check.

use crate::{MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, Refusal, checks};
use num_complex::Complex64;
use std::collections::BTreeMap;

/// Why the reference did not do an operation: refused, or a state larger
/// than it holds.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ReferenceError {
    /// The call's arguments break the contract.
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// A capacity with more sites than the reference holds: a failure of
    /// the reference, not a refusal, since another engine holds such a
    /// state.
    #[error("the exact reference holds at most {max} sites, not {requested}")]
    TooManySites {
        /// The sites asked for.
        requested: usize,
        /// [`ExactReference::MAX_SITES`].
        max: usize,
    },
    /// A sample whose values do not fit in memory: a failure, not a
    /// refusal, since the arguments break no rule.
    #[error("{shots} shots of {sites} sites are more values than memory holds")]
    TooManyValues {
        /// The shots asked for.
        shots: usize,
        /// How many sites each shot draws.
        sites: usize,
    },
}

impl MpsEngineError for ReferenceError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            Self::TooManySites { .. } | Self::TooManyValues { .. } => None,
        }
    }
}

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
    pub fn site_count(&self) -> usize {
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

    /// The state with these amplitudes, basis state `i` with site `k` as
    /// bit `k`: a state no sequence of calls need reach, such as a
    /// truncated one, for tests.
    #[cfg(test)]
    pub(crate) fn from_amplitudes(amplitudes: &[Complex64]) -> Self {
        let sites = amplitudes.len().trailing_zeros() as usize;
        assert!(
            amplitudes.len() > 1 && amplitudes.len() == 1 << sites,
            "a state of n sites has 2^n amplitudes, n at least 1"
        );
        Self {
            re: amplitudes.iter().map(|a| a.re).collect(),
            im: amplitudes.iter().map(|a| a.im).collect(),
            sites,
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

    /// ⟨ψ|ψ⟩, the sum of [`probability`](Self::probability) over the basis
    /// states.
    fn norm(&self) -> f64 {
        (0..self.re.len()).map(|i| self.probability(i)).sum()
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
    /// Gaussian elimination with partial pivoting; a pivot below 1e-10 in
    /// magnitude counts as zero, so this is the numerical rank at absolute
    /// tolerance 1e-10.
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

    fn prepare(&self, capacity: &MpsShape) -> Result<ReferenceState, ReferenceError> {
        checks::prepare(capacity)?;
        let sites = capacity.site_count();
        if sites > Self::MAX_SITES {
            return Err(ReferenceError::TooManySites {
                requested: sites,
                max: Self::MAX_SITES,
            });
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
        checks::apply(state.sites, sites, matrix)?;
        let k = sites.len();
        let bits: Vec<usize> = sites.iter().map(|&site| 1 << site).collect();
        let dim = 1usize << k;
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
        checks::probability_of_one(state.sites, site)?;
        let bit = 1 << site;
        let ones: f64 = (0..state.re.len())
            .filter(|i| i & bit != 0)
            .map(|i| state.probability(i))
            .sum();
        Ok(ones / state.norm())
    }

    /// Sums the probabilities of the basis states that agree with `given`,
    /// as a marginal with projected modes contracts them: no copy.
    fn conditional_probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, ReferenceError> {
        checks::conditional_probability_of_one(state.sites, site, given)?;
        let bit = 1 << site;
        let (mut mask, mut want) = (0usize, 0usize);
        for &(at, one) in given {
            let b = 1 << at;
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
            Err(Refusal::ZeroProbabilityOutcome.into())
        }
    }

    /// Computes `p` as the kept amplitudes' weight over the whole state's,
    /// so it keeps ⟨ψ|ψ⟩ whatever that is.
    fn project(
        &self,
        state: &mut ReferenceState,
        site: usize,
        one: bool,
    ) -> Result<f64, ReferenceError> {
        checks::project(state.sites, site)?;
        let bit = 1 << site;
        let kept = |i: &usize| (i & bit != 0) == one;
        let total = state.norm();
        let kept_weight: f64 = (0..state.re.len())
            .filter(kept)
            .map(|i| state.probability(i))
            .sum();
        let p = kept_weight / total;
        if p.is_nan() || p <= 0.0 {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        let root = p.sqrt();
        for (i, (re, im)) in state.re.iter_mut().zip(state.im.iter_mut()).enumerate() {
            if kept(&i) {
                *re /= root;
                *im /= root;
            } else {
                (*re, *im) = (0.0, 0.0);
            }
        }
        Ok(p)
    }

    fn fork(&self, state: &ReferenceState) -> Result<ReferenceState, ReferenceError> {
        Ok(state.clone())
    }

    /// Draws each shot's values in order, One when a uniform from a
    /// `SplitMix64` stream seeded with `seed` is below P(One | the shot's
    /// earlier values). Shots that drew the same values so far share that
    /// probability, so it is computed once per distinct prefix. Fails with
    /// [`ReferenceError::TooManyValues`] when the values cannot be held
    /// (`shots × sites.len()` overflows a `usize`, or the allocation fails).
    fn sample(
        &self,
        state: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, ReferenceError> {
        checks::sample(state.sites, sites)?;
        let mut rng = SplitMix64(seed);
        let mut known: BTreeMap<Vec<(usize, bool)>, f64> = BTreeMap::new();
        let too_many = ReferenceError::TooManyValues {
            shots,
            sites: sites.len(),
        };
        let Some(count) = shots.checked_mul(sites.len()) else {
            return Err(too_many);
        };
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| too_many)?;
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

    /// Divides each Pauli term's ⟨ψ|P|ψ⟩ by ⟨ψ|ψ⟩, and adds the identity's
    /// coefficient as it is, since ⟨ψ|ψ⟩ / ⟨ψ|ψ⟩ is 1.
    fn expectation(
        &self,
        state: &ReferenceState,
        observable: &PauliSum,
    ) -> Result<Complex64, ReferenceError> {
        checks::expectation(state.sites, observable)?;
        let norm = state.norm();
        Ok(observable
            .terms()
            .iter()
            .fold(observable.identity_coefficient(), |sum, term| {
                let (mut x, mut z) = (0usize, 0usize);
                for &(site, pauli) in term.factors() {
                    let bit = 1 << site;
                    match pauli {
                        Pauli::X => x |= bit,
                        Pauli::Y => (x, z) = (x | bit, z | bit),
                        Pauli::Z => z |= bit,
                    }
                }
                sum + term.coefficient() * (state.pauli_expectation(x, z) / norm)
            }))
    }

    fn shape(&self, state: &ReferenceState) -> Result<MpsShape, ReferenceError> {
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
        Ok(MpsShape::new(sites)
            .expect("a chain of one or more qubit sites with Schmidt-rank bonds is a chain"))
    }

    /// ⟨ψ|ψ⟩: the reference holds the weight as the norm, since it never
    /// truncates and its projection keeps the norm. On every state its
    /// calls reach, that is 1 but for the drift the unitarity tolerance of
    /// `apply` allows.
    fn retained_weight(&self, state: &ReferenceState) -> Result<f64, ReferenceError> {
        Ok(state.norm())
    }
}
