// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Complex64Abi`]: the complex number every buffer and operator the engine
//! hands cuTensorNet is made of, in the layout of CUDA's `cuDoubleComplex`.
//!
//! The engine writes matrices and reads results as `num_complex::Complex64`,
//! whose layout Rust does not promise; cuTensorNet reads two `f64`s, real
//! then imaginary, aligned to 16 bytes. This type is that layout,
//! `repr(C, align(16))`, and converts to and from `Complex64`. The `const`
//! assertions below fail the build if its size, alignment or field offsets
//! change; change it only with the bindings that pass it.

use num_complex::Complex64;
use std::mem::{align_of, offset_of, size_of};

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Complex64Abi {
    pub(super) re: f64,
    pub(super) im: f64,
}

impl Complex64Abi {
    pub(super) const fn new(re: f64, im: f64) -> Self {
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

const _: () = assert!(size_of::<Complex64Abi>() == 16);
const _: () = assert!(align_of::<Complex64Abi>() == 16);
const _: () = assert!(offset_of!(Complex64Abi, re) == 0);
const _: () = assert!(offset_of!(Complex64Abi, im) == 8);
