// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The algebra's fixed settings: the device, the SVD algorithm, the
//! rounding a split's report may carry, and how the contractions of a move
//! of the centre find and keep their paths. No caller tunes them. The SVD
//! algorithm was chosen by measurement on an A100, given below; the gate
//! split's algorithm and the path settings were carried over from the GPU
//! prototype of this algebra, not measured here.
//!
//! A gate split runs cuTensorNet's reduced algorithm
//! (`CUTENSORNET_GATE_SPLIT_ALGO_REDUCED`, which `gate_split.rs`
//! describes) rather than the direct one, as the GPU prototype did: its SVD
//! is of the small core alone, not of the whole gated pair. It is applied
//! where the gate split is called, in `native_calls/loaded_libraries`,
//! since the native calls offer no other.

use crate::engine::native_calls::tensor_calls::SvdSetting;

/// The CUDA device every algebra runs on. A process that runs on another
/// device selects it with `CUDA_VISIBLE_DEVICES`.
pub(super) const DEVICE_ORDINAL: i32 = 0;

/// The SVD algorithm of every gate split: LAPACK's `gesvd`, through
/// cuSOLVER.
///
/// Of the others, GESVDR cannot apply a discarded-weight cutoff, which
/// every split asks for. GESVDP ran in 0.39 to 0.88 of GESVD's time at
/// bonds 8 to 512, but its factors were off by 2.2e-7 on rank-deficient
/// splits, where the canonical MPS's tests ask for 1e-10, and it failed
/// with `CUSOLVER_ERROR` (status 25) within 26 s of an H₂ shot and at the
/// 24th step of an 8-site polyene (cuTensorNet 2.13.0, CUDA 12.9, on an
/// A100).
pub(super) const SVD_ALGORITHM: SvdSetting = SvdSetting::AlgorithmGesvd;

/// How far below zero a gate split's reported discarded weight may lie and
/// still be read as zero, nothing discarded. cuTensorNet reports the
/// weight rounded: a split that discarded nothing reported -2.2e-16 on an
/// A100, as one minus a retained fraction rounded above 1 would.
/// That rounding, over the at most `k` singular values of a split, is
/// about `k·ε` at worst, so 1e-12 covers splits of up to about 4,000
/// values (`k·ε` = 9.1e-13 at `k` = 4,096), while a negative weight of any
/// real size, a library fault, is still refused. A split of more values
/// would need a larger tolerance. A constant rather than a bound in `k`: the
/// bound would rest on how the library sums the squares, which it does not
/// document.
pub(crate) const DISCARDED_WEIGHT_TOLERANCE: f64 = 1e-12;

/// The device memory, in bytes, the path search of a contraction may plan
/// for (1 GiB): a pairwise contraction of two sites needs far less, so the
/// limit never shapes the path. Carried over from the prototype.
pub(super) const PATH_WORKSPACE_LIMIT: u64 = 1 << 30;

/// The prepared contraction networks kept, one per shape, before all are
/// released and the cache starts again. A run's moves of the centre repeat
/// a few hundred shapes, so the cache stays small and each shape's path is
/// found once; the bound keeps a run of ever new shapes from holding
/// native objects without limit. Carried over from the prototype.
pub(crate) const NETWORKS_KEPT: usize = 512;
