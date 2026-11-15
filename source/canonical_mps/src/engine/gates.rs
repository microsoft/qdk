// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gates on two neighbours, fused into one pending gate until a split, and
//! the arithmetic of the gates the chain keeps pending on the host.
//!
//! A gate on two neighbours moves the centre into their pair and becomes
//! the pair's pending gate ([`FusedPair`]); the gates that follow on the
//! same pair, one-site gates on either site and routing SWAPs included,
//! multiply into it, and the first other operation (a two-site gate on
//! another pair, a read, a fork) splits the pair, by one
//! [`gate_split`](crate::algebra::SiteAlgebra::gate_split) for all of
//! them ([`split`](super::split)). The pair's gate starts as the two sites'
//! pending one-site gates, so they reach the backend in the same call.
//! Without fusion (the tests' comparison), each gate splits its pair at
//! once and leaves the centre on the side the caller names.
//!
//! The gates are row-major, the first site the most significant bit: a
//! one-site gate `G[s, p]` at `2s + p`, a two-site gate `G[(s, t), (p, q)]`
//! at `(2s + t)·4 + 2p + q`, as [`GateSplitCall`] reads them.
//!
//! [`GateSplitCall`]: crate::algebra::GateSplitCall

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain};
use crate::algebra::{CentreSide, SiteAlgebra};
use num_complex::Complex64;

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// Applies `gate` to the sites `cut` and `cut + 1`, whose split then
    /// drops by `truncation`. With fusion it leaves them pending, and the
    /// operation that splits them chooses the side; without, it splits them
    /// and leaves the centre on `side`.
    pub(super) fn apply_adjacent(
        &self,
        chain: &mut Chain<A::Tensor>,
        cut: usize,
        gate: &[Complex64; 16],
        side: CentreSide,
        truncation: Truncation,
    ) -> Result<(), CanonicalMpsError<A::Error>> {
        if chain.fused.is_none_or(|fused| fused.cut != cut) {
            self.split_pair_toward(chain, cut)?;
            let nearest = if chain.centre <= cut { cut } else { cut + 1 };
            self.move_centre(chain, nearest)?;
            chain.fused = Some(FusedPair {
                cut,
                gate: chain.take_pending_pair(cut),
                truncation,
            });
        }
        let fused = chain.fused.as_mut().expect("a pending pair at the cut");
        fused.truncation = truncation;
        fused.gate = multiply4(gate, &fused.gate);
        if self.fuses {
            Ok(())
        } else {
            self.split_pair(chain, side)
        }
    }
}

/// The gates applied to the sites `cut` and `cut + 1` since their last
/// split, as one 4×4 gate, and what their split drops.
#[derive(Clone, Copy, Debug)]
pub(super) struct FusedPair {
    pub(super) cut: usize,
    pub(super) gate: [Complex64; 16],
    pub(super) truncation: Truncation,
}

/// What a split drops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Truncation {
    /// A split at rest, after a gate or a return SWAP: the cutoff, then
    /// the capacity's bond.
    CutoffAndCapacity,
    /// A split after a route's outbound SWAP: the cutoff alone.
    CutoffOnly,
}

/// The one-site identity.
pub(super) const IDENTITY: [Complex64; 4] = [
    Complex64::ONE,
    Complex64::ZERO,
    Complex64::ZERO,
    Complex64::ONE,
];

/// `a ⊗ b` of two one-site gates, `a` on the most significant bit.
pub(super) fn kron(a: &[Complex64; 4], b: &[Complex64; 4]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 4, at % 4);
        a[2 * (row >> 1) + (col >> 1)] * b[2 * (row & 1) + (col & 1)]
    })
}

/// `a·b` of two 4×4 gates.
pub(super) fn multiply4(a: &[Complex64; 16], b: &[Complex64; 16]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 4, at % 4);
        (0..4).map(|k| a[4 * row + k] * b[4 * k + col]).sum()
    })
}

/// `a·b` of two one-site gates.
pub(super) fn multiply2(a: &[Complex64; 4], b: &[Complex64; 4]) -> [Complex64; 4] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 2, at % 2);
        (0..2).map(|k| a[2 * row + k] * b[2 * k + col]).sum()
    })
}
