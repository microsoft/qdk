// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The types of a gate split's calls: its descriptors and operands, its
//! SVD's settings and report, and the cuTensorNet objects that hold them.

use super::{TensorDescriptor, TensorOperand};
use crate::engine::native_calls::{memory_calls::DeviceBuffer, native_pointer};

/// The descriptors of one gate split: the inputs A and B, the gate, and the
/// outputs U and V. U's and V's shared label is the bond the SVD cuts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateSplitDescriptors {
    pub(crate) a: TensorDescriptor,
    pub(crate) b: TensorDescriptor,
    pub(crate) gate: TensorDescriptor,
    pub(crate) u: TensorDescriptor,
    pub(crate) v: TensorDescriptor,
}

/// The operands of one gate split, as in [`GateSplitDescriptors`], and a
/// device buffer for the singular values. cuTensorNet writes none there
/// when it absorbs them into U or V, and then accepts a null pointer. Only
/// a buffer has been run on a GPU, so one is always passed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateSplitOperands {
    pub(crate) a: TensorOperand,
    pub(crate) b: TensorOperand,
    pub(crate) gate: TensorOperand,
    pub(crate) u: TensorOperand,
    pub(crate) v: TensorOperand,
    pub(crate) singular_values: DeviceBuffer,
}

/// One setting of an SVD configuration
/// (`cutensornetTensorSVDConfigSetAttribute`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SvdSetting {
    /// The SVD runs as LAPACK's `gesvd`.
    AlgorithmGesvd,
    /// The kept singular values are scaled to unit L2 norm.
    NormalizationL2,
    /// The singular values are absorbed into U.
    PartitionUs,
    /// The singular values are absorbed into V.
    PartitionSv,
    /// The smallest singular values are dropped while their squared sum is
    /// at most this fraction of the total, and at least one is kept.
    /// Setting it, 0 included, asks for a truncation by value.
    DiscardedWeightCutoff(f64),
}

/// What a gate split's truncation did
/// (`cutensornetTensorSVDInfoGetAttribute`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SvdTruncation {
    /// The extent of the shared mode as U and V hold it.
    pub(crate) reduced_extent: i64,
    /// The extent of the shared mode without truncation: the smaller side
    /// of the smallest equivalent matrix SVD problem. Under
    /// `CUTENSORNET_GATE_SPLIT_ALGO_REDUCED`, that problem may be smaller
    /// than the product of A, B and the gate as a matrix, which no GPU run
    /// has checked yet.
    pub(crate) full_extent: i64,
    /// The squared sum of the dropped singular values over that of all.
    pub(crate) discarded_weight: f64,
}

native_pointer! {
    /// A cuTensorNet SVD configuration (`cutensornetTensorSVDConfig_t`):
    /// how a gate split's SVD truncates, normalizes and absorbs.
    TensorSvdConfig
}

native_pointer! {
    /// A cuTensorNet SVD info (`cutensornetTensorSVDInfo_t`): what the last
    /// gate split given it truncated.
    TensorSvdInfo
}
