// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Truncation: the split of two neighbours, a gate applied, back into two
//! sites, by one [`gate_split`](SiteAlgebra::gate_split), the one place
//! where the engine truncates the state's bonds to keep the state small;
//! an MPO's factorisation drops only an operator's values at or below
//! `1e-12` relative ([`OPERATOR_RANK_CUTOFF`]).
//!
//! A window on two neighbours ends in one split, its operator the split's
//! gate; a window applied as a matrix product operator
//! ([`mpo`](super::mpo)) ends in one split per cut of its span, with the
//! identity as gate. Because the centre is on one of the two sites and the
//! rest of the chain is made of isometries, the split's singular value
//! decomposition is the state's Schmidt decomposition across the cut:
//! keeping the largest values gives the closest state, in norm, of that
//! bond, and loses exactly the weight of the dropped squares. The rules,
//! stated for callers on [`CanonicalMpsEngine`] (Truncation):
//!
//! - **What it keeps.** The algebra drops the tail within the engine's
//!   cutoff, never the largest value, and keeps at most the capacity's
//!   bond at the cut: every cut is a caller's cut, where the state must
//!   fit the capacity anyway.
//! - **The weight.** The algebra normalises the values it keeps, and the
//!   split multiplies the state's retained weight by the share they held,
//!   failing with [`CanonicalMpsError::WeightUnderflow`] when the weight
//!   would fall below the smallest normal `f64`. The window is lost then,
//!   as when the algebra fails.
//! - **Where the centre goes.** The kept values go into the side the
//!   caller names, which becomes the centre.
//! - **What it reports.** It counts the split and records its bond as the
//!   largest at rest.
//!
//! The windows' operators are unitary and the chain's norm about 1, so the
//! gated pair is never zero, as [`GateSplitCall`] requires. A non-unitary
//! operator (the planned `apply_kraus`) would have to keep it so.
//!
//! [`OPERATOR_RANK_CUTOFF`]: super::policy::OPERATOR_RANK_CUTOFF

use super::{
    CanonicalMpsEngine, CanonicalMpsError,
    chain::{Chain, Site},
};
use crate::algebra::{CentreSide, GateSplitCall, SiteAlgebra};
use num_complex::Complex64;

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies `gate` (row-major, site `cut` the most significant bit) to
    /// the sites `cut` and `cut + 1`, one of them the centre, splits them,
    /// truncated, and leaves the centre on `side`.
    pub(super) fn split(
        &self,
        chain: &mut Chain<A::Tensor>,
        cut: usize,
        gate: [Complex64; 16],
        side: CentreSide,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        debug_assert!(
            chain.centre == cut || chain.centre == cut + 1,
            "a split with the centre outside its pair"
        );
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
                max_extent: chain.capacity_bonds[cut],
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
        statistics.largest_bond_at_rest = statistics.largest_bond_at_rest.max(bond);
        Ok(())
    }
}
