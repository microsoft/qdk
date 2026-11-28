// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Splits: a gate on two neighbours, one of them the centre, applied and
//! split back into two sites by one [`gate_split`](SiteAlgebra::gate_split).
//! A split is the one place where the engine truncates the state's bonds,
//! to keep the state small.
//!
//! A window on two neighbours ends in one split, its operator the split's
//! gate. A window applied as a matrix product operator
//! ([`mpo`](super::mpo)) ends in one split per cut of its span, with the
//! identity as gate.
//!
//! ```text
//!  the pair:    ─ A ─── B ─       A or B the centre; the other sites are
//!                 │     │         isometries towards the pair
//!
//!  gated pair:  ─ A ─── B ─
//!                ┌┴─────┴┐
//!                │   G   │    =   ─ θ ─     G: the gate on both qubits;
//!                └┬─────┬┘         │ │      θ: A, B and G contracted into
//!                 │     │                   one tensor, its qubits free
//!
//!  SVD:         θ = U · S · V†    S: the Schmidt values across the cut
//!  truncate:    keep the largest values of S, at most the capacity's
//!               bond; unless that cap cuts, drop at most the cutoff's
//!               share of their squares; the kept values normalised
//!  new pair:    ─ U ─k─ S·V† ─    CentreSide::Right: the centre on cut + 1
//!               ─ U·S ─k─ V† ─    CentreSide::Left:  the centre on cut
//! ```
//!
//! [`GateSplitCall`] gives `θ`'s indices and layout. The centre is on one
//! of the two sites, and the rest of the chain is made of isometries. So
//! the split's singular value decomposition is the state's Schmidt
//! decomposition across the cut (the crate README's Concepts). Keeping the
//! largest values gives a closest state, in norm, of that bond. It loses
//! exactly the weight of the dropped squares. [`CanonicalMpsEngine`]'s
//! Truncation states the rules a caller relies on. The split multiplies the
//! retained weight by the fraction the algebra reports. Below the smallest
//! normal `f64`, floats lose digits. So the split fails with
//! [`CanonicalMpsError::WeightUnderflow`] instead of storing a weight below
//! it.
//!
//! The windows' operators are unitary and the chain's norm about 1. So the
//! gated pair is never zero, as [`GateSplitCall`] requires. A planned
//! non-unitary operator, `apply_kraus`, would have to keep it so.

use super::{CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsStatistics, chain::Chain};
use crate::site_algebra::{CentreSide, GateSplitCall, GateSplitOutcome, SiteAlgebra};
use num_complex::Complex64;

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies `gate` (row-major, site `cut` the most significant bit) to
    /// the sites `cut` and `cut + 1`, one of them the centre. Splits them,
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
        let new_pair = self.pair_gated_and_split(chain, cut, gate, side)?;
        chain.retained_weight =
            retained_weight_after(chain.retained_weight, new_pair.retained_fraction)?;
        let bond = new_pair.bond;
        chain.replace_pair(cut, bond, new_pair.left, new_pair.right, side);
        count_split(&mut chain.statistics, bond);
        Ok(())
    }

    /// The pair at `cut` with `gate` applied and split back into two
    /// sites by one [`gate_split`](SiteAlgebra::gate_split), truncated to
    /// the capacity's bond there and to the cutoff. The chain is unchanged;
    /// the caller stores the pair.
    fn pair_gated_and_split(
        &self,
        chain: &Chain<A::Tensor>,
        cut: usize,
        gate: [Complex64; 16],
        side: CentreSide,
    ) -> Result<GateSplitOutcome<A::Tensor>, CanonicalMpsError<A::Error>> {
        let (left, right) = (&chain.sites[cut], &chain.sites[cut + 1]);
        self.algebra
            .gate_split(&GateSplitCall {
                left: &left.tensor,
                right: &right.tensor,
                left_extent: left.left,
                bond: left.right,
                right_extent: right.right,
                gate,
                max_extent: chain.capacity_bonds[cut],
                cutoff: self.cutoff,
                centre: side,
            })
            .map_err(CanonicalMpsError::Algebra)
    }
}

/// `weight` times `fraction`: the retained weight after a split that kept
/// `fraction` of the squared singular values. Fails rather than fall below
/// the smallest normal `f64`.
fn retained_weight_after<E>(weight: f64, fraction: f64) -> Result<f64, CanonicalMpsError<E>> {
    // The fraction lies in (0, 1] (`GateSplitOutcome`), so the weight only
    // shrinks, and is never NaN.
    let weight = weight * fraction;
    if weight < f64::MIN_POSITIVE {
        return Err(CanonicalMpsError::WeightUnderflow);
    }
    Ok(weight)
}

/// Counts one split that kept `bond`.
fn count_split(statistics: &mut CanonicalMpsStatistics, bond: usize) {
    statistics.splits += 1;
    statistics.largest_bond_at_rest = statistics.largest_bond_at_rest.max(bond);
}

#[cfg(test)]
mod tests;
