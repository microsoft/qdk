// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the fake looks up of a contraction network and its optimizer
//! infos: the network an info is for and whether it holds a path, the
//! path a network was optimized to and the scratch lent at its
//! preparation, and the objects that are part of it.

use super::super::{Inner, Object, failed, id};
use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::session_calls::CuTensorNetHandle;
use crate::engine::native_calls::tensor_calls::{ContractionOptimizerInfo, NetworkDescriptor};

impl Inner {
    /// The network an optimizer info, which must belong to `handle`, is
    /// for, and whether it holds a path.
    pub(super) fn optimizer_info(
        &self,
        handle: CuTensorNetHandle,
        info: ContractionOptimizerInfo,
        operation: &'static str,
    ) -> Result<(usize, bool), CudaAlgebraError> {
        self.owned(handle, info, operation)?;
        match self.objects.get(&id(info)) {
            Some(&Object::OptimizerInfo { network, optimized }) => Ok((network, optimized)),
            _ => Err(failed(operation, "no such optimizer info")),
        }
    }

    /// The info whose path `network`, which must belong to `handle`, was
    /// last optimized to, and, once prepared, the device scratch bytes lent
    /// at its last preparation.
    pub(super) fn network(
        &self,
        handle: CuTensorNetHandle,
        network: NetworkDescriptor,
        operation: &'static str,
    ) -> Result<(Option<usize>, Option<i64>), CudaAlgebraError> {
        self.owned(handle, network, operation)?;
        match self.objects.get(&id(network)) {
            Some(&Object::Network { path, prepared, .. }) => Ok((path, prepared)),
            _ => Err(failed(operation, "no such network")),
        }
    }

    /// Fails unless `info` holds an optimized path of `network`.
    pub(super) fn path_of(
        &self,
        handle: CuTensorNetHandle,
        network: NetworkDescriptor,
        info: ContractionOptimizerInfo,
        operation: &'static str,
    ) -> Result<(), CudaAlgebraError> {
        self.owned(handle, network, operation)?;
        match self.optimizer_info(handle, info, operation)? {
            (of, _) if of != id(network) => Err(failed(
                operation,
                "the optimizer info is for another network",
            )),
            (_, false) => Err(failed(operation, "the path is not optimized")),
            _ => Ok(()),
        }
    }

    /// Whether a live object is `is_part` of `whole`.
    pub(super) fn has_part(&self, whole: usize, is_part: impl Fn(&Object, usize) -> bool) -> bool {
        self.objects.values().any(|object| is_part(object, whole))
    }
}
