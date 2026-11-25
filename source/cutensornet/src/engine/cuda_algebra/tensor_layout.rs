// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TensorLayout`]: a tensor's extents and its modes' labels, the two
//! lists that a tensor descriptor and a contraction's input take together.
//!
//! The gate split and the move of the centre each lay out their tensors
//! in a table (their module headers). Each row is one `TensorLayout`, so
//! the code reads as the table does, and a tensor's extents never part
//! from its labels.

use super::extents::element_count;
use crate::engine::cuda_algebra_error::CudaAlgebraError;

/// The extents of a tensor of `RANK` modes, and the label of each mode
/// (`modes.rs`), in the order its buffer lays them out.
#[derive(Clone, Copy)]
pub(super) struct TensorLayout<const RANK: usize> {
    pub(super) extents: [usize; RANK],
    pub(super) modes: [i32; RANK],
}

impl<const RANK: usize> TensorLayout<RANK> {
    /// The tensor's element count, refused if it does not fit a `usize`.
    #[inline]
    pub(super) fn elements(&self) -> Result<usize, CudaAlgebraError> {
        element_count(&self.extents)
    }
}
