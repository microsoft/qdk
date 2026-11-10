// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SplitStatistics`]: what keeping a state small cost, which a simulator
//! reads to choose the capacity and judge the cost of a run.

/// What the splits of a state's history cost: how many SVD splits it took,
/// how many SWAPs only routed a site, and the largest bonds the splits kept,
/// at rest and during routes.
///
/// A split at rest (a gate's, or a route's return SWAP's, in the caller's
/// order) keeps at most the capacity's bond at its cut: with one χ at every
/// cut, a largest bond at rest below χ says the capacity cut nothing, and
/// one at χ that it may have, so a smaller χ would cut more. A route's
/// outbound SWAPs, and under [`Routing::MeetHalfway`] every split while the
/// chain is out of the caller's order, keep transient bonds of up to twice
/// the capacity's largest bond (see Routing on
/// [`CpuMpsEngine`](crate::CpuMpsEngine)), which
/// [`MpsEngine::shape`](tensornet::MpsEngine::shape) no longer shows once
/// the caller's order is back and within the capacity: the larger of the
/// two figures is the largest bond the state held. The split counts are
/// what the engine's time scales with: a split's SVD is most of a gate's
/// cost.
///
/// With fusion, a pair left unsplit counts when it is split, under its last
/// two-site operation: as transient if that was an outbound SWAP or a
/// split out of the caller's order, at rest otherwise.
///
/// [`Routing::MeetHalfway`]: crate::Routing::MeetHalfway
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
    /// back, and those that restored the caller's order under
    /// [`Routing::MeetHalfway`](crate::Routing::MeetHalfway). Each takes a
    /// split of its own, except, with fusion, a route's first and last: the
    /// first shares the split of the pair it lands on when the operation
    /// before it left that pair unsplit, and the last shares its split with
    /// the operations that follow on its pair.
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

    /// The largest bond the split of a route's outbound SWAP, or a split
    /// out of the caller's order, kept, at most twice the capacity's largest
    /// bond; 1 for a state without such splits.
    #[must_use]
    pub fn largest_transient_bond(&self) -> usize {
        self.largest_transient_bond.max(1)
    }
}
