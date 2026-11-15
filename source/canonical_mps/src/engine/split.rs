// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Truncation: the split of the pending pair back into two sites, by one
//! [`gate_split`](SiteAlgebra::gate_split), the one place where the engine
//! drops weight to keep the state small.
//!
//! Because the pair holds the centre and the rest of the chain is made of
//! isometries, the split's singular value decomposition is the state's
//! Schmidt decomposition across the pair's cut: keeping the largest values
//! gives the closest state, in norm, of that bond, and loses exactly the
//! weight of the dropped squares. The rules, stated for callers on
//! [`CanonicalMpsEngine`] (Truncation):
//!
//! - **What it keeps.** The algebra drops the tail within the engine's
//!   cutoff, never the largest value; a split at rest keeps at most the
//!   capacity's bond at its cut, and a route's outbound SWAP's split has
//!   no cap ([`Truncation`]).
//! - **The weight.** The algebra normalises the values it keeps, and the
//!   split multiplies the state's retained weight by the share they held,
//!   failing with [`CanonicalMpsError::WeightUnderflow`] when the weight
//!   would fall below the smallest normal `f64`. The pair is lost then, as
//!   when the algebra fails.
//! - **Where the centre goes.** The kept values go into the side the
//!   caller names, which becomes the centre.
//! - **What it reports.** It counts the split and records its bond as the
//!   largest at rest or, after an outbound SWAP, the largest transient one.
//!
//! The pair's gate is unitary and the chain's norm about 1, so the gated
//! pair is never zero, as [`GateSplitCall`] requires. A non-unitary
//! operator (the planned `apply_kraus`) would have to keep it so.

use super::{
    CanonicalMpsEngine, CanonicalMpsError,
    chain::{Chain, Site},
    gates::{FusedPair, Truncation},
};
use crate::algebra::{CentreSide, GateSplitCall, SiteAlgebra};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Splits the pending pair, if any, and leaves the centre on the side
    /// of the pair nearer the site `toward`.
    pub(super) fn split_pair_toward(
        &self,
        chain: &mut Chain<A::Tensor>,
        toward: usize,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        match chain.fused {
            Some(fused) => {
                let side = if toward <= fused.cut {
                    CentreSide::Left
                } else {
                    CentreSide::Right
                };
                self.split_pair(chain, side)
            }
            None => Ok(()),
        }
    }

    /// Splits the pending pair, if any, truncated, and leaves the centre on
    /// `side`.
    pub(super) fn split_pair(
        &self,
        chain: &mut Chain<A::Tensor>,
        side: CentreSide,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let Some(FusedPair {
            cut,
            gate,
            truncation,
        }) = chain.fused.take()
        else {
            return Ok(());
        };
        let max_extent = match truncation {
            Truncation::CutoffAndCapacity => chain.capacity_bonds[cut],
            // An outbound SWAP's bond stays within twice the capacity's bond
            // at the cut below it without a cap of its own (Routing on
            // `CanonicalMpsEngine`): a route moves only the higher site,
            // down, so left of the cut lie the caller's sites left of
            // `cut − 1` and the moved site.
            Truncation::CutoffOnly => usize::MAX,
        };
        let (left, right) = (&chain.sites[cut], &chain.sites[cut + 1]);
        let (left_extent, right_extent) = (left.left, right.right);
        let outcome = self
            .algebra
            .gate_split(&GateSplitCall {
                left: &left.tensor,
                right: &right.tensor,
                left_extent,
                bond: left.right,
                right_extent,
                gate,
                max_extent,
                cutoff: self.cutoff,
                centre: side,
            })
            .map_err(CanonicalMpsError::Algebra)?;
        // The fraction lies in (0, 1] (`GateSplitOutcome`), so the weight
        // only shrinks, and is never NaN.
        let weight = chain.retained_weight * outcome.retained_fraction;
        if weight < f64::MIN_POSITIVE {
            return Err(CanonicalMpsError::WeightUnderflow);
        }
        chain.retained_weight = weight;
        let bond = outcome.bond;
        chain.sites[cut] = Site {
            left: left_extent,
            right: bond,
            tensor: outcome.left,
        };
        chain.sites[cut + 1] = Site {
            left: bond,
            right: right_extent,
            tensor: outcome.right,
        };
        chain.centre = match side {
            CentreSide::Left => cut,
            CentreSide::Right => cut + 1,
        };
        let statistics = &mut chain.statistics;
        statistics.splits += 1;
        let largest = match truncation {
            Truncation::CutoffAndCapacity => &mut statistics.largest_bond_at_rest,
            Truncation::CutoffOnly => &mut statistics.largest_transient_bond,
        };
        *largest = (*largest).max(bond);
        Ok(())
    }
}
