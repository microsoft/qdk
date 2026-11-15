// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gates on two sites in either order and at any distance: on neighbours
//! directly, on sites further apart by routing (`swap-back-2x`).
//!
//! A program applies two-site gates to any two qubits, while the chain
//! joins only neighbouring sites. A gate on two sites `low < high` that are
//! not neighbours moves the site at `high` down next to `low` by SWAPs on
//! neighbours (outbound), applies the gate, and moves the site back up
//! (return), so every site is back where the caller put it when the call
//! returns. The rules, stated for callers on
//! [`CanonicalMpsEngine`] (Routing):
//!
//! - **What a route drops.** The outbound SWAPs' splits apply the cutoff
//!   alone; the gate's and the return's apply the cutoff and the capacity.
//! - **Where the centre goes.** Each SWAP leaves the centre on the site
//!   the next one needs (left outbound, right on return), so once it
//!   reaches the first SWAP the route moves it by its splits alone, with no
//!   [`qr_move`](crate::algebra::SiteAlgebra::qr_move).
//! - **Its cost.** A gate on sites `d` apart takes `2(d − 1) + 1` splits
//!   without fusion; with fusion, its first and last may share theirs with
//!   the operations around it.

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain, gates::Truncation};
use crate::algebra::{CentreSide, SiteAlgebra};
use num_complex::Complex64;

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies `gate` (row-major, checked, site `first` the most
    /// significant bit) to the sites `first` and `second`; when they are
    /// not neighbours, by `swap-back-2x`.
    pub(super) fn apply_pair(
        &self,
        chain: &mut Chain<A::Tensor>,
        first: usize,
        second: usize,
        gate: &[Complex64],
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        let (low, high) = (first.min(second), first.max(second));
        let gate: [Complex64; 16] = if first < second {
            std::array::from_fn(|at| gate[at])
        } else {
            swap_operands(gate)
        };
        if high == low + 1 {
            let side = side_nearer_centre(chain.centre, low);
            return self.apply_adjacent(chain, low, &gate, side, Truncation::CutoffAndCapacity);
        }
        for cut in (low + 1..high).rev() {
            self.swap(chain, cut, CentreSide::Left, Truncation::CutoffOnly)?;
        }
        self.apply_adjacent(
            chain,
            low,
            &gate,
            CentreSide::Right,
            Truncation::CutoffAndCapacity,
        )?;
        for cut in low + 1..high {
            self.swap(chain, cut, CentreSide::Right, Truncation::CutoffAndCapacity)?;
        }
        Ok(())
    }

    /// A SWAP on the sites `cut` and `cut + 1`: one step of a route.
    fn swap(
        &self,
        chain: &mut Chain<A::Tensor>,
        cut: usize,
        side: CentreSide,
        truncation: Truncation,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        self.apply_adjacent(chain, cut, &SWAP, side, truncation)?;
        chain.statistics.swaps += 1;
        Ok(())
    }
}

/// The side of the pair at `cut` nearer `centre`, where a split leaves the
/// centre without moving it further.
fn side_nearer_centre(centre: usize, cut: usize) -> CentreSide {
    if centre <= cut {
        CentreSide::Left
    } else {
        CentreSide::Right
    }
}

/// A two-site gate on `[b, a]` rewritten for `[a, b]`.
fn swap_operands(m: &[Complex64]) -> [Complex64; 16] {
    let flip = |index: usize| ((index & 1) << 1) | (index >> 1);
    std::array::from_fn(|at| m[4 * flip(at / 4) + flip(at % 4)])
}

/// The SWAP gate: a route's step.
const SWAP: [Complex64; 16] = {
    let mut swap = [Complex64::ZERO; 16];
    swap[0] = Complex64::ONE;
    swap[4 + 2] = Complex64::ONE;
    swap[8 + 1] = Complex64::ONE;
    swap[15] = Complex64::ONE;
    swap
};
