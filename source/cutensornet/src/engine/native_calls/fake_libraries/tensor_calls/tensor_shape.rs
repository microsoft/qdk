// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TensorShape`]: what the fake keeps of a tensor descriptor, the
//! extent and the label of each mode, checked when the descriptor is
//! created, and the dense tensor a buffer holds by it.

use super::super::{dense_tensor::DenseTensor, failed, to_usize};
use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::tensor_calls::TensorModes;
use num_complex::Complex64;

/// A tensor descriptor's modes: the extent and the label of each.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TensorShape {
    pub(super) extents: Vec<i64>,
    pub(super) modes: Vec<i32>,
}

impl TensorShape {
    pub(super) fn of(
        tensor: TensorModes<'_>,
        operation: &'static str,
    ) -> Result<Self, CudaAlgebraError> {
        if tensor.extents.len() != tensor.modes.len() {
            return Err(failed(operation, "extents and modes of different lengths"));
        }
        if tensor.extents.iter().any(|&extent| extent < 1) {
            return Err(failed(operation, "an extent below 1"));
        }
        for (at, mode) in tensor.modes.iter().enumerate() {
            if tensor.modes[..at].contains(mode) {
                return Err(failed(operation, "a repeated mode"));
            }
        }
        Ok(Self {
            extents: tensor.extents.to_vec(),
            modes: tensor.modes.to_vec(),
        })
    }

    pub(super) fn extent(&self, mode: i32) -> Option<i64> {
        self.modes
            .iter()
            .position(|&m| m == mode)
            .map(|at| self.extents[at])
    }

    /// The modes other than `shared`, and their extents.
    pub(super) fn free_modes(&self, shared: i32) -> (Vec<i32>, Vec<i64>) {
        self.modes
            .iter()
            .zip(&self.extents)
            .filter(|&(&mode, _)| mode != shared)
            .map(|(&mode, &extent)| (mode, extent))
            .unzip()
    }

    pub(super) fn elements(&self) -> Result<usize, CudaAlgebraError> {
        product(&self.extents)
    }

    pub(super) fn dense(&self, data: Vec<Complex64>) -> Result<DenseTensor, CudaAlgebraError> {
        Ok(DenseTensor {
            extents: self
                .extents
                .iter()
                .map(|&extent| to_usize(extent))
                .collect::<Result<_, _>>()?,
            modes: self.modes.clone(),
            data,
        })
    }
}

/// The number of elements of a tensor of `extents`. A count that
/// overflows `usize` is refused, as no buffer could hold it.
pub(super) fn product(extents: &[i64]) -> Result<usize, CudaAlgebraError> {
    extents.iter().try_fold(1, |product: usize, &extent| {
        product
            .checked_mul(to_usize(extent)?)
            .ok_or(CudaAlgebraError::ResourceSizeOverflow {
                resource: "tensor elements",
            })
    })
}
