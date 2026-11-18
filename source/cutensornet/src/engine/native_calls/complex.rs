// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Complex64Abi`]: the complex number every buffer handed to cuTensorNet
//! is made of, in the layout of CUDA's `cuDoubleComplex`.
//!
//! The CUDA algebra writes tensors and gates and reads results as
//! `num_complex::Complex64`:
//! two `f64`s, real then imaginary (`repr(C)`), aligned to 8 bytes. CUDA's
//! `cuDoubleComplex`, which cuTensorNet reads, is the same pair aligned to
//! 16. This type is that layout, `repr(C, align(16))`, and converts to and
//! from `Complex64`. The `const` assertions below fail the build if its
//! size, alignment or field offsets change; change it only with the bindings
//! that pass it. [`complex_bytes`] sizes a device buffer of them, for the
//! algebra and the native calls alike.

use crate::engine::CudaAlgebraError;
use num_complex::Complex64;
use std::mem::{align_of, offset_of, size_of};

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Complex64Abi {
    pub(crate) re: f64,
    pub(crate) im: f64,
}

impl Complex64Abi {
    pub(crate) const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
}

impl From<Complex64Abi> for Complex64 {
    fn from(value: Complex64Abi) -> Self {
        Self::new(value.re, value.im)
    }
}

impl From<Complex64> for Complex64Abi {
    fn from(value: Complex64) -> Self {
        Self::new(value.re, value.im)
    }
}

/// The bytes of a device buffer of `elements` complex numbers, named
/// `resource` in an overflow. An empty buffer is refused before any native
/// call, as an invalid call: no tensor has zero elements (every extent is at
/// least 1), so an empty one is the caller's fault, not the library's.
pub(crate) fn complex_bytes(
    elements: usize,
    resource: &'static str,
) -> Result<usize, CudaAlgebraError> {
    if elements == 0 {
        return Err(CudaAlgebraError::InvalidNativeCall {
            reason: format!("a {resource} needs at least one element"),
        });
    }
    elements
        .checked_mul(size_of::<Complex64Abi>())
        .ok_or(CudaAlgebraError::ResourceSizeOverflow { resource })
}

const _: () = assert!(size_of::<Complex64Abi>() == 16);
const _: () = assert!(align_of::<Complex64Abi>() == 16);
const _: () = assert!(offset_of!(Complex64Abi, re) == 0);
const _: () = assert!(offset_of!(Complex64Abi, im) == 8);
