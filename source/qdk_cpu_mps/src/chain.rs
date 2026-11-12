// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CanonicalMps`]: the state a [`CpuMpsEngine`](crate::CpuMpsEngine) holds
//! for its caller, a matrix product state in canonical form, and
//! [`SplitStatistics`], what keeping it small cost.
//!
//! A simulator holds one state per shot: it passes the state to every call of
//! the engine, forks it, may move it to another thread, and reads its
//! statistics to choose the capacity and judge the cost (the splits and SWAPs
//! taken, and the largest bonds kept, see [`SplitStatistics`]). This module
//! defines what that state is.
//!
//! **A matrix product state (MPS).** The state of `n` qubits has `2ⁿ`
//! amplitudes, one per string of values `s₀…sₙ₋₁`. An MPS writes each amplitude
//! as a product of `n` matrices, one per qubit, chosen by that qubit's value:
//! `ψ(s₀…sₙ₋₁) = T₀[s₀]·T₁[s₁]⋯Tₙ₋₁[sₙ₋₁]`. A qubit's two matrices form its
//! site tensor (a [`Site`], `T[a, s, b]`, with `s` the qubit's value and `a`,
//! `b` the row and the column), and the sites form a chain. Neighbouring sites
//! share a bond: the columns of one site's matrices match the rows of the next
//! one's, and their number is the bond's size (the first site has one row and
//! the last one column, so the product is a number). A product state needs
//! bonds of size 1; the more entangled the qubits on the two sides of a cut,
//! the larger the bond the cut needs, up to `2ᵏ` with `k` qubits on the smaller
//! side. The caller's capacity ([`MpsShape`](tensornet::MpsShape)) bounds the
//! bond at each cut (at most `χ`, the max bond dimension), and the engine
//! keeps the bonds within it by truncation (`engine::split`).
//!
//! **The canonical form.** The same state has many chains: the matrices on the
//! two sides of a bond can be multiplied by any invertible `X` and by `X⁻¹`
//! without changing an amplitude. The engine keeps a chain in which every site
//! but one is orthonormal: a site left of that one, its two matrices stacked
//! one above the other, has orthonormal columns (it is left-orthonormal), and a
//! site right of it, its two matrices side by side, has orthonormal rows
//! (right-orthonormal). The one site left, the orthogonality centre, holds the
//! state's norm: each orthonormal site contracts with its own conjugate to the
//! identity, so the state's squared norm is the sum of the centre's squared
//! entries. This is the mixed canonical form.
//!
//! **Why it matters.** With the centre on a site, the rest of the chain drops
//! out of any read of that site: the probability of a qubit's value is the
//! weight of that value's entries at the centre over the centre's whole weight.
//! A gate on two neighbours, one of which is the centre, changes only those two
//! sites and leaves the form of the others. So the engine moves the centre to
//! where an operation is (`engine::canonical_form`), and a read or a gate costs
//! what its sites and the distance the centre moves cost, not the chain's
//! length; and the split that follows a gate truncates the state at its best,
//! since the centre is in the pair (`engine::split`).
//!
//! [`Chain`] holds the sites, the centre's position, the capacity's bond at
//! each cut, the retained weight (the share of the state's weight the
//! truncations kept) and, while it is not split again, the pair of sites a
//! gate contracted into one block ([`ContractedPair`]); [`CanonicalMps`]
//! wraps it for the caller.
//! The module also holds the kernels that need no factorization: applying a
//! gate's matrix to a site or a pair, projecting a site, and weighing a
//! site's values; the engine composes them with the products, QR and SVD
//! ([`LinearAlgebra`](crate::linear_algebra::LinearAlgebra)).

use crate::linear_algebra::MatrixRef;
use num_complex::Complex64;
use std::cell::RefCell;

pub(crate) const fn c(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

/// The one-site identity, row-major.
const IDENTITY: [Complex64; 4] = [c(1.0), c(0.0), c(0.0), c(1.0)];

/// A state of a [`CpuMpsEngine`](crate::CpuMpsEngine): a chain of site
/// tensors in mixed canonical form, with the bond each cut may grow to.
///
/// A matrix product state holds the state of `n` qubits as a chain of `n`
/// small tensors, one per qubit (its site), each joined to its neighbours by
/// a bond. The more entangled the qubits on the two sides of a cut, the
/// larger the bond the cut needs; the capacity the caller passes to
/// [`MpsEngine::prepare`] bounds the bond at each cut (at most `χ`, the max
/// bond dimension).
///
/// The chain is kept in mixed canonical form: every site but one is
/// orthonormal, so it contracts with its own conjugate to the identity and
/// drops out of any read, and the one site left, the orthogonality centre,
/// holds the state's norm. So a read of a qubit, or a gate on two
/// neighbours, needs only its own sites once the centre is among them, and
/// the engine first moves the centre there, by one QR decomposition per site
/// crossed.
///
/// Every method that takes the state by shared reference may change its
/// chain: the reads ([`MpsEngine::probability_of_one`] and the others) move
/// the orthogonality centre, which changes the tensors but not the state
/// they describe; and the reads and [`MpsEngine::fork`] split a contracted
/// pair that fusion left unsplit (see Fusion on
/// [`CpuMpsEngine`](crate::CpuMpsEngine)), which
/// applies the truncation its gates deferred. So the chain sits in a
/// [`RefCell`]. The state is `Send`: a shot may run on another thread than
/// the one that prepared its state.
///
/// [`MpsEngine::prepare`]: tensornet::MpsEngine::prepare
/// [`MpsEngine::probability_of_one`]: tensornet::MpsEngine::probability_of_one
/// [`MpsEngine::fork`]: tensornet::MpsEngine::fork
#[derive(Debug)]
pub struct CanonicalMps {
    pub(crate) chain: RefCell<Chain>,
}

impl Clone for CanonicalMps {
    fn clone(&self) -> Self {
        Self {
            chain: RefCell::new(self.chain.borrow().clone()),
        }
    }
}

impl CanonicalMps {
    /// What the splits of this state's history cost so far. A state made by
    /// [`MpsEngine::fork`](tensornet::MpsEngine::fork) starts with its
    /// parent's history.
    #[must_use]
    pub fn split_statistics(&self) -> SplitStatistics {
        self.chain.borrow().statistics
    }
}

/// What the splits of a state's history cost: how many SVD splits it took,
/// how many SWAPs only routed a site, and the largest bonds the splits kept,
/// at rest and during routes.
///
/// A split at rest (a gate's, or a route's return SWAP's) keeps at most the
/// capacity's bond at its cut: with one χ at every cut, a largest bond at
/// rest below χ says the capacity cut nothing, and one at χ that it may
/// have, so a smaller χ would cut more. A route's outbound SWAPs keep
/// transient bonds of up to twice the capacity's largest bond (see
/// [`CpuMpsEngine`](crate::CpuMpsEngine)), which
/// [`MpsEngine::shape`](tensornet::MpsEngine::shape) no longer shows once
/// the route is undone: the larger of the two figures is the largest bond
/// the state held. The split counts are what the engine's time scales with:
/// a split's SVD is most of a gate's cost.
///
/// With fusion, a pair left unsplit counts when it is split, under its last
/// two-site operation: as transient if that was an outbound SWAP, at rest
/// otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SplitStatistics {
    pub(crate) splits: u64,
    pub(crate) swaps: u64,
    pub(crate) largest_bond_at_rest: usize,
    pub(crate) largest_transient_bond: usize,
}

impl SplitStatistics {
    /// The SVD splits of two adjacent sites, SWAPs included.
    #[must_use]
    pub fn splits(&self) -> u64 {
        self.splits
    }

    /// The SWAPs that brought two sites of a gate together or moved them
    /// back. Each takes a split of its own, except, with fusion, a route's
    /// first and last: the first shares the split of the pair it lands on
    /// when the operation before it left that pair unsplit, and the last
    /// shares its split with the operations that follow on its pair.
    #[must_use]
    pub fn swaps(&self) -> u64 {
        self.swaps
    }

    /// The largest bond a split at rest kept, at most the capacity's largest
    /// bond; 1 for a state without such splits.
    #[must_use]
    pub fn largest_bond_at_rest(&self) -> usize {
        self.largest_bond_at_rest.max(1)
    }

    /// The largest bond the split of a route's outbound SWAP kept, at most
    /// twice the capacity's largest bond; 1 for a state without routes.
    #[must_use]
    pub fn largest_transient_bond(&self) -> usize {
        self.largest_transient_bond.max(1)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Chain {
    pub(crate) sites: Vec<Site>,
    /// The orthogonality centre: the one site that is not orthonormal.
    pub(crate) centre: usize,
    /// The capacity's bond at each cut.
    pub(crate) capacity_bonds: Vec<usize>,
    /// The contracted pair of neighbours a gate applies to, if it is not
    /// split yet (with fusion, until another operation: a two-site gate on
    /// another pair, or any read or fork); its two sites in `sites` are
    /// stale until it is split.
    pub(crate) pair: Option<ContractedPair>,
    pub(crate) statistics: SplitStatistics,
    /// The retained weight: the product of the fractions of their total
    /// squared norm that the truncating splits kept. The chain itself is
    /// renormalized at each of them, so its squared norm stays about 1.
    pub(crate) retained_weight: f64,
}

impl Chain {
    /// The state |0…0⟩ on as many sites as `capacity_bonds` has cuts plus
    /// one.
    pub(crate) fn zero(capacity_bonds: Vec<usize>) -> Self {
        Self {
            sites: vec![Site::zero(); capacity_bonds.len() + 1],
            centre: 0,
            capacity_bonds,
            pair: None,
            statistics: SplitStatistics::default(),
            retained_weight: 1.0,
        }
    }

    /// Applies the one-site gate `m` (row-major) to `site`, in the
    /// contracted pair if `site` is one of its sites.
    pub(crate) fn apply_one(&mut self, site: usize, m: &[Complex64]) {
        match self.pair.as_mut() {
            Some(pair) if pair.cut == site || pair.cut + 1 == site => {
                let gate = if pair.cut == site {
                    kron(m, &IDENTITY)
                } else {
                    kron(&IDENTITY, m)
                };
                pair.apply(&gate);
            }
            _ => self.sites[site].apply(m),
        }
    }
}

/// What the split of a [`ContractedPair`] discards: the tail below the
/// cutoff, then what exceeds the capacity's bond at its cut; or, for a pair
/// whose last operation was an outbound SWAP of a route, the tail below the
/// cutoff alone (see [`CpuMpsEngine`](crate::CpuMpsEngine)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Truncation {
    /// A split at rest, after a gate or a return SWAP.
    CutoffAndCapacity,
    /// A split after an outbound SWAP, whose bond is at most twice the
    /// capacity's bond at the cut below.
    CutoffOnly,
}

/// The contracted block of the sites `cut` and `cut + 1`, with the gates
/// applied to it since it was contracted: `θ[a, s, t, b]` of extents
/// `[left, 2, 2, right]`, laid out at `a + left·(s + 2·(t + 2·b))`. It holds
/// the state's norm, as the centre does.
#[derive(Clone, Debug)]
pub(crate) struct ContractedPair {
    pub(crate) cut: usize,
    pub(crate) left: usize,
    pub(crate) right: usize,
    pub(crate) block: Vec<Complex64>,
    /// What its split discards, set by the last two-site operation applied.
    pub(crate) truncation: Truncation,
}

impl ContractedPair {
    /// Applies the two-site gate `m` (row-major, site `cut` the most
    /// significant bit).
    pub(crate) fn apply(&mut self, m: &[Complex64]) {
        let (l, theta) = (self.left, &mut self.block);
        for b in 0..self.right {
            for a in 0..l {
                let at = |s: usize, t: usize| a + l * (s + 2 * (t + 2 * b));
                let index = [at(0, 0), at(0, 1), at(1, 0), at(1, 1)];
                let v = index.map(|i| theta[i]);
                for (row, &i) in index.iter().enumerate() {
                    let m = &m[4 * row..4 * row + 4];
                    theta[i] = m[0] * v[0] + m[1] * v[1] + m[2] * v[2] + m[3] * v[3];
                }
            }
        }
    }
}

/// One site tensor `T[a, s, b]` of extents `[left, 2, right]`, column-major:
/// element `a + left·(s + 2·b)`. Read as a `2·left × right` matrix its row
/// is `(a, s)`, and as a `left × 2·right` matrix its column is `(s, b)`, both
/// without a copy.
#[derive(Clone, Debug)]
pub(crate) struct Site {
    pub(crate) left: usize,
    pub(crate) right: usize,
    pub(crate) elements: Vec<Complex64>,
}

impl Site {
    fn zero() -> Self {
        Self {
            left: 1,
            right: 1,
            elements: vec![c(1.0), c(0.0)],
        }
    }

    /// The tensor as a `2·left × right` matrix.
    pub(crate) fn rows_left(&self) -> MatrixRef<'_> {
        MatrixRef::new(2 * self.left, self.right, &self.elements)
    }

    /// The tensor as a `left × 2·right` matrix.
    pub(crate) fn cols_right(&self) -> MatrixRef<'_> {
        MatrixRef::new(self.left, 2 * self.right, &self.elements)
    }

    /// The squared norm of the elements whose physical index is `level`.
    pub(crate) fn weight_of(&self, level: usize) -> f64 {
        let l = self.left;
        (0..self.right)
            .map(|b| {
                let start = l * (level + 2 * b);
                self.elements[start..start + l]
                    .iter()
                    .map(Complex64::norm_sqr)
                    .sum::<f64>()
            })
            .sum()
    }

    pub(crate) fn weight(&self) -> f64 {
        self.elements.iter().map(Complex64::norm_sqr).sum()
    }

    /// Applies the one-site gate `m` (row-major) to the physical index.
    pub(crate) fn apply(&mut self, m: &[Complex64]) {
        let l = self.left;
        for b in 0..self.right {
            for a in 0..l {
                let (i0, i1) = (a + 2 * l * b, a + l + 2 * l * b);
                let (v0, v1) = (self.elements[i0], self.elements[i1]);
                self.elements[i0] = m[0] * v0 + m[1] * v1;
                self.elements[i1] = m[2] * v0 + m[3] * v1;
            }
        }
    }

    /// Keeps the elements whose physical index is `level`, zeroes the
    /// others, and divides the site by the kept elements' norm, so that the
    /// site, as the centre, holds a state of norm 1. The norm is computed on
    /// the elements divided by the largest of their moduli, so that the sum
    /// of squares, at least 1, does not underflow however small the kept
    /// weight; the kept elements are not all zero.
    pub(crate) fn project(&mut self, level: usize) {
        let l = self.left;
        for (at, x) in self.elements.iter_mut().enumerate() {
            if (at / l) % 2 != level {
                *x = c(0.0);
            }
        }
        let largest = self.elements.iter().map(|x| x.norm()).fold(0.0, f64::max);
        let scaled: f64 = self.elements.iter().map(|x| (x / largest).norm_sqr()).sum();
        let norm = scaled.sqrt();
        for x in &mut self.elements {
            *x = *x / largest / norm;
        }
    }
}

/// `a ⊗ b` of two one-site gates (row-major), with `a` on the most
/// significant bit.
fn kron(a: &[Complex64], b: &[Complex64]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 4, at % 4);
        a[2 * (row >> 1) + (col >> 1)] * b[2 * (row & 1) + (col & 1)]
    })
}
