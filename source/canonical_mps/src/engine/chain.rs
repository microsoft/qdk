// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Chain`]: what a state holds, its sites in canonical form and the work
//! still pending on them, and [`SplitStatistics`], what keeping it small
//! cost.
//!
//! The chain holds, besides its sites, what lets a gate cost no backend
//! call until a split or a move of the centre needs one: one pending
//! one-site gate per site, applied by the next call that touches the site
//! (a unitary on a site's qubit keeps the canonical form), and the pending
//! pair ([`FusedPair`]), a 4×4 gate on two neighbours that their next
//! split applies.

use super::CanonicalMpsError;
use super::gates::{FusedPair, IDENTITY, kron, multiply2, multiply4};
use crate::algebra::SiteAlgebra;
use num_complex::Complex64;
use tensornet::MpsShape;

/// The chain of a state: its sites, the centre, the capacity, the work
/// pending on the host, the retained weight and the statistics.
pub(super) struct Chain<T> {
    pub(super) sites: Vec<Site<T>>,
    /// The one-site gate (row-major) each site's tensor still owes, if any.
    pub(super) pending: Vec<Option<[Complex64; 4]>>,
    /// The orthogonality centre: the one site that is not an isometry.
    pub(super) centre: usize,
    /// The capacity's bond at each cut.
    pub(super) capacity_bonds: Vec<usize>,
    /// The pair of neighbours whose gates wait for one split, if any; the
    /// centre is one of its two sites.
    pub(super) fused: Option<FusedPair>,
    /// The product of the shares of their squared values the splits kept.
    /// The chain itself is normalised at each split, so its squared norm
    /// stays about 1.
    pub(super) retained_weight: f64,
    pub(super) statistics: SplitStatistics,
}

/// A site tensor `T[a, s, b]`, of extents `[left, 2, right]`.
pub(super) struct Site<T> {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) tensor: T,
}

impl<T> Site<T> {
    /// The number of elements the site holds.
    pub(super) fn len(&self) -> usize {
        2 * self.left * self.right
    }
}

impl<T> Chain<T> {
    /// |0…0⟩ on `capacity_bonds.len() + 1` sites, the centre on site 0.
    pub(super) fn zero<A: SiteAlgebra<Tensor = T>>(
        algebra: &A,
        capacity_bonds: Vec<usize>,
    ) -> Result<Self, CanonicalMpsError<A::Error>> {
        let n = capacity_bonds.len() + 1;
        let zero = [Complex64::ONE, Complex64::ZERO];
        let sites = (0..n)
            .map(|_| {
                algebra.upload(&zero).map(|tensor| Site {
                    left: 1,
                    right: 1,
                    tensor,
                })
            })
            .collect::<Result<_, _>>()
            .map_err(CanonicalMpsError::Algebra)?;
        Ok(Self {
            sites,
            pending: vec![None; n],
            centre: 0,
            capacity_bonds,
            fused: None,
            retained_weight: 1.0,
            statistics: SplitStatistics::default(),
        })
    }

    /// A copy whose sites are new tensors, duplicated by `algebra`.
    pub(super) fn duplicate<A: SiteAlgebra<Tensor = T>>(
        &self,
        algebra: &A,
    ) -> Result<Self, CanonicalMpsError<A::Error>> {
        let sites = self
            .sites
            .iter()
            .map(|site| {
                algebra
                    .duplicate(&site.tensor, site.len())
                    .map(|tensor| Site {
                        left: site.left,
                        right: site.right,
                        tensor,
                    })
            })
            .collect::<Result<_, _>>()
            .map_err(CanonicalMpsError::Algebra)?;
        Ok(Self {
            sites,
            pending: self.pending.clone(),
            centre: self.centre,
            capacity_bonds: self.capacity_bonds.clone(),
            fused: self.fused,
            retained_weight: self.retained_weight,
            statistics: self.statistics,
        })
    }

    /// Applies the one-site gate `m` (row-major, checked) to `site`: into
    /// the pending pair's gate when `site` is one of its two, otherwise
    /// into the site's pending gate.
    pub(super) fn apply_one(&mut self, site: usize, m: &[Complex64]) {
        let m: [Complex64; 4] = std::array::from_fn(|at| m[at]);
        match self.fused.as_mut() {
            Some(fused) if fused.cut == site || fused.cut + 1 == site => {
                let gate = if fused.cut == site {
                    kron(&m, &IDENTITY)
                } else {
                    kron(&IDENTITY, &m)
                };
                fused.gate = multiply4(&gate, &fused.gate);
            }
            _ => {
                let pending = &mut self.pending[site];
                *pending = Some(match pending {
                    Some(earlier) => multiply2(&m, earlier),
                    None => m,
                });
            }
        }
    }

    /// The pending gates of `cut` and `cut + 1` as one 4×4 gate, cleared:
    /// what the pair's split applies first.
    pub(super) fn take_pending_pair(&mut self, cut: usize) -> [Complex64; 16] {
        let left = self.pending[cut].take().unwrap_or(IDENTITY);
        let right = self.pending[cut + 1].take().unwrap_or(IDENTITY);
        kron(&left, &right)
    }

    /// The chain's shape: one qubit per site, with the bond each cut holds.
    /// The pending pair, if any, has not changed its bond yet.
    pub(super) fn shape(&self) -> MpsShape {
        let n = self.sites.len();
        let sites = self
            .sites
            .iter()
            .enumerate()
            .map(|(at, site)| {
                let left = (at > 0).then_some(site.left);
                let right = (at + 1 < n).then_some(site.right);
                left.into_iter().chain([2]).chain(right).collect()
            })
            .collect();
        MpsShape::new(sites).expect("a chain whose neighbours share each bond")
    }
}

/// What the splits and moves of a state's history cost: how many splits it
/// took, how many SWAPs only routed a site, how many times the centre moved
/// by one site, and the largest bonds the splits kept, at rest and during
/// routes.
///
/// A split at rest (a gate's, or a route's return SWAP's) keeps at most the
/// capacity's bond at its cut: with one `χ` at every cut, a largest bond at
/// rest below `χ` says the capacity cut nothing, and one at `χ` that it may
/// have, so a smaller `χ` would cut more. A route's outbound SWAPs keep
/// transient bonds of up to twice the capacity's largest bond (Routing on
/// [`CanonicalMpsEngine`](super::CanonicalMpsEngine)), which
/// [`MpsEngine::shape`](tensornet::MpsEngine::shape) no longer shows once
/// the route is undone: the larger of the two figures is the largest bond
/// the state held. The splits and the moves of the centre are what the
/// engine's time scales with: each is one call of the backend.
///
/// With fusion, a pair left pending counts when it is split, under its last
/// two-site operation: as transient if that was an outbound SWAP, at rest
/// otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SplitStatistics {
    pub(super) splits: u64,
    pub(super) swaps: u64,
    pub(super) centre_moves: u64,
    pub(super) largest_bond_at_rest: usize,
    pub(super) largest_transient_bond: usize,
}

impl SplitStatistics {
    /// The splits of two neighbouring sites, a SWAP's included: each one
    /// [`gate_split`](crate::algebra::SiteAlgebra::gate_split).
    #[must_use]
    pub fn splits(&self) -> u64 {
        self.splits
    }

    /// The SWAPs that brought two sites of a gate together or moved them
    /// back. Each takes a split of its own, except, with fusion, a route's
    /// first and last: the first shares the split of the pair it lands on
    /// when the operation before it left that pair pending, and the last
    /// shares its split with the operations that follow on its pair.
    #[must_use]
    pub fn swaps(&self) -> u64 {
        self.swaps
    }

    /// The moves of the centre by one site, outside the splits: each one
    /// [`qr_move`](crate::algebra::SiteAlgebra::qr_move).
    #[must_use]
    pub fn centre_moves(&self) -> u64 {
        self.centre_moves
    }

    /// The largest bond a split at rest kept, at most the capacity's
    /// largest bond; 1 for a state without such splits.
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
