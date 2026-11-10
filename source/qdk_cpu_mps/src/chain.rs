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
//! truncations kept), where each of the caller's sites sits in the chain
//! ([`Layout`]) and, while it is not split again, the pair of sites a gate
//! contracted into one block ([`ContractedPair`]); [`CanonicalMps`] wraps it
//! for the caller. Each part is in its own file:
//!
//! - [`site`]: a site tensor ([`Site`]) and its kernels, which need no
//!   factorization: applying a one-site gate, projecting the site, and
//!   weighing its values;
//! - [`contracted_pair`]: the block of two contracted sites
//!   ([`ContractedPair`]), applying a gate to it, and what its split
//!   discards ([`Truncation`]);
//! - [`layout`]: the map between the caller's sites and the chain's
//!   positions ([`Layout`]), the caller's order unless the routing
//!   `MeetHalfway` moved them;
//! - [`statistics`]: what the splits cost ([`SplitStatistics`]).
//!
//! The engine composes the kernels with the products, QR and SVD
//! ([`LinearAlgebra`](crate::linear_algebra::LinearAlgebra)).

mod contracted_pair;
mod layout;
mod site;
mod statistics;

pub(crate) use contracted_pair::{ContractedPair, Truncation};
pub(crate) use layout::Layout;
pub(crate) use site::Site;
pub use statistics::SplitStatistics;

use num_complex::Complex64;
use std::cell::RefCell;

pub(crate) const fn c(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

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
    /// Where each of the caller's sites sits in the chain: the caller's
    /// order, unless `MeetHalfway` left sites where its routes moved them.
    /// `sites`, `centre`, `capacity_bonds` and `pair` are indexed by chain
    /// position; the caller's site `s` is at `layout.position(s)`.
    pub(crate) layout: Layout,
    /// The lowest and the highest cut whose split, while the chain was out
    /// of the caller's order, kept more than the capacity's bond there,
    /// since the order was last restored (`MeetHalfway`'s deferred cut).
    pub(crate) over_capacity_cuts: Option<(usize, usize)>,
    /// Whether `MeetHalfway` fell back to `swap-back-2x` for the rest of the
    /// state's life: a bond exceeded the capacity, or the layout drifted too
    /// far. The chain is then in the caller's order.
    pub(crate) fell_back: bool,
}

impl Chain {
    /// The state |0…0⟩ on as many sites as `capacity_bonds` has cuts plus
    /// one.
    pub(crate) fn zero(capacity_bonds: Vec<usize>) -> Self {
        let sites = capacity_bonds.len() + 1;
        Self {
            sites: vec![Site::zero(); sites],
            centre: 0,
            capacity_bonds,
            pair: None,
            statistics: SplitStatistics::default(),
            retained_weight: 1.0,
            layout: Layout::in_order(sites),
            over_capacity_cuts: None,
            fell_back: false,
        }
    }

    /// Applies the one-site gate `m` (row-major) to the site at chain
    /// position `at`, in the contracted pair if it is one of its sites.
    pub(crate) fn apply_one(&mut self, at: usize, m: &[Complex64]) {
        match self.pair.as_mut() {
            Some(pair) if pair.cut == at || pair.cut + 1 == at => pair.apply_one(at, m),
            _ => self.sites[at].apply(m),
        }
    }
}
