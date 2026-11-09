// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Complex64Abi`]: the complex number every buffer and operator the engine
//! hands cuTensorNet is made of, in the layout of CUDA's `cuDoubleComplex`.
//!
//! The engine writes matrices and reads results as `num_complex::Complex64`:
//! two `f64`s, real then imaginary (`repr(C)`), aligned to 8 bytes. CUDA's
//! `cuDoubleComplex`, which cuTensorNet reads, is the same pair aligned to
//! 16. This type is that layout, `repr(C, align(16))`, and converts to and
//! from `Complex64`. The `const` assertions below fail the build if its
//! size, alignment or field offsets change; change it only with the bindings
//! that pass it. [`complex_bytes`] sizes a device buffer of them, for the
//! engine and the native calls alike.

use crate::engine::EngineError;
use num_complex::Complex64;
use std::mem::{align_of, offset_of, size_of};

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Complex64Abi {
    pub(in crate::engine) re: f64,
    pub(in crate::engine) im: f64,
}

impl Complex64Abi {
    pub(in crate::engine) const fn new(re: f64, im: f64) -> Self {
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
/// call, as an invalid call: no tensor the engine allocates or copies is
/// empty, so an empty one is the engine's fault, not the library's.
pub(crate) fn complex_bytes(elements: usize, resource: &'static str) -> Result<usize, EngineError> {
    if elements == 0 {
        return Err(EngineError::InvalidNativeCall {
            reason: format!("a {resource} needs at least one element"),
        });
    }
    elements
        .checked_mul(size_of::<Complex64Abi>())
        .ok_or(EngineError::ResourceSizeOverflow { resource })
}

const _: () = assert!(size_of::<Complex64Abi>() == 16);
const _: () = assert!(align_of::<Complex64Abi>() == 16);
const _: () = assert!(offset_of!(Complex64Abi, re) == 0);
const _: () = assert!(offset_of!(Complex64Abi, im) == 8);
