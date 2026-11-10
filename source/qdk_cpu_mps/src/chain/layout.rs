// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Layout`]: where each of the caller's sites sits in the chain, so that
//! the routing `MeetHalfway` may leave sites where its routes moved them and
//! every call still answers in the caller's sites.
//!
//! The caller names qubits by their site in the capacity it prepared. Under
//! `swap-back-2x` the chain holds them in that order after every call, and
//! the layout stays the identity. `MeetHalfway` moves both sites of a
//! distant gate halfway toward each other and leaves them there, so the
//! engine translates every site a call names to its chain position, and
//! restores the caller's order when the capacity or the drift requires it
//! (`engine::routing`). The layout also measures how far the order drifted,
//! which decides when `MeetHalfway` falls back.

/// Where each of the caller's sites sits in the chain (its position), and
/// which caller's site each position holds: two inverse permutations, the
/// identity until a routing leaves a site moved; with the number of pairs
/// of sites the chain holds in the other order than the caller's (its
/// inversions), kept as SWAPs exchange neighbours.
#[derive(Clone, Debug)]
pub(crate) struct Layout {
    /// The chain position of each caller's site.
    position: Vec<usize>,
    /// The caller's site at each chain position.
    site: Vec<usize>,
    /// The pairs of caller's sites `s < t` whose positions are in the other
    /// order.
    inversions: usize,
}

impl Layout {
    /// The caller's order on `sites` sites.
    pub(crate) fn in_order(sites: usize) -> Self {
        Self {
            position: (0..sites).collect(),
            site: (0..sites).collect(),
            inversions: 0,
        }
    }

    /// The chain position of the caller's `site`.
    pub(crate) fn position(&self, site: usize) -> usize {
        self.position[site]
    }

    /// Records that a SWAP exchanged the sites at positions `cut` and
    /// `cut + 1`: the pair of them changes order, no other pair does.
    pub(crate) fn swap(&mut self, cut: usize) {
        let (a, b) = (self.site[cut], self.site[cut + 1]);
        if a < b {
            self.inversions += 1;
        } else {
            self.inversions -= 1;
        }
        self.site.swap(cut, cut + 1);
        self.position[a] = cut + 1;
        self.position[b] = cut;
    }

    /// Whether every site is where the caller put it.
    pub(crate) fn is_in_order(&self) -> bool {
        self.inversions == 0
    }

    /// The layout drift: the pairs of sites the chain holds in the other
    /// order than the caller's, over all pairs; 0 in the caller's order, 1
    /// reversed, and 0 on fewer than two sites.
    pub(crate) fn drift(&self) -> f64 {
        let n = self.site.len();
        if n < 2 {
            return 0.0;
        }
        #[expect(clippy::cast_precision_loss, reason = "counts far below 2^53")]
        let drift = self.inversions as f64 / (n * (n - 1) / 2) as f64;
        drift
    }
}
