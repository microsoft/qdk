// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the knobs and the clocks of the
//! tuning bench (`examples/tuning_bench.rs`), which measures what each of
//! the engine's fixed choices (`policy.rs`) costs on a GPU.
//!
//! The defaults are the engine's behaviour: an engine never given a
//! `DevTuning` computes and reads exactly as before. The clocks only add
//! up the time spent in each phase of the engine's native calls.

use std::time::Instant;

/// The SVD a compute's truncations use (`CUTENSORNET_STATE_CONFIG_MPS_SVD_ALGO`).
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SvdAlgorithm {
    /// cuSOLVER's QR-based SVD, the engine's choice.
    #[default]
    Gesvd,
    /// cuSOLVER's Jacobi SVD.
    Gesvdj,
    /// cuSOLVER's randomized SVD.
    Gesvdr,
}

/// What a `CuTensorNetEngine` does differently for the tuning bench.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevTuning {
    pub svd_algorithm: SvdAlgorithm,
    /// `CUTENSORNET_EXPECTATION_CONFIG_NUM_HYPER_SAMPLES` of every read.
    pub expectation_hyper_samples: i32,
    /// Keeps freed device buffers in a per-state pool keyed by size and
    /// reuses them, instead of a `cudaMalloc` and a `cudaFree` each.
    pub pooled_buffers: bool,
    /// Reads P(One) of the site the computed MPS is canonical around
    /// (`CuTensorNetEngine::dev_set_canonical_centre`) from that site's
    /// tensor, copied to the host, instead of an expectation.
    pub canonical_centre_reads: bool,
}

impl Default for DevTuning {
    fn default() -> Self {
        Self {
            svd_algorithm: SvdAlgorithm::Gesvd,
            expectation_hyper_samples: super::policy::EXPECTATION_HYPER_SAMPLES,
            pooled_buffers: false,
            canonical_centre_reads: false,
        }
    }
}

/// The time spent in one phase, and how many times it ran.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhaseClock {
    pub seconds: f64,
    pub count: u64,
}

impl PhaseClock {
    pub(crate) fn add_since(&mut self, started: Instant) {
        self.seconds += started.elapsed().as_secs_f64();
        self.count += 1;
    }
}

/// The time spent in each phase of an engine's native calls, summed over
/// its states since the engine was made or `dev_reset_times` was called.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DevTimes {
    /// `cutensornetStatePrepare`, the workspace size and the scratch.
    pub state_prepare: PhaseClock,
    /// `cutensornetStateCompute` and the stream's synchronization.
    pub state_compute: PhaseClock,
    /// `cutensornetStateCaptureMPS` and the release of the old outputs.
    pub capture: PhaseClock,
    /// A read's whole expectation: create, prepare, compute, release.
    pub expectation: PhaseClock,
    /// A canonical-centre read: the copy to the host and the sum.
    pub canonical_read: PhaseClock,
    /// An operator's upload and `cutensornetStateApplyTensorOperator`.
    pub apply: PhaseClock,
    /// A restart (`computed_mps.rs`).
    pub restart: PhaseClock,
    /// Device buffers allocated (`cudaMalloc`).
    pub allocations: u64,
    /// Device buffers taken from the pool instead.
    pub pool_hits: u64,
    /// Reads that wanted the canonical centre but had to use an expectation.
    pub canonical_fallbacks: u64,
}
