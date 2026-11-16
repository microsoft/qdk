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
//! (`contract`). Its user, `CudaAlgebra`, is that simulator's linear
//! algebra on the GPU.
//!
//! Every tensor is complex double (`CUDA_C_64F`), its first mode fastest
//! (null strides), and every call computes in double precision
//! (`CUTENSORNET_COMPUTE_64F`). The objects a caller creates here
//! (descriptors, SVD configs and infos, networks, optimizer infos) belong
//! to the handle they were created with, and are used only with it.
//! A call that runs on a stream (`gate_split`, `tensor_qr`, `contract`) runs
//! in a workspace last sized for that call (its `…_workspace_sizes`;
//! preparing a state or an expectation in it sizes it for the state API
//! instead) and lent the memory each slot asks for
//! (`MemoryCalls::set_workspace`); its results are ready once the caller
//! synchronizes the stream.
//!
//! The tensor API is not required to open the libraries, since the engine
//! never calls it; `CudaAlgebra` checks it when it opens:
//! [`check_tensor_functions`](TensorCalls::check_tensor_functions) names
//! each function the library lacks; when any is missing, every other
//! method fails with the same error.

use super::{OpaqueHandle, Stream, memory_calls::MemoryCalls};
use crate::native_calls::error::NativeError;

/// The calls of cuTensorNet's tensor API that a canonical MPS needs: tensor
/// descriptors, the gate split and its SVD settings and report, the QR, and
/// pairwise contractions.
///
/// A contraction runs through a network of its two inputs and its output.
/// The network is created
/// ([`create_contraction_network`](Self::create_contraction_network)),
/// given a path ([`optimize_contraction`](Self::optimize_contraction)), a
/// workspace sized for it
/// ([`contraction_workspace_sizes`](Self::contraction_workspace_sizes)) and
/// lent its memory (`MemoryCalls::set_workspace`), and prepared
/// ([`prepare_contraction`](Self::prepare_contraction)) once, then
/// contracted any number of times, each time on the buffers given to
/// [`contract`](Self::contract) and with at least the device scratch lent
/// at the preparation, so a caller may keep one network per shape. Each
/// path search, even into the same optimizer info, may find another path,
/// so it needs a new sizing and preparation. Its optimizer infos are
/// destroyed before it.
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
    ///
    /// A, B and the gate "are expected to form a fully connected graph"
    /// (each pair shares a mode), and their modes not contracted go to U
    /// and V: U's "consistent with" A and the gate, V's with B and the gate
    /// (cuTensorNet's `cutensornetGateSplit`); so A's own modes go to U, B's
    /// to V, and the gate's to either.
    ///
    /// The shared mode's extent in U's and V's descriptors "shall not
    /// exceed the minimum of m (row dimension) and n (column dimension) of
    /// the smallest equivalent matrix SVD problem" (cuTensorNet's
    /// `cutensornetGateSplit`): at most the smaller side of the product of
    /// A, B and the gate as a matrix, its rows U's other modes and its
    /// columns V's, and perhaps less (see [`SvdTruncation::full_extent`]).
    ///
    /// U's and V's buffers hold their descriptors' extents as given, before
    /// truncation, and the singular values' buffer as many doubles as the
    /// shared mode's extent, since the reduced extent is known only once
    /// the split has run. Under a discarded-weight cutoff, the call lowers
    /// the shared mode's extent in U's and V's descriptors to the reduced
    /// extent, which caps the next split that reuses them; otherwise U's
    /// and V's descriptors keep their extents.
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

    /// A network that contracts two tensors into a tensor of the labels
    /// `output_modes`, in that order: every label absent from the output is
    /// summed, and each output mode takes its extent from the inputs.
    fn create_contraction_network(
        &self,
        handle: OpaqueHandle,
        inputs: [TensorModes<'_>; 2],
        output_modes: &[i32],
    ) -> Result<ContractionNetwork, NativeError>;
    fn destroy_contraction_network(&self, network: OpaqueHandle) -> Result<(), NativeError>;
    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, NativeError>;
    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), NativeError>;
    /// Finds a contraction path for `network`, into `info` and the network,
    /// passing `workspace_limit` bytes as the optimizer's memory
    /// constraint; the optimizer's configuration is created and destroyed
    /// inside. The path's workspace may exceed the limit, since slicing is
    /// disabled (below); [`contraction_workspace_sizes`] gives what it
    /// needs.
    ///
    /// Slicing is disabled, so the path contracts the network in one
    /// contraction per call: slicing trades memory for one contraction per
    /// slice, and a pairwise contraction needs no such trade. Should a
    /// path come back sliced anyway, [`contract`](Self::contract) still
    /// contracts every slice, so its result stays whole.
    ///
    /// [`contraction_workspace_sizes`]: Self::contraction_workspace_sizes
    fn optimize_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace_limit: u64,
        info: OpaqueHandle,
    ) -> Result<(), NativeError>;
    /// Sizes `workspace` for contracting `network` along `info`'s path.
    fn contraction_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError>;
    /// Prepares `network` to be contracted along the path it was last
    /// optimized to, in `workspace`, which must be sized for that path and
    /// lent its memory (`MemoryCalls::set_workspace`): cuTensorNet checks
    /// the device scratch's size here, and [`contract`](Self::contract)
    /// needs at least the device scratch lent at this preparation.
    fn prepare_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError>;
    /// Queues the contraction of `inputs` into `output` on `stream`, by the
    /// prepared `network`, whole: every slice of its path, should it have
    /// several ([`optimize_contraction`](Self::optimize_contraction)). The
    /// buffers are given to the network at each call, so one prepared
    /// network contracts new buffers of its shapes each time.
    fn contract(
        &self,
        handle: OpaqueHandle,
        network: ContractionNetwork,
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

/// A contraction network and the ids cuTensorNet gave its two inputs, by
/// which [`TensorCalls::contract`] hands it their buffers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContractionNetwork {
    pub(crate) descriptor: OpaqueHandle,
    pub(crate) input_ids: [i64; 2],
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
