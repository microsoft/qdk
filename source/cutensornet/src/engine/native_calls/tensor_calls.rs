// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TensorCalls`]: cuTensorNet's tensor API, which factors and contracts
//! one or two tensors at a time for a caller that holds an MPS in its own
//! device buffers, and the types its operations share. The gate split's
//! and the contraction's own types are in their files; the QR has none.
//!
//! An engine that keeps its MPS canonical, site by site, needs the
//! library's small steps: split a two-site gate's result by a truncated SVD
//! (`gate_split`), move the canonical centre by a QR (`tensor_qr`), and
//! contract two tensors (`contract`).
//!
//! Every tensor is complex double (`CUDA_C_64F`), its first mode fastest
//! (null strides), and every call computes in double precision
//! (`CUTENSORNET_COMPUTE_64F`). The objects a caller creates here
//! (descriptors, SVD configs and infos, networks, optimizer infos) belong
//! to the handle they were created with, and are used only with it.
//!
//! A call that runs on a stream (`gate_split`, `tensor_qr`, `contract`)
//! runs in a workspace last sized for that call (its `…_workspace_sizes`)
//! and lent the memory each slot asks for (`MemoryCalls::set_workspace`).
//! Its results are ready once the caller synchronizes the stream.

use super::{
    memory_calls::{DeviceBuffer, MemoryCalls, WorkspaceDescriptor},
    native_pointer,
    session_calls::{CuTensorNetHandle, CudaStream},
};
use crate::engine::CudaAlgebraError;

mod contraction;
mod gate_split;

pub(crate) use contraction::{
    ContractionOptimizerInfo, NativeContractionNetwork, NetworkDescriptor,
};
pub(crate) use gate_split::{
    GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation, TensorSvdConfig,
    TensorSvdInfo,
};

/// The calls of cuTensorNet's tensor API that a canonical MPS needs: tensor
/// descriptors, the gate split and its SVD settings and report, the QR, and
/// pairwise contractions.
///
/// A contraction runs through a network of its two inputs and its output.
/// Once, the network is:
///
/// 1. created
///    ([`create_contraction_network`](Self::create_contraction_network));
/// 2. given a path ([`optimize_contraction`](Self::optimize_contraction));
/// 3. given a workspace sized for it
///    ([`contraction_workspace_sizes`](Self::contraction_workspace_sizes))
///    and lent its memory (`MemoryCalls::set_workspace`);
/// 4. prepared ([`prepare_contraction`](Self::prepare_contraction)).
///
/// It is then contracted any number of times, each on the buffers given to
/// [`contract`](Self::contract), with at least the device scratch lent at
/// the preparation. So a caller may keep one network per shape. Each path
/// search, even into the same optimizer info, may find another path, so it
/// needs a new sizing and preparation. A network's optimizer infos are
/// destroyed before it.
pub(crate) trait TensorCalls: MemoryCalls {
    /// A new descriptor of `tensor`'s extents and mode labels
    /// (`cutensornetCreateTensorDescriptor`), which a split or a QR names
    /// its operands by.
    fn create_tensor_descriptor(
        &self,
        handle: CuTensorNetHandle,
        tensor: TensorModes<'_>,
    ) -> Result<TensorDescriptor, CudaAlgebraError>;
    /// Releases `descriptor` (`cutensornetDestroyTensorDescriptor`), once,
    /// after the calls that use it.
    fn destroy_tensor_descriptor(
        &self,
        descriptor: TensorDescriptor,
    ) -> Result<(), CudaAlgebraError>;

    /// A new SVD configuration (`cutensornetCreateTensorSVDConfig`), with
    /// cuTensorNet's defaults: no truncation beyond the output descriptors'
    /// extents, no normalization, and the singular values returned apart.
    fn create_svd_config(
        &self,
        handle: CuTensorNetHandle,
    ) -> Result<TensorSvdConfig, CudaAlgebraError>;
    /// Sets one setting of `config`
    /// (`cutensornetTensorSVDConfigSetAttribute`).
    fn configure_svd(
        &self,
        handle: CuTensorNetHandle,
        config: TensorSvdConfig,
        setting: SvdSetting,
    ) -> Result<(), CudaAlgebraError>;
    /// Releases `config` (`cutensornetDestroyTensorSVDConfig`), once, after
    /// the splits that use it.
    fn destroy_svd_config(&self, config: TensorSvdConfig) -> Result<(), CudaAlgebraError>;

    /// A new SVD report (`cutensornetCreateTensorSVDInfo`), which each gate
    /// split given it overwrites.
    fn create_svd_info(&self, handle: CuTensorNetHandle)
    -> Result<TensorSvdInfo, CudaAlgebraError>;
    /// What the last gate split given `info` truncated
    /// (`cutensornetTensorSVDInfoGetAttribute`); valid once the stream it
    /// ran on is synchronized.
    fn svd_truncation(
        &self,
        handle: CuTensorNetHandle,
        info: TensorSvdInfo,
    ) -> Result<SvdTruncation, CudaAlgebraError>;
    /// Releases `info` (`cutensornetDestroyTensorSVDInfo`), once, after its
    /// report is read.
    fn destroy_svd_info(&self, info: TensorSvdInfo) -> Result<(), CudaAlgebraError>;

    /// Sizes `workspace` for one gate split of these descriptors under
    /// `config` (`cutensornetWorkspaceComputeGateSplitSizes`, with
    /// `CUTENSORNET_GATE_SPLIT_ALGO_REDUCED`).
    fn gate_split_workspace_sizes(
        &self,
        handle: CuTensorNetHandle,
        descriptors: &GateSplitDescriptors,
        config: TensorSvdConfig,
        workspace: WorkspaceDescriptor,
    ) -> Result<(), CudaAlgebraError>;
    /// Queues one gate split on `stream` (`cutensornetGateSplit`): contracts
    /// A, B and the gate, and factors the result by a truncated SVD into U
    /// and V, which are packed at the reduced extent of their shared mode.
    /// `info` reports the truncation.
    ///
    /// A, B and the gate "are expected to form a fully connected graph"
    /// (each pair shares a mode). Their modes not contracted go to U and V:
    /// U's "consistent with" A and the gate, V's with B and the gate
    /// (cuTensorNet's `cutensornetGateSplit`). So A's own modes go to U,
    /// B's to V, and the gate's to either.
    ///
    /// The shared mode's extent in U's and V's descriptors "shall not
    /// exceed the minimum of m (row dimension) and n (column dimension) of
    /// the smallest equivalent matrix SVD problem" (cuTensorNet's
    /// `cutensornetGateSplit`). That is at most the smaller side of the
    /// product of A, B and the gate as a matrix, its rows U's other modes
    /// and its columns V's, and perhaps less
    /// (see [`SvdTruncation::full_extent`]).
    ///
    /// The reduced extent is known only once the split has run. So U's and
    /// V's buffers hold their descriptors' extents as given, before
    /// truncation, and the singular values' buffer holds as many doubles as
    /// the shared mode's extent. Under a discarded-weight cutoff, the call
    /// lowers the shared mode's extent in U's and V's descriptors to the
    /// reduced extent, which caps the next split that reuses them.
    /// Otherwise U's and V's descriptors keep their extents.
    fn gate_split(
        &self,
        handle: CuTensorNetHandle,
        operands: &GateSplitOperands,
        config: TensorSvdConfig,
        info: TensorSvdInfo,
        workspace: WorkspaceDescriptor,
        stream: CudaStream,
    ) -> Result<(), CudaAlgebraError>;

    /// Sizes `workspace` for one QR of `input` into `q` and `r`
    /// (`cutensornetWorkspaceComputeQRSizes`).
    fn qr_workspace_sizes(
        &self,
        handle: CuTensorNetHandle,
        input: TensorDescriptor,
        q: TensorDescriptor,
        r: TensorDescriptor,
        workspace: WorkspaceDescriptor,
    ) -> Result<(), CudaAlgebraError>;
    /// Queues one thin QR on `stream` (`cutensornetTensorQR`): Q's and R's
    /// shared mode has the smaller extent of the two sides.
    fn tensor_qr(
        &self,
        handle: CuTensorNetHandle,
        input: TensorOperand,
        q: TensorOperand,
        r: TensorOperand,
        workspace: WorkspaceDescriptor,
        stream: CudaStream,
    ) -> Result<(), CudaAlgebraError>;

    /// A network that contracts two tensors into a tensor of the labels
    /// `output_modes`, in that order (`cutensornetCreateNetwork`): every
    /// label absent from the output is summed, and each output mode takes
    /// its extent from the inputs.
    fn create_contraction_network(
        &self,
        handle: CuTensorNetHandle,
        inputs: [TensorModes<'_>; 2],
        output_modes: &[i32],
    ) -> Result<NativeContractionNetwork, CudaAlgebraError>;
    /// Releases `network` (`cutensornetDestroyNetwork`), once, after its
    /// optimizer infos and the contractions that use it.
    fn destroy_contraction_network(
        &self,
        network: NetworkDescriptor,
    ) -> Result<(), CudaAlgebraError>;
    /// A new optimizer info of `network`
    /// (`cutensornetCreateContractionOptimizerInfo`), which a path search
    /// writes its path into.
    fn create_optimizer_info(
        &self,
        handle: CuTensorNetHandle,
        network: NetworkDescriptor,
    ) -> Result<ContractionOptimizerInfo, CudaAlgebraError>;
    /// Releases `info` (`cutensornetDestroyContractionOptimizerInfo`), once,
    /// before its network.
    fn destroy_optimizer_info(
        &self,
        info: ContractionOptimizerInfo,
    ) -> Result<(), CudaAlgebraError>;
    /// Finds a contraction path for `network`
    /// (`cutensornetContractionOptimize`), into `info` and the network,
    /// passing `workspace_limit` bytes as the optimizer's memory
    /// constraint. The optimizer's configuration is created and destroyed
    /// inside. The path's workspace may exceed the limit, since slicing is
    /// disabled (below). [`contraction_workspace_sizes`] gives what it
    /// needs.
    ///
    /// Slicing is disabled, so the path contracts the network in one
    /// contraction per call. Slicing trades memory for one contraction per
    /// slice, and a pairwise contraction needs no such trade. Should a
    /// path come back sliced anyway, [`contract`](Self::contract) still
    /// contracts every slice, so its result stays whole.
    ///
    /// [`contraction_workspace_sizes`]: Self::contraction_workspace_sizes
    fn optimize_contraction(
        &self,
        handle: CuTensorNetHandle,
        network: NetworkDescriptor,
        workspace_limit: u64,
        info: ContractionOptimizerInfo,
    ) -> Result<(), CudaAlgebraError>;
    /// Sizes `workspace` for contracting `network` along `info`'s path
    /// (`cutensornetWorkspaceComputeContractionSizes`).
    fn contraction_workspace_sizes(
        &self,
        handle: CuTensorNetHandle,
        network: NetworkDescriptor,
        info: ContractionOptimizerInfo,
        workspace: WorkspaceDescriptor,
    ) -> Result<(), CudaAlgebraError>;
    /// Prepares `network` to be contracted along the path it was last
    /// optimized to (`cutensornetNetworkPrepareContraction`), in
    /// `workspace`, which must be sized for that path and lent its memory
    /// (`MemoryCalls::set_workspace`). cuTensorNet checks the device
    /// scratch's size here, and [`contract`](Self::contract) needs at least
    /// the device scratch lent at this preparation.
    fn prepare_contraction(
        &self,
        handle: CuTensorNetHandle,
        network: NetworkDescriptor,
        workspace: WorkspaceDescriptor,
    ) -> Result<(), CudaAlgebraError>;
    /// Queues the contraction of `inputs` into `output` on `stream`
    /// (`cutensornetNetworkContract`), by the prepared `network`, whole:
    /// every slice of its path, should it have several
    /// ([`optimize_contraction`](Self::optimize_contraction)). The buffers
    /// are given to the network at each call, so one prepared network
    /// contracts new buffers of its shapes each time.
    fn contract(
        &self,
        handle: CuTensorNetHandle,
        network: NativeContractionNetwork,
        inputs: [DeviceBuffer; 2],
        output: DeviceBuffer,
        workspace: WorkspaceDescriptor,
        stream: CudaStream,
    ) -> Result<(), CudaAlgebraError>;
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
    pub(crate) descriptor: TensorDescriptor,
    pub(crate) data: DeviceBuffer,
}

native_pointer! {
    /// A cuTensorNet tensor descriptor (`cutensornetTensorDescriptor_t`):
    /// a tensor's extents and mode labels, which a split or a QR names its
    /// operands by.
    TensorDescriptor
}
