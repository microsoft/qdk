// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Truncation: how the engine splits a contracted pair of sites back into two
//! by an SVD, the one place where it discards weight to keep the state small.
//!
//! A gate on two neighbouring sites contracts them into one block and applies
//! to it ([`gates`](super::gates)). To keep the chain, the engine then splits
//! the block back into two sites, and that split chooses the bond between them:
//! it is where the engine keeps the state within the caller's capacity, and
//! where it loses accuracy.
//!
//! The engine reads the block as a matrix, its rows (left bond, first qubit's
//! value) and its columns (second qubit's value, right bond), and factors it by
//! a singular value decomposition (SVD), `U·diag(s)·V†`. Because the pair holds
//! the centre and the rest of the chain is orthonormal
//! ([`chain`](crate::chain)), this is the state's Schmidt decomposition across
//! the cut between the two sites: the state is a sum of products of a left part
//! and a right part, one product per singular value, weighed by it. The values'
//! squares sum to the state's squared norm, and how many are not negligible
//! measures the entanglement across the cut: it is the bond the cut needs.
//! Keeping the `k` largest values and dropping the others gives the closest
//! state, in norm, whose bond at that cut is at most `k`, and loses exactly the
//! weight of the dropped squares. That is what the two settings trade: the
//! cutoff bounds the weight a split may drop, so the bond is as small as the
//! state allows; `χ`, the capacity's bond, bounds the bond, whatever weight
//! that drops. The rules, stated for callers on [`CpuMpsEngine`]:
//!
//! - **What it keeps.** It drops the smallest values whose squares sum to at
//!   most the cutoff's share of all their squares, never the largest value,
//!   then keeps at most the capacity's bond at its cut. The split after a SWAP
//!   that moves a site toward a gate's other site (a route's outbound SWAP,
//!   [`gates`](super::gates)) applies the cutoff alone.
//! - **The weight.** It scales the kept values so that they hold the block's
//!   squared norm, and multiplies the state's retained weight by the fraction
//!   they held, both computed on the values divided by the largest, so that no
//!   sum of squares underflows. It fails with [`CpuMpsError::WeightUnderflow`]
//!   when the weight would fall below the smallest normal `f64`, and with
//!   [`CpuMpsError::Factorization`] when the SVD fails; the pair is lost either
//!   way.
//! - **Where the centre goes.** It multiplies the kept values into the side the
//!   caller asks for, which becomes the centre; the other side keeps
//!   orthonormal singular vectors, so the form holds.
//! - **What it reports.** It counts the split and records its bond as the
//!   largest at rest or, for an outbound SWAP, the largest transient one
//!   ([`CanonicalMps::split_statistics`]).
//!
//! [`Engine::split_pair`] takes the SVD, chooses the bond by [`Engine::kept`],
//! rescales by [`Engine::kept_rescale`] and writes the two sites;
//! [`Engine::split_pair_toward`] splits toward the site an operation is about
//! to need.
//!
//! [`CpuMpsEngine`]: crate::CpuMpsEngine
//! [`CanonicalMps::split_statistics`]: crate::CanonicalMps::split_statistics

use super::Engine;
use crate::{
    CpuMpsError,
    chain::{Chain, ContractedPair, Site, Truncation},
    linear_algebra::{LinearAlgebra, MatrixRef},
};

/// Where a split leaves the orthogonality centre: on the left or the right
/// site of the pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CentreSide {
    Left,
    Right,
}

impl<A: LinearAlgebra> Engine<A> {
    /// Splits the contracted pair, if any, and leaves the centre on the side
    /// of the pair nearer the site `toward`.
    pub(super) fn split_pair_toward(
        &self,
        chain: &mut Chain,
        toward: usize,
    ) -> Result<(), CpuMpsError> {
        match &chain.pair {
            Some(pair) => {
                let side = if toward <= pair.cut {
                    CentreSide::Left
                } else {
                    CentreSide::Right
                };
                self.split_pair(chain, side)
            }
            None => Ok(()),
        }
    }

    /// Splits the contracted pair by one SVD, truncated, and leaves the
    /// centre on `side`.
    pub(super) fn split_pair(
        &self,
        chain: &mut Chain,
        side: CentreSide,
    ) -> Result<(), CpuMpsError> {
        let Some(ContractedPair {
            cut,
            left,
            right,
            block,
            truncation,
        }) = chain.pair.take()
        else {
            return Ok(());
        };
        let svd = self
            .algebra
            .svd(MatrixRef::new(2 * left, 2 * right, &block))?;
        let max_bond = match truncation {
            Truncation::CutoffAndCapacity => chain.capacity_bonds[cut],
            // An outbound SWAP's split keeps every value above the cutoff,
            // and its bond is still at most twice `capacity_bonds[cut − 1]`,
            // so a route's transient bond stays within twice the capacity's
            // largest bond. A route moves only the higher site, down
            // (`apply_pair`), so an outbound SWAP at `cut` (`cut ≥ 1`, as
            // the route ends at the pair at `low`, below it) has just moved
            // it from `cut + 1` to `cut`. Left of the cut are then the
            // caller's sites `0..cut` and the moved site: their Schmidt rank
            // is at most twice the rank at the caller's cut `cut − 1`. The
            // block's `2 · left` rows bound it too: `left` is the bond at
            // cut `cut − 1`, which no split of the route has set yet (they
            // run from the top cut down), so it is a bond the chain holds
            // at rest, at most `capacity_bonds[cut − 1]`. Truncating to
            // twice that would never bind. (A site moving up, not built,
            // would have right of the cut the caller's sites right of cut
            // `cut + 1` and the moved site, a bond of at most twice
            // `capacity_bonds[cut + 1]`.)
            Truncation::CutoffOnly => svd.singular_values.len(),
        };
        let bond = self.kept(&svd.singular_values, max_bond);
        let rescale = Self::kept_rescale(chain, &svd.singular_values, bond)?;
        chain.statistics.splits += 1;
        let largest = match truncation {
            Truncation::CutoffAndCapacity => &mut chain.statistics.largest_bond_at_rest,
            Truncation::CutoffOnly => &mut chain.statistics.largest_transient_bond,
        };
        *largest = (*largest).max(bond);
        let values = &svd.singular_values[..bond];
        let rank = svd.singular_values.len();
        // The first `bond` columns of U and rows of V†.
        let mut left_site = svd.u.into_elements();
        left_site.truncate(2 * left * bond);
        let v_adjoint = svd.v_adjoint.into_elements();
        let mut right_site = Vec::with_capacity(bond * 2 * right);
        for column in v_adjoint.chunks_exact(rank) {
            right_site.extend_from_slice(&column[..bond]);
        }
        match side {
            CentreSide::Left => {
                for (column, value) in left_site.chunks_exact_mut(2 * left).zip(values) {
                    for x in column {
                        *x *= value * rescale;
                    }
                }
                chain.centre = cut;
            }
            CentreSide::Right => {
                for column in right_site.chunks_exact_mut(bond) {
                    for (x, value) in column.iter_mut().zip(values) {
                        *x *= value * rescale;
                    }
                }
                chain.centre = cut + 1;
            }
        }
        chain.sites[cut] = Site {
            left,
            right: bond,
            elements: left_site,
        };
        chain.sites[cut + 1] = Site {
            left: bond,
            right,
            elements: right_site,
        };
        Ok(())
    }

    /// How many of `values` (nonincreasing) a split keeps: drop the tail
    /// whose squares sum to at most the cutoff's share, but never the first
    /// value, then keep at most `max_bond` (at least one: a shape has no
    /// extent zero).
    /// The squares are of the values divided by the largest, so that none
    /// underflows however small the values.
    pub(super) fn kept(&self, values: &[f64], max_bond: usize) -> usize {
        let largest = values[0];
        let square = |s: f64| (s / largest) * (s / largest);
        let total: f64 = values.iter().map(|&s| square(s)).sum();
        let allowed = self.cutoff * total;
        let mut keep = values.len();
        let mut discarded = 0.0;
        while keep > 1 {
            let next = discarded + square(values[keep - 1]);
            if next > allowed {
                break;
            }
            discarded = next;
            keep -= 1;
        }
        keep.min(max_bond)
    }

    /// The factor by which to multiply the first `bond` of the singular
    /// `values` so that they hold the squared norm of all of them: the
    /// split's kept part then has the squared norm of the whole block. It
    /// also multiplies the chain's retained weight by the fraction of the
    /// squared values that the first `bond` hold. Both are computed on the
    /// values divided by the largest, so that the sums of squares, at least
    /// 1, do not underflow. Fails with [`CpuMpsError::WeightUnderflow`] when
    /// the weight would fall below [`f64::MIN_POSITIVE`], or is NaN.
    pub(super) fn kept_rescale(
        chain: &mut Chain,
        values: &[f64],
        bond: usize,
    ) -> Result<f64, CpuMpsError> {
        if bond == values.len() {
            return Ok(1.0);
        }
        let largest = values[0];
        let square = |s: &f64| (s / largest) * (s / largest);
        let total: f64 = values.iter().map(square).sum();
        let kept: f64 = values[..bond].iter().map(square).sum();
        let weight = chain.retained_weight * (kept / total);
        if weight.is_nan() || weight < f64::MIN_POSITIVE {
            return Err(CpuMpsError::WeightUnderflow);
        }
        chain.retained_weight = weight;
        Ok((total / kept).sqrt())
    }
}
