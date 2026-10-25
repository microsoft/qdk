//! The complex scalar ABI shared with cuTensorNet and CUDA.
//!
//! `Complex64Abi` is `repr(C, align(16))`, two `f64`s, matching
//! `cuDoubleComplex`; the `const` assertions fail the build if its size,
//! alignment or field offsets change. It converts to and from `num_complex`
//! values. Change it only together with the bindings that pass it.

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
