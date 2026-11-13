// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): gates on two sites, as
//! `qdk_cpu_mps`'s `engine/gates.rs` with fusion on (its default): a gate
//! on neighbours moves the centre into the pair and joins the pair's fused
//! gate, and a gate on distant sites is routed by SWAPs (`swap-back-2x`),
//! the outbound SWAPs' splits applying the cutoff alone.

use super::device_chain::{DeviceChain, FusedGate, SWAP, Truncation, multiply4, swap_operands};
use super::{CudaMpsEngine, CudaMpsError, TensorCalls};
use num_complex::Complex64;

impl<T: TensorCalls> CudaMpsEngine<T> {
    /// Applies `gate` (row-major, site `first` the most significant bit)
    /// to the sites `first` and `second`, as `qdk_cpu_mps`'s `apply_pair`.
    pub(super) fn apply_pair(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        first: usize,
        second: usize,
        gate: &[Complex64],
    ) -> Result<(), CudaMpsError> {
        let (low, high) = (first.min(second), first.max(second));
        let gate: [Complex64; 16] = if first < second {
            std::array::from_fn(|at| gate[at])
        } else {
            swap_operands(gate)
        };
        if high == low + 1 {
            return self.apply_adjacent(chain, low, &gate, Truncation::CutoffAndCapacity);
        }
        for cut in (low + 1..high).rev() {
            self.swap(chain, cut, Truncation::CutoffOnly)?;
        }
        self.apply_adjacent(chain, low, &gate, Truncation::CutoffAndCapacity)?;
        for cut in low + 1..high {
            self.swap(chain, cut, Truncation::CutoffAndCapacity)?;
        }
        Ok(())
    }

    /// Multiplies `gate` into the fused gate of the pair at `cut`, after
    /// splitting any other pair and moving the centre into this one, as
    /// `qdk_cpu_mps`'s `apply_adjacent` with fusion.
    fn apply_adjacent(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        cut: usize,
        gate: &[Complex64; 16],
        truncation: Truncation,
    ) -> Result<(), CudaMpsError> {
        if chain.fused.is_none_or(|fused| fused.cut != cut) {
            self.split_pair_toward(chain, cut)?;
            let nearest = if chain.centre <= cut { cut } else { cut + 1 };
            self.move_centre(chain, nearest)?;
            chain.fused = Some(FusedGate {
                cut,
                gate: chain.take_pending_pair(cut),
                truncation,
            });
        }
        let fused = chain.fused.as_mut().expect("a fused pair at the cut");
        fused.truncation = truncation;
        fused.gate = multiply4(gate, &fused.gate);
        Ok(())
    }

    /// A SWAP on the sites `cut` and `cut + 1`, a step of a route.
    fn swap(
        &self,
        chain: &mut DeviceChain<T::Buffer>,
        cut: usize,
        truncation: Truncation,
    ) -> Result<(), CudaMpsError> {
        self.apply_adjacent(chain, cut, &SWAP, truncation)?;
        self.count(|counters| counters.swaps += 1);
        Ok(())
    }
}
