// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Sides`]: the two outputs of a factorization (U and V, or Q and R),
//! the matrix of the inputs they split, and the factors laid out in
//! their descriptors' mode orders.

use super::super::{
    dense_tensor::{DenseTensor, contract},
    failed, to_usize,
};
use super::tensor_shape::{TensorShape, product};
use crate::engine::CudaAlgebraError;
use num_complex::Complex64;

/// The two outputs of a split (U and V, or Q and R): the one mode they
/// share and its extent, and each one's other modes and their extents, the
/// rows and the columns of the split's matrix.
pub(super) struct Sides {
    pub(super) shared: i32,
    pub(super) shared_extent: i64,
    pub(super) left: (Vec<i32>, Vec<i64>),
    pub(super) right: (Vec<i32>, Vec<i64>),
}

impl Sides {
    pub(super) fn of(
        left: &TensorShape,
        right: &TensorShape,
        operation: &'static str,
    ) -> Result<Self, CudaAlgebraError> {
        let mut shared = left.modes.iter().filter(|mode| right.modes.contains(mode));
        let (Some(&shared), None) = (shared.next(), shared.next()) else {
            return Err(failed(operation, "the outputs share other than one mode"));
        };
        let shared_extent = left.extent(shared).unwrap_or(1);
        if right.extent(shared) != Some(shared_extent) {
            return Err(failed(operation, "the outputs' shared extents differ"));
        }
        Ok(Self {
            shared,
            shared_extent,
            left: left.free_modes(shared),
            right: right.free_modes(shared),
        })
    }

    /// The contraction of `inputs` as a column-major matrix, its rows the
    /// left output's other modes and its columns the right's, with its row
    /// and column counts.
    pub(super) fn matrix(
        &self,
        inputs: &[&DenseTensor],
        operation: &'static str,
    ) -> Result<(Vec<Complex64>, usize, usize), CudaAlgebraError> {
        let modes = [self.left.0.as_slice(), self.right.0.as_slice()].concat();
        let matrix = contract(inputs, &modes).map_err(|message| failed(operation, message))?;
        let extents: Vec<usize> = [self.left.1.as_slice(), self.right.1.as_slice()]
            .concat()
            .into_iter()
            .map(to_usize)
            .collect::<Result<_, _>>()?;
        if matrix.extents != extents {
            return Err(failed(
                operation,
                "an output's extent differs from the inputs'",
            ));
        }
        Ok((matrix.data, product(&self.left.1)?, product(&self.right.1)?))
    }

    /// The factors `matrices`, left (rows × `extent`) and right (`extent` ×
    /// columns), both column-major, laid out in their descriptors' mode
    /// orders `layouts`.
    pub(super) fn factors(
        &self,
        matrices: [Vec<Complex64>; 2],
        extent: i64,
        layouts: [&[i32]; 2],
        operation: &'static str,
    ) -> Result<[DenseTensor; 2], CudaAlgebraError> {
        let [left, right] = matrices;
        let left = TensorShape {
            extents: [self.left.1.as_slice(), &[extent]].concat(),
            modes: [self.left.0.as_slice(), &[self.shared]].concat(),
        }
        .dense(left)?;
        let right = TensorShape {
            extents: [&[extent], self.right.1.as_slice()].concat(),
            modes: [&[self.shared], self.right.0.as_slice()].concat(),
        }
        .dense(right)?;
        let laid_out = |matrix: &DenseTensor, layout: &[i32]| {
            contract(&[matrix], layout).map_err(|message| failed(operation, message))
        };
        Ok([laid_out(&left, layouts[0])?, laid_out(&right, layouts[1])?])
    }
}
