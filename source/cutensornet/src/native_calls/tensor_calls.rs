// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TensorCalls`]: cuTensorNet's tensor API, which factors and contracts
//! one or two tensors at a time for a caller that holds an MPS in its own
//! device buffers.
//!
//! The engine hands its MPS to cuTensorNet's state API (`StateCalls`), which
//! applies the gates and truncates inside the library. A simulator that
//! keeps its MPS canonical, site by site, needs the library's smaller steps
//! instead: split a two-site gate's result by a truncated SVD (`gate_split`),
//! move the canonical centre by a QR (`tensor_qr`), and contract two tensors
//! (`contract`). Its planned user, `CudaAlgebra`, is that simulator's linear
//! algebra on the GPU; nothing calls these yet.
//!
//! Every tensor is complex double (`CUDA_C_64F`), its first mode fastest
//! (null strides), and every call computes in double precision
//! (`CUTENSORNET_COMPUTE_64F`). The objects a caller creates here
//! (descriptors, SVD configs and infos, networks, optimizer infos, plans)
//! belong to the handle they were created with, and are used only with it.
//! A call that runs on a stream (`gate_split`, `tensor_qr`, `contract`) runs
//! in a workspace sized for that call (its `…_workspace_sizes`) and lent
//! the memory each slot asks for (`MemoryCalls::set_workspace`); its results
//! are ready once the caller synchronizes the stream.
//!
//! The tensor API is not required to open the libraries, since the engine
//! never calls it:
//! [`check_tensor_functions`](TensorCalls::check_tensor_functions) names
//! each function the library lacks; when any is missing, every other
//! method fails with the same error.

use super::{OpaqueHandle, Stream, memory_calls::MemoryCalls};
use crate::native_calls::error::NativeError;

/// The calls of cuTensorNet's tensor API that a canonical MPS needs: tensor
/// descriptors, the gate split and its SVD settings and report, the QR, and
/// pairwise contractions.
pub(crate) trait TensorCalls: MemoryCalls {
    /// Fails unless the library exports every function of the tensor API,
    /// naming each missing one (`NativeError::MissingNativeFunctions`).
    fn check_tensor_functions(&self) -> Result<(), NativeError>;

    fn create_tensor_descriptor(
        &self,
        handle: OpaqueHandle,
        tensor: TensorModes<'_>,
    ) -> Result<OpaqueHandle, NativeError>;
    fn destroy_tensor_descriptor(&self, descriptor: OpaqueHandle) -> Result<(), NativeError>;

    /// A new SVD configuration, with cuTensorNet's defaults: no truncation
    /// beyond the output descriptors' extents, no normalization, and the
    /// singular values returned apart.
    fn create_svd_config(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, NativeError>;
    /// Sets one setting of `config`.
    fn configure_svd(
        &self,
        handle: OpaqueHandle,
        config: OpaqueHandle,
        setting: SvdSetting,
    ) -> Result<(), NativeError>;
    fn destroy_svd_config(&self, config: OpaqueHandle) -> Result<(), NativeError>;

    /// A new SVD report, which each gate split given it overwrites.
    fn create_svd_info(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, NativeError>;
    /// What the last gate split given `info` truncated; valid once the
    /// stream it ran on is synchronized.
    fn svd_truncation(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<SvdTruncation, NativeError>;
    fn destroy_svd_info(&self, info: OpaqueHandle) -> Result<(), NativeError>;

    /// Sizes `workspace` for one gate split of these descriptors under
    /// `config` (`CUTENSORNET_GATE_SPLIT_ALGO_REDUCED`).
    fn gate_split_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        descriptors: &GateSplitDescriptors,
        config: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError>;
    /// Queues one gate split on `stream`: contracts A, B and the gate, and
    /// factors the result by a truncated SVD into U and V, which are packed
    /// at the reduced extent of their shared mode; `info` reports the
    /// truncation.
    fn gate_split(
        &self,
        handle: OpaqueHandle,
        operands: &GateSplitOperands,
        config: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), NativeError>;

    /// Sizes `workspace` for one QR of `input` into `q` and `r`.
    fn qr_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        input: OpaqueHandle,
        q: OpaqueHandle,
        r: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError>;
    /// Queues one thin QR on `stream`: Q's and R's shared mode has the
    /// smaller extent of the two sides.
    fn tensor_qr(
        &self,
        handle: OpaqueHandle,
        input: TensorOperand,
        q: TensorOperand,
        r: TensorOperand,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), NativeError>;

    /// A network that contracts two tensors into `output`: every label
    /// absent from the output is summed.
    fn create_contraction_network(
        &self,
        handle: OpaqueHandle,
        inputs: [TensorModes<'_>; 2],
        output: TensorModes<'_>,
    ) -> Result<OpaqueHandle, NativeError>;
    fn destroy_contraction_network(&self, network: OpaqueHandle) -> Result<(), NativeError>;
    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, NativeError>;
    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), NativeError>;
    /// Finds a contraction path for `network` within `workspace_limit`
    /// bytes, into `info`, unsliced, since [`contract`](Self::contract)
    /// runs the whole network at once: a path the optimizer sliced anyway
    /// is refused (`NativeError::InvalidNativeResult`), so a contraction is
    /// never partial.
    fn optimize_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace_limit: u64,
        info: OpaqueHandle,
    ) -> Result<(), NativeError> {
        self.find_contraction_path(handle, network, workspace_limit, info)?;
        let slices = self.contraction_slice_count(handle, info)?;
        if slices > 1 {
            return Err(NativeError::InvalidNativeResult {
                reason: format!(
                    "the contraction optimizer cut the network into {slices} slices, \
                     but a contraction runs it whole"
                ),
            });
        }
        Ok(())
    }
    /// The search behind [`optimize_contraction`](Self::optimize_contraction),
    /// which checks its result: finds a path for `network` within
    /// `workspace_limit` bytes, into `info`, with slicing disabled; the
    /// optimizer's configuration is created and destroyed inside.
    fn find_contraction_path(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace_limit: u64,
        info: OpaqueHandle,
    ) -> Result<(), NativeError>;
    /// How many slices `info`'s path cuts its network into: 1 if unsliced.
    fn contraction_slice_count(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<i64, NativeError>;
    /// Sizes `workspace` for contracting `network` along `info`'s path.
    fn contraction_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError>;
    /// A plan to contract `network` along `info`'s path, in `workspace`,
    /// which must be sized for it.
    fn create_contraction_plan(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<OpaqueHandle, NativeError>;
    fn destroy_contraction_plan(&self, plan: OpaqueHandle) -> Result<(), NativeError>;
    /// Queues the contraction of `inputs` into `output` on `stream`, by
    /// `plan`, whole: its path is unsliced
    /// ([`optimize_contraction`](Self::optimize_contraction)).
    fn contract(
        &self,
        handle: OpaqueHandle,
        plan: OpaqueHandle,
        inputs: [OpaqueHandle; 2],
        output: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), NativeError>;
}

/// One tensor's modes as a descriptor takes them: the extent and the label
/// of each mode, first mode fastest. Two tensors' modes of the same label
/// are one index, contracted or shared.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TensorModes<'a> {
    pub(crate) extents: &'a [i64],
    pub(crate) modes: &'a [i32],
}

/// A tensor descriptor and the device buffer it describes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TensorOperand {
    pub(crate) descriptor: OpaqueHandle,
    pub(crate) data: OpaqueHandle,
}

/// The descriptors of one gate split: the inputs A and B, the gate, and the
/// outputs U and V. U's and V's shared label is the bond the SVD cuts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateSplitDescriptors {
    pub(crate) a: OpaqueHandle,
    pub(crate) b: OpaqueHandle,
    pub(crate) gate: OpaqueHandle,
    pub(crate) u: OpaqueHandle,
    pub(crate) v: OpaqueHandle,
}

/// The operands of one gate split, as in [`GateSplitDescriptors`], and a
/// device buffer for the singular values. cuTensorNet writes none there
/// when it absorbs them into U or V, and accepts a null pointer then, but
/// only a buffer has been run on a GPU, so one is always passed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateSplitOperands {
    pub(crate) a: TensorOperand,
    pub(crate) b: TensorOperand,
    pub(crate) gate: TensorOperand,
    pub(crate) u: TensorOperand,
    pub(crate) v: TensorOperand,
    pub(crate) singular_values: OpaqueHandle,
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
    DiscardedWeightCutoff(f64),
}

/// What a gate split's truncation did
/// (`cutensornetTensorSVDInfoGetAttribute`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SvdTruncation {
    /// The extent of the shared mode as U and V hold it.
    pub(crate) reduced_extent: i64,
    /// The extent of the shared mode without truncation: the smaller side
    /// of the split.
    pub(crate) full_extent: i64,
    /// The squared sum of the dropped singular values over that of all.
    pub(crate) discarded_weight: f64,
}
