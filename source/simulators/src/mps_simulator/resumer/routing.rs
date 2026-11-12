// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only: [`Routing`], how the resumer brings the
//! two sites of a distant two-site operator together, and [`Placement`],
//! where each layout site sits in a state's chain while a routing leaves
//! sites moved.
//!
//! [`Routing::Engine`], the default, sends a distant operator as it is,
//! and the engine routes it (swap in, apply, swap back). With
//! [`Routing::MeetHalfway`] the resumer moves both sites halfway toward
//! each other by adjacent SWAPs, applies the operator where they meet,
//! and leaves them there: the chain's order drifts from the layout's, so
//! each state carries its [`Placement`] (cloned on a fork), and every
//! later operator, measurement and reset goes to the sites it gives.
//!
//! The guard, ported from the CPU engine's `MeetHalfway` and adapted to
//! what the engine contract lets the resumer see:
//!
//! - **The drift fallback.** A route that leaves the drift (the pairs of
//!   sites in the other order than the layout's, over all pairs) above
//!   [`FALLBACK_DRIFT`] restores the layout's order by SWAPs and falls
//!   back to `Engine` for the rest of the state's life (its forks inherit
//!   it): past one half the order is nearer reversed, where routes grow
//!   long.
//! - **The capacity fallback.** A bond read that reaches the max bond
//!   dimension says the capacity may bind; the state restores the layout's
//!   order at its next batch and falls back. The CPU engine cuts at twice
//!   the capacity while out of order, so that the capacity cuts only at
//!   the layout's cuts; the resumer cannot (the engine truncates at the
//!   capacity it prepared), so until the read, a drifted state may lose
//!   weight at a cut that is not the layout's.

use crate::qir::QirGate;

use super::lowering::{Operator, Step};

/// EXPERIMENTAL: how the resumer routes a two-site operator whose sites are
/// not neighbours (see the module documentation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Routing {
    /// The operator goes to the engine as it is; the engine routes it. The
    /// default: the resumer's behaviour without this phase.
    #[default]
    Engine,
    /// Both sites move halfway toward each other by SWAPs and stay where
    /// they met, guarded; routed after lowering and before gate fusion, so
    /// that a SWAP may fuse with the gates beside it.
    MeetHalfway,
    /// As [`MeetHalfway`](Self::MeetHalfway), routed after gate fusion:
    /// fusion groups the gates on the layout's sites, and each fused
    /// operator is routed once.
    MeetHalfwayAfterFusion,
}

/// The drift above which a route makes a state fall back to
/// [`Routing::Engine`]: one half, the CPU engine's threshold (H₂ stays
/// below 0.13 there; a QFT, whose order drifts to the reverse, crosses it).
const FALLBACK_DRIFT: f64 = 0.5;

/// Where each layout site sits in a state's chain (its position), and
/// which layout site each position holds: two inverse permutations, the
/// identity until a route leaves sites moved; with the pairs of sites in
/// the other order (its inversions), and whether the state fell back.
#[derive(Clone, Debug)]
pub(crate) struct Placement {
    position: Vec<usize>,
    site: Vec<usize>,
    inversions: usize,
    fell_back: bool,
    restore_pending: bool,
}

impl Placement {
    /// The layout's order on `sites` sites.
    pub(crate) fn in_layout_order(sites: usize) -> Self {
        Self {
            position: (0..sites).collect(),
            site: (0..sites).collect(),
            inversions: 0,
            fell_back: false,
            restore_pending: false,
        }
    }

    /// The chain position of the layout's `site`.
    #[cfg(test)]
    pub(crate) fn position(&self, site: usize) -> usize {
        self.position[site]
    }

    /// Whether the state fell back to [`Routing::Engine`] for good.
    #[cfg(test)]
    pub(crate) fn fell_back(&self) -> bool {
        self.fell_back
    }

    /// Records that a bond read reached the max bond dimension: the next
    /// [`route`](Self::route) restores the layout's order and falls back.
    pub(crate) fn capacity_reached(&mut self) {
        if !self.fell_back {
            self.restore_pending = true;
        }
    }

    /// `steps`, on the layout's sites, on the chain's: each distant
    /// two-site operator routed by `MeetHalfway` (until the state falls
    /// back), every operator, measurement and reset on the positions its
    /// sites hold when it runs; this placement left as the steps leave the
    /// chain.
    pub(crate) fn route(&mut self, steps: Vec<Step>) -> Vec<Step> {
        let mut routed = Vec::with_capacity(steps.len());
        if self.restore_pending {
            self.restore_pending = false;
            self.fall_back(&mut routed);
        }
        for step in steps {
            match step {
                Step::Apply(operator) => self.route_operator(operator, &mut routed),
                Step::Measure {
                    site,
                    result,
                    reset,
                } => routed.push(Step::Measure {
                    site: self.position[site],
                    result,
                    reset,
                }),
                Step::Reset { site } => routed.push(Step::Reset {
                    site: self.position[site],
                }),
            }
        }
        routed
    }

    #[expect(
        clippy::large_types_passed_by_value,
        reason = "the operator moves into the routed steps"
    )]
    fn route_operator(&mut self, operator: Operator, routed: &mut Vec<Step>) {
        if let &[a, b] = operator.sites() {
            let (low, high) = {
                let (pa, pb) = (self.position[a], self.position[b]);
                (pa.min(pb), pa.max(pb))
            };
            if !self.fell_back && high - low >= 2 {
                let meet = low + (high - low - 1) / 2;
                for cut in low..meet {
                    self.swap(cut, routed);
                }
                for cut in (meet + 1..high).rev() {
                    self.swap(cut, routed);
                }
                routed.push(Step::Apply(self.on_chain(operator)));
                if self.drift() > FALLBACK_DRIFT {
                    self.fall_back(routed);
                }
                return;
            }
        }
        routed.push(Step::Apply(self.on_chain(operator)));
    }

    /// `operator` on the positions its layout sites hold.
    fn on_chain(&self, mut operator: Operator) -> Operator {
        let count = operator.matrix.qubit_count();
        for slot in &mut operator.site_slots[..count] {
            *slot = self.position[*slot];
        }
        operator
    }

    /// Restores the layout's order by SWAPs (site 0 down to position 0,
    /// then site 1, and so on), and falls back for good.
    fn fall_back(&mut self, routed: &mut Vec<Step>) {
        for site in 0..self.site.len() {
            for cut in (site..self.position[site]).rev() {
                self.swap(cut, routed);
            }
        }
        self.fell_back = true;
    }

    /// A SWAP of the positions `cut` and `cut + 1`, emitted and recorded:
    /// the pair of their sites changes order, no other pair does.
    fn swap(&mut self, cut: usize, routed: &mut Vec<Step>) {
        let (a, b) = (self.site[cut], self.site[cut + 1]);
        if a < b {
            self.inversions += 1;
        } else {
            self.inversions -= 1;
        }
        self.site.swap(cut, cut + 1);
        self.position[a] = cut + 1;
        self.position[b] = cut;
        routed.push(Step::Apply(Operator {
            site_slots: [cut, cut + 1],
            matrix: QirGate::Swap.matrix_at(None),
        }));
    }

    /// The pairs of sites in the other order than the layout's, over all
    /// pairs: 0 in its order, 1 reversed.
    fn drift(&self) -> f64 {
        let n = self.site.len();
        if n < 2 {
            return 0.0;
        }
        #[expect(clippy::cast_precision_loss, reason = "counts far below 2^53")]
        let drift = self.inversions as f64 / (n * (n - 1) / 2) as f64;
        drift
    }
}
