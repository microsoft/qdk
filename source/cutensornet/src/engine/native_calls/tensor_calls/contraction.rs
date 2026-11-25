// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The types of a contraction's calls: cuTensorNet's network of two inputs
//! and an output, and the optimizer info that holds its path.

use crate::engine::native_calls::native_pointer;

/// cuTensorNet's descriptor of a contraction network, and the ids it gave
/// the network's two inputs, by which
/// [`TensorCalls::contract`](super::TensorCalls::contract) hands it their
/// buffers: the library's handle to a network, not the network.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeContractionNetwork {
    pub(crate) descriptor: NetworkDescriptor,
    pub(crate) input_ids: [i64; 2],
}

native_pointer! {
    /// A cuTensorNet network descriptor (`cutensornetNetworkDescriptor_t`):
    /// a contraction's inputs and output.
    NetworkDescriptor
}

native_pointer! {
    /// A cuTensorNet contraction optimizer info
    /// (`cutensornetContractionOptimizerInfo_t`): the path a search found
    /// for a network.
    ContractionOptimizerInfo
}
