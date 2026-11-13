// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Gates on two sites: on neighbours, on sites further apart by routing, and
//! fused into one block until a split.
//!
//! A program applies two-site gates to any two qubits, in either order, while
//! the chain joins only neighbouring sites ([`chain`](crate::chain)). (A
//! one-site gate needs none of this: it acts on its site's tensor alone,
//! wherever the centre is.) A gate on two neighbours moves the centre into the
//! pair, contracts the pair's two tensors into one block and applies the gate
//! to it; the block is split again by one SVD, which truncates
//! ([`split`](super::split)).
//!
//! A gate on two sites `low < high` that are not neighbours is routed
//! (`swap-back-2x`): the engine moves the site at `high` down next to `low` by
//! SWAP gates on neighbours (the outbound SWAPs), applies the gate, and moves
//! the site back up (the return SWAPs). With fusion, a gate on two neighbours
//! leaves their block unsplit, and the gates that follow on the same pair,
//! routing SWAPs and one-site gates on either site included, apply to it; the
//! first other operation (a two-site gate on another pair, a read, a fork)
//! splits it, by one SVD for all of them. The rules, stated for callers on
//! [`CpuMpsEngine`] (Routing, Fusion):
//!
//! - **The caller's order.** When the call returns, every site is back where
//!   the caller put it, so no read and no shape shows the route.
//! - **What a route discards.** Only what the cutoff drops at the outbound
//!   SWAPs' splits, and what the cutoff and the capacity drop at the gate's and
//!   the return's ([`split`](super::split)). The outbound splits apply no
//!   capacity, so during a route a bond may reach twice the capacity's largest
//!   bond.
//! - **Its cost.** A gate on sites `d` apart takes `2(d − 1) + 1` splits
//!   without fusion (with fusion, its first and last may share theirs with the
//!   operations around it). The centre rides along: each SWAP's split leaves it
//!   on the site the next one needs, so once it reaches the first SWAP the
//!   route takes no QR.
//! - **Without fusion,** a gate on neighbours splits its pair and leaves the
//!   centre on the side nearer where it was.
//!
//! [`Engine::apply_pair`] orders the operands and routes;
//! [`Engine::apply_adjacent`] brings the centre into the pair, contracts it
//! unless it is the pair already pending, applies the gate, and splits it when
//! not fusing; [`Engine::swap`] is one step of a route.
//!
//! [`CpuMpsEngine`]: crate::CpuMpsEngine

use super::{Engine, split::CentreSide};
use crate::{
    CpuMpsError,
    chain::{Chain, ContractedPair, Truncation, c},
    linear_algebra::LinearAlgebra,
};
use num_complex::Complex64;

/// The SWAP gate, row-major, on two sites with the first as the most
/// significant bit: a route's step.
const SWAP: [Complex64; 16] = [
    c(1.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(1.0),
    c(0.0),
    c(0.0),
    c(1.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(0.0),
    c(1.0),
];

/// A two-site gate on `[b, a]` rewritten for `[a, b]`: both are row-major
/// with the first site as the most significant bit.
fn swap_operands(m: &[Complex64]) -> [Complex64; 16] {
    let flip = |index: usize| ((index & 1) << 1) | (index >> 1);
    std::array::from_fn(|at| m[4 * flip(at / 4) + flip(at % 4)])
}

impl<A: LinearAlgebra> Engine<A> {
    /// Applies `gate` (row-major, site `first` the most significant bit) to
    /// the sites `first` and `second`; when they are not neighbours, by
    /// `swap-back-2x` (see [`CpuMpsEngine`]).
    ///
    /// [`CpuMpsEngine`]: crate::CpuMpsEngine
    pub(super) fn apply_pair(
        &self,
        chain: &mut Chain,
        first: usize,
        second: usize,
        gate: &[Complex64],
    ) -> Result<(), CpuMpsError> {
        let (low, high) = (first.min(second), first.max(second));
        let swapped;
        let gate = if first < second {
            gate
        } else {
            swapped = swap_operands(gate);
            &swapped
        };
        if high == low + 1 {
            let side = Self::side_nearer_centre(chain, low);
            return self.apply_adjacent(chain, low, gate, side, Truncation::CutoffAndCapacity);
        }
        for cut in (low + 1..high).rev() {
            self.swap(chain, cut, CentreSide::Left, Truncation::CutoffOnly)?;
        }
        self.apply_adjacent(
            chain,
            low,
            gate,
            CentreSide::Right,
            Truncation::CutoffAndCapacity,
        )?;
        for cut in low + 1..high {
            self.swap(chain, cut, CentreSide::Right, Truncation::CutoffAndCapacity)?;
        }
        Ok(())
    }

    /// Applies `gate` (row-major, site `cut` the most significant bit) to
    /// sites `cut` and `cut + 1`, whose split then discards by `truncation`.
    /// Without fusion it splits them and leaves the centre on `side`; with
    /// fusion it leaves them unsplit, and the operation that splits them
    /// chooses the side.
    pub(super) fn apply_adjacent(
        &self,
        chain: &mut Chain,
        cut: usize,
        gate: &[Complex64],
        side: CentreSide,
        truncation: Truncation,
    ) -> Result<(), CpuMpsError> {
        if chain.pair.as_ref().is_none_or(|pair| pair.cut != cut) {
            self.split_pair_toward(chain, cut)?;
            let nearest = if chain.centre <= cut { cut } else { cut + 1 };
            self.move_centre(chain, nearest)?;
            let (left, right) = (chain.sites[cut].left, chain.sites[cut + 1].right);
            let block = self
                .algebra
                .multiply(
                    chain.sites[cut].rows_left(),
                    chain.sites[cut + 1].cols_right(),
                )
                .into_elements();
            chain.pair = Some(ContractedPair {
                cut,
                left,
                right,
                block,
                truncation,
                dev_swap: false,
            });
        }
        let pair = chain.pair.as_mut().expect("a pair at the cut");
        pair.truncation = truncation;
        pair.dev_swap = crate::dev_trace::in_swap();
        pair.apply(gate);
        if self.fuses {
            Ok(())
        } else {
            self.split_pair(chain, side)
        }
    }

    /// Applies a SWAP to the sites `cut` and `cut + 1`, as a step of a route.
    pub(super) fn swap(
        &self,
        chain: &mut Chain,
        cut: usize,
        side: CentreSide,
        truncation: Truncation,
    ) -> Result<(), CpuMpsError> {
        crate::dev_trace::set_in_swap(true);
        let applied = self.apply_adjacent(chain, cut, &SWAP, side, truncation);
        crate::dev_trace::set_in_swap(false);
        applied?;
        chain.statistics.swaps += 1;
        Ok(())
    }

    /// The side of the pair at `cut` nearer the centre, where a split leaves
    /// it without moving it further.
    pub(super) fn side_nearer_centre(chain: &Chain, cut: usize) -> CentreSide {
        if chain.centre <= cut {
            CentreSide::Left
        } else {
            CentreSide::Right
        }
    }
}
