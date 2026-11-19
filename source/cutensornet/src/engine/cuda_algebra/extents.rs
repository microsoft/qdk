// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A tensor's extents as cuTensorNet takes them ([`native_extents`]), and
//! its element count ([`element_count`]), for the calls that describe,
//! read and make tensors. Both refuse a size that does not fit, with
//! [`CudaAlgebraError::ResourceSizeOverflow`].

use crate::engine::cuda_algebra_error::CudaAlgebraError;

/// `extents` as a descriptor takes them.
pub(super) fn native_extents(extents: &[usize]) -> Result<Vec<i64>, CudaAlgebraError> {
    extents
        .iter()
        .map(|&extent| {
            i64::try_from(extent).map_err(|_| CudaAlgebraError::ResourceSizeOverflow {
                resource: "tensor extent",
            })
        })
        .collect()
}

/// The element count of a tensor of `extents`.
pub(super) fn element_count(extents: &[usize]) -> Result<usize, CudaAlgebraError> {
    extents
        .iter()
        .try_fold(1_usize, |count, &extent| count.checked_mul(extent))
        .ok_or(CudaAlgebraError::ResourceSizeOverflow {
            resource: "device tensor",
        })
}
