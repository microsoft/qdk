// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): splits and centre moves, each one
//! `GateSplit` (or, with `with_dev_qr_moves`, each move one QR move), as
//! `qdk_cpu_mps`'s `engine/split.rs` and `engine/canonical_form.rs`.
//!
//! - A truncating split (the fused pair's) applies the cutoff, then the
//!   capacity's bond unless an outbound SWAP's, and multiplies the
//!   retained weight by `1 − discarded` when it dropped a value.
//! - A centre move is a `GateSplit` of the centre and its neighbour with no
//!   cutoff and the pair's bond as the largest extent: exact, as the CPU's
//!   QR is, its bond at most the CPU's (`min(2·left, 2·right)` and the old
//!   bond), and it takes the two sites' pending gates with it.
//! - EXPERIMENTAL, with `with_dev_qr_moves`: a centre move is the CPU's
//!   instead, one QR of the site the centre leaves (its pending gate
//!   applied first) and one contraction of the small factor into the new
//!   centre ([`TensorCalls::qr_move`]), its bond the CPU's
//!   (`min(2·left, bond)` rightward, `min(bond, 2·right)` leftward). A move
//!   never carries a two-site gate: every caller splits the fused pair
//!   first.

use super::device_chain::{DeviceChain, DeviceSite, FusedGate, Truncation};
use super::{CudaMpsEngine, CudaMpsError, GateSplitCall, Partition, QrMoveCall, TensorCalls};
use num_complex::Complex64;

/// Where a split leaves the orthogonality centre, as `qdk_cpu_mps`'s
/// `CentreSide`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CentreSide {
    Left,
    Right,
}

/// What one split of the chain kept.
struct Kept {
    reduced: usize,
    full: usize,
    discarded_weight: f64,
}

impl<T: TensorCalls> CudaMpsEngine<T> {
    /// Splits the fused pair, if any, and leaves the centre on the side
    /// of the pair nearer the site `toward`.
    pub(super) fn split_pair_toward(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        toward: usize,
    ) -> Result<(), CudaMpsError> {
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

    /// Splits the fused pair, truncated, and leaves the centre on `side`.
    pub(super) fn split_pair(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        side: CentreSide,
    ) -> Result<(), CudaMpsError> {
        let Some(FusedGate {
            cut,
            gate,
            truncation,
        }) = chain.fused.take()
        else {
            return Ok(());
        };
        let max_extent = match truncation {
            Truncation::CutoffAndCapacity => chain.capacity_bonds[cut],
            // As the CPU engine's: an outbound SWAP's bond stays within
            // twice the capacity's largest bond without a cap of its own.
            Truncation::CutoffOnly => usize::MAX,
        };
        let (partition, centre) = match side {
            CentreSide::Left => (Partition::Us, cut),
            CentreSide::Right => (Partition::Sv, cut + 1),
        };
        let kept = self.split_sites(chain, cut, gate, max_extent, Some(self.cutoff), partition)?;
        chain.centre = centre;
        self.count(|counters| {
            counters.splits += 1;
            counters.largest_bond = counters.largest_bond.max(kept.reduced);
        });
        if kept.reduced < kept.full {
            let weight = chain.retained_weight * (1.0 - kept.discarded_weight);
            if weight.is_nan() || weight < f64::MIN_POSITIVE {
                return Err(CudaMpsError::WeightUnderflow);
            }
            chain.retained_weight = weight;
        }
        Ok(())
    }

    /// Moves the centre to `to`, one site at a time: by a QR move when
    /// `with_dev_qr_moves` is on, by an exact `GateSplit` otherwise.
    pub(super) fn move_centre(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        to: usize,
    ) -> Result<(), CudaMpsError> {
        while chain.centre != to {
            let (cut, partition, next) = if chain.centre < to {
                (chain.centre, Partition::Sv, chain.centre + 1)
            } else {
                (chain.centre - 1, Partition::Us, chain.centre - 1)
            };
            if self.dev_qr_moves {
                self.qr_move(chain, cut, partition)?;
            } else {
                let gate = chain.take_pending_pair(cut);
                let bond = chain.sites[cut].right;
                self.split_sites(chain, cut, gate, bond, None, partition)?;
            }
            chain.centre = next;
            self.count(|counters| counters.centre_moves += 1);
        }
        Ok(())
    }

    /// One QR move of the centre across `cut`, toward the side `partition`
    /// names, as `qdk_cpu_mps`'s `shift_centre_right` and
    /// `shift_centre_left`. The site the centre leaves takes its pending
    /// gate into its QR, since it must be an isometry with it; the new
    /// centre's pending gate stays pending.
    fn qr_move(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        cut: usize,
        partition: Partition,
    ) -> Result<(), CudaMpsError> {
        let leaving = match partition {
            Partition::Sv => cut,
            Partition::Us => cut + 1,
        };
        let gate = chain.pending[leaving].take();
        let (left_site, right_site) = (&chain.sites[cut], &chain.sites[cut + 1]);
        let (left, right) = (left_site.left, right_site.right);
        let outcome = self.timed_qr_move(&QrMoveCall {
            left: &left_site.buffer,
            right: &right_site.buffer,
            left_extent: left,
            bond: left_site.right,
            right_extent: right,
            gate,
            partition,
        })?;
        chain.sites[cut] = DeviceSite {
            left,
            right: outcome.bond,
            buffer: outcome.left,
        };
        chain.sites[cut + 1] = DeviceSite {
            left: outcome.bond,
            right,
            buffer: outcome.right,
        };
        Ok(())
    }

    /// Splits the fused pair, if any, and moves the centre to `site`, for
    /// a read of that site alone.
    pub(super) fn centre_on(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        site: usize,
    ) -> Result<(), CudaMpsError> {
        self.split_pair_toward(chain, site)?;
        self.move_centre(chain, site)
    }

    /// One `GateSplit` of the sites `cut` and `cut + 1` with `gate`,
    /// replacing both.
    fn split_sites(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        cut: usize,
        gate: [Complex64; 16],
        max_extent: usize,
        cutoff: Option<f64>,
        partition: Partition,
    ) -> Result<Kept, CudaMpsError> {
        let (left_site, right_site) = (&chain.sites[cut], &chain.sites[cut + 1]);
        let (left, right) = (left_site.left, right_site.right);
        // GateSplit's REDUCED algorithm splits through a middle tensor of
        // extent at most `4 · bond`, and the extent asked may not exceed it;
        // θ's rank is at most `4 · bond` anyway (a two-site gate's operator
        // Schmidt rank is at most 4), so the cap loses nothing.
        let full = (2 * left).min(2 * right).min(4 * left_site.right);
        let outcome = self.timed_gate_split(&GateSplitCall {
            left: &left_site.buffer,
            right: &right_site.buffer,
            left_extent: left,
            bond: left_site.right,
            right_extent: right,
            gate,
            max_extent: max_extent.min(full),
            cutoff,
            partition,
            read_values: false,
        })?;
        let reduced = outcome.reduced;
        chain.sites[cut] = DeviceSite {
            left,
            right: reduced,
            buffer: outcome.u,
        };
        chain.sites[cut + 1] = DeviceSite {
            left: reduced,
            right,
            buffer: outcome.v,
        };
        Ok(Kept {
            reduced,
            full: outcome.full,
            discarded_weight: outcome.discarded_weight,
        })
    }
}
