// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the tensor calls [`CudaMpsEngine`]
//! makes, behind one trait, implemented today by `DeviceTensors` on the GPU
//! (cuTensorNet's `GateSplit`), so that the engine's rules are written once
//! over buffers whatever holds them.
//!
//! Every tensor is complex double, its first mode fastest (Fortran order):
//! a site `T[a, s, b]` of extents `[left, 2, right]` sits at
//! `a + left·(s + 2·b)`, as `qdk_cpu_mps` lays its sites out.
//!
//! [`CudaMpsEngine`]: super::CudaMpsEngine

use super::CudaMpsError;
use num_complex::Complex64;

/// The calls an MPS engine on cuTensorNet's tensor API needs: moving
/// tensors between the host and where they live, and one `GateSplit`.
#[doc(hidden)]
pub trait TensorCalls {
    /// Where a tensor lives: device memory, or a host vector.
    type Buffer;

    /// A new buffer holding `values`.
    ///
    /// # Errors
    ///
    /// When an allocation or a copy fails.
    fn upload(&self, values: &[Complex64]) -> Result<Self::Buffer, CudaMpsError>;

    /// The first `count` values of `buffer`.
    ///
    /// # Errors
    ///
    /// When a copy fails.
    fn download(&self, buffer: &Self::Buffer, count: usize)
    -> Result<Vec<Complex64>, CudaMpsError>;

    /// A new buffer holding the first `count` values of `buffer`.
    ///
    /// # Errors
    ///
    /// When an allocation or a copy fails.
    fn duplicate(&self, buffer: &Self::Buffer, count: usize) -> Result<Self::Buffer, CudaMpsError>;

    /// `GateSplit`: contracts `left` and `right` over their shared bond,
    /// applies `gate` to the two physical modes, and splits the result
    /// again by one SVD, truncated as `call` says, the singular values
    /// normalised to an L2 norm of 1 and absorbed on the side
    /// `call.partition` names.
    ///
    /// # Errors
    ///
    /// When a call fails.
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Buffer>,
    ) -> Result<SplitOutcome<Self::Buffer>, CudaMpsError>;
}

/// One `GateSplit`: `θ[a, s, t, b] = Σ G[(s, t), (p, q)] · A[a, p, j] ·
/// B[j, q, b]`, split as `θ = U·S·V` with `U` of extents
/// `[left, 2, k]` and `V` of extents `[k, 2, right]`.
#[doc(hidden)]
pub struct GateSplitCall<'b, B> {
    /// `A`, of extents `[left, 2, bond]`.
    pub left: &'b B,
    /// `B`, of extents `[bond, 2, right]`.
    pub right: &'b B,
    /// `A`'s left extent.
    pub left_extent: usize,
    /// The shared bond's extent before the split.
    pub bond: usize,
    /// `B`'s right extent.
    pub right_extent: usize,
    /// The gate, row-major, the left site the most significant bit:
    /// `G[(s, t), (p, q)]` at `(2s + t)·4 + 2p + q`.
    pub gate: [Complex64; 16],
    /// The largest bond the split may keep: a fixed-extent truncation when
    /// below `min(2·left, 2·right)`.
    pub max_extent: usize,
    /// The discarded-weight cutoff: drop the smallest singular values
    /// whose squares sum to at most this fraction of their total, never
    /// the largest. `None`: drop nothing but what `max_extent` cuts.
    pub cutoff: Option<f64>,
    /// The side that absorbs the singular values.
    pub partition: Partition,
    /// Whether to return the kept singular values (a self-check reads
    /// them; the engine does not).
    pub read_values: bool,
}

/// The side of a split that absorbs its singular values, and so holds the
/// orthogonality centre afterwards.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Partition {
    /// `U·S`: the centre on the left site.
    Us,
    /// `S·V`: the centre on the right site.
    Sv,
}

/// What one `GateSplit` returned.
#[doc(hidden)]
pub struct SplitOutcome<B> {
    /// `U`, of extents `[left, 2, reduced]`, packed.
    pub u: B,
    /// `V`, of extents `[reduced, 2, right]`, packed.
    pub v: B,
    /// The bond kept.
    pub reduced: usize,
    /// The bond before truncation, `min(2·left, 2·right)`.
    pub full: usize,
    /// The fraction of the squared singular values the truncation dropped.
    pub discarded_weight: f64,
    /// The kept singular values, normalised, largest first, when
    /// [`GateSplitCall::read_values`] asked for them; empty otherwise.
    pub singular_values: Vec<f64>,
}
